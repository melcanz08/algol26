# ADR 0042: LLVM dynamic list representation

Status: Proposed (design; implementation pending)

## Context

`List<T>` on the LLVM backend has been a **stack-allocated,
fixed-size array** since ADR 0036 shipped. A list literal `[1, 2, 3]`
compiles to `alloca [3 x i64]`; the length is tracked in a Rust-side
`HashMap<String, usize>` (`list_lengths`) rather than in memory. See
`docs/features/list_llvm.md` for the full account.

This is what makes `List.length` free, iteration unrollable, and
list literals allocation-free. It is also why ADR 0028
(`List.append`) is interpreter-only: a stack alloca cannot grow,
and there is no runtime length to update.

The capability check refuses `Feature::ListAppend` on LLVM today.
That refusal is correct — the current representation cannot lower a
growing list — but it means ALGOL26 programs that filter,
accumulate, or otherwise build variable-length lists run only on the
interpreter. ADR 0028 already documents this as a deliberate
deferral, and this ADR is the follow-up that closes it.

Two design constraints shape the decision:

1. **List literals should stay allocation-free where possible.**
   `val xs := [1, 2, 3]` is a common pattern and should not need a
   `malloc` — the array is a stack alloca and its lifetime is the
   enclosing scope.

2. **Anything that grows must be heap-allocated and length-aware.**
   `.append(x)` needs to (a) know the current length, (b) grow the
   buffer, (c) update the length. Neither (a) nor (c) is possible
   with a compile-time `HashMap`.

Both constraints can be satisfied at once by a discriminated
representation.

## Decision

Change the LLVM representation of `List<T>` from "pointer to
`[N x T]` alloca" to a **three-word struct**:

    { ptr, i64, i64 }   =   { buffer, length, capacity }

with the following semantics:

| Field | Meaning |
|---|---|
| `buffer` | Pointer to the element storage. Either a stack alloca (when `capacity == 0`) or a heap buffer (when `capacity > 0`). |
| `length` | Number of live elements. Always `<= capacity` when `capacity > 0`, or `== initial_length` when `capacity == 0`. |
| `capacity` | `0` → the buffer is a stack alloca owned by the enclosing frame; must not be reallocated or freed. `> 0` → the buffer is heap-allocated; `capacity` is the number of element slots available. |

### Allocation discipline

A list is **stack-backed** when it is created from a literal and has
not been appended to. It is **heap-backed** once `.append` is called
on a `var` binding, and remains heap-backed for the rest of its
lifetime.

- **Literal construction.** `val xs := [a, b, c]` allocates
  `[N x T]` on the stack, sets `buffer` to that alloca, `length = N`,
  `capacity = 0`.
- **Move.** `var b := a` copies the struct verbatim. No allocation.
  Both bindings point at the same buffer for the instant before `a`
  is invalidated by the analyzer — after the move, only `b` is live.
- **Append on a stack-backed list.** First append converts to heap:
  `malloc` a buffer of size `max(2 * length, length + 1)`, `memcpy`
  the current elements, write the new element at index `length`,
  set `length = length + 1`, `capacity = new_capacity`. Cost is
  O(N) one time.
- **Append on a heap-backed list with spare capacity.** Write the
  element at `buffer[length]`, increment `length`. O(1).
- **Append on a heap-backed list at capacity.** `realloc` to
  `2 * capacity`, write, increment `length`. Amortized O(1).
- **Free.** A heap-backed list is freed at scope exit or region
  exit via `free(buffer)`. A stack-backed list is left alone — its
  alloca is reclaimed by the frame epilogue.

### Length

`length` is now the single source of truth at runtime. The
`list_lengths` codegen map becomes a **compile-time hint**, not a
contract:

- When `list_lengths[name]` is present and the variable has never
  been appended to, codegen may still emit the literal length (an
  `i64` constant) for `List.length`. This preserves the current
  performance for programs that never touch `.append`.
- When the variable has been appended to, or is a function
  parameter, codegen loads `length` from the struct field.

The `list_arrays` / `list_array_types` maps remain for the
stack-backed case. A new discriminant — a `list_heap` set, or a
per-variable flag — distinguishes the two paths at each use site.

### Function ABI

A `List<T>` parameter and return value is the three-word struct, by
value. This is a change from the current `ptr` ABI and requires:

- `declare_function` to declare list params/returns as the struct
  type rather than `ptr`.
- `compile_function`'s param-binding loop to receive the struct and
  store it in an alloca (or unpack the three fields into the
  bookkeeping maps, preserving the old representation for the
  function body — see the migration plan).
- Call sites to build the struct from `{buffer, length, capacity}`
  before the call.
- `List.length` in a callee to read `length` from the struct — the
  compile-time hint is not available across a call.

**Migration shortcut:** in the first implementation, keep the
function-level ABI as `ptr` for the buffer and pass `length` and
`capacity` as two extra hidden arguments. This is smaller than
restructuring the struct-passing ABI, gets the semantics right, and
can be replaced by struct-by-value in a follow-up.

### Iteration

Iteration over a stack-backed list continues to unroll (the length
is known). Iteration over a heap-backed list — the current
fail-closed case for parameters — reads `length` from the struct
and emits a rolled loop. `IteratorInit` therefore has two paths:

- Static length known and no `.append` on the variable → unroll.
- Length is dynamic (parameter, or appended variable) → rolled loop
  with a runtime bound.

The fail-closed branch added in `627a01e` ("cannot iterate list
parameter") is removed once the rolled-loop path exists.

## Consequences

**Positive.**

- `List.append` becomes lowerable on LLVM. The capability check
  stops refusing it.
- Lists can be iterated inside a function that received them as
  parameters — currently a fail-closed error.
- Filter / accumulate / collect patterns work on the compiled
  backend.
- The `{ptr, len, cap}` layout is the same shape Rust's `Vec` uses
  internally and the same shape most C++ containers use; the
  semantics are familiar and the realloc-growth policy is standard.

**Negative.**

- The list value is now 24 bytes instead of 8 (the pointer). Every
  `List<T>` alloca, struct field, and function argument carrying a
  list becomes larger. Programs with many list bindings pay a
  memory cost.
- `list_lengths`, `list_arrays`, `list_array_types`, and the new
  heap flag must be kept consistent at every list-producing site.
  This is exactly the class of bug A2 and its follow-ups have been
  fixing; adding two more tables increases the coordination surface.
- Two code paths exist for list access (stack-backed vs
  heap-backed), and every site that reads elements must branch on
  the discriminator.
- Function ABI change breaks any external assembly or FFI that
  passes raw `ptr` for a list. The FFI boundary already refuses
  lists (see `docs/soundness/ffi`), so this is internal-only.

**Neutral.**

- The interpreter's representation (`Vec<RuntimeValue>`) is
  unchanged. It has always had runtime length and heap allocation;
  the LLVM backend now catches up rather than diverging.
- `docs/features/list_llvm.md` becomes largely obsolete and should
  be rewritten after implementation.

## Alternatives considered

### A. Uniform heap representation (drop stack-backed lists)

Every list literal allocates on the heap. Simpler — one code path —
but loses the "no allocation for list literals" property and forces
`free` on every list binding, expanding the region machinery's
responsibilities.

**Rejected** because the common `val xs := [1, 2, 3]` pattern would
pay `malloc`/`free` for no benefit.

### B. Keep the current representation; implement `List.append` only
on the interpreter forever

This is the status quo under ADR 0028. It works, but means any
program building a variable-length list cannot use the compiled
backend.

**Rejected** as a permanent answer; it may be the right short-term
choice if this ADR is not implemented soon.

### C. Two representations with a type-level distinction

Introduce `StackList<T>` and `HeapList<T>` as separate types, one
fixed-size, one growable. The user picks.

**Rejected** because it leaks the representation into the type
system for no user-facing benefit. `List<T>` is one concept; the
codegen should handle the distinction internally.

### D. Length in a side table with runtime insertion

Keep `List<T>` as `ptr` at the LLVM level, but maintain a global
`HashMap<ptr, length>` at runtime.

**Rejected** as a runtime-side re-invention of a struct field with
worse performance and worse cache locality.

## Migration plan

Four sequential commits, each buildable and testable in isolation:

1. **Introduce the struct.** Add `Type::List(_)` → `{ptr, i64, i64}`
   in `map_type`. Update every site that currently reads the value
   as a bare `ptr` to read the struct and extract fields. No
   behavioral change — all lists still have `capacity == 0`.
   Run the full suite; expect fallout at every list access site.

2. **Runtime length.** Make `List.length`, iteration, and bounds
   checks load `length` from the struct when the compile-time hint
   is absent, and keep the hint path when it's present. Still no
   `append`.

3. **Iteration over dynamic lists.** Replace the fail-closed
   `IteratorInit` branch with a rolled-loop path. Remove the
   "cannot iterate list parameter" diagnostic.

4. **`List.append`.** Add the capability to LLVM. Lower `append` to
   the grow/conversion logic above. Add acceptance tests. Rewrite
   `docs/features/list_llvm.md`.

Each commit passes the full suite. Rollback is a revert; nothing is
pre-committed to the next step.

## Open questions

- **Function ABI: hidden args or struct by value?** Hidden args are
  easier to land first; struct by value is cleaner long-term. The
  migration plan implies hidden args in step 1 and a follow-up
  change to struct by value.
- **Does `region` cleanup need to know about heap-backed lists?**
  Yes: `RegionExit` currently frees stack allocas. It must also call
  `free(buffer)` when `capacity > 0`. The exact interaction with
  the region snapshot machinery (which already handles
  reallocation of a raw `alloc(n)` pointer) needs a follow-up ADR
  or an amendment to 0007.
- **Should `.append` on a `val` binding be a compile error?** ADR
  0028 says yes, mutating `val` requires `var`. This ADR does not
  change that; the analyzer's mutability check stands.
- **Should the initial capacity be `2 * length` or `length + 1`?**
  `2 * length` amortizes better across many appends but wastes
  memory on the common "one append then never again" case.
  `length + 1` is tighter for the common case; the doubling policy
  kicks in on the second append. Recommend `length + 1` for the
  conversion, `2 * capacity` for the realloc growth.
- **Does WASM need a parallel representation?** Probably yes, but
  WASM currently refuses lists entirely (`docs/decisions/0037-`).
  Defer to a future ADR.

## See also

- `docs/decisions/0028-list-append.md` — the language feature this
  unblocks
- `docs/decisions/0036-llvm-records.md` — the records-in-lists
  interplay
- `docs/features/list.md` — the language-level list contract
- `docs/features/list_llvm.md` — the current representation, to be
  superseded by this ADR after implementation
- `src/backends/llvm_codegen/{mod,types,instruction,value}.rs` —
  the sites this ADR touches
- `627a01e` — the current fail-closed `IteratorInit` branch this
  ADR proposes to replace

# Feature: LLVM List Representation

> Documentation of the LLVM backend's existing list model, frozen
> so records can depend on it. This file is not an ADR — the model
> was already in the code, undocumented. It is written down here
> because ADR 0036 (LLVM records) needs to know what shape a list
> takes in memory before deciding what shape a record field holds.

## Summary

The LLVM backend represents lists as **statically-sized LLVM arrays**
allocated on the stack. A list literal `[1, 2, 3]` compiles to:

```llvm
%arr = alloca [3 x i64]
; store 1, 2, 3 into arr[0..2]
```

The array's length is a **compile-time property tracked in the
codegen's Rust-side bookkeeping**, not stored in memory alongside
the array. `List.length` compiles to a literal integer constant.

This is deliberately different from the interpreter, which uses
Rust's `Vec` and stores the length at runtime. The two agree on
*observable behavior* for programs the analyzer accepts; they
disagree on the memory layout of a list value, which is invisible
to well-typed programs.

## The three bookkeeping tables

The codegen tracks every list variable with three parallel maps:

```rust
list_arrays:      HashMap<String, PointerValue>   // alloca'd array pointer
list_array_types: HashMap<String, BasicTypeEnum>  // LLVM array type
list_lengths:     HashMap<String, usize>          // compile-time length
```

All three are updated together in `Instruction::Declare` and
`Instruction::Assign` when the value is a `TypedIRValue::List`.
Missing the third (or forgetting to update it after a grow) is the
class of bug that shows up as `List.length` returning a stale
value; missing the second is what caused the `IteratorInit`
regression ADR 0021 fixed.

## What this buys and costs

**Buys:**

- No runtime allocation for list literals. The array is a stack
  alloca; freeing is a `pop`.
- `List.length` is free — a literal `i64`.
- `for x in list` can unroll with a compile-time-known bound.

**Costs:**

- No dynamic growth. `List.append` cannot be lowered — the array's
  size is fixed at `alloca` time. (Today `List.append` is
  interpreter-only.)
- A list value passed by value to a function has no length at the
  call site; the callee must know it from the type or from a
  separate argument.
- Anonymous list values (a `TypedIRValue::List` not bound to a
  name) are refused by `value.rs:77` — there is nowhere to store
  the bookkeeping.

## What is refused and where

| Position | Status | Evidence |
|---|---|---|
| `val xs := [1, 2, 3]` | Supported | `instruction.rs:22` |
| `xs.length` | Supported | `builtins.rs:216` |
| `for x in xs` | Supported | `IteratorInit` reads `list_arrays` + `list_lengths` |
| `f([1, 2, 3])` | Refused | `value.rs:77` |
| `xs.append(4)` | Refused | no LLVM codegen; interpreter-only |
| `return [1, 2, 3]` | Refused | `value.rs:77` |
| `xs` as a function parameter | Supported | `map_type(Type::List)` gives the type; the callee sees a pointer |
| `xs` as a function return value | Refused (indirectly) | return value would be a `TypedIRValue::List`, hit `value.rs:77` |

## The `types.rs` mismatch

`src/backends/llvm_codegen/types.rs:63` maps `Type::List(inner)` to
an LLVM struct `{ elem, i64 }` — a pointer-plus-length shape.
Nothing in the codegen uses this mapping for list values. The
comment in that file already says so:

> NOTE (Tier 2 follow-up): the current runtime representation of a
> list in `instruction.rs` is a fixed-size LLVM array `[N x elem]`,
> but this mapping returns `{elem, i64}`. The two disagree. Callers
> that actually allocate list storage use the array form directly,
> so this mapping is not load-bearing for correct programs.

This file freezes the array form as **the** representation. The
`{elem, i64}` mapping is a stub for a future dynamic-list feature;
until that feature exists, the mapping should not be trusted.

### What to do about the stub

Two options for `map_type(Type::List(_))`:

- **(a) Leave it as-is.** The comment stays; the function keeps
  returning `{elem, i64}`; callers that need the actual shape use
  the array form. A latent miscompile waits for a caller that
  trusts the map.
- **(b) Make it `unreachable!()`.** Same shape as `Type::Record`
  and `TypedIRValue::Record` — refuse loudly rather than answer
  wrong. Safer, but breaks any caller currently reaching the map
  for a legitimate reason.

**Recommendation: (b), as a small follow-up commit.** Find every
call site of `map_type` where the type could be `Type::List`,
verify it doesn't need the map, then change the arm to panic with
a message pointing at this doc.

## What this means for ADR 0036 (records)

**Records with list-typed fields are out of scope for ADR 0036
v1.** Two independent blockers:

1. A record literal `Wrap { items: [1, 2, 3] }` needs
   `TypedIRValue::List` to lower to a value the field slot can
   hold. It doesn't.
2. Even if it did, the field's LLVM type would come from
   `map_type(Type::List(_))` — currently a stub that returns the
   wrong shape.

ADR 0036's §Scope boundary is amended to say: records in v1 may
contain primitive fields, `String` fields, and nested record
fields. Lists, maps, and channels as record fields are deferred
until a dynamic list representation exists.

This is not a shortcoming of ADR 0036 — it's the correct scoping,
given what LLVM currently supports. The interpreter supports list
fields already; a record with list fields will run there.

## Migration

**None.** No existing program changes representation. The model
documented here is what the code already does; the doc makes it
explicit and cites the code that implements it.

The only follow-up is the `map_type` hardening (option (b) above),
which is a separate commit and does not change program behavior
for any program the analyzer currently accepts.

## See also

- `src/backends/llvm_codegen/instruction.rs:22-105` — the list
  allocation and assignment paths.
- `src/backends/llvm_codegen/builtins.rs:216-245` — `List.length`.
- `src/backends/llvm_codegen/types.rs:63-77` — the type-map stub.
- `src/backends/llvm_codegen/value.rs:74-82` — where the literal
  is refused.
- `docs/decisions/0036-llvm-records.md` — the ADR this doc
  unblocks, with an amended scope boundary.
- `docs/decisions/0021-list-iterator-type-registration.md` — the
  `list_array_types` bookkeeping that keeps `for` loops correct.
- `docs/features/list.md` — the language-level list contract
  (interpreter-independent).
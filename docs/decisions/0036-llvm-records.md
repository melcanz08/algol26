# ADR 0036 — LLVM Record Support

## Status

Proposed. Requires investigation before implementation.

> **Status note (2026-10-06).** The methods feature (ADR 0033)
> shipped on the interpreter only. LLVM refuses records at the
> capability boundary. This ADR proposes closing that gap by
> lowering the three record-related IR shapes to LLVM struct
> operations. The first implementation step is investigation —
> the LLVM backend's existing idioms for list and string values
> need to be read before the layout and passing decisions are
> finalized.

## Context

Records have been in the language since ADR 0024. They work
end-to-end on the interpreter: literals, field access, field
assignment, methods (ADR 0033), write-through on `&mut self`
(ADR 0035), and complex-receiver method calls. None of it runs
through LLVM.

The capability scan in `src/backends/capabilities/scan.rs`
refuses any program mentioning `Type::Record` when the target
backend is LLVM. The refusal is correct — the LLVM backend has
no codegen for the three IR shapes records produce:

- `TypedIRValue::Record { name, fields, record_type }` — literal
  construction.
- `TypedIRValue::FieldAccess { object, field, field_type }` —
  field read.
- `Instruction::FieldAssign { target, field, value }` — field
  write.

The rest of the LLVM backend works: scalars, strings, lists,
references, control flow, function calls, builtins. Records are
the largest remaining gap.

Parity today:

| Backend | Records | References | Methods |
|---|---|---|---|
| Interpreter | Yes | Read-only, pass-through | All three modes |
| LLVM | No | Yes | Blocked on records |
| WASM | No | No | Blocked on both |

Methods with any receiver mode currently cannot be compiled to
native code, because every method takes a record or a reference
to one. Closing this gap makes the entire methods feature
usable on the fast backend.

## Decision

Records lower to **LLVM named struct types**. Record values are
represented as **pointers to memory** (allocas or region
allocations), not as SSA struct values. Field operations are
**GEP + load/store**.

### Layout

A record `rec User { name: String, age: Int }` lowers to an
LLVM type:

```llvm
%User = type { %String, i64 }
```

Fields appear in declaration order. Field offsets are determined
by LLVM's standard struct layout (padding to natural alignment).
No packed layout, no reordering — the field order in the source
is the field order in memory.

Nested records inline their fields' struct types recursively.
Field types that are already pointers (`String`, `List<T>`,
`Map<K,V>`) are stored as their existing pointer
representations.

### Value representation

A record *value* in the IR (`TypedIRValue::Record`) becomes an
alloca of the struct type plus a store into each field slot. The
expression's result is a pointer to that alloca.

This mirrors how the existing backend handles other
compound values. Investigation step: confirm this matches the
convention used for `String` and `List<T>` — if those use a
different shape (e.g., a struct of {ptr, len}), records should
follow suit rather than invent a parallel convention.

### Field access

`u.name` lowers to:

```llvm
%field_ptr = getelementptr inbounds %User, %User* %u_ptr, i32 0, i32 0
%value = load %String, %String* %field_ptr
```

The GEP computes the address of the field within the struct; the
load retrieves the value. Field indices come from the record
declaration's field order.

### Field assignment

`u.name := x` lowers to a GEP to the field address followed by a
store:

```llvm
%field_ptr = getelementptr inbounds %User, %User* %u_ptr, i32 0, i32 0
store %String %x, %String* %field_ptr
```

No new IR is needed. The builder already emits
`Instruction::FieldAssign`; this is purely a codegen addition.

### Function parameters

A function taking `u: User` by value passes a pointer to an
alloca in the caller. The callee receives the pointer and
reads/writes through it. Copy-in semantics: the callee's
parameter is a *fresh* alloca the callee owns, populated from
the caller's alloca at call time.

A function taking `u: &User` or `u: &mut User` passes the
caller's existing pointer directly, matching LLVM's reference
lowering (already implemented).

### Return values

A function returning a record uses LLVM's standard sret
convention: the caller allocates space, passes a hidden pointer
as the first argument, and the callee writes the return value
there. This is what clang does for large C structs and what
LLVM's ABI lowering expects.

Alternatively, LLVM's `insertvalue`/`extractvalue` could be used
to build small structs in SSA form. Deferred — see
§Alternatives considered.

### Copy semantics are compile-time only

The language treats records as `Copy` iff every field is `Copy`
(ADR 0024, structural Copy). At the LLVM level this matters only
at *assignment* — `val u2 := u1` copies the struct bytes into a
new alloca. Moves and borrows are compile-time concepts; the
codegen does not distinguish them. A moved-out record's alloca
is simply unused afterward.

### Capability matrix update

`BackendCapabilities::llvm()` gains `Feature::Records` in its
supported set. The capability scan already detects `Type::Record`
in signature and value positions; once LLVM declares support,
the scan passes and the codegen arms fire.

## Scope boundary

In scope for v1:

- Non-generic records.
- Records with fields of primitive type (`Int`, `Float`, `Bool`),
  `String`, and other records.
- Record literals, field access, field assignment, methods with
  all three receiver modes, records as function parameters and
  return values.

**Amended (2026-10-06, pre-implementation).** Fields of `List<T>`,
`Map<K,V>`, or `Channel<T>` type are **not** in scope for v1. Two
blockers, documented in `docs/features/list_llvm.md`:

1. `TypedIRValue::List` has no LLVM lowering — a record literal
   with a list-typed field would need one to construct the field
   value.
2. `map_type(Type::List(_))` returns a shape that disagrees with the
   runtime representation. A record field of list type would inherit
   the disagreement.

Deferred to a follow-up ADR once LLVM has a dynamic list
representation. The interpreter supports list-typed record fields
today; the LLVM backend does not.

Not in scope for v1 (each needs its own ADR or follow-up):

- Generic records (`rec Pair<T>`). The IR side already exists;
  LLVM-side monomorphization would follow the pattern for
  generic functions (ADR 0013, ADR 0034).
- Records crossing the FFI boundary. Already excluded by the
  FFI capability scan; the reasoning (ABI stability, layout
  guarantees) applies independently.
- Pattern-matching records in `match` arms. The IR supports
  `SemanticPattern::Record`; LLVM-side lowering is a separate
  codegen problem.
- Records in `Set<T>` or as `Map` keys. `Set` currently requires
  a subrange or enum element type; extending it is out of
  scope.

## Implementation order

Investigation first. The layout and passing decisions above
assume the LLVM backend's existing idioms. Confirm those before
writing code.

```
L0. Investigation. Read src/backends/llvm_codegen/value.rs,
    types.rs, and builtins.rs. Confirm: how are String and
    List<T> represented? Alloca+pointer, or SSA aggregate?
    What does the codebase call a 'value pointer'? Document
    in the ADR's review record; revise layout/passing decisions
    if the existing convention differs from the proposal above.

L1. Type lowering. Extend the LLVM type map to produce named
    struct types for Type::Record. Add a name-mangling rule
    for record type names (`%User`, `%User_Int` for generic
    instantiations, ...).

L2. Record literal codegen. TypedIRValue::Record → alloca +
    store each field. Return the alloca pointer.

L3. Field access codegen. TypedIRValue::FieldAccess → GEP +
    load.

L4. Field assignment codegen. Instruction::FieldAssign → GEP +
    store.

L5. Function parameters and returns. sret for return values,
    alloca+copy for by-value parameters.

L6. Capability matrix. Add Feature::Records to LLVM's set.

L7. Fixtures. Enable all record fixtures on the LLVM backend.
    Add differential tests: same program on interpreter and
    LLVM produces identical output. This is the load-bearing
    test — the whole point of record support is that both
    backends agree.
```

Each step compiles and is revertable. L0 through L2 are
self-contained; L3 and L4 are small; L5 is the largest; L6–L7
are mechanical.

## Consequences

### Positive

- Methods work on the fast backend. All three receiver modes,
  all type owners, method dispatch — the entire feature
  becomes available to LLVM-compiled programs.
- The differential test suite can start running record
  programs. Backend agreement is the strongest possible
  correctness signal for a compiler.
- The record layout becomes a stable ABI concern for the
  language. Programs compiled by different compiler versions
  will agree on field offsets if the layout rule is frozen
  here.

### Negative

- Every record operation goes through memory: alloca, store,
  load, GEP. The SSA-aggregate alternative (insertvalue /
  extractvalue) avoids this for small records but loses for
  large ones. See §Alternatives.
- The layout rule freezes a design decision that has
  consequences for FFI compatibility, if records are ever
  allowed across the FFI boundary. A future C-ABI-compatible
  layout would need `#[repr(C)]`-style opt-in.
- Record type names must be mangled for LLVM. Two same-named
  records in different modules need distinct LLVM names; this
  is the same identity question ADR 0033 flagged for method
  mangling (see §See also).

### Neutral

- The IR is unchanged. Records already exist in `TypedIRValue`
  and `Instruction`; the codegen additions consume existing
  shapes.
- The verifier is unchanged. It already checks `Type::Record`
  consistency at the semantic level.

## Alternatives considered

### SSA aggregate representation (insertvalue / extractvalue)

LLVM's `insertvalue` and `extractvalue` build and read structs
in SSA form without going through memory. For small records
(few fields, all primitive), this can be faster than allocas
because LLVM's SROA pass can optimize field access entirely.

**Rejected for v1.** The language's ownership model means
records are passed by reference more often than by value, and
mutating through a reference is much cheaper with a pointer than
with SSA values that need to be threaded through the CFG.
Record values that stay in registers are the exception, not the
rule. A future optimization pass could switch to SSA form for
records that never escape, but the base representation should
be the one that matches the common case.

### Return by value (LLVM struct type) instead of sret

LLVM allows returning a struct by value directly. This works
for small records and avoids the sret hidden pointer.

**Rejected.** sret is the convention LLVM's ABI lowering uses
for anything larger than a register pair, and it's what clang
emits for C struct returns. Consistency with the C ABI matters
if records ever cross the FFI boundary. The performance
difference is negligible for realistic record sizes.

### Packed layout

LLVM structs can be `packed`, which removes padding. Field
offsets become exact sums of preceding field sizes.

**Rejected.** Packed layout hurts alignment-sensitive
operations (unaligned loads on some platforms are slower;
some require explicit unaligned load intrinsics). The memory
savings are not worth the complexity. Standard layout is what
every systems language uses by default.

### Reuse the interpreter's `RuntimeValue::Record` model

The interpreter represents records as `Record { name, fields:
Vec<(String, RuntimeValue)> }` — a name-tagged list of fields.
One could imagine the LLVM backend building the equivalent
dynamically.

**Rejected.** That model is right for a tree-walking
interpreter with no fixed layout, and wrong for compiled code
with static field offsets. The IR's `TypedIRValue::Record`
already carries a `record_type`; the backend should use it to
compute layout at compile time.

## Review record

No outside review yet. This ADR is the first pass. The L0
investigation step is expected to surface corrections to the
layout and passing decisions; those should be recorded here as
amendments before L1 begins.

## References

- ADR 0024 (Records) — the source-level feature.
- ADR 0005 (Ownership Model) — Copy, move, and borrow at the
  semantic level; the codegen must not distinguish them.
- ADR 0010 (Canonical IR) — why records are not a special IR
  shape.
- ADR 0013 (Executable IR Generic Invariant) — the
  specialization machinery generic records would extend.
- ADR 0033 (Methods) — the feature blocked on this ADR.
- ADR 0034 (Generic Impls) — the source-level extension that
  generic record support would parallel.
- ADR 0037 (WASM Records and References) — the sibling problem
  for the other unfinished backend.
- `src/backends/llvm_codegen/` — the codegen module this ADR
  extends.
- `src/backends/capabilities/scan.rs` — the capability check
  that refuses records today.

## See also

- `docs/features/methods.md` — the feature contract; records
  support is a prerequisite for methods on LLVM.
- `docs/features/record.md` — the record feature contract.
- `docs/decisions/README.md` — the ADR index and convention.
# ADR 0019 — References

## Status

Accepted. Implemented. This ADR was backfilled on 2026-11-02 to
record a decision that was made in the code long before the record
existed; STATUS.md referenced "ADR 0019" for years before this file
was written (see `LANGUAGE_GAPS.md` G-drift note below).

## Context

ALGOL26 has an ownership model (`ADR 0005`) and a region memory
model (`ADR 0007`), but until this decision there was no
first-class *reference* type — the way to hold an alias without
transferring ownership, and the way to write through a mutable
alias.

Several features already leaned on an implicit reference concept:
the interpreter's `ReadReference` instruction, LLVM's `AddrOf`,
and the whole `&x` / `&mut x` surface. The decision was to make
that surface concrete, with a documented capability matrix.

## Decision

**Four reference forms** are part of the language:

    &x          — shared borrow: read-only alias to x
    &mut x      — mutable borrow: read-write alias to x
    *r          — dereference: read through a reference
    AddrOf(x)   — address-of: take a raw reference from a place

**Capability matrix:**

- **Interpreter:** refused at the capability check via
  `Feature::References`. The interpreter is a tree-walker over
  `TypedIRValue` values; without a real address space, references
  don't have a meaningful runtime representation. A program that
  uses `&x` / `&mut x` / `*r` / `AddrOf` is refused *before*
  execution, not silently misbehaved.
- **LLVM:** supported. `&x` lowers to a pointer to the stack slot
  (or heap allocation) that x names. `&mut x` lowers to the same
  but carries a mutable flag through the analyzer. `*r` is a load
  through the pointer. `AddrOf` is a pointer-to-pointer when the
  operand is itself a reference.
- **WASM:** supported via the same lowering as LLVM.

**Analyzer enforcement.** The borrow checker (`ADR 0043`,
`ADR 0044`) enforces that `&mut x` has at most one live alias
and that `&x` and `&mut x` don't coexist. `*r` is a read of the
borrow's referent and is tracked as a "use" of the borrow.

**References are not raw pointers.** A raw pointer (`*Int`,
`Pointer<Int>`) carries no lifetime or region obligation and is
legal only in `unsafe` blocks (see `ADR 0009`). References carry
both and are legal in safe code.

## Consequences

### Positive

- Programs that manipulate state indirectly (linked lists,
  in-place algorithms, struct field updates via a helper) have a
  safe, checked way to do so.
- The capability boundary is honest: interpreter-only features
  refuse cleanly rather than crash.
- The borrow checker (`ADR 0043`, `ADR 0044`) has a concrete
  reference type to reason about.

### Negative

- The interpreter can never run reference-using programs. That's
  a real limitation; users have to build (`build` or `wasm`)
  rather than `--interpreter run` to exercise them.
- Two lowering paths for references (LLVM, WASM) must stay in
  sync.

### Not yet supported

- Reference-to-reference (`&&T`). The analyzer permits one level
  of reference; nested borrows are ADR 0043 follow-up work.
- Reference-coercion chains (`&mut T -> &T`). Explicit
  re-borrows are needed. Documented in `ADR 0043` §Scope.
- Reference containment escape (`&T` stored in a record and
  outliving its referent). ADR 0046 documents the escape
  analysis gap.

## References

- `ADR 0004` — memory model
- `ADR 0005` — ownership model
- `ADR 0009` — unsafe enforcement (raw pointer distinction)
- `ADR 0043` — non-lexical borrow lifetimes
- `ADR 0044` — place-based borrows
- `ADR 0046` — reference containment escape

## Backfill note

This ADR was written retroactively. The decision was made and
implemented before ADR 0019 existed on disk; STATUS.md referenced
it as though it did. The gap was found during the feature
inventory reconciliation (2026-11-02). The ADR records the
decision as it stands, not as it was arrived at.

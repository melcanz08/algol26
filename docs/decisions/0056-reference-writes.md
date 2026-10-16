# ADR 0056: Reference Writes

Status: Proposed (design)

## Context

The language has a complete reference *read* surface (ADR 0053):
`*r` reads the referent of `r`, `r.f` reads field `f` through a
reference (ADR 0012), and `r[i]` reads the `i`-th element through a
reference. The interpreter's `resolve_place` implements this by
walking the `Place { base, projections }` (ADR 0044) against the
runtime `variables` map.

There is no corresponding *write* surface. A procedure that mutates
its argument cannot be written:

    proc bump(r: &mut Int)
        *r := *r + 1

Today `*r := *r + 1` is a parse error; `parse_stmt`'s `Token::Star`
arm was added in step 2 (commit `<fill in>`) but the analyzer
rejects `Stmt::DerefAssign` with a `todo!()`.

The absence forces one of two workarounds:

  1. Return the new value and let the caller rebind:
         fn bump(r: &Int) -> Int
             *r + 1
         # caller: x := bump(&x)
     This works but is unidiomatic and fails for procedures that
     mutate more than one argument.

  2. Use `unsafe` and raw pointers. This bypasses the borrow
     checker entirely and is the wrong default.

## Decision

Introduce `*place := value` as a statement form. The LHS is a
*place expression* of reference type; the RHS is a value of the
referent's type. The write updates the referent in place.

### Grammar

    deref_assign := "*" expr ":=" expr

`expr` on the left must resolve to a place of type `&mut T`
(or a place whose outermost projection is a deref of `&mut T`;
see "Compound places" below). The RHS is type-checked with
expected type `T`.

### Semantics

For a plain `*r := v` where `r: &mut T`:

1. `r` must be a place expression of type `&mut T`. Writing
   through `&T` (shared) is a compile error (`E-BORROW-00X`,
   code to be assigned).
2. `v` is type-checked against `T` and moved into the referent.
   The value is no longer usable at the assignment site.
3. The old value in `*r` is dropped per ADR 0050/0051's region
   cleanup, *before* the store. See "Drop-before-store" below.
4. Borrow-check (ADR 0044): the write is a *use* of the loan
   that produced `r`'s referent place. Any other loan that
   overlaps that place and is live at the write site is a
   conflict.

### Compound places

The LHS is not restricted to a bare `*r`. Two compound forms
are admitted, and they mean *different* things:

  **Form A** — `*r.f := v` where `r: &mut Record`:
      Write `v` to field `f` of the *referent* of `r`.
      The place being written is `r`'s referent place, projected
      with `Field("f")`.

  **Form B** — `r.f := v` where `r.f: &mut T`:
      Write `v` to the *referent* of `r.f`. Requires `Record` to
      have a field `f` whose type is `&mut T`.

The parser produces the same AST for both — a `Deref` under or
over a `FieldAccess` — and the analyzer disambiguates by the
type of the innermost `object`. The rule is:

  > Resolve the target place left-to-right. The first sub-place
  > whose type is `&mut T` (or `&T`) is dereferenced; subsequent
  > projections apply to the referent.

Under this rule:

  - `*r.f` where `r: &mut Record`, `Record.f: Int`  → Form A:
      place is `{ base: r, projections: [Deref, Field("f")] }`.
  - `r.f` where `Record.f: &mut Int`               → Form B:
      place is `{ base: r, projections: [Field("f"), Deref] }`.

The two are distinguished by *where* the `Deref` sits in the
projection list, which is exactly what a type-driven analysis
computes. This is the same convention Rust uses and the same one
ADR 0012 already assumes for reads.

### Place representation

`Place` (ADR 0044) gains one variant:

    enum Projection {
        Field(String),
        Index(usize),
        Deref,        // ADR 0056
    }

`Deref` is appended when the analyzer dereferences a reference
during place resolution. Two places overlap per ADR 0044's
prefix rule; `Deref` participates in the prefix comparison like
any other projection:

  - `*r` (place `{ r, [Deref] }`) and `r` (`{ r, [] }`) overlap:
    yes, `[]` is a prefix of `[Deref]`.
  - `*r.x` (`{ r, [Deref, Field(x)] }`) and `*r.y`
    (`{ r, [Deref, Field(y)] }`): no, neither prefix matches.
  - `*r` (`{ r, [Deref] }`) and `*r.x`
    (`{ r, [Deref, Field(x)] }`): yes, `[Deref]` is a prefix.

This gives the borrow checker the right granularity: writing
through `*r` conflicts with reading `*r.x`, but not with a read
of `r` alone (the reference itself is still live).

### Drop-before-store

`*r := v` where `*r` currently holds a value of a type with
heap storage (list, string, map, set, region) must drop the old
value *before* the store, or the old allocation leaks.

The drop is emitted by the region cleanup pass (ADR 0050/0051)
at the point of the store. This requires the IR to carry the
old value's type at the store site — the analyzer records it
during type-checking.

**Open question**: whether the drop is emitted *by* the region
cleanup pass or by the IR builder as part of `DerefAssign`
lowering. Deferred; see "Consequences".

## Non-goals

- **Write through `&T`.** Shared references are read-only;
  `*r := v` where `r: &T` is a compile error.
- **Write through a raw pointer outside `unsafe`.** ADR 0009
  governs; `*ptr := v` where `ptr: Pointer<T>` is permitted
  only inside an `unsafe` block.
- **Partial-store leaving a record half-updated.** `r.f := v`
  writes the whole field; there is no way to write only part of
  a scalar. Structural updates are whole-value.
- **Reborrow on write.** `*r := v` stores `v`; it does not
  implicitly reborrow or coerce.

## Alternatives considered

### A. Return-value-only (status quo)

Force `fn bump(&Int) -> Int` and caller-side rebinding. Rejected:
does not scale to procedures with multiple `&mut` parameters,
and diverges from every systems language the syntax is
borrowing from.

### B. General `Stmt::AssignTo { place, value }`

Replace `Stmt::Assign`, `Stmt::FieldAssign`, `Stmt::ArrayAssign`,
and `Stmt::DerefAssign` with a single lvalue-based assignment
statement. This is the correct long-term shape — it removes
three variants and unifies place resolution — but it is a
mechanical migration of the analyzer, IR builder, and codegen
that should not ride along with a new language feature.
Rejected *for now*; revisit as its own ADR once 0056 lands.

### C. Make `*r := v` an expression

Give it a value of type `Void` and allow it in expression
position (`x := (*r := 5)`). Rust does this. Rejected: the
language already separates `Stmt::Assign` from expressions and
there is no reason to make writes expression-y.

## Consequences

### Positive

- Procedures can mutate their arguments in place, the standard
  idiom. `proc bump(&mut Int)` becomes writable.
- Closes the last gap in the reference surface: read (0053),
  borrow (0005/0044), write (0056).
- The `Place` extension is minimal (`Projection::Deref`) and
  composes with the existing overlap rule.

### Negative

- **Drop-before-store is the highest-risk piece.** A wrong
  ordering leaks the old value; a double-drop corrupts memory.
  The adversarial tests must cover list and string replacement
  specifically.
- **Analyzer complexity grows.** The type-driven place
  resolution for compound targets (Form A vs Form B) has to be
  written carefully and tested against the type-checker's
  expectations.
- **Parser ambiguity is resolved only by the analyzer.** A
  syntactically valid `*r.f := v` may fail type-checking with a
  confusing error if `r` is the wrong type. Diagnostics should
  point at the innermost sub-place that failed.

### Neutral

- No new IR instruction if `DerefAssign` lowers to the existing
  `Store` with a `Deref`-projected place. If `Store` cannot
  express deref, a new instruction is added.
- No new LLVM/WASM intrinsic; `store` through a pointer is
  already available.

## Implementation sketch

Step 2 (parser + AST + display) is done. Remaining:

1. **`Projection::Deref` in `Place`.** Add the variant in
   `src/semantics/state/mod.rs`. Update `Place::overlaps`
   (already prefix-based — should just work). Update `Display`.

2. **Analyzer arm in `analyzer/stmt.rs`.** Replace the `todo!()`
   with a real arm that:
   - resolves the target's place via `place_of_expr` extended to
     handle deref-of-place,
   - resolves the first `&mut T` sub-place and inserts `Deref`,
   - type-checks `value` against `T`,
   - checks the RHS is not a shared borrow of `T`'s pointee.

3. **Borrow-check side.** The write is a use of the loan on the
   referent place. Update `analyzer/ownership.rs` if the write
   path needs to consult loans at all (it may not — the
   `SemanticState::is_mutably_borrowed` query already covers
   it, provided `Place::overlaps` handles `Deref`).

4. **IR lowering.** `Stmt::DerefAssign` → `Store { place, value }`.
   Verify `Store`'s `place` field accepts a `Deref` projection.

5. **Interpreter.** Add `assign_place(&mut self, place, value)`
   in `src/backends/interpreter/eval.rs`, sibling to
   `resolve_place`. Recursive helper taking `&mut RuntimeValue`.
   Add an instruction arm for `Store`.

6. **LLVM/WASM codegen.** `store` through a pointer. Both
   backends already have load; add store.

7. **Tests.** `tests/adr_examples/0056_reference_writes/`:
   - `pass_write_through_mut_ref.gol`
   - `pass_write_field_through_mut_ref.gol`  (Form A)
   - `pass_write_through_ref_field.gol`      (Form B)
   - `fail_write_through_shared_ref.gol`
   - `fail_write_while_shared_loan_live.gol`
   - `fail_write_through_temporary_ref.gol`
   - `pass_write_replaces_list_drops_old.gol`  ← critical
   - `fail_write_while_other_mut_loan_live.gol`

## See also

- ADR 0005 (ownership model)
- ADR 0009 (unsafe)
- ADR 0012 (implicit deref convention)
- ADR 0043 (NLL) — depends on this for the "write is a use" rule
- ADR 0044 (place-based borrows)
- ADR 0048 (procedure/function split)
- ADR 0050, ADR 0051 (region cleanup)
- ADR 0053 (interpreter reference semantics)
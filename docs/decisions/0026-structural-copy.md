# ADR 0026: Structural `Copy` for records

Status: Accepted (design); implementation pending

## Context

ADR 0024 introduced `rec` records. It deferred one design point:

> **Value semantics, structural `Copy` (deferred).** A record is
> `Copy` iff every field is `Copy`. `Point { x: Int, y: Int }` should
> be `Copy`; `Person { name: String, age: Int }` should not. The v1
> interpreter ships with records treated as move-only — `is_copy`
> does not yet consult the record field table. A follow-up wires the
> analyzer's record table into the ownership decision.

This is that follow-up.

Today, every record moves. `val p := Point { x: 1, y: 2 }` followed
by `val q := p` transitions `p` to `Moved`, and any later `p.x`
produces `E-MOVE-001` — even though `Point` holds only `Int` fields
and a copy would be trivially safe. Programs that want a "list of
structured things" fall back to parallel `List<Int>` / `List<String>`
accumulators, exactly the workaround ADR 0024 set out to eliminate.

The missing piece is small: the analyzer already has the record
declaration table (`SemanticAnalyzer::records`). The ownership code
just does not consult it.

## Decision

A record type is `Copy` iff every field's type is `Copy`.

"`Copy`" is the existing predicate on `Type`:

    Type::Int | Type::Float | Type::Bool | Type::Ptr  →  Copy
    everything else                                    →  not Copy

The rule is structural and recursive:

- `rec Point { x: Int, y: Int }` — `Point` is `Copy`.
- `rec Person { name: String, age: Int }` — `Person` is not `Copy`,
  because `String` is not `Copy`.
- `rec Line { start: Point, end: Point }` — `Line` is `Copy` iff
  `Point` is, which it is.
- `rec Pair<T> { first: T, second: T }` — `Pair<Int>` is `Copy`;
  `Pair<String>` is not. The check substitutes the concrete type
  arguments before evaluating the fields.

`Type::Unknown` remains not `Copy`. If the analyzer cannot determine
a type, it treats it as move-only. That is the conservative choice:
marking a value `Copy` when it might not be would silently duplicate
ownership.

There is no new syntax. No new keyword. No opt-in attribute. The
rule is a property of the type, not a declaration the programmer
writes.

## Semantics

**Move semantics are unchanged for non-`Copy` records.** `Person`
still moves on assignment, on argument passing, and on return.
`E-MOVE-001` still fires on use-after-move of a `Person`.

**`Copy` records do not move.** `val q := p` where `p: Point` leaves
`p` valid. The same for argument passing and return. `p.x` after the
copy is legal.

**Mutability is orthogonal.** A `Copy` record bound with `val` is
still immutable: `val p := Point { x: 1, y: 2 }; p.x := 5` produces
the existing `E0007` immutability diagnostic. `Copy` affects *move*
semantics, not assignment-through-the-binding semantics.

**Borrowing is unchanged.** `&p` and `&mut p` still work on records
of any `Copy`-ness. `Copy` records can be borrowed; the borrow rules
do not distinguish them. `Copy` decides whether a value is duplicated
on move, not whether it can be borrowed.

**Pattern matching is unchanged.** `case Point { x, y }` binds `x`
and `y` regardless of `Copy`-ness. A `Copy` record's bindings are
copies; a non-`Copy` record's bindings are moves.

## Where the change lives

The analyzer's ownership code consults `Type::is_copy()` at a small
number of sites. Every one needs to route through a new method on
`SemanticAnalyzer`:

    fn is_type_copy(&self, ty: &Type) -> bool

which consults the record table for `Type::Record` and delegates to
`Type::is_copy()` otherwise.

    fn is_type_copy(&self, ty: &Type) -> bool {
        match ty {
            Type::Record(name, args) => {
                let Some(info) = self.records.get(name) else {
                    // Unknown record — conservatively move-only.
                    return false;
                };
                if info.type_params.len() != args.len() {
                    return false;
                }
                let subst: HashMap<String, Type> = info
                    .type_params
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();
                info.fields.iter().all(|(_, field_ty)| {
                    self.is_type_copy(&field_ty.substitute(&subst))
                })
            }
            _ => ty.is_copy(),
        }
    }

The call sites that need to change:

- `src/semantics/analyzer/stmt.rs` — `Stmt::VarDecl`. The existing
  check `!value_type.is_copy()` before `mark_moved(source)` becomes
  `!self.is_type_copy(&value_type)`.
- `src/semantics/analyzer/expr.rs` — any ownership decision that
  branches on `is_copy`. Grep for `.is_copy()` in the analyzer.
- `src/semantics/state/mod.rs` — if `SemanticState` has its own
  `is_copy` consultation, route it through the analyzer. If it
  does not, no change is needed; the state module tracks the
  outcome, not the predicate.

Grep for the exact set:

    grep -rn '\.is_copy()' src/semantics/ src/ir/ src/compiler/

Every hit inside the analyzer that operates on a value's type is a
candidate. Hits inside `Type` itself (the method definition) and
inside the IR verifier are not — the verifier's type check does
not track ownership.

**No IR changes.** The IR has no copy/move distinction. `Declare`
and `Assign` are the same instructions whether the value is `Copy`
or not. The IR builder does not need to know.

**No interpreter changes.** `RuntimeValue` is Rust-owned and cloned
on insertion into `self.variables`, so records are already
value-semantic at runtime. The analyzer was the only thing enforcing
move-only; once it stops, the interpreter does the right thing.

**No capability changes.** A `Copy` record is still a record. It
still fires `Feature::Records`. LLVM and WASM still refuse it. The
`Copy` property does not change the capability contract.

## Cycles are impossible

A record cannot contain itself, directly or through a chain of
other records:

- The parser registers records in declaration order. A field type
  that names an undeclared record fails at analysis with the
  existing `Unknown record` diagnostic.
- ALGOL26 has no forward references, no `Option<Self>`-wrapping
  convention, and no heap indirection to break the recursion.

So `is_type_copy` does not need cycle detection. The recursion
depth is bounded by the nesting depth of the record declarations,
which is small in practice.

If a future feature (recursive types, sum types with recursive
variants) introduces the possibility of a cycle, `is_type_copy`
will need a visited-set. Note it here so the future author sees
the assumption.

## What is deliberately not in v1

- **User-defined `Copy` / `Drop` traits.** The rule is structural.
  A programmer cannot opt a non-`Copy` record into `Copy`, nor opt
  a `Copy` record out. That matches the ADR 0024 decision not to
  introduce `Drop` for records alone.
- **Field-level move.** Reading `p.name` where `name: String` does
  not currently move the `String` out of `p`. This is a pre-existing
  question — the same question applies to `list[0]` on a
  `List<String>`, and to `tuple.0` if tuples were first-class. It is
  out of scope for this ADR and unchanged by it.
- **`Copy` across region boundaries.** A record inside a `region`
  belongs to that region via the existing attribution. Making the
  record `Copy` does not change when its region's allocations are
  freed. Non-`Copy` records containing pointers into a region remain
  subject to the existing region-escape rules.
- **LLVM lowering of `Copy` records.** Still deferred, still
  refused by the capability gate. The LLVM follow-up ADR (see
  ADR 0024) will decide how structural `Copy` interacts with
  `memcpy`.
- **Caching the `Copy` predicate.** `is_type_copy` recomputes on
  every call. Records are shallow and the recursion is cheap. If
  profiling shows a hotspot, a memo table keyed by
  `(name, args)` can be added later.

## Consequences

**Positive.**

- `rec Point { x: Int, y: Int }` becomes genuinely value-typed.
  `val q := p` copies, and `p` remains usable.
- The `List<Row>` pattern becomes natural. Rows with only scalar
  fields can be copied in and out of a list without ownership
  gymnastics.
- `rec` brings ALGOL26 closer to what the ADR originally promised:
  value semantics extend naturally to structured data.
- The change is small and contained. No new syntax, no new IR, no
  new capability, no new interpreter code.

**Negative.**

- A record is `Copy` or not based on its fields, and nothing the
  programmer writes changes that. If a design needs a
  small-but-non-`Copy` record, the workaround is to hold the
  non-`Copy` field behind an indirection the language supports —
  there is none today. In practice this is rare.
- The predicate is invisible in the source. A programmer reads
  `val q := p` and has to know whether `Point` is `Copy` to reason
  about whether `p` is still usable. The ownership diagnostics make
  the answer visible when it matters (a use-after-move error either
  fires or it does not), so the friction is small.

**Neutral.**

- Records move from "always move" to "copy iff all fields are Copy."
  This is exactly the semantics Rust's `struct` has had since 1.0.
  No design risk.

## Tests

- `copy_record_survives_assignment` — `Point` copies on `val q := p`;
  `p.x` is still readable afterward.
- `non_copy_record_moves_on_assignment` — `Person` moves on
  `val q := p`; `p.name` produces `E-MOVE-001`.
- `copy_record_survives_argument_pass` — `Point` passed to a function
  leaves the caller's binding valid.
- `nested_copy_record_is_copy` — `rec Line { start: Point, end: Point }`
  is `Copy`.
- `mixed_record_is_not_copy` — `rec NamedPoint { name: String, pt: Point }`
  is not `Copy`, because `String` is not.
- `generic_record_copy_depends_on_args` — `Pair<Int>` is `Copy`;
  `Pair<String>` is not.
- `unknown_type_is_not_copy` — a record whose field type resolves to
  `Unknown` is treated as move-only.
- `copy_does_not_affect_mutability` — `val p := Point { ... };
  p.x := 5` still produces `E0007`.
- `copy_record_can_still_be_borrowed` — `&p` and `&mut p` on a
  `Copy` record work as before.

## See also

- `docs/decisions/0024-record.md` — the feature this builds on
- `docs/decisions/0005-ownership-model.md` — the move/borrow rules
- `docs/decisions/0006-immutability.md` — `val` vs. `var`, the
  orthogonal axis
- Rust's `Copy` marker trait semantics for the structural rule

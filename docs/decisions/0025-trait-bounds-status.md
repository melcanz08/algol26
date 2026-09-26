# ADR 0025: Trait bounds — current state

Status: Informational

## Context

ALGOL26 parses `where T: TraitName` clauses on generic functions.
The analyzer verifies the trait *name* is declared but does not
verify that a concrete type at a call site *satisfies* the bound.

Two programs demonstrate the boundary:

    trait Comparable
        function compare(self: Int, other: Int) -> Int

    function max<T>(a: T, b: T) -> T where T: Comparable
        return a

    procedure main
        print(max(1, 2))

This compiles and runs. `Int` implements `Comparable` only if the
user wrote `impl Comparable for Int` — but the check is never
performed, so the clause has no effect.

    function max<T>(a: T, b: T) -> T where T: Nonexistent
        return a

This is rejected with `E0004: Unknown trait 'Nonexistent'`.

So the effective rule is: trait name must be declared, concrete
satisfaction is not checked.

## What was removed

`src/ir/monomorphize.rs` contained
`Monomorphizer::check_trait_bounds_for_instantiation`, which used a
hardcoded table:

    "Comparable" => matches!(concrete_type, Type::Int | Type::Float),
    "Display"    => matches!(...),
    "Add"        => matches!(...),
    _            => true,

That table was not connected to the trait registry and was not on
the compiler pipeline. It was called only by
`tests/semantics/trait_bounds_enforcement.rs`, which therefore
asserted the behavior of a stub the compiler never invoked.

The monomorphize phase was removed in ADR 0013 Stage 3.2d. The
module survived because it had tests; the tests survived because
they used the module. Neither was reachable from the pipeline.

## What this means going forward

Two paths, undecided:

**Enforce.** Add `where_clauses` to `FunctionInfo`. In the
`FunctionCall` arm, after `type_bindings` is computed, resolve
each constraint's concrete type and check
`trait_registry.type_implements_trait(concrete, name)`. Reject if
no impl exists. Roughly 40 lines. The registry machinery
(`type_implements_trait`) already exists.

**Remove.** Delete the `where` grammar, the analyzer check, and
the `WhereClause` AST node. The language loses a capability no
corpus program uses.

Documenting-only is the third option and the one this ADR takes
for now: the syntax is accepted, the name is checked, the bound
is not enforced, and the discrepancy is noted here rather than
silently carried.

## Consequences

**Positive.** The gap is documented. The stub code and its
misleading tests are removed. A future ADR that implements
enforcement has a clear starting point.

**Negative.** `where T: Comparable` continues to read as a
guarantee without being one. Programs that rely on the bound to
prevent misuse will not be caught.

## See also

- `docs/decisions/0003-type-system.md` — trait mechanism
- `docs/decisions/0013-executable-ir-generic-invariant.md` — removed
  the monomorphize phase
- `src/semantics/trait_registry/` — the registry, including
  `type_implements_trait`
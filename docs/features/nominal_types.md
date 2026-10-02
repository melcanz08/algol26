# Feature: Nominal Types

    type UserId distinct Int

## Syntax

    type Name distinct Base

`distinct` is required. `type X distinct Y` declares a nominal type
whose identity is distinct from `Y` and from every other nominal type,
even when their bases coincide. `type X Y` (a type alias) is reserved
for a future ADR and is not implemented.

Base must be `Int`, `Float`, `Bool`, or `String`. `Ptr` and record
bases are deferred.

## Typing

    UserId     != Int
    UserId     != PriceCents      (even when both base on Int)
    UserId     == UserId          (same NominalTypeId)

Identity is `NominalTypeId`, assigned once by the analyzer in
declaration order. Two declarations with the same printed name in
different modules are different types. `Display` uses the name; no
compiler decision uses the name.

## Conversion

    UserId.from_base(x: Int) -> UserId
    id.to_base()             -> Int
    id.to_base               -> Int      (no-parens form)

`from_base` consumes its argument. `to_base` consumes the receiver.
For Copy bases (`Int`, `Float`, `Bool`) this is invisible. For
non-Copy bases (`String`) the argument / receiver is moved.

There is no implicit conversion in either direction: not in
assignment, not in parameter passing, not in return position, not
in equality, not in arithmetic.

## Operators

Nominal types inherit nothing from their base:

    ==, !=          require a trait impl
    <, <=, >, >=    require a trait impl
    +, -, *, /      require a trait impl
    and, or, not    require a trait impl

`can_coerce_to` returns false for `Distinct <-> base` and for
`Distinct{A} <-> Distinct{B}`. `can_cast_to` permits `Distinct <->
base` as an explicit `as` cast; that is what the intrinsic wrap /
unwrap lowers to. Equality on two `Distinct` values is rejected in
the analyzer's `BinOp::Equal` arm.

## Ownership

Nominal types follow their base's ownership rules:
`is_copy(Distinct<T>) == is_copy(T)`.

## IR

`Type::Distinct { id, name, base }` flows through `SemanticProgram`
and `VerifiedIR` unchanged. The analyzer assigns the id; every
downstream consumer reads the resolved map
(`TypedProgram::nominal_types`).

`from_base` and `to_base` lower to `TypedIRValue::Cast` with the
nominal type as `target_type`. The cast is a no-op — the value's
representation is unchanged.

## Backends

    LLVM        - supported (Cast is a no-op)
    WASM        - supported (reuses IRCodeGen)
    Interpreter - supported (Cast returns the inner value)

No new capability feature. Any backend that supports the base
supports the nominal type. This is unusual for the capability
matrix and is deliberate.

## Map keys

`Map<UserId, V>` is valid iff `UserId`'s base is a valid key
(`Int`, `String`, or `Bool`). `is_hashable_key` recurses through
`Distinct`.

## Capability

No `Feature::NominalTypes` variant. The capability matrix does not
track this — nominal types are transparent to capability checks.

## Tests

    tests/conformance/valid/nominal_types/*.gol
    tests/conformance/invalid/nominal_*.gol

Coverage matrix maturity: `AllBackends`.

## See also

- `docs/decisions/0029-nominal-types.md`

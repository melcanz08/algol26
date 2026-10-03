# Feature: Enum Types

    enum Day
        Monday
        Tuesday
        Wednesday

## Syntax

    enum Name
        Variant1
        Variant2
        ...

One variant per indented line. Variants receive 0-based ordinals
in declaration order. At least one variant is required. Duplicate
variant names within a declaration are rejected at parse time.

`enum` is recognized as an identifier at the top level, not a
reserved keyword. Programs that use `enum` as a variable name
keep working.

## Typing

    Day             distinct from every other type
    Day             == Day (same EnumTypeId)
    Day             != Int
    Day             != OtherEnum

Identity is `EnumTypeId`, assigned by the analyzer in declaration
order. Two declarations with the same name in different modules
are different types. Display uses the name.

Enums are `Copy` and `is_hashable_key`. They are not numeric.

## Values

Enum values are their ordinals. The runtime representation is
`i64`, matching `Int`.

    Day.from_ordinal(n: Int) -> Day
    d.to_ordinal()           -> Int
    d.to_ordinal             -> Int          (no-parens form)

A literal `from_ordinal` argument that is out of range is a
compile-time error:

    Day.from_ordinal(7)      // E0002 if Day has 3 variants

Runtime out-of-range `from_ordinal` is not checked in v1. The
ADR documents this as a follow-up.

No implicit conversion between `Int` and an enum. Explicit `as`
casts are permitted (the intrinsic wrap/unwrap lowers to them).

## Equality and comparison

Unlike nominal types (ADR 0029), enums get `==`, `!=`, `<`, `<=`,
`>`, `>=` for free when both sides have the same `EnumTypeId`.
Comparison is by ordinal. This is total and unambiguous — the
declaration order is the canonical order.

Cross-enum comparison is a type error.

## Patterns

Enum variants in pattern position are written bare:

    match d
        case Monday
            print 100
        case _
            print 0

Parsing rule: an identifier starting with an uppercase letter in
pattern position is a `Pattern::Variant`. Lowercase identifiers
remain `Pattern::Binding`. This is a documented side effect:
`case X` for a single-uppercase-letter binding no longer binds a
variable. Users write lowercase bindings.

## Exhaustiveness

A `match` on an enum must cover every variant, unless a wildcard
or binding fallback (`case _`) is present. Reports the first
uncovered variant:

    match on enum 'Day' is missing a case for variant 'Wednesday'

## Printing

`print d` prints the ordinal (`0`, `1`, `2`, ...). The variant
name is not available at runtime in v1. A follow-up ADR can add
name-based printing via a trait.

## IR

`Type::Enum { id, name, variants }` flows through `SemanticProgram`
and `VerifiedIR` unchanged. `from_ordinal` / `to_ordinal` lower to
`TypedIRValue::Cast`, a no-op.

`SemanticPattern::Variant { name, ordinal }` carries the ordinal
resolved at IR-build time from the matched type. The name is
carried for display only.

## Backends

    LLVM        - Partial
    WASM        - Partial
    Interpreter - Full

Enum declaration, resolution, and the `from_ordinal` / `to_ordinal`
intrinsics compile through all three backends: at runtime an enum
is an `i64`, and the `Cast` is a no-op.

However, **any program that matches on an enum cannot run on LLVM
or WASM**: those backends refuse `match` at the capability check,
independent of the pattern's shape. `match` on `Option`, `Result`,
and `Bool` has the same limitation today.

This feature row is therefore `InterpreterOnly`: the interesting
part of enum types (variant matching) is interpreter-only until
`match` lands on LLVM/WASM.

## Capability

No `Feature::EnumTypes` variant. Enum types are transparent to the
capability matrix — a program that declares an enum but does not
match on it compiles through every backend. The `match` refusal is
the same one that applies to all pattern matching.

## Map keys

`Map<Day, V>` is valid; enums are `is_hashable_key`.

## Iteration

`for d in Day` is deferred. It requires `IntoIterator`, which is
beyond v1. Iterate manually via `from_ordinal` / `to_ordinal`:

    var i := 0
    while i <= 2
        val d := Day.from_ordinal(i)
        print d.to_ordinal()
        i := i + 1

## Tests

No dedicated conformance directory. The interesting behavior
(match on enums) is interpreter-only and the conformance harness
does not support per-fixture backend selection. This is the same
shape as `records` and `map`.

Unit tests live in `src/semantics/analyzer/tests.rs`.

## See also

- `docs/decisions/0030-enum-types.md`
- `docs/features/nominal_types.md` — the sibling feature
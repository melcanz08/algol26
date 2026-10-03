# Feature: Subrange Types

    type Percentage Int in 0..100
    type Month      Int in 1..12

    enum Day
        Monday
        Tuesday
        Wednesday
        Thursday
        Friday
        Saturday
        Sunday

    type WorkDay Day in Monday..Friday

## Syntax

    type Name Base in Low..High

Inclusive on both ends. `Base` is `Int` or a user enum. `Low`
and `High` are single atoms — Int literals for Int bases, variant
names for enum bases. `low > high` is a compile-time error. Bases
other than Int and enums (Float, Bool, String, records, other
subranges) are rejected.

## Typing

    Percentage      distinct from every other type
    Percentage      == Percentage (same SubrangeTypeId)
    Percentage      != Int
    Percentage      != Month

Identity is `SubrangeTypeId`, assigned by the analyzer in
declaration order. Two declarations with the same name in
different modules are different types.

Subranges are `Copy` and `is_hashable_key`, matching their base.
They are not numeric.

## Values

Construction:

    Percentage(75)           // ok, 75 is in 0..100
    Percentage(150)          // compile error, literal out of range
    Percentage(n)            // runtime check if n is not a literal

Literal coercion:

    val p: Percentage := 75  // ok
    val q: Percentage := 150 // compile error, literal out of range

Extraction:

    p.to_base()              // -> Int
    p.to_base                // -> Int (no-parens form)

## Arithmetic and comparison

No arithmetic (`+`, `-`, `*`, `/`) is defined on subranges.
A Percentage plus a Percentage is not necessarily a Percentage.
Write `p.to_base() + q.to_base()` and construct the result
explicitly.

Comparison (`==`, `!=`, `<`, `<=`, `>`, `>=`) is available between
two values of the *same* subrange. Cross-subrange comparison is
a type error. Unlike nominal types, subranges inherit a canonical
total order from their base.

## Bounds checking

Literal Int arguments to `T(v)` and literal Int coercions are
checked at compile time.

Non-literal arguments emit a runtime check that fails the program
on out-of-range values:

    val n := 150
    val p := Percentage(n)   // compiles; runtime error on
                             // the construction

The runtime error prints "Percentage: value out of range 0..100"
and exits with status 1 on LLVM/WASM, or returns
`EvalError::Runtime` on the interpreter.

## IR

`Type::Subrange { id, name, base, low, high }` flows through
`SemanticProgram` and `VerifiedIR` unchanged.

Construction lowers to `BoundsCheck` (for non-literal args)
followed by a no-op `Cast`. Extraction lowers to a no-op `Cast`.

## Backends

    LLVM        - Full
    WASM        - Full
    Interpreter - Full

Subranges lower to their base at every backend (Int or the enum's
ordinal, both i64). The `BoundsCheck` instruction has native
lowering on LLVM (icmp + branch to an error block), reused by
WASM via the shared IRCodeGen; the interpreter evaluates it
directly.

## Capability

No `Feature::Subranges` variant. Subranges are transparent to the
capability matrix — any backend that supports the base supports
the subrange.

## Map keys

`Map<Percentage, V>` is valid if the base is a valid key. Int and
enum bases are both valid.

## Tests

    tests/conformance/valid/subrange_types/

## See also

- `docs/decisions/0031-subrange-types.md`
- `docs/features/nominal_types.md`
- `docs/features/enum_types.md`
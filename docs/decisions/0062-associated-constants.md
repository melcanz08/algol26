# ADR 0062 — Associated constants

## Status

Accepted. Implemented. Backfilled 2026-11-02.

## Context

ALGOL26 has traits (`ADR 0033`) with associated types
(`ADR 0041`). A trait can declare a *type* that each impl
supplies. It cannot declare a *constant* — a value known at
compile time, uniform for every `T` that implements the trait,
and specialized per impl.

Use cases:
- `Sizable::SIZE` — the byte-width of a type. `Int::SIZE = 8`,
  `Float::SIZE = 8`, `Point::SIZE = 16` (two i64s).
- `Nameable::DEFAULT` — a default value of the implementing type.
- `Version::MAJOR` / `::MINOR` — version tags on nominal types.

Without a language feature, users would have to encode these as
per-type functions (`Int::size()`), which is a call rather than
a constant and can't appear in type-level positions.

## Decision

**Trait-declared constants.** A trait may declare constants in
its body:

    trait Sizable
        const SIZE: Int

**Impl-supplied values.** Each impl binds the constant to an
expression of the declared type:

    impl Sizable for Int
        const SIZE: Int := 8

    impl Sizable for Point
        const SIZE: Int := 16

The expression must type-check at declaration time. It can be a
literal, a call to a compile-time-known function, or any other
expression the analyzer accepts. The expression is *stored*, not
evaluated at declaration — it's inlined at each use site.

**Use-site syntax.** Constants are referenced as `Type::NAME`:

    val n := Int::SIZE
    val bytes := Point::SIZE * count

**Parser folds `::` into a compound identifier.** The parser
represents `Int::SIZE` as `Var("Int::SIZE")`, a single identifier
whose text includes the `::`. This avoids adding a new AST
variant for a use case that's always followed by an identifier.
The analyzer resolves the compound name against the
`const_values` map; the IR builder inlines the stored expression.

**Semantics of use.** `Type::NAME` is a *pure* expression — it
cannot be assigned to, borrowed, or moved. It's not a binding;
it's a compile-time substitution.

## Consequences

### Positive

- Type-level constants can be declared once and used everywhere
  the trait is implemented.
- The value is specialized per impl; no runtime dispatch needed.
- `Type::NAME` is a real expression in ordinary code (can be
  used in arithmetic, comparisons, function arguments), not just
  a type-level annotation.

### Negative

- The expression is inlined at each use site, so if the
  expression is expensive (a function call that isn't
  compile-time-folded), the runtime cost is paid per use. For
  constants that should be true literals, users should write
  literals.
- `::` folding into identifiers means `Int::SIZE` and a
  hypothetical `Int` variable with a `SIZE` method look the
  same in the AST. The analyzer disambiguates by trying
  `const_values` first, but this is implicit coupling.

### Not yet supported

- **Generic constants** — `const N: Int` where `N` varies with
  a trait's type parameter. Requires type-level integers or
  const generics, which is a much larger feature.
- **Associated constant defaults** — `const SIZE: Int := 8` in
  the trait declaration, overridable per impl. Currently every
  impl must define every constant; no default.
- **Borrowing / moving a constant.** `Type::NAME` is pure; a
  borrow would have no place to point at.
- **Constants outside impls.** Only trait and inherent impls may
  declare constants. Top-level `const` items aren't part of the
  language.

## Implementation

- Parser: `const NAME: Type [:= expr]` recognized in trait and
  impl bodies. Stored on `TraitDecl.constants` and
  `ImplBlock.constants`.
- Analyzer: `const_values: HashMap<String, (Type, Expr)>` keyed
  by `"Type::NAME"`. Resolves the constant's declared type;
  checks the impl expression type-checks against it.
- IR builder: reads `const_values` at each `Var("Type::NAME")`
  use site and inlines the stored `Expr`.
- Verification: the inlined expression is verified in the
  enclosing function's context. No separate "constant
  verification" pass.

## References

- `ADR 0033` — methods and `self` (impl structure this builds on)
- `ADR 0041` — associated types (the closest analogue; constants
  follow the same trait-declared / impl-supplied pattern)
- `docs/features/associated_constants.md` — user-facing syntax

## Backfill note

Backfilled 2026-11-02. The feature was implemented, documented,
and tested (via `tests/conformance/valid/associated_constants/`)
before ADR 0062 existed — in fact before any ADR described it.
`docs/features/associated_constants.md` served as the only design
doc. The reconciliation found the gap and this ADR closes it.

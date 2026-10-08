# ADR 0041 — Associated Types

## Status

Accepted. Implemented on all three backends (interpreter, LLVM,
WASM). The projection is resolved to a concrete type before IR
construction; the runtime representation is the concrete type's.

## Context

A trait today can declare methods and constants. Both are
concrete at the call site. Neither can express a type that
depends on the implementing type — the classic case being a
container whose element type is only known once you know the
container type.

    trait Container
        function first(self: &Self) -> Self::Item

    impl Container for List<Int>
        type Item = Int
        function first(self: &List<Int>) -> Int
            return self[0]

`Self::Item` cannot be written today: `TypeSyntax` has no form
for a projection, and `Type` has no associated variant. The
parser, the type system, the trait registry, the analyzer, the
verifier, and the codegen all need to learn one new form.

This ADR fixes the design so the implementation has a target.

## Decisions

### D1. `Self::Item` appears in four positions

- Method signatures inside the trait body:
  `function first(self: &Self) -> Self::Item`
- Method signatures inside the impl:
  `function first(self: &List<Int>) -> Int` (matching the
  trait with `Item` replaced by the concrete projection)
- Generic bounds: `function f<C: Container>(c: C) -> C::Item`
- Nested in other types: `List<C::Item>`, `Option<C::Item>`,
  `Map<String, C::Item>`

All four are supported. The parser, TypeSyntax, and Type all
accept the projection form wherever a type appears.

### D2. Normalization happens at call-site unification

`C::Item` is stored symbolically by the analyzer. It is
resolved to a concrete type when the enclosing generic is
instantiated with concrete type arguments — the same point
where where-clause bounds (ADR 0025) are checked and where
generic impls record their `Instantiation` (ADR 0034).

Registration does not attempt to normalize. A projection with
an unbound `Self` has no concrete meaning; attempting to
resolve it early would require guessing.

### D3. Projections are transparent after normalization

Once `C::Item` resolves to `Int`, it *is* `Int` for every
downstream check: type equality, coercion, method dispatch,
codegen. No `.to_base()` is required at use sites. This matches
Rust: `let x: <Pair<Int> as Container>::Item = 5;` is legal and
`x` has type `Int`.

The consequence is that the verifier must see only normalized
types. Any `Type::Associated { .. }` reaching the verifier is a
compiler bug — the same discipline that keeps `TypeVar` out of
verified IR.

### D4. Mangling includes the concrete projection binding

A trait method whose return type (or any signature position)
mentions `Self::Item` gets a distinct symbol per impl. The
existing pattern is `{Trait}_{Owner}_{method}` for trait impls
and adds `_{TypeArg}` segments for generic impls. Associated
types extend the same pattern with the projection bindings:

    Container_Pair_Int_first
    Container_Pair_String_first

Each impl of `Container for Pair<...>` produces one symbol per
`Item` binding. The pattern is not new; it is the ADR 0034
monomorphization scheme with the projection treated as an
additional type argument.

### D5. `Instantiation` gains associated bindings

`Instantiation` (see ADR 0013) currently carries
`type_params: Vec<String>` and `type_args: Vec<Type>`. It gains

    associated_bindings: HashMap<String, Type>

keyed by the associated type's declared name (`"Item"`) and
valued by the concrete type the impl supplied. The IR builder
reads this to select the correct specialization, same as it
reads `type_args` today.

### D6. Associated types only on trait impls

`impl Foo { type Item = Int }` — an inherent impl with an
associated type — is a compile error. The projection has no
meaning without a trait declaring it. Rust does the same.

### D7. Trait declaration syntax: no semicolon

    trait Container
        type Item

The line ends the declaration. This matches `const NAME: Type`,
which also does not use a terminator. The `;` form (Rust's
convention) is not adopted.

## Syntax summary

Declaration in a trait body:

    trait Container
        type Item
        function first(self: &Self) -> Self::Item

Definition in a trait impl:

    impl Container for List<Int>
        type Item = Int
        function first(self: &List<Int>) -> Int
            return self[0]

Use inside a generic function:

    function head<C: Container>(c: C) -> C::Item
        return c.first()

Use nested in another type:

    function all<C: Container>(c: C) -> List<C::Item>
        ...

## Implementation plan (for the follow-on session)

Ordered by dependency. Each step is a commit.

1. **AST.** `TraitDecl.associated_types: Vec<String>`,
   `ImplBlock.associated_types: Vec<(String, TypeSyntax)>`,
   `TypeSyntax::Projection { base, name }`.

2. **Parser.** Read `type Name` in trait bodies, `type Name = T`
   in impl bodies, and `Base::Name` in type syntax. The `::`
   token already exists (ADR for UFCS).

3. **Type enum.** `Type::Associated { base: Box<Type>,
   trait_name: String, assoc_name: String }`.

4. **Analyzer registration.** Register each impl's associated
   type bindings keyed by `(trait_name, target_type, assoc_name)`.
   Error if the trait declared an associated type the impl does
   not define, or if the impl defines an associated type the
   trait did not declare (mirrors the constant rule).

5. **Signature substitution.** When checking an impl's method
   against the trait's, replace `Self::Item` in the trait's
   declaration with the impl's concrete type before comparing.

6. **Unification.** Extend `unify_types` to normalize
   `Type::Associated` against the current impl's bindings. In a
   generic body, `C::Item` binds through `C`'s trait impl once
   `C` is concrete.

7. **Mangling.** Add the projection bindings to the mangle at
   the same site where `type_args` are appended today. Extend
   `Instantiation` with `associated_bindings`.

8. **Verifier.** Reject any `Type::Associated` reaching the
   verifier — the same rule as `TypeVar`.

9. **IR builder.** Substitute projection types at use sites,
   matching the existing `substitute_type_vars` shape.

10. **Fixtures.** A `Container`/`Pair` example on all three
    backends; conformance valid and invalid cases; coverage
    matrix row; feature doc.

## Scope boundary

Explicitly **out of scope** for v1:

- **Associated type defaults.** `type Item = Int` in the trait
  declaration itself. Rust has this; it is optional.
- **Associated type bounds.** `type Item: Display`.
- **Supertrait associated types.** A subtrait inheriting the
  parent's associated types.
- **Generic associated types.** `type Item<T>`.
- **Associated types on inherent impls.** See D6.

Each of these can land in its own ADR if a motivating use case
appears.

## Why this is not just syntax

The surface is small: one new declaration form, one new type
syntax, one new `Type` variant. The weight is in the type
system: projections introduce a second kind of type equality
(projection to concrete) that the verifier, the mangler, and
the IR builder must all agree on. Getting this right the first
time means writing the decisions down — which is this document
— rather than discovering the equality rules halfway through
the implementation.

## Consequences

**Positive.**

- The language gains a standard expressiveness feature that
  most trait-based type systems eventually need.
- The implementation reuses the machinery of ADR 0025
  (unification-time checks) and ADR 0034 (monomorphization).
  No new subsystems.

**Negative.**

- Every consumer of `Type` gains a case to match or explicitly
  delegate. The compiler gets one more dimension of equality.
- Projection normalization is a new failure mode: a projection
  that never resolves (because its base never does) must be
  reported, not silently dropped.

## See also

- ADR 0025 — trait bound satisfaction; projections are checked
  at the same point bounds are.
- ADR 0034 — generic impls; the mangling and instantiation
  scheme this ADR extends.
- ADR 0013 — executable IR and generic invariants; the
  `Instantiation` record this ADR extends.
- ADR 0040 — function-value surface; a sibling deferral with a
  similar 'small surface, large model' shape.

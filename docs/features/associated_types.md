# Associated types

## Summary

A trait can declare an associated type that its implementations
bind to a concrete type. The declaration lives in the trait body
as `type Name`; the impl supplies the concrete type with
`type Name := ConcreteType`. Uses of `Self::Name` inside the
trait method signatures resolve to the binding at call-site
specialization.

## Surface syntax

```algol26
trait Container
    type Item
    function first(self: &Self) -> Self::Item

impl Container for List<Int>
    type Item := Int
    function first(self: &List<Int>) -> Int
        val inner := *self
        return inner[0]
```

The associated type is referenced in type position as
`Base::Name`. Three bases are recognized:

- **`Self`** inside a trait declaration, referring to the
  implementing type. `Self::Item` appears in the trait's method
  signatures.
- **A type variable with a where-clause bound.**
  `function head<C>(c: C) -> C::Item where C: Container`.
- **A concrete type.** `List<Int>::Item` is legal type syntax,
  though the analyzer resolves most uses through the type table
  before this form appears.

## Rules

- A trait declares zero or more associated types with `type Name`
  on its own line. No semicolon, no bound, no default. Bounds
  (`type Item: Display`) and defaults (`type Item := Int` in the
  trait itself) are out of scope for v1.
- An impl of the trait must define every associated type the
  trait declares, and may not define one it does not.
- A projection is only valid on a base whose trait bound declares
  the named associated type. `C::Item` where `C: Container` and
  `Container` does not declare `Item` is a compile error.
- Inherent impls (`impl Foo`) cannot declare associated types.
  The projection has no trait to project through.

## Normalization

The analyzer stores a projection symbolically as
`Type::Associated { base, trait_name, assoc_name }`. When the
base becomes concrete — at a generic call site, or when the
receiver's type is known — the projection is replaced by the
concrete type the impl bound.

Two normalization points:

- **Analyzer, `substitute_type_vars`.** When a generic function's
  body substitutes a type variable, the resulting `Associated`
  value is reduced against the registry's bindings.
- **IR builder, `type_of_expr`.** Every expression's type is
  passed through `normalize_assoc` after substitution, so the
  verifier sees only concrete types.

A projection that never becomes concrete — a `C::Item` return
from a generic function that is never instantiated, or a
malformed bound — remains symbolic and is rejected by the
verifier under the same rule that rejects `TypeVar`: executable
IR cannot contain unresolved types.

## Diagnostics

| Code | When |
|------|------|
| `E0002` | Impl missing a declared associated type; extra associated type not declared by the trait; inherent impl declares an associated type; projection through an undeclared bound |
| `E0002` (verifier) | An unnormalized `Type::Associated` reaches executable IR (a compiler bug, not user error) |

## Known limitations (v1)

- **No associated type bounds.** `type Item: Display` is out of
  scope.
- **No associated type defaults.** `type Item := Int` in the
  trait declaration is out of scope.
- **No supertrait associated types.** A subtrait does not inherit
  the parent's associated types.
- **No generic associated types.** `type Item<T>` is out of scope.
- **Concrete-base projection.** `List<Int>::Item` as written
  source syntax is parsed and resolved, but the direct lookup of
  a concrete base against the impl table is a future step. In
  practice the analyzer resolves through the type table before
  this path is taken.
- **Generic-function specialization return types.** A generic
  function whose return type is a projection
  (`function head<C>(c: C) -> C::Item where C: Container`) is
  supported at the analyzer level but the builder's specialization
  emit path currently resolves the return type to `Unknown`
  instead of normalizing `C::Item` to the concrete binding. The
  builder consults `resolve_type_syntax` on the raw AST, which has
  no trait-registry access. The direct-projection case (methods on
  concrete receivers) works. The fix threads the analyzer's
  resolved return types to the builder; it is a follow-up.

## Capability

All three backends support associated types identically. The
projection is resolved to a concrete type before codegen; the
runtime representation is the concrete type's representation.

## References

- `docs/decisions/0041-associated-types.md` — design rationale.
- `docs/decisions/0025-trait-bounds-status.md` — the bound
  satisfaction rules projections interact with.
- `docs/decisions/0034-generic-impls.md` — the monomorphization
  scheme projections extend.
- `docs/features/trait.md` — the trait mechanism.

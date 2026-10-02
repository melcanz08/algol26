# ADR 0029 — Nominal Types

## Status

Accepted. A1–A6 implemented; A7–A8 pending.

`NominalTypeId` is assigned once, in the analyzer's
`register_nominal_types`, and propagated through `TypedProgram`'s
`nominal_types` map to every downstream consumer. No consumer
reconstructs it.

## Context

ALGOL26 has strong guarantees for memory, ownership, and control flow.
It has weaker guarantees for *meaning*. Two values of the same
structural type are interchangeable even when they represent different
concepts:

```algol26
val userId: Int := 42
val priceCents: Int := 42
val total := userId + priceCents    // compiles; semantically nonsense
```

Pascal-family languages distinguish *type aliases* from *nominal
types*. This ADR adopts the concept, adapted to ALGOL26's existing
type system, ownership model, and trait/generic infrastructure.

## Decision

Add nominal types via the `distinct` modifier:

```algol26
type UserId distinct Int
type PriceCents distinct Int
type Meters distinct Float
```

This is the **only** new declaration form. `type X Y` (true type
alias) is not implemented here; if needed, it gets its own ADR. The
grammar reserves `type` for both forms but only `distinct` is
implemented.

### Nominal identity

Nominal identity is *not* the printed name. Two declarations of
`type Id = distinct Int` in different modules produce different
types. The representation reflects this:

```rust
pub struct NominalTypeId(pub u32);

pub enum Type {
    // ...
    Distinct {
        id: NominalTypeId,   // identity — used for equality, mangling, lookup
        name: String,         // presentation — used for display only
        base: Box<Type>,      // representation — used for lowering
    },
}
```

The invariant: **identity is `id`; `name` is never consulted for
equality, coercion, mangling, or any compiler decision.** This
matches the ExprId migration: identity must not be reconstructed
from display strings.

`NominalTypeId` values are assigned by the analyzer during type
declaration registration and are unique within a compilation unit
across all modules. A compile-unit counter is sufficient; no
global/persistent identity is needed.

### Supported base types

First implementation: `Int`, `Float`, `Bool`, `String`.

**Not supported in v1: `Ptr`.** Nominal pointer types would require
every capability scanner, verifier, backend check, and lowering
stage to recursively see through `Distinct(Ptr)` without
misclassifying it as a non-pointer value. That is solvable but
belongs in a separate ADR focused on pointer safety. Adding it
later does not require reopening the identity model.

### Conversion

Two conversions exist per nominal type:

```
T.from_base(base_value) -> T
T.to_base(self) -> base
```

**Ownership semantics:**

- `from_base` **consumes** the base value.
- `to_base` **consumes** the nominal value and transfers the
  underlying value to the caller.

For Copy bases (`Int`, `Float`, `Bool`), this is invisible — the
value is copied into and out of the wrapper. For non-Copy bases
(`String`), `to_base` moves out:

```algol26
type Name = distinct String

val n: Name := Name.from_base("Rommel")
val s: String := n.to_base()      // n is consumed; not usable after
```

A borrowed accessor `as_base(&self) -> &base` is a possible future
addition; it is not part of v1.

**No implicit conversion** in either direction: not in assignment,
parameter passing, return position, equality, or arithmetic.

**Intrinsics, not user functions.** `from_base` and `to_base` are
compiler-owned associated conversions, not entries in the ordinary
builtin-function namespace. Source syntax is `T.from_base(x)`,
resolved by the analyzer through a dedicated intrinsic table. User
code cannot redefine them; an imported module cannot shadow them;
generic specialization cannot treat them as user functions.

### Equality and operators

Nominal types inherit **nothing** from their base type implicitly:

- `==` and `!=` require a trait impl. Two nominal values are not
  comparable by default.
- `<`, `<=`, `>`, `>=` require a trait impl.
- `+`, `-`, `*`, `/` require a trait impl.
- `and`, `or`, `not` require a trait impl.

This is a deliberate change to the existing operator rules. The
current type checker accepts `left == right` when the types are
structurally identical. That path must be narrowed: for
`Type::Distinct`, equality is not automatic. The change is
localized to the `BinOp::Equal | BinOp::NotEqual` arm and the
comparison arm of `analyze_expr_inner`.

**Verify before implementing:** the existing `is_numeric()` check
must return `false` for `Distinct` regardless of base. This
already prevents `userId + priceCents`. What it does not prevent
is `userId == userId` succeeding via structural equality; that
requires an explicit guard in the equality arm.

### Usability

Nominal types are usable as:

- Function parameter and return types
- Record field types
- Generic arguments: `List<UserId>`, `Option<UserId>`,
  `Result<UserId, E>`
- **Map keys**, when the base type is a valid key (see below)
- Trait impl targets

Nominal types are **not** usable as:

- Match patterns
- Loop iterable element types
- Operands of any binary operator without an explicit trait impl

### Map keys

**A nominal type is a valid Map key iff its base type is.**

```algol26
type UserId  = distinct Int      // valid key
type Code    = distinct String   // valid key
type Flag    = distinct Bool     // valid key
type Meters  = distinct Float    // not a valid key (Float isn't either)
```

This is consistent with the ADR 0027 key rule and preserves the
nominal boundary while letting the map use the underlying
representation for hashing and equality. The Map key check is
extended to recurse:

```
is_hashable_key(Distinct { base, .. }) := is_hashable_key(base)
```

### Identity must participate everywhere

`NominalTypeId` — not `name` — must flow through:

- `Type` equality and `PartialEq` impl
- `can_coerce_to` — `Distinct{A} -> Distinct{B}` iff `A.id == B.id`
- `common_supertype`
- Trait implementation lookup
- Generic specialization and name mangling
- Type equality used by the verifier
- Map key determination
- Diagnostics (identity for decisions, name for display)

**Verify before implementing generic specialization.** The
mangler in `src/ir/instantiation_plan.rs` currently mangles
types by structure. A `Distinct` case must mangle by `id`, not
by `name`. Otherwise `first<a.Id>` and `first<b.Id>` would
collide at the IR level even though the analyzer keeps them
distinct.

**Verify before implementing trait lookup.** The trait registry
keys impls partly by type string. If so, that path also needs to
use `NominalTypeId`. This is a spot-check, not a rewrite — the
registry has `test_generic_impl`, `test_register_trait`, and
`test_type_implements_trait` that will catch regressions.

## Consequences

### Data model

See Decision above for the `Type::Distinct` shape and the new
`NominalTypeId`. Changes ripple through `src/common/types.rs`:

- `Display` — prints `name` only
- `can_coerce_to` — `Distinct{A} -> Distinct{B}` iff `A.id == B.id`
  (recursing through `base` is never performed here)
- `common_supertype` — same `id` required; else `Unknown`
- `substitute` — recurse into `base` only
- `contains_type_var`, `contains_unknown` — recurse into `base`
- `is_copy` — `base.is_copy()`
- `is_numeric` — always `false`
- `is_hashable_key` — recurse into `base`

### Frontend

New syntax in `src/frontend/parser/items.rs`:

```
type Name = distinct BaseType
```

New AST node:

```rust
pub struct DistinctDecl {
    pub name: String,
    pub base: TypeSyntax,
    pub span: Span,
}
```

Added to `Program.distinct_decls: Vec<DistinctDecl>`.

### Analyzer

New analyzer state:

```rust
nominal_types: HashMap<String, Type>,
next_nominal_id: u32,
```

`register_nominal_type` assigns a fresh `NominalTypeId`, stores
the corresponding `Type::Distinct`, and registers the two
associated intrinsics in a dedicated `intrinsics: HashMap<(NominalTypeId, String), IntrinsicKind>` table.

`resolve_type_syntax` checks `nominal_types` before `records`.

### IR

`SemanticIR` carries `Type::Distinct` values unchanged. The IR
verifier's type rules must include `Distinct` in equality and
coercion, using `id`.

### Backends

Every backend unwraps at lowering:

```rust
Type::Distinct { base, .. } => self.map_type(base),
```

Backends never see the `id` or `name`. No new instruction, no
new capability, no matrix change. Any backend that supports the
base supports the nominal type.

### Diagnostics

Nominal-type diagnostics **should** use structured secondary
labels — e.g. pointing at the variable declaration on an
assignment mismatch — when the framework supports it. This ADR
does not require completing the diagnostics migration; it
consumes whatever support already exists at implementation time.

Display uses `name`. Decision logic uses `id`.

### Tests

Beyond the basic valid/invalid conversion matrix:

**Identity collision.** Two modules each declare `type Id =
distinct Int`. Verify `a.Id != b.Id` and that assignment between
them fails. This is the test for `NominalTypeId`.

**Generic specialization.** `fn first<T>(xs: List<T>) -> Option<T>`
instantiated on `List<a.Id>` and `List<b.Id>` produces two
distinct specializations with distinct mangled names.

**Trait separation.** `type Meters = distinct Float` and
`type Seconds = distinct Float`; `impl Display for Meters`.
Verify `Meters` displays, `Seconds` does not, and `Float` does
not gain `Display` via this impl.

**Map key.** `type UserId = distinct Int`; `Map<UserId, String>`
compiles, inserts, and reads correctly across interpreter and
LLVM.

Plus coverage matrix and maturity rows, hand-added per ADR 0028's
precedent.

## Implementation order

```
A1. Type::Distinct + NominalTypeId          (no parser, no analyzer)
A2. Parser: type X = distinct Y             (AST + parse)
A3. Analyzer: register + resolve            (registry, resolution)
A4. Coercion / equality / operator rules    (the semantic core)
A5. Intrinsics: from_base / to_base         (dedicated table)
A6. Trait lookup + generic mangling         (verify first)
A7. Backends: unwrap in map_type            (LLVM, WASM, interp)
A8. Contract tests + docs/features file     (freeze the contract)
```

Each step compiles, tests, and is revertable. A6 is the riskiest
and should not be started until A4 and A5 are green and the
existing trait and generic tests all still pass.

## Alternatives considered

**`name: String` as identity.** Rejected. Breaks as soon as
modules exist. Same failure class that motivated ExprId.

**Reuse `Type::Generic`.** Rejected — generics are structural.

**Reuse `Type::Record` with zero fields.** Rejected — records
have field access, patterns, and construction syntax that
nominal types must not have.

**Keyword `newtype`.** `distinct` matches Pascal-family
terminology and does not carry Haskell's allocation connotations.

**Implicit conversion to base.** Rejected. The explicit
conversion is the feature. Implicit `to_base` would reopen
`userId + priceCents`.

**Equality inherited from base.** Considered and rejected. Two
nominal types can share a base and mean different things; even
same-type equality is a policy choice. Trait-based is the strict
answer, and it is what the first implementation uses.

**`distinct Ptr` in v1.** Deferred per the capability-recursion
concern above.

**Alias form `type X = Y` in this ADR.** Deferred. Two forms
with two semantics in one ADR invites a migration seam.

## Open questions

**`=` vs `==`.** The lexer currently reserves bare `=` and requires
`==` for equality. Pascal-family languages use `=` for equality and
`:=` for assignment, which would let `type X = distinct Y` read
naturally. This is a language-wide syntax question, not a
nominal-types question, and it is deferred to a future ADR. Until
then, nominal type declarations use `type X distinct Y` (no `=`
separator).

## References

- Free Pascal Reference Guide, "Type Compatibility", "Type Identity"
- ADR 0024 (Records) — same declaration-addition pattern
- ADR 0026 (Structural Copy) — `is_copy` propagation model
- ADR 0027 (Map) — the key rule this ADR extends
- `docs/architecture-direction.md` — type-variant checklist
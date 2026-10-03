# ADR 0030 — Enumeration Types

## Status

Accepted. A1-A6 implemented; A7 (matrix row, feature doc) landed
together with the feature.

Implementation note: the visible portion of enum types —
`from_ordinal`, `to_ordinal`, and variant matching — works on the
interpreter. LLVM and WASM refuse the `match` construct itself,
independent of pattern shape, so a program that matches on an enum
is interpreter-only. The `enum_types` coverage matrix row reflects
this with `InterpreterOnly` and a note on the boundary.

Deferred to future ADRs: runtime out-of-range `from_ordinal`
(v1 rejects literal out-of-range only), name-based printing,
`for d in Day` iteration.

## Context

ADR 0029 gave ALGOL26 a way to make types *nominal*: `UserId` is not
`Int`, even though they share a representation. Nominal types protect
*meaning*.

They do not give you a *finite domain*. A `UserId` still ranges over
every `Int`; nothing about the type says "these five values and no
others."

Enumerations do. The Pascal-family lineage — which ADR 0029 drew from
— treats enums as ordinal: `Monday` is the 0th value of `Day`,
`Tuesday` the 1st, and so on. That ordinality is what makes sets and
subranges possible later.

This ADR introduces ordinal enumerations, `enum Name ...`. Payload-
carrying enums (Rust-style `enum Shape { Circle(Float), ... }`) are
out of scope.

## Decision

Add ordinal enumerations:

```algol26
enum Day
    Monday
    Tuesday
    Wednesday
    Thursday
    Friday
    Saturday
    Sunday
```

Each variant is an identifier on its own line. Variants receive
0-based ordinals in declaration order.

### Ordinals and identity

```rust
pub struct EnumTypeId(pub u32);

pub enum Type {
    // ...
    Enum {
        id: EnumTypeId,        // identity — used for equality, mangling, lookup
        name: String,          // presentation only
        variants: Vec<String>, // declaration order; ordinal is index
    },
}
```

Same identity discipline as `NominalTypeId`: `id` is created once, in
the analyzer, and propagated. No consumer reconstructs it from `name`.

`Display` prints `name` for the type. Variant values print as their
ordinal (see "Printing" below).

### The design questions

**1. Syntax.**

Indentation-based, matching the rest of ALGOL26. `enum` is recognized
as an identifier at the top level (`Identifier(s) if s == "enum"`),
following the same pattern ADR 0029 used for `type`. It is not added
to the keyword table, so `enum` remains usable as a variable name in
other contexts.

No `=` separator, no per-variant type annotation. The grammar is:

```
enum Name
    Variant1
    Variant2
    ...
```

At least one variant is required. Duplicate variant names within a
declaration are an error.

**2. Runtime representation.**

Enums lower to `i64` at the LLVM/WASM level, matching `Int`. The
variant is the ordinal value. The interpreter stores
`RuntimeValue::Int(ordinal)` — no new runtime variant.

This is the same choice ADR 0029 made for nominal types: identity is a
compile-time property, representation is the base.

**3. Equality.**

`==` and `!=` are allowed between two values of the *same* enum
(same `EnumTypeId`). The comparison is by ordinal. This is total and
well-defined, unlike nominal types where equality was rejected
because structural equality could not be trusted.

`Day == Day` → Bool. `Day == Int` → type error. `DayA == DayB` (two
different enums) → type error.

**4. Comparison.**

`<`, `<=`, `>`, `>=` are allowed between two values of the same enum,
comparing ordinals. `Monday < Tuesday` is `true`.

This is deliberately *more permissive* than nominal types, where
comparison required a trait impl. Enums have a canonical total order
imposed by declaration; there is no risk of the user picking a wrong
one.

**5. Ordinal conversion.**

Two compiler-owned intrinsics, same shape as `from_base` / `to_base`:

```algol26
Day.from_ordinal(n: Int) -> Day
d.to_ordinal()         -> Int
d.to_ordinal           -> Int          (no-parens form)
```

`from_ordinal` with a statically-known out-of-range literal is a
compile error. With a runtime value, it panics with a clear message
("ordinal 7 is out of range for Day; valid range is 0..6").

A `Result`-based `try_from_ordinal(n: Int) -> Result<Day, String>` is
a candidate for a follow-up ADR; it is not in v1.

**6. Match exhaustiveness.**

A `match` on an enum must cover every variant, unless a `case _`
fallback is present. This extends `check_match_exhaustiveness`, which
currently handles `Option`, `Result`, and `Bool` as hardcoded cases;
user enums make it data-driven.

**7. Pattern syntax.**

Enum variants in pattern position are written bare, matching the
variant declaration:

```algol26
match d
    case Monday
        print 0
    case Saturday
        print 1
    case _
        print 2
```

Parsing rule: an identifier starting with an uppercase letter in
pattern position becomes `Pattern::Variant(name)`. Any other
identifier becomes `Pattern::Binding(name)`. The existing
`Some(x)` / `None` / `Ok(x)` / `Error(x)` patterns keep their
dedicated AST variants; `Pattern::Variant` is new and covers user
enums.

The analyzer resolves `Pattern::Variant("Monday")` against the
matched type:
- matched type is `Type::Enum { variants, .. }` and `Monday` is in
  `variants` → variant match
- matched type is an enum and `Monday` is not a variant → error:
  `no variant 'Monday' on enum 'Day'`
- matched type is not an enum → error: `variant pattern requires an
  enum type; matched type is 'Int'`

A side effect of the rule: `case X` for a single-uppercase-letter
binding no longer means "bind a variable". Users write lowercase
bindings. Documented.

**8. Printing.**

`print Monday` prints the ordinal: `0`. The variant's name is a
compile-time property and is not available at runtime in v1.

This is a known limitation. A follow-up ADR can add name-based
printing via a trait (`impl Display for Day`), or via a compiler-owned
`print` special case. Neither is in scope here.

**9. Iteration.**

`for d in Day` is deferred. It requires `IntoIterator for Day`, a
trait machinery beyond v1. Users iterate manually via ordinals:

```algol26
var i := 0
while i <= 6
    val d := Day.from_ordinal(i)
    ...
    i := i + 1
```

Ugly but works. A follow-up ADR can add the trait-based form.

## Consequences

### Data model

New `Type` variant, plus `EnumTypeId` (defined in the same file as
`NominalTypeId`):

```rust
pub struct EnumTypeId(pub u32);
```

Match arms in `src/common/types.rs`:
- `Display` — print `name`
- `can_coerce_to` — `Enum{A} -> Enum{B}` iff `A.id == B.id`
- `can_cast_to` — allow `Enum <-> Int` as an explicit cast (the
  intrinsic wrap/unwrap lowers to this)
- `common_supertype` — same id required
- `substitute`, `contains_type_var`, `contains_unknown` — no-op on
  the enum itself
- `is_copy` — always `true` (enums are scalars)
- `is_numeric` — always `false`
- `is_hashable_key` — always `true` (enums are `Int`-sized)

### Frontend

New syntax in `src/frontend/parser/items.rs`:

```rust
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}
```

Added to `Program.enum_decls: Vec<EnumDecl>`. Threaded through
`ParsedProgram`, `AstPayload` alongside `distinct_decls`.

Pattern: new `Pattern::Variant(String)` variant in
`src/frontend/ast.rs`. Parser rule in `parser/pattern.rs`.

### Analyzer

New registry:

```rust
enum_types: HashMap<String, Type>,   // name -> Type::Enum { id, ... }
next_enum_id: u32,
```

Registered before nominal types (an enum field in a record or a
nominal type over an enum could arise in a later ADR; order is:
enums, then nominal types, then records).

`resolve_type_syntax` checks `enum_types` before `nominal_types`.

`check_pattern_type` extended: `Pattern::Variant` requires the
matched type to be an enum and the name to be a variant.

`bind_pattern_variables` extended: `Pattern::Variant` binds nothing.

`check_match_exhaustiveness` extended: for `Type::Enum`, enumerate
`variants` and require each to be covered by a `Pattern::Variant`
case, unless a wildcard or binding fallback exists.

`take_enum_types()` mirrors `take_nominal_types()`.

### IR

`SemanticIR` carries `Type::Enum` unchanged. The intrinsic wrap/unwrap
emits `TypedIRValue::Cast`, matching the nominal-types pattern.

`SemanticPattern::Variant(String)` added to the IR pattern enum.
The IR builder translates `Pattern::Variant` to it.

### Backends

`map_type` unwraps `Type::Enum` to `i64`:

```rust
Type::Enum { .. } => self.context.i64_type().into(),
```

LLVM `Cast` no-op arms: `(BasicValueEnum::IntValue(_), Type::Enum)`
and `(_, Type::Int)` when the source is an enum-typed int — same
handling as nominal types.

Interpreter `Cast` catch-all already returns the inner value; an
explicit `Type::Enum { .. }` arm is added for symmetry.

No new capability. All three backends support enums.

### Capability matrix

One row, `AllBackends`, `conformance_dir: Some("enum_types")`,
matching `nominal_types`.

### Diagnostics

Compile-time rejection of:
- duplicate variant names
- out-of-range literal `from_ordinal`
- `match` on enum missing a variant
- `Pattern::Variant` on non-enum
- `Pattern::Variant` with unknown name
- equality / comparison between different enums

Runtime error (interpreter and generated code):
- `from_ordinal` with out-of-range runtime value

### Tests

Conformance fixtures:
```
tests/conformance/valid/enum_types/
    basic.gol                 declare, use as param, from_ordinal/to_ordinal
    match_exhaustive.gol      match on enum, all variants covered
    map_key.gol               Map<Day, String>
tests/conformance/invalid/
    enum_duplicate_variant.gol
    enum_match_missing_variant.gol
    enum_from_ordinal_literal_oob.gol
    enum_cross_type_eq.gol
```

Plus the existing `nominal_*` shape of unit tests in the analyzer.

## Implementation order

```
A1. EnumTypeId + Type::Enum { id, name, variants }   (types.rs only)
A2. Parser: enum declaration, EnumDecl, thread through
A3. Analyzer: register + resolve enums
A4. Pattern::Variant parsing + analyzer resolution
A5. Match exhaustiveness for user enums
A6. Intrinsics from_ordinal / to_ordinal (analyzer + IR builder)
A7. Backend map_type arms (LLVM, WASM, interpreter)
A8. Feature doc, conformance fixtures, coverage matrix
```

Each step compiles, tests, and is revertable.

## Alternatives considered

**String-valued enums.** Rejected. Ordinal representation is what
makes subranges and sets possible later, and it is the Pascal-family
convention.

**Payload-carrying enums in v1.** Rejected. Adds a tagged-union
representation, LLVM struct types, pattern destructuring, and
considerable codegen. A separate ADR.

**Reuse `Pattern::Literal` for variants.** Rejected. Parsing a variant
as a literal expression forces the analyzer to re-classify, and
conflates two different things (a value compared at runtime vs. a
compile-time-known variant).

**Uppercase-first as a binding convention enforced only in the
analyzer.** Rejected. The parser is where the ambiguity is created;
resolving it there is cleaner than deferring to the analyzer.

**Implicit `Int <-> Enum` conversion.** Rejected. Same reasoning as
ADR 0029: the explicit conversion is what makes the type useful.

**`for d in Day` in v1.** Rejected. Requires trait machinery that is
out of scope.

**Printing by name.** Deferred. Ordinal printing is a documented v1
limitation.

## References

- ADR 0029 (Nominal Types) — the pattern this ADR mirrors
- ADR 0013 (Executable IR generic invariant) — identity discipline
- ADR 0027 (Map) — the key-type rule enums extend
- Free Pascal Reference Guide, "Enumerated types"

## See also

- `docs/features/enum_types.md` (to be written in A8)
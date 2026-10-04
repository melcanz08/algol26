# ADR 0032 — Set Types

## Status

Proposed. Not yet implemented.

> **Status note (2026-10-04).** Amended `Percentage` example from
> `Int in 0..100` to `Int in 0..63`. The original had 101 values,
> exceeding the 64-element ceiling the ADR itself specifies. The
> implementation in A1 (`Type::set_domain_size`) already enforces
> the 64-element limit; the example was inconsistent with the
> decision and is corrected here rather than in the code.

## Context

ADR 0029 gave ALGOL26 nominal types. ADR 0030 gave it ordinal
enumerations. ADR 0031 gave it subranges over ordinals. Together
they establish the notion of a *bounded ordinal domain*: a type
whose values are `0..N` for some small `N`, either as named
variants (`Day`) or as an interval (`Percentage` = `Int in
0..100`).

A set is the natural next construct: a subset of a bounded
ordinal domain. Pascal-family languages represent such sets as
compact bitsets — one bit per domain element — which is what
makes them a distinct abstraction from a general-purpose
`HashSet<T>`. `Set<Day>` over a seven-variant enum is seven bits;
`Set<WorkDay>` over `Monday..Friday` is five bits.

This ADR adds `Set<T>` as a first-class type. The domain
restriction is what makes the compact representation possible,
and it is enforced by the analyzer: only types whose domain is
at most 64 elements are valid set element types.

## Decision

Add set types:

```algol26
enum Day
    Monday
    Tuesday
    Wednesday
    Thursday
    Friday
    Saturday
    Sunday

type WorkDay Day in Monday..Friday
type Percentage Int in 0..63

val weekend: Set<Day> { Day.Saturday, Day.Sunday }
val weekdays: Set<WorkDay> { WorkDay(Monday), ... }
val small: Set<Percentage> { Percentage(5), Percentage(50) }
```

### Identity and representation

```rust
pub enum Type {
    // ...
    Set(Box<Type>),
}
```

Sets are *structural*, like `List<T>` and `Map<K, V>`. There is
no `SetTypeId`: `Set<Day>` is identified by the `EnumTypeId`
carried by its element type, and two `Set<Day>` in different
modules are the same type iff their `Day` is the same enum.

Runtime representation is a single `u64`. Bit `i` is set iff
domain element `i` is in the set. Domain elements are numbered:

- **Enum `Day`:** bit `i` is the variant with ordinal `i`.
  `Monday` is bit 0, `Sunday` is bit 6.
- **Subrange over Int `Int in L..H`:** bit `i` is the value `L + i`.
  For `Percentage = Int in 0..63`: bit 5 is `Percentage(5)`.
- **Subrange over enum `Day in A..B`:** bit `i` is the enum
  variant with ordinal `A + i`. For `WorkDay = Day in
  Monday..Friday`: bit 0 is `Monday`, bit 4 is `Friday`.
- **Bool:** bit 0 is `false`, bit 1 is `true`.

### The design questions

**1. Syntax for set literals.**

`Set<T> { e1, e2, ... }`, empty form `Set<T> {}`. Matches the
existing brace-literal shape for `Map<K, V> { ... }` and
`RecordName { ... }`. The `[1, 2, 3]` form is taken by lists and
is not reused.

Inferred form `Set { e1, e2, ... }` is not supported in v1: the
elements' type cannot always be inferred (see design question 5).
The type annotation is required.

**2. Element type constraint.**

Valid element types, with domain size:

- enum with ≤ 64 variants
- subrange over Int with `(high - low + 1) ≤ 64`
- subrange over an enum (inherits the enum's size, already ≤ 64)
- `Bool` (domain 2)

Invalid element types:

- `Int` (unbounded)
- `Float` (not ordinal)
- `String`
- `List<T>`, `Map<K, V>` (not ordinal)
- a record type
- `Set<T>` (set-of-sets: domain is `2^64`)
- a nominal type over Int (e.g. `UserId = distinct Int` — unbounded)

The analyzer rejects invalid element types with a message naming
the constraint: "`Int` cannot be a set element type: its domain
is unbounded; use a subrange or an enum".

**3. Operators.**

```algol26
d in s          // membership; d has element type, s has Set<T>
s1 + s2         // union
s1 - s2         // difference
s1 * s2         // intersection
s1 == s2        // set equality (bit-for-bit)
s1 != s2
s1 <= s2        // subset: every element of s1 is in s2
s1 <  s2        // strict subset
s1 >= s2        // superset
s1 >  s2        // strict superset
```

Both sides of a binary set operator must have the same element
type. `Set<Day> + Set<WorkDay>` is a type error even when the
two enum types are related by a subrange. Users extract via
`.to_base()` or reconstruct.

**4. Element expressions in set literals.**

Elements are expressions of the element type. For an enum type,
the ADR adds *qualified variant values*: `Day.Saturday` is an
expression of type `Day`. This is a new AST form (the parser
already produces `FieldAccess { object: Var("Day"), field:
"Saturday" }`; the analyzer reinterprets it when `Day` names an
enum).

Bare variants (`Saturday` without `Day.`) are deferred to a
separate ADR — they require expected-type-aware analysis of
identifier expressions, a change that touches more than just sets.

For a subrange over Int, elements are constructed with the
existing form: `Percentage(5)`, `Percentage(50)`.

For a subrange over an enum, elements are constructed with the
subrange constructor: `WorkDay(Day.Monday)`.

**5. Empty set and inference.**

`Set<Day> {}` is the empty set of `Day`. The element type must
come from the annotation, because there are no elements to infer
from. Same shape as `Map<String, Int> {}` (ADR 0027).

The `Set { ... }` inferred form is not supported in v1. The
rationale is design question 4: without an expected type, a bare
variant like `Saturday` is ambiguous (it could be a variable
name), and a set literal that silently disagrees with its
binding is worse than a required annotation.

**6. Assignment and parameter passing.**

`Set<T>` values are `Copy` — the representation is a single u64,
which is trivially duplicated. No ownership transfer.

**7. `Map<Set<T>, V>` and other containers of sets.**

`Set<T>` is a valid `Map` key iff the element type is valid as a
Map key. All valid set element types are Int-sized ordinals, so
`is_hashable_key(Set<T>)` is `true` for any well-formed `Set<T>`.

`List<Set<T>>` and `Map<K, Set<T>>` are valid.

**8. Printing.**

`print s` prints the ordinals of the members, comma-separated in
braces: `{0, 5, 6}`. The variant names are not available at
runtime in v1, matching ADR 0030's restriction on enum printing.

A follow-up ADR could add name-based printing via a trait or a
compiler intrinsic.

**9. Iteration.**

`for d in s` is deferred. It requires iterating set bits, which
is a natural extension of the existing `IteratorNext` terminator
but adds design questions (order of iteration, whether elements
are bound as ordinals or as the element type). Users iterate
manually via a `while` loop over ordinals:

```algol26
var i := 0
while i <= 6
    if Day.from_ordinal(i) in weekend
        print i
    i := i + 1
```

Ugly but works. Deferred to a follow-up ADR.

**10. Extracting members to a list.**

`s.to_list() -> List<T>` is deferred. Same reasoning as
iteration: the runtime representation is bits, not a list, and
materializing the list is a separate concern.

## Consequences

### Data model

New `Type` variant: `Set(Box<Type>)`. No `SetTypeId` — sets are
structural.

Match arms in `src/common/types.rs`:

- `Display` — `Set<T>`
- `can_coerce_to` — same element type required
- `common_supertype` — same element type required; else Unknown
- `substitute` — recurse into element
- `contains_type_var`, `contains_unknown` — recurse into element
- `is_copy` — always `true`
- `is_numeric` — always `false`
- `is_hashable_key` — always `true` (sets are u64)

### Frontend

New expression form: `ExprKind::SetLiteral { element_type:
TypeSyntax, elements: Vec<Expr>, span: Span }`. The parser
recognizes `Set<T> { ... }` following the same lookahead that
`Map<K, V> { ... }` uses (`looks_like_map_type_args` generalizes
to `looks_like_set_type_args`).

New AST node for enum variant values: `ExprKind::EnumVariant {
enum_name: String, variant: String, span: Span }`. Emitted by
the analyzer during the `FieldAccess` pass, not by the parser.

### Analyzer

No new registry — `Set<T>` is a structural type, resolved on
sight. The `TypeSyntax::Generic { name: "set", args: [element] }`
case in `resolve_type_syntax` resolves the element and validates
its domain size.

The `FieldAccess` arm gains an enum-variant branch: if the object
is a bare `Var(name)` where `name` is a registered enum, and
`field` names a variant of that enum, produce an `EnumVariant`
value.

The `Binary` arm gains arms for the new operators:

- `BinOp::In` — left is element type, right is `Set<T>`
- `BinOp::SetUnion`, `SetIntersection`, `SetDifference`
- `BinOp::Subset`, `StrictSubset`, `Superset`, `StrictSuperset`

`check_pattern_type` is unchanged — sets are not pattern-matched.

### IR

New value variant: `TypedIRValue::Set { bits: u64, element_type:
Type }`. Set literals compile to a constant u64 in the IR when
all elements are known; otherwise the IR builder emits a
sequence of `Instruction::SetInsert` operations.

New instructions: `SetInsert { target: TypedIRValue, element:
TypedIRValue }`. Set operations (`+`, `-`, `*`, `in`, subsets) are
lowered to `TypedIRValue::BinaryOp` with new `SemanticBinOp`
variants — they are pure functions of two u64s and don't need
dedicated instructions.

`SemanticPattern` gains no new variants.

### Backends

`map_type` unwraps `Type::Set(_)` to `i64`:

```rust
Type::Set(_) => self.context.i64_type().into(),
```

LLVM set operations lower to `and` / `or` / `andnot` / `icmp`:

```
s1 + s2   -> or    s1, s2
s1 * s2   -> and   s1, s2
s1 - s2   -> andnot s1, s2
d in s    -> icmp ne 0, (and s, shl(1, d.to_ordinal() - low))
s1 <= s2  -> icmp eq 0, (and s1, not s2)
s1 == s2  -> icmp eq s1, s2
```

Cast handlers get `Type::Set(_)` as a no-op arm, same as
`Distinct` / `Enum` / `Subrange`.

Interpreter: `RuntimeValue::Set(u64)`. All set operations are
direct bit operations.

### Capability matrix

One row, `AllBackends`, with a conformance directory. Sets lower
to a single u64 on every backend; no capability barrier.

### Diagnostics

Compile-time:

- element type has domain > 64
- element type is not ordinal (Int, Float, String, record, ...)
- `Set { }` inferred form is not supported; annotation required
- set operands have different element types
- element type in a literal disagrees with the declared element
- `d in s` where `d` is not of the set's element type

### Tests

    tests/conformance/valid/set_types/

Fixtures: basic literal and `in`, union/intersection/difference,
subset/superset, empty set, `Map<Set<T>, V>`, subrange element
types, enum element types.

## Implementation order

```
A1. Type::Set + match arms + domain-size check
A2. Parser: Set<T> { ... } literal, SetLiteral AST node
A3. Analyzer: resolve Set<T>, validate element domain
A4. Enum.Variant expression syntax (analyzer reinterprets FieldAccess)
A5. IR: TypedIRValue::Set, SemanticBinOp variants, IR lowering
A6. Backends: LLVM (bit ops), interpreter (u64), WASM (via IRCodeGen)
A7. Coverage matrix, feature doc, ADR status
```

Each step compiles, tests, and is revertable.

## Alternatives considered

**General `Set<T>` over any hashable type (HashSet).** Rejected.
That is a container, not a type-system feature; it belongs in
the same space as `Map<K, V>` and would be an ADR of its own,
not a Pascal-family progression step.

**Variable-width bitsets (u128, u256, or Vec<u64>).** Rejected
for v1. A single u64 covers 64-element domains, which is where
the compact-representation benefit is meaningful. Larger
domains are a natural follow-up ADR — the u64 representation
is a deliberate ceiling, not an architectural limit.

**`[1, 2, 3]` literal syntax.** Rejected. Lists own that syntax.

**`{1, 2, 3}` bare-brace literal.** Rejected. Ambiguous with
record literals and `Map` literals; requires type inference the
analyzer does not perform in that context.

**Set equality via a method rather than `==`.** Rejected. Set
equality is structural, total, and unambiguous; it belongs as an
operator, matching Pascal.

**`in` as a method (`s.contains(d)`).** Rejected. `in` reads
naturally, matches Pascal, and the parser can distinguish it
from the `for x in list` keyword use because the loop consumes
its own `in` token before the expression parser runs.

**Bare variant values (`Saturday`).** Deferred. Requires
expected-type-aware analysis of identifier expressions, which
touches every use of an identifier in every context. The
qualified form `Day.Saturday` is unambiguous and sufficient for
sets in v1.

## References

- ADR 0027 (Map) — the brace-literal pattern this ADR reuses
- ADR 0029 (Nominal Types) — the intrinsic and Cast discipline
- ADR 0030 (Enum Types) — sets over enums build on the ordinal
  representation
- ADR 0031 (Subrange Types) — sets over subranges build on the
  bounded interval representation
- Free Pascal Reference Guide, "Set types"

## See also

- `docs/features/set_types.md` (to be written in A7)
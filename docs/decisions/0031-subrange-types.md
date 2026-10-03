# ADR 0031 — Subrange Types

## Status

Accepted. A1-A7 implemented.

Subranges over Int and user enums work end-to-end on all three
backends. Literal bounds are checked at compile time; non-literal
bounds check at runtime via `Instruction::BoundsCheck`. Extraction
is `.to_base()` (parens and no-parens). Arithmetic on subranges is
deliberately rejected — see design question 6.

Deferred to a follow-up: trait-based checked arithmetic
(`Add` for a subrange returning `Result<T, RangeError>`), and a
`try_from_ordinal`-style non-panicking construction.

## Context

ADR 0029 gave ALGOL26 nominal types. ADR 0030 gave it ordinal
enumerations. Together they cover "this Int means a UserId" and
"this value is one of seven named days".

Neither expresses "this Int is between 0 and 100". That is a
*subrange*: a type whose value domain is an interval of an
ordinal type. The Pascal-family lineage treats subranges as a
first-class type constructor, and the domain restriction is what
makes them useful: a `Percentage` is not merely *an* Int, it is
an Int that cannot be 150.

This ADR introduces subrange types over `Int` and over user
enums. Subranges over `Float` (which is not ordinal) and over
`Bool` (two values, no useful range) are out of scope.

## Decision

Add subrange declarations:

```algol26
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
```

A subrange declaration names a type with a finite domain:
`low..high`, inclusive on both ends. The bounds are literal —
integer literals for an Int base, variant names for an enum base.

### Identity and representation

```rust
pub struct SubrangeTypeId(pub u32);

pub enum Type {
    // ...
    Subrange {
        id: SubrangeTypeId,  // identity — used for equality, mangling, lookup
        name: String,        // presentation only
        base: Box<Type>,     // Int or Enum
        low: i64,            // inclusive; ordinal if base is Enum
        high: i64,           // inclusive; ordinal if base is Enum
    },
}
```

Same identity discipline as `NominalTypeId` and `EnumTypeId`:
`id` is created once, in the analyzer, and propagated. No
consumer reconstructs it from `name`.

For an enum base, `low` and `high` are *ordinals*. The variant
names are recovered from `base` when diagnostics need them.
Display uses `name`.

Runtime representation is the base: an `Int` subrange is an
`i64`; an enum subrange is the enum's ordinal, also an `i64`.

### The design questions

**1. Syntax.**

`type Name Base in Low..High` — no `=` separator, matching ADR
0029's `type X distinct Y` and ADR 0030's `enum X ...`. The
keyword `in` is already reserved (`for x in list`); it reads
naturally as "an Int in the range 0..100".

`..` is the existing range operator. It appears here in *type*
position, which is a distinct parse context from expression
position; no grammar ambiguity arises.

Bases allowed: `Int`, user enum. Not `Float` (not ordinal), not
`Bool` (two values, no useful interval), not `String`, not a
nominal type, not another subrange.

**2. Construction.**

```algol26
Percentage(75)               // Int subrange, in range
Percentage(n)                // runtime check if n is not a literal
WorkDay(Day.from_ordinal(2)) // enum subrange, Wednesday -> ok
WorkDay(Day.from_ordinal(6)) // Sunday -> out of range
```

The constructor is `T(v)` where `v` has the base type. This
matches the shape of `Some(x)`, `Ok(x)`, and `Error(x)` —
UpperCamelCase constructor functions. The parser produces a
`FunctionCall` node; the analyzer intercepts before the ordinary
function dispatch, the same way it intercepts `T.from_base`
for nominal types (ADR 0029).

If a user declares a function named `Percentage`, the subrange
wins. Documented; types are UpperCamelCase by convention, so the
collision is unlikely.

**3. Extraction.**

```algol26
Percentage(75).to_base()     -> Int
WorkDay(d).to_base()         -> Day
```

Same name as ADR 0029's `to_base`, same no-op lowering. The
no-parens form `p.to_base` is also accepted.

**4. Literal coercion.**

An integer literal in range coerces to an Int subrange:

```algol26
val p: Percentage := 75      // ok, 75 is in 0..100
val q: Percentage := 150     // compile error: out of range
```

This is a narrow, single-valued coercion: it applies only when
the source expression is a literal `Int` and the expected type
is a subrange over Int. The rule lives in
`analyze_expr_with_context`, not in `can_coerce_to` — the
coercion check is value-dependent and `can_coerce_to` is
type-dependent only.

For enum subranges, no literal coercion exists. The base enum
has no bare-variant value syntax, so construction always goes
through `T(v)`.

**5. Equality and comparison.**

`==`, `!=`, `<`, `<=`, `>`, `>=` are allowed between two values
of the *same* subrange. Comparison is by base value.

This matches ADR 0030's enum rule and *differs* from ADR 0029's
nominal rule. Rationale: a subrange inherits a canonical total
order from its base — the domain is an interval, so "less than"
is unambiguous. A nominal type does not (two `UserId`s are
Ints, but sorting user IDs is not inherently meaningful).

Cross-subrange comparison is a type error:

```algol26
val p: Percentage := Percentage(50)
val m: Month := Month(6)
p == m   // error: Percentage and Month have different ids
```

**6. Operators.**

Arithmetic (`+`, `-`, `*`, `/`) is not defined on subranges.
Users write `p.to_base() + q.to_base()` if they want integer
arithmetic, and construct the result explicitly.

Rationale: a Percentage plus a Percentage is not necessarily a
Percentage (50 + 60 = 110). Three ways to resolve:

- widen to `Int` (percentage + percentage produces an Int)
- clamp or wrap (surprising)
- panic on out-of-range result (also surprising)

None is clearly right, so v1 refuses all arithmetic. A follow-up
ADR can add a trait-based `Add` implementation for types where
the user wants checked arithmetic.

**7. Bounds checking.**

Construction with a literal argument is checked at compile time:

```algol26
Percentage(150)              // compile error, literal out of range
```

Construction with a non-literal argument inserts a runtime
bounds check:

```algol26
val n := read_int()          // whatever runtime returns
val p := Percentage(n)       // runtime check: if n < 0 || n > 100,
                             // print an error and exit(1)
```

A new IR instruction carries the check:

```rust
Instruction::BoundsCheck {
    value: TypedIRValue,
    low: i64,
    high: i64,
    message: String,   // e.g. "Percentage: 150 is out of range 0..100"
}
```

The interpreter evaluates it directly. LLVM emits an `icmp`
against both bounds, a conditional branch to an error block,
and `printf` + `exit(1)` in the error block — same shape as
the existing array out-of-bounds check in
`llvm_codegen/instruction.rs`. WASM reuses `IRCodeGen`, so the
LLVM lowering covers it.

**8. Interaction with other types.**

Subranges are:

- `Copy` — same as their base (`Int` is Copy; enums are Copy)
- `is_hashable_key` — same as their base (Int and enums are both keys)
- `is_numeric` — `false`. Same reasoning as nominal types: even
  though `Percentage` lowers to `i64`, arithmetic is not defined
  and treating it as numeric would reopen the `p + q` question

A subrange's `Type::Subrange` does not appear inside another
subrange; bases are restricted to `Int` and user enums.

## Consequences

### Data model

New `Type` variant (above), plus `SubrangeTypeId`. Match arms in
`src/common/types.rs`:

- `Display` — print `name`
- `can_coerce_to` — same id required; no widening to base
- `can_cast_to` — `Subrange <-> base` allowed as an explicit
  cast (the intrinsic wrap/unwrap lowers to this); the
  conversion preserves identity since it is a no-op at runtime
- `common_supertype` — same id required; else `Unknown`
- `substitute`, `contains_type_var`, `contains_unknown` — recurse
  into `base`
- `is_copy` — `base.is_copy()`
- `is_numeric` — `false`
- `is_hashable_key` — `base.is_hashable_key()`

### Frontend

New syntax in `src/frontend/parser/items.rs`:

```rust
pub struct SubrangeDecl {
    pub name: String,
    pub base: TypeSyntax,
    pub low: Expr,
    pub high: Expr,
    pub span: Span,
}
```

Added to `Program.subrange_decls: Vec<SubrangeDecl>`, threaded
through `ParsedProgram` and `AstPayload` alongside
`distinct_decls` and `enum_decls`.

The parser reads `type Name Base in Low..High` and produces the
decl. `Low` and `High` are parsed as expressions; the analyzer
validates their shape (Int literals for Int bases, variant-name
`ExprKind::Var` for enum bases).

### Analyzer

New registry:

```rust
subrange_types: HashMap<String, Type>,  // name -> Type::Subrange
next_subrange_id: u32,
```

Registered after enums and nominal types (a subrange base may
name an enum; a subrange is never a base for another type in v1).

`resolve_type_syntax` checks `subrange_types` after `enum_types`
and `nominal_types`.

Construction (`T(v)`) intercepts in `analyze_expr_inner`'s
`ExprKind::FunctionCall` arm, before the nominal `from_base`
intercept. Same shape: check arity, analyze the argument against
the base type, produce the subrange type.

Literal coercion for `val p: Percentage := 75` is handled in
`analyze_expr_with_context`: if the expected type is a subrange
over Int and the expression is a literal `Int(n)` in range,
return the subrange type.

### IR

`Instruction::BoundsCheck` added. The IR builder emits it before
the `Cast` when translating `T(v)`. For a literal argument,
constant propagation can elide the check.

`SemanticIR` carries `Type::Subrange` unchanged. `from_base` and
`to_base` intrinsics lower to no-op `Cast`, matching ADR 0029.

### Backends

`map_type` unwraps `Type::Subrange` to its base:

```rust
Type::Subrange { base, .. } => self.map_type(base),
```

LLVM codegen gains an arm for `BoundsCheck`:

```
value in [low, high]  ? continue_block : error_block
error_block: printf("<message>\n"); exit(1)
```

Same shape as the array out-of-bounds check in the ArrayAssign
and ArrayAccess paths.

Interpreter evaluates `BoundsCheck` directly: if out of range,
return `EvalError::Runtime(message)`.

Cast handlers in all three backends get `Type::Subrange` as a
no-op arm, same as `Type::Distinct` and `Type::Enum`.

### Capability matrix

One row, `AllBackends`, no conformance directory (the harness
does not support per-fixture backend selection; same shape as
nominal_types).

### Diagnostics

Compile-time:

- literal out of range in `T(v)`
- literal out of range in coercion position
- base is not Int or enum
- `low > high`
- non-literal bounds (`type X Int in a..b` where a or b is a variable)
- argument to `T(v)` has the wrong type

Runtime (interpreter and codegen):

- `T(v)` with v out of range

### Tests

No conformance directory; unit tests in `src/semantics/analyzer/tests.rs`.

## Implementation order

```
A1. SubrangeTypeId + Type::Subrange { id, name, base, low, high }
A2. Parser: type X Base in Low..High
A3. Analyzer: register + resolve + literal coercion
A4. Construction (T(v)) intercept, compile-time bounds check
A5. Instruction::BoundsCheck: IR + interpreter
A6. LLVM codegen for BoundsCheck
A7. Feature doc, matrix row, ADR status
```

Each step compiles, tests, and is revertable.

## Alternatives considered

**Widen subranges to their base for arithmetic.** Rejected per
design question 6. Even `p + q` producing an `Int` loses the
bound information silently, which is exactly what the type
existed to preserve.

**Overflow-checked arithmetic returning `Result<T, RangeError>`.**
Attractive, but adds `Result` handling to every arithmetic
expression on a subrange. Deferred to a follow-up ADR.

**Clamping.** Rejected. Silent value mangling is the failure
mode the type exists to prevent.

**Compile-time-only bounds, no runtime check.** Rejected. That
would make `Percentage(read_int())` impossible to express, which
removes the interesting case. ADR 0031 takes the outsider's
position: the invariant is part of the language's runtime
semantics, not an optional debug switch.

**Allow `Float` bases.** Rejected. Ranges over floats are not
ordinal; there is no `low..high` that means anything without
precise semantics for `Percentage(0.5 + 0.5)`.

**Allow subranges over subranges.** Rejected. Composition would
require reasoning about intersecting bounds and gives no clear
benefit; users can write the intersection explicitly.

**Rename `to_base` to `to_int` for Int subranges.** Rejected.
`to_base` matches ADR 0029's naming, and one name for the
operation regardless of base is simpler than two.

**`T.from_base(v)` instead of `T(v)`.** Considered for
consistency with ADR 0029. Rejected in favour of `T(v)` because
subrange construction is common enough to deserve the short form,
and `T(v)` matches the shape of `Some(x)` / `Ok(x)` / `Error(x)`
constructors.

**Bare enum-variant value syntax** (`Monday` as a value, resolved
by expected type). Rejected for v1. It is a separate design with
its own ambiguity rules; enum subranges are constructible via
`WorkDay(Day.from_ordinal(2))`, which is verbose but unambiguous.

## References

- ADR 0029 (Nominal Types) — the identity discipline and
  `from_base` / `to_base` shape this ADR reuses
- ADR 0030 (Enum Types) — subranges over enums build on the
  ordinal representation
- ADR 0027 (Map) — the key rule subranges inherit from their base
- Free Pascal Reference Guide, "Subrange types"

## See also

- `docs/features/subrange_types.md` (to be written in A7)
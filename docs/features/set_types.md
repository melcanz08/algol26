# Feature: Set Types

> Per-feature contract, following the pattern described in
> `docs/architecture-direction.md`. This file is the authoritative
> answer to "what is `Set<T>` in ALGOL26, and where does it live?"

## Summary

`Set<T>` is a compact bitset over a bounded ordinal domain, in the
Pascal tradition. The element type `T` must have a domain of at
most 64 values; the runtime representation is a single `u64`. See
ADR 0032.

## Syntax

Type annotation:

```gol
val s: Set<Day> := ...
```

Literal:

```gol
Set<Day> { Day.Saturday, Day.Sunday }   // non-empty
Set<Day> {}                              // empty
```

The inferred form `Set { ... }` is **not** supported: without an
expected type, a bare variant like `Saturday` is ambiguous.
Annotations are required.

## Element type constraint

Legal element types (with domain size):

| Type | Domain |
|---|---|
| `Bool` | 2 |
| `Enum` with ≤ 64 variants | `variants.len()` |
| `Subrange` over `Int` with ≤ 64 values | `high - low + 1` |
| `Subrange` over an enum | inherits the enum's size |

Illegal element types (rejected at declaration):

- `Int` — unbounded
- `Float` — not ordinal
- `String`, `Ptr`, `Void` — not ordinal
- `Distinct` (nominal) — nominal identity, even over an ordinal base
- `List`, `Map`, `Option`, `Result`, `Record`, `Array`, `Tuple`,
  `Channel` — not ordinal
- `Set<T>` — domain would be 2^64
- `Subrange` with more than 64 values

The analyzer's diagnostic names the constraint.

## Operators

| Syntax | Meaning | Result |
|---|---|---|
| `d in s` | membership | `Bool` |
| `s1 + s2` | union | `Set<T>` |
| `s1 - s2` | difference | `Set<T>` |
| `s1 * s2` | intersection | `Set<T>` |
| `s1 == s2` | equality | `Bool` |
| `s1 != s2` | inequality | `Bool` |
| `s1 <= s2` | subset | `Bool` |
| `s1 < s2` | strict subset | `Bool` |
| `s1 >= s2` | superset | `Bool` |
| `s1 > s2` | strict superset | `Bool` |

Both operands of a binary set operator must be `Set<T>` with the
same `T`. `Set<Day> + Set<WorkDay>` is a type error even when the
two enum types are related by subrange.

## Representation

A `Set<T>` value is a single `u64`. Bit `i` is set iff domain
element `i` is a member:

- **Enum `Day`:** bit `i` is the variant with ordinal `i`.
  `Day.Monday` is bit 0.
- **Subrange `Int in L..H`:** bit `i` is the value `L + i`.
  `Byte(15)` where `Byte = Int in 10..20` is bit 5.
- **Subrange over enum:** bit `i` is the variant with ordinal `L + i`.
- **`Bool`:** bit 0 is `false`, bit 1 is `true`.

## IR

`TypedIRValue::Set { bits: u64, element_type: Type }` — a constant
set value. Set operations lower to `SemanticBinOp` variants
(`SetUnion`, `SetIntersection`, `SetDifference`, `SetMember`,
`SetSubset`, `SetStrictSubset`, `SetSuperset`, `SetStrictSuperset`).
Equality reuses `Equal` / `NotEqual` — `u64` bit-equality is
correct for sets.

## Backends

| Backend | Literal | Operators | Notes |
|---|---|---|---|
| Interpreter | ✅ | ✅ | Runtime value is `Int(u64 bits)`. |
| LLVM | ✅ | ✅ | Bit ops on `i64`; membership is `(1 << d) & s != 0`. |
| WASM | ✅ | ✅ | Inherits LLVM's `IRCodeGen` path. |

## Capability

`AllBackends`. Sets lower to a single `u64` on every backend; no
capability barrier.

## Non-constant elements

Set literal elements may be constant (`Day.Saturday`, `Byte(15)`,
`true`/`false`) or runtime values (a variable, a function call
result). Constants fold into a base `u64` bitmask at IR-build time.
Non-constant elements each become a `TypedIRValue::SetSingleton`,
and the whole literal lowers to a `SetUnion` chain over the
constants and singletons.

At runtime, a `SetSingleton` computes `1 << (ordinal - low)` for
its element, where `low` is the subrange offset (0 for enums and
`Bool`). All three backends support this path.

A function whose *return type* is an enum is a separate
limitation: the LLVM and WASM backends currently refuse such
functions at the capability check, before any set work runs. The
interpreter handles them. See `docs/features/enum_types.md`.

## Iteration

`for d in s` is deferred. Users iterate manually via a `while`
loop over ordinals:

```gol
var i := 0
while i <= 6
    if Day.from_ordinal(i) in weekend
        print i
    i := i + 1
```

## Printing

`print s` prints the u64 as an integer, not as a set literal. A
name-aware printing path is a follow-up.

## Tests

- `tests/conformance/valid/set_types/` — end-to-end fixtures
- `src/common/types.rs` unit tests — `set_domain_size`, `is_ordinal`
- `src/semantics/analyzer/tests.rs` — analyzer acceptance/rejection

## See also

- ADR 0032 (Set Types) — the design
- `docs/decisions/0030-enum-types.md` — ordinal representation
- `docs/decisions/0031-subrange-types.md` — subrange as ordinal
- `docs/decisions/0029-nominal-types.md` — why `Distinct` is
  excluded
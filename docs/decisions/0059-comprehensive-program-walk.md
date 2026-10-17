# ADR 0059 — Comprehensive Program Walk

Status: Accepted

## Context

`examples/library.gol` is the first comprehensive program written
in ALGOL26. It exercises records, traits, generics, unions,
pattern matching, regions, references, lists, maps, and
Result/Option together. Writing it surfaced three rules that no
earlier ADR had needed to state because earlier tests didn't
combine enough features.

## Decision

Three rules, each surfaced by writing `library.gol`:

### Rule 1 — Dotted-name method dispatch resolves the receiver path as a place chain

For a method call `a.b.c.method(...)`, the receiver path
`a.b.c` is resolved as a place chain (base variable `a`, field
projections `b` and `c`), and the **last** dotted segment
(`method`) is the method name. The receiver type is the
resolved type of the place chain, not just `a`'s type.

This applies to the analyzer's dotted-name dispatch and to
the IR builder's corresponding arm. Both must implement the
same place-chain walk.

### Rule 2 — Type construction from `TypeSyntax` must go through `resolve_type_syntax`, not `to_type`

`TypeSyntax::to_type` is a pure syntax-to-type function with no
access to the record, union, enum, or nominal tables. It maps
`Book` to `Type::Unknown` because it doesn't know the
declaration exists.

`resolve_type_syntax` performs the same syntactic resolution
but consults the analyzer's type tables. It maps `Book` to
`Type::record("Book", [])`.

**Every place in the analyzer and IR builder that constructs a
`Type` from a `TypeSyntax` must call `resolve_type_syntax`.**
Using `to_type` anywhere that a user-declared type could appear
silently produces `Type::Unknown` and fails downstream.

### Rule 3 — Mutating container methods accept any place as the receiver

`Map.insert` and `List.append` are mutating methods. Their
receiver must be a **place** — a variable, a field access, an
array element, or a deref — not a temporary.

The analyzer and interpreter both resolve the receiver as a
`Place`. `m.insert(k, v)`, `self.members.insert(k, v)`, and
`arr[0].append(v)` are all valid; `f().insert(k, v)` is rejected.

## Consequences

**Positive:**

- The three rules are now explicit. Any future method tier or
  type-constructing site can consult this ADR.
- `examples/library.gol` is the reference for a program that
  exercises the full language.

**Negative:**

- The two dispatch sites (analyzer and IR builder) must be kept
  in sync for every method tier. This is a maintenance hazard:
  adding a new method tier requires editing both. A future ADR
  may merge them.

## See also

- ADR 0027 — Map<K, V>
- ADR 0028 — List.append
- ADR 0033 — Methods and receivers
- ADR 0038 — Dynamic dispatch
- ADR 0041 — Associated types
- ADR 0055 — Sum types

## References

- `examples/library.gol` — the comprehensive program
- `src/semantics/analyzer/expr.rs` — analyzer dispatch
- `src/semantics/builder/expr.rs` — IR builder dispatch

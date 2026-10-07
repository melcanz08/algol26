# Where-clauses on impls

An impl may put bounds on its own type parameters:

```
impl<T> Sortable for List<T> where T: Ordered
    function sort_me(self: &List<T>) -> String
        return "sorted"
```

The bound is checked at method-resolution time. When the analyzer
resolves `xs.sort_me()` for `xs: List<Foo>`, it unifies the impl's
declared self (`List<T>`) against the receiver, then checks each
`where T: Trait` clause against the resulting binding (`T = Foo`).
If `Foo` does not implement `Ordered`, the impl does not apply and
the call is rejected with `E0002`.

See ADR 0025 for the trait bound-satisfaction model that this
builds on.

## Syntax

- `impl<T1, T2, ...> Trait for Type<T1, T2, ...> where T1: A, T2: B`
- Inherent impls (`impl<T> Foo<T> where T: Bar`) parse the same way.

## Semantics

- **Per-call check.** Bounds are not checked at impl declaration;
  they are checked when a method call resolves to that impl.
- **Symbolic bindings skip.** A call inside another generic body
  has a receiver whose type args are still `TypeVar` — the check
  is deferred to the outer instantiation.
- **Ambiguity.** If two impls match the receiver but only one
  satisfies its where-clauses, the other is filtered out before
  ambiguity is reported.
- **Non-generic impls** ignore this path — no type parameters,
  nothing to bind, nothing to check.

## Not yet supported

- Where-clauses on inherent impls are parsed but not enforced
  (no generic method dispatch path to hook into yet).
- Associated-type projections in where-clauses.

## Diagnostics

| Code | Trigger |
|------|---------|
| E0002 | Receiver's concrete type args do not satisfy an impl's `where` clause. |

## Tests

- `tests/conformance/valid/impl_where/impl_where.gol`
- `tests/conformance/invalid/impl_where_violation.gol`
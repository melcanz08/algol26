# Feature: Option<T>

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is `Option<T>` in ALGOL26, and where does it live?"

## Summary

`Option<T>` is ALGOL26's nullable-value type. It has exactly two variants:

- `Some(x)` — a value of type `T`
- `None`     — the absence of a value

There is no implicit null. A value of type `T` is always present; a value that
may be absent must be typed `Option<T>`.

## Syntax

Construction:

```gol
val present := Some(42)
val absent  := None
```

The type of `None` is inferred from context. Where the context does not
determine it, an explicit type annotation is required:

```gol
val x: Option<Int> := None
```

Pattern matching:

```gol
match x
    Some(v) -> print(v)
    None    -> print("absent")
```

## Typing rules

| Expression | Type |
|---|---|
| `Some(x)` where `x: T` | `Option<T>` |
| `None` in a context requiring `Option<T>` | `Option<T>` |

Type coercion is covariant: `Option<Int>` coerces to `Option<Float>`.
Subtyping follows the inner type.

Type-level unification treats `Option<T>` as a distinct type; there is no
implicit flattening. `Option<Option<T>>` is legal and distinct from `Option<T>`.

Location in the type system: `src/common/types.rs`, variant `Type::Option(Box<Type>)`.
Constructor: `Type::option(inner)`.

Parsing: `Type::from_str` accepts both `Option<T>` and `option[T]` syntax.

## Ownership

The payload of `Some(x)` follows normal ownership rules:

- If `T` is `Copy` (`Int`, `Float`, `Bool`, `Ptr`), constructing and matching
  `Some(x)` copies the value; the original remains available.
- If `T` is not `Copy` (`String`, `List`, user types), `Some(x)` **moves** `x`
  into the option. `x` is not usable afterward.

Matching does not by itself move the payload unless a binding pattern is used:

```gol
val a := Some("hello")
match a
    Some(s) -> print(s)   // s is bound by move; a is now partially moved
    None    -> print("no")
// after this match, `a` is not usable if the Some arm executed
```

This is enforced by the semantic analyzer's move/borrow tracking. Diagnostics
use the same codes as any other move error (`E-MOVE-001`, `E-MOVE-002`).

## IR representation

In `src/ir/semantic_ir.rs`:

| Concept | Variant |
|---|---|
| `Some(x)` value | `TypedIRValue::Some(Box<TypedIRValue>)` |
| `None` value | `TypedIRValue::None { option_type: Type }` |
| `Some(x)` pattern | `SemanticPattern::Some { binding: String }` |
| `None` pattern | `SemanticPattern::None` |

`Some` carries its payload as a boxed `TypedIRValue`. `None` carries the
full `Option<T>` type so that a bare `None` is still typed unambiguously
downstream.

Pattern matching lowers to `Terminator::Switch` with a case per pattern. The
binding introduced by `Some(binding)` is scoped to the target block only, and
is not visible in sibling branches.

### IR verifier rules

The verifier enforces:

- `TypedIRValue::Some(v)` is well-typed iff `v` is well-typed and the claimed
  result type is `Option<typeof(v)>`.
- `TypedIRValue::None { option_type }` requires `option_type` to be an
  `Option<_>`; a bare `None` with a non-Option type is rejected.
- Pattern bindings from `Some`/`None` are introduced only in the target
  block of their `Switch` case.

## Pattern matching

The semantic analyzer enforces exhaustiveness for `Option` matches:

- A match that omits `None` is rejected.
- A match that omits `Some(_)` is rejected.
- A wildcard arm (`_`) satisfies exhaustiveness.

Tests confirming this: `test_match_option_missing_none_rejected`,
`test_match_option_with_both_arms_accepted`,
`test_match_option_with_wildcard_accepted` (in `src/semantics/analyzer/`).

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported | `interpreter_accepts_option_values` in `src/backends/capabilities/tests.rs` |
| LLVM | **Unsupported** | `llvm_rejects_option_values` in `src/backends/capabilities/tests.rs` |
| WASM | **Unverified** | No test currently pins this. Adding `wasm_rejects_option_values` would close the gap. |

The interpreter represents `Option<T>` as
`RuntimeValue::Option(Option<Box<RuntimeValue>>)` in
`src/backends/interpreter/runtime.rs`.

`runtime_eq` structural comparison handles `Option` by comparing payloads
when both sides are `Some`, and considering both `None` equal.

## Optimizer rules

None implemented yet. Candidates for future work:

- `match Some(x) with | Some(v) -> v | None -> unreachable` folds to `x`
  (pattern-known-Some elimination).
- Constant folding of `Some(constant)` into an eagerly-typed constant.

Every optimizer rule must be conservative: no rule may fire on
`Option<T>` where the payload has side effects.

## Safety

- No implicit null: a value of type `T` is never actually `None`.
- Exhaustiveness is enforced at analysis time, so no runtime check for
  "did we forget a case?" is emitted.
- Move semantics of the payload are enforced by the ownership analyzer;
  using a payload after a `Some(binding)` match is a compile error, not
  a runtime panic.

## Test coverage

Current coverage across the tree:

- `tests/conformance/valid/option_match.gol`
- `examples/ownership/safety/option.gol`
- `src/common/types.rs`: `test_type_parsing` covers `option<float>` parse
- `src/backends/capabilities/tests.rs`: interpreter/LLVM accept/reject
- `src/semantics/analyzer/`: exhaustiveness tests listed above

### Gaps

- No per-backend expected-output fixture under `tests/conformance/`.
- No differential test that runs the same `Option`-using program through
  interpreter and (hypothetically) LLVM, because LLVM support does not exist.
- No test for `Option<Option<T>>`.
- No test for covariant coercion (`Option<Int>` used where `Option<Float>`
  is required).

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Option<T>
    semantics:   Stable
    parsed:      yes
    typed:       yes
    validated:   yes
    IR:          yes
    verified:    yes
    interpreter: supported
    LLVM:        unsupported
    WASM:        unverified
    optimized:   no rules
```

Declared stable on the interpreter backend. Attempting to compile an
`Option`-using program to LLVM correctly produces a capability error via
`src/backends/capabilities/scan.rs`.

## Checklist for related features

If you are adding a feature *like* `Option<T>` (a payload-carrying enum with
pattern matching), you need to touch:

1. `src/common/types.rs` — new `Type` variant + constructor + parsing +
   `can_coerce_to` + `common_supertype` + `contains_type_var` + `substitute`
   + `inner_type` + `Display`.
2. `src/ir/semantic_ir.rs` — new `TypedIRValue` variant(s) + new
   `SemanticPattern` variant(s).
3. `src/ir/verifier/` — rules for the new variants in `value.rs` and
   `terminator.rs`.
4. `src/semantics/analyzer/` — exhaustiveness rules for the new patterns.
5. `src/backends/interpreter/runtime.rs` — a `RuntimeValue` variant.
6. `src/backends/interpreter/eval.rs` — evaluation.
7. `src/backends/interpreter/pattern.rs` — matching.
8. `src/backends/interpreter/runtime.rs` — extend `runtime_eq`.
9. `src/backends/capabilities/scan.rs` — declare which backends support it.
10. `src/backends/capabilities/tests.rs` — one accept test per supporting
    backend, one reject test per non-supporting backend.
11. `tests/conformance/valid/<feature>.gol` — the program fixture.
12. `docs/features/<feature>.md` — this file.

That is the checklist. It is long, but it is finite, and every step is
pinned to a specific file path. Compare this list to the fifteen-subsystem
treasure hunt described in `docs/architecture-direction.md` — the goal is to
shrink this list over time, not to make it disappear.

## Open questions

- Should `None` require explicit type annotation in all contexts, or infer
  from the nearest expected type? Current behavior: infer when possible,
  error when not. This matches Rust.
- Should there be a `?.`-style optional chaining operator? Not currently.
  Would be a new feature contract.
- Should `Option<T>` where `T: Copy` support implicit unwrapping? No —
  explicit match or helper functions only, to keep semantics uniform.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/decisions/0003-type-system.md`
- `docs/decisions/0005-ownership-model.md`
- `src/common/types.rs`
- `src/ir/semantic_ir.rs`
- `tests/conformance/valid/option_match.gol`
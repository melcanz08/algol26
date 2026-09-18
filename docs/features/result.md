# Feature: Result<T, E>

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is `Result<T, E>` in ALGOL26, and where does it live?"

## Summary

`Result<T, E>` is ALGOL26's error-propagation type. It has exactly two variants:

- `Ok(x)`    — success, carrying a value of type `T`
- `Error(e)` — failure, carrying a value of type `E`

Unlike exceptions, `Result` is a plain value. A function that can fail
returns `Result<T, E>`; the caller must unwrap it explicitly through
`match` or `try/catch`.

## Syntax

Construction:

```gol
val ok  := Ok(42)
val err := Error("bad input")
```

Type annotation:

```gol
val r: Result<Int, String> := Ok(42)
```

Pattern matching:

```gol
match r
    Ok(v)  -> print(v)
    Error(e) -> print(e)
```

Try/catch (shorthand for the same match, see `tests/ir/try_catch_test.rs`):

```gol
try
    val v := may_fail()
    print(v)
catch e
    print(e)
```

## Typing rules

| Expression | Type |
|---|---|
| `Ok(x)` where `x: T` | `Result<T, E>` for any inferred `E` |
| `Error(e)` where `e: E` | `Result<T, E>` for any inferred `T` |

Type coercion is covariant in both parameters: `Result<Int, String>`
coerces to `Result<Float, String>` (success side widens) and to
`Result<Int, AnyError>` (error side widens) if such a coercion exists.
Current implementation: covariance follows the same rules as `Option<T>`
for the `ok` side; the `error` side is invariant in practice because no
error supertype is defined yet.

Location in the type system: `src/common/types.rs`, variant
`Type::Result { ok: Box<Type>, error: Box<Type> }`.
Constructor: `Type::result(ok_type, error_type)`.

Parsing: `Type::from_str` accepts `Result<OkType, ErrorType>` syntax.
Note: parsing uses `s_trimmed` slicing with a fixed offset of 7, which
happens to work because `Result<` and `result<` have the same length.
This is fragile — see Tier 8.2 in the roadmap for the planned
`from_str` refactor.

## Ownership

The payload of `Ok(x)` and `Error(e)` follows normal ownership rules:

- If `T` (or `E`) is `Copy` (`Int`, `Float`, `Bool`, `Ptr`), constructing
  and matching copies the value.
- Otherwise, constructing the variant **moves** the payload in, and
  matching with a binding pattern moves it out.

Same analysis as `Option<T>` — the semantic analyzer tracks moves and
borrows through `Ok`/`Error` construction and pattern binding.

## IR representation

In `src/ir/semantic_ir.rs`:

| Concept | Variant |
|---|---|
| `Ok(x)` value | `TypedIRValue::Ok { value: Box<TypedIRValue>, result_type: Type }` |
| `Error(e)` value | `TypedIRValue::Error { value: Box<TypedIRValue>, result_type: Type }` |
| `Ok(x)` pattern | `SemanticPattern::Ok { binding: String }` |
| `Error(e)` pattern | `SemanticPattern::Error { binding: String }` |

Unlike `Option`, both variants carry the **full** `Result<T, E>` type in
`result_type`, not just the payload type. This keeps the error side
unambiguous when only `Ok` is visible in the source.

The `type_of()` method returns `result_type` for both variants, so
downstream passes see the declared `Result<T, E>` rather than a bare
`T` or `E`.

Pattern matching lowers to `Terminator::Switch` with one case per
variant. The `try/catch` construct lowers to the same IR shape — a
`Switch` on the result value with a synthetic `Error` case that jumps to
the catch block. See `tests/ir/try_catch_test.rs` for the OK and error
path tests.

### IR verifier rules

The verifier enforces:

- `TypedIRValue::Ok { value, result_type }` requires `value` to be
  well-typed and `result_type` to be `Result<typeof(value), _>`.
- `TypedIRValue::Error { value, result_type }` requires `value` to be
  well-typed and `result_type` to be `Result<_, typeof(value)>`.
- Pattern bindings from `Ok`/`Error` are introduced only in the target
  block of their `Switch` case.

## Pattern matching

The semantic analyzer enforces exhaustiveness for `Result` matches:

- A match that omits `Error(_)` is rejected.
- A match that omits `Ok(_)` is rejected.
- A wildcard arm (`_`) satisfies exhaustiveness.

Test confirming this: `test_match_result_missing_error_rejected` in
`src/semantics/analyzer/`.

The `try/catch` construct is sugar for a match that binds the error to a
name and runs the catch block for the error path. It does not skip the
exhaustiveness requirement — a `try` without `catch` is a syntax error.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported | `interpreter_accepts_result_values` in `src/backends/capabilities/tests.rs` |
| LLVM | **Unsupported** | `llvm_rejects_result_values` in `src/backends/capabilities/tests.rs` |
| WASM | **Unsupported** | `wasm_rejects_result_values` in `src/backends/capabilities/tests.rs` |

The interpreter represents `Result<T, E>` as
`RuntimeValue::Result { is_ok: bool, value: Box<RuntimeValue> }` in
`src/backends/interpreter/runtime.rs`.

`runtime_eq` structural comparison handles `Result` by comparing
`is_ok` flags and then comparing payloads.

Both the LLVM and WASM capability checks fail closed: a program using
`Result<T, E>` is rejected with a capability diagnostic before the
backend's lowering code runs. This is the correct fail-closed behavior —
the alternative (silently lowering `Ok(x)` to `x` and dropping the error
side) would produce wrong results with no diagnostic.

## Optimizer rules

None implemented yet. Candidates for future work:

- `match Ok(x) with | Ok(v) -> v | Error(_) -> unreachable` folds to `x`
  (pattern-known-Ok elimination).
- Dead catch-block elimination when the try block cannot fail.

Every optimizer rule must be conservative: no rule may fire on
`Result<T, E>` where the payload has side effects, and no rule may
remove an `Error` branch if the inner expression could produce one.

## Safety

- No implicit unwrapping: `Result<T, E>` cannot be used where `T` is
  expected. Extraction requires `match` or `try/catch`.
- Exhaustiveness is enforced at analysis time, so the error case cannot
  be silently ignored.
- No panic: the language has no `.unwrap()`. A caller that wants to
  crash must do so explicitly via a builtin that exits.
- Move semantics of the payload are enforced by the ownership analyzer;
  using a payload after an `Ok(binding)` match is a compile error, not
  a runtime panic.

## Test coverage

Current coverage across the tree:

- `tests/corpus/corpus_14_try_catch.gol` — language-level try/catch
- `tests/ir/try_catch_test.rs` — `test_try_catch_ok_path`, `test_try_catch_error_path`
- `tests/conformance/valid/result_try.gol`
- `src/common/types.rs`: `test_type_parsing` covers `Result<Int, String>` parse
- `src/common/types.rs`: `test_display` covers `Result<Int, String>` formatting
- `src/backends/capabilities/tests.rs`: interpreter/LLVM/WASM accept/reject
- `src/semantics/analyzer/`: `test_match_result_missing_error_rejected`

### Gaps

- No per-backend expected-output fixture under `tests/conformance/`.
- No differential test that runs the same `Result`-using program through
  interpreter and any other backend, because no other backend supports it.
- No test for `Result<Result<T, E1>, E2>` (nested results).
- No test for error-side covariance.
- No test for `try/catch` where the error payload is a non-`Copy` type
  (`Error(String)` matched to a moved binding).

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Result<T, E>
    semantics:   Stable
    parsed:      yes
    typed:       yes
    validated:   yes
    IR:          yes
    verified:    yes
    interpreter: supported
    LLVM:        unsupported
    WASM:        unsupported
    optimized:   no rules
```

Declared stable on the interpreter backend. Attempting to compile a
`Result`-using program to LLVM or WASM correctly produces a capability
error via `src/backends/capabilities/scan.rs`.

The interesting design question this feature surfaces: **is it acceptable
for a feature to be Stable on only one backend?** The capability matrix
says yes — a feature's maturity is per-backend, not global. When LLVM
and WASM add `Result` support, the maturity table above is updated and
the capability tests flip from `*_rejects_result_values` to
`*_accepts_result_values`.

## Checklist for related features

If you are adding a feature *like* `Result<T, E>` (a two-variant
payload-carrying enum with a `try` form), you need to touch:

1. `src/common/types.rs` — new `Type` variant + constructor + parsing +
   `can_coerce_to` + `common_supertype` + `contains_type_var` +
   `substitute` + `inner_type` + `Display`.
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

If the feature also introduces a new syntactic construct (like `try/catch`
did for `Result`), add:

13. `src/frontend/lexer/` — new keyword.
14. `src/frontend/parser/` — new expression or statement form.
15. `src/frontend/ast.rs` — new AST node.
16. `src/ir/semantic_ir.rs` — lowering target (often a `Switch`).
17. `tests/frontend/` — parser tests for the new syntax.

The checklist is long. It will shrink as the canonical IR work (Tier 7)
replaces per-backend re-interpretation with shared lowering rules.

## Open questions

- **Should `try/catch` and `match` be unified?** Currently `try/catch`
  is sugar over `match` with one case per variant. If the sugar ever
  grows features that `match` lacks (like auto-`?` propagation), they
  diverge. If not, they should stay unified.
- **Should error types be unified?** `Result<T, E>` requires the caller
  to know `E`. Many languages use a single `Error` trait object or an
  enum of common error kinds. ALGOL26 currently keeps `E` fully
  generic. This is more flexible but produces longer function
  signatures.
- **Should there be a `?` propagation operator?** Not currently. If
  added, it would need a per-feature contract describing how it
  interacts with `defer`, `match` exhaustiveness, and the error type
  coercion rules.
- **What does the LLVM lowering look like?** Likely an `i1 is_ok`
  flag plus a union of `T` and `E`. Needs a design pass before
  implementation.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/option.md` — the sibling enum-with-payload feature
- `docs/decisions/0003-type-system.md`
- `docs/decisions/0005-ownership-model.md`
- `src/common/types.rs`
- `src/ir/semantic_ir.rs`
- `tests/ir/try_catch_test.rs`
- `tests/corpus/corpus_14_try_catch.gol`
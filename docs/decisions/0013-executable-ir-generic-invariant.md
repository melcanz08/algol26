# 0013 — Executable IR is fully monomorphized

**Status:** Proposed
**Date:** 2026-09-23
**Supersedes:** —
**Related:** 0009 (unsafe), 0012 (implicit deref convention), `docs/compiler/type-table-addressing.md`

## Context

ALGOL26 supports parametric polymorphism:

```
function identity<T>(x: T) -> T
    return x

val v := 1.0
val p := &v
val q := identity(p)
```

Today, `cargo run -- tests/adversarial/39_generic_identity_reference.gol`
panics:

```
map_type called on unresolved type `TypeVar("T")` —
the IR verifier should have rejected this
    src/backends/llvm_codegen/types.rs:97
```

The same panic appears on the WASM path, because both backends share
`llvm_codegen::types::map_type`. The test
`generic_function_reaches_backend_with_resolved_types` in
`tests/backends/wasm_backend_test.rs` is marked `#[ignore]` and
documents the failure; it is the acceptance criterion for this ADR.

### Current pipeline

```
lex → parse → imports → desugar → expand impls → monomorphize
    → assign ExprIds → type-check → build IR → verify → optimize → backend
```

Every phase from `lex` through `assign ExprIds` runs inside
`Compiler::prepare_frontend`. Type checking consumes the resulting
`AstPayload`, producing `TypedProgram { functions, type_table_id, ... }`.
IR construction consumes that.

### What fails, and why

Two independent defects combine:

**Cause A — instantiation discovery runs before there is anything to
discover from.** `Monomorphizer::collect_instantiations` walks the AST
and calls `infer_expr_type` on each call argument. That function
recognizes literals (`Int`, `Float`, `String`, `Bool`, `List`) but
falls through to `Type::Unknown` for `ExprKind::Var`. So the call

```
val q := identity(p)
```

produces `arg_types = [Unknown]`, `has_unresolved` returns true, and
the loop in `monomorphize` skips specialization. No `identity_Borrow_Float`
is emitted.

The monomorphizer then does the correct thing given its information:
it falls back to keeping the generic template in the output and
trusting the analyzer to bind type variables at call sites. That
fallback works when the generic template is consumed by the
interpreter (which has semantic type information available at
lowering) but not when it is consumed by a native backend.

**Cause B — the executable-IR boundary does not enforce its own
precondition.** `SemanticProgram::verify` was written under the
assumption that generic templates are legitimate content. That
assumption was reasonable while the interpreter was the only
consumer. Once the LLVM and WASM backends started lowering
`SemanticProgram` directly, the assumption became false, but the
verifier was not tightened to match.

`map_type` is where the mismatch becomes visible. It has a
`debug_assert!` on the type variable and — worse — an `f64` fallback
on the release path. Under `cargo build --release`, an unresolved
type parameter silently becomes `f64`, producing wrong code instead of
a compiler error.

### Why the current "fix" is the wrong fix

The obvious local patch is to teach `infer_expr_type` about more
expression forms — `Var`, `Deref`, `ArrayAccess`, calls to other
functions — until it can infer the argument types for `identity(p)`.

That would work for this input. It would not generalize, and it would
recreate the exact architectural disease this project spent Phase 2
removing: a second type-inference engine that lives beside the real
one, disagrees with it in corner cases, and drifts as the language
evolves.

`infer_expr_type` is a *pre-typecheck approximation*. It cannot see
`SemanticState`, it cannot see the analyzer's scope chain, and it
cannot see `type_table_id`. Asking it to classify arbitrary
expressions is asking a syntactic pass to reproduce a semantic
analysis.

## Decision

**The invariant:**

> Executable `SemanticIR` is fully monomorphized. No `TypeVar` may
> cross the executable-IR verification boundary.

This has four corollaries.

**1. Generic function templates are semantic-analysis artifacts.**
They appear in the AST and in the analyzer's working state; they do
not appear in the `SemanticProgram` handed to a backend. The pipeline
contains a phase — after analysis, before IR construction — whose job
is to eliminate them.

**2. Instantiation discovery is the type checker's job.** The
analyzer already computes `ExprId → Type` for every expression. When
it resolves a call to a function whose declaration has non-empty
`type_params`, it is the only phase that knows the concrete types of
the arguments on the `ExprId`-keyed side. It records the result of
that knowledge as an *instantiation fact*.

**3. The verifier enforces the invariant structurally.**
`SemanticProgram::verify` rejects any `TypeVar` reachable from any
function signature, instruction operand, or terminator. This is a
hard failure, not a warning. The verifier is the enforcement point;
if it permits a `TypeVar` through, the invariant does not exist.

**4. Backend type mappers return errors, not fallbacks.**
`llvm_codegen::types::map_type` and its WASM analogue return
`Result<_, CompileError>`. The `debug_assert!` and the `f64` fallback
are both removed. The verifier makes this path unreachable in
practice; the `Result` is defense-in-depth against a future verifier
regression.

## Implementation

The change is staged so that each commit is individually testable
and lands before the next. The order matters: stages 3.1 and 3.2 must
land before 3.3, because 3.3's removal of the template fallback is
only safe once the monomorphizer no longer needs it.

### Stage 3.1 — Type checker records instantiations

Add `instantiations: Vec<Instantiation>` to `SemanticAnalyzer`.

```
pub struct Instantiation {
    /// The call expression where the instantiation occurs.
    pub call_site: ExprId,
    /// The name of the generic function being called.
    pub function: String,
    /// Concrete type arguments, in the order the function declares
    /// its type parameters.
    pub type_args: Vec<Type>,
}
```

In `ExprKind::FunctionCall`, after the argument loop has populated
`type_bindings`, if the resolved function has non-empty `type_params`,
push an `Instantiation` with `type_args` sorted into declaration
order (the same normalization `Monomorphizer::substitute_in_function`
uses).

Expose via `SemanticAnalyzer::take_instantiations()`. Thread through
`TypedProgram` as a new `instantiations: Vec<Instantiation>` field,
alongside `type_table_id`.

This stage does not change behavior. The list is written but not
yet read.

### Stage 3.2 — Monomorphizer consumes the analyzer's records

Replace `Monomorphizer::collect_instantiations` (AST walk) with an
input parameter: the analyzer's `Vec<Instantiation>`.

`Monomorphizer::monomorphize` still walks each generic function's
body to apply substitution — that part is unchanged — but the *set*
of specializations it produces is now driven by the analyzer's
facts, not by the monomorphizer's own guesses.

`infer_expr_type` is deleted. The `_ => Type::Unknown` fallback that
lurked there disappears with it. Call-site rewriting during the
substitution pass consults `Instantiation.type_args` by `call_site`
`ExprId` rather than re-inferring.

After this stage, `identity(p)` produces a specialization because
the analyzer recorded `identity, [Borrow(Float)]`.

### Stage 3.3 — Generic templates are excluded from executable IR

`Monomorphizer::monomorphize` currently emits the generic template
in addition to its specializations:

```
// Keep the generic function in the output. If a call site's type
// args could not be resolved at monomorphize time, the analyzer
// looks the generic up by name and binds type variables from the
// real argument types.
result.push(func.clone());
```

After 3.2, "could not be resolved at monomorphize time" cannot
occur. Remove the template from the output list. Every function in
the specialized AST has empty `type_params`.

### Stage 3.4 — Verifier rejects `TypeVar` in executable IR

Extend `SemanticProgram::verify` (or the enclosing verifier pass) to
walk:

- every `SemanticFunction`'s parameter and return types;
- every `SemanticInstruction`'s operand types;
- every `Terminator`'s operand types.

If any contains `Type::TypeVar(_)`, fail with:

```
executable IR contains unresolved type variable `T` in function
`identity` — monomorphization did not specialize this function
```

The check is structural: `TypeVar` may appear in an AST-derived
`TypeSyntax`, never in a `SemanticProgram`.

### Stage 3.5 — Backend type mappers return `Result`

`llvm_codegen::types::map_type` — signature change:

```
fn map_type(ty: &Type) -> Result<BasicTypeEnum, CompileError>
```

Remove the `debug_assert!` and the `f64` fallback. Propagate the
`Result` through every caller. The same treatment for the WASM type
mapper if it has an analogous function.

After 3.4, no `TypeVar` should reach this function. If one does, the
error message says so and names the type — the previous behavior
was either a panic (debug) or silently-wrong `f64` (release).

### Stage 3.6 — Un-ignore the test

`generic_function_reaches_backend_with_resolved_types` in
`tests/backends/wasm_backend_test.rs` loses its `#[ignore]` and
becomes the end-to-end acceptance test for this ADR. Add parallel
cases for: `identity(&mut x)`, `identity([1, 2, 3])`,
`identity(Some(1.0))`, and a nested generic call.

## Consequences

### Positive

- **One type authority.** The analyzer's `ExprId → Type` table is the
  only type-inference engine in the compiler. The monomorphizer no
  longer carries a parallel approximation that drifts.
- **Silent no-ops are eliminated.** Unresolved generics fail the
  verifier, at the correct architectural boundary, on both backends.
- **Fail-closed behavior for both LLVM and WASM.** The release-path
  `f64` fallback is gone. An unresolved type is a `CompileError`, not
  a wrong program.
- **Instantiation facts are keyed by `ExprId`.** Each `Instantiation`
  carries the `ExprId` of the call site. This is only possible because
  Phase 2 landed stable AST identity first; under the pointer-keyed
  type table, an instantiation record would have had to carry an
  address, and every pass that cloned an AST would have silently
  invalidated it.

### Negative

- **The pipeline gains a phase.** `prepare_frontend` no longer
  encloses every AST transformation. The new shape is:

  ```
  prepare_frontend     (lex → parse → imports → desugar → expand impls
                        → assign ExprIds)
       ↓
  type-check            (annotates AST, records instantiations)
       ↓
  monomorphize          (specializes generics using the analyzer's records)
       ↓
  build IR              (consumes concrete, template-free AST)
       ↓
  verify → optimize → backend
  ```

  The "one frontend path" property from Phase 2 is preserved —
  `prepare_frontend` is still the only place normalization happens —
  but monomorphization is no longer part of it. Code that assumed
  `prepare_frontend` returned executable AST needs to be revisited.

- **The IR builder must be fed the post-monomorphization AST.** The
  analyzer typed the *pre*-monomorphization AST, so its
  `type_table_id` keys refer to nodes that the monomorphizer replaces.
  Two approaches are viable:

  **A. Deferred substitution.** The monomorphizer does not rewrite
  AST bodies. It produces a `MonomorphizationTable` mapping
  `(call_site: ExprId, function: String) -> type_args`. The IR builder
  consults the table at each call site, emits a call to the mangled
  name, and reads types from `type_table_id` for the original
  `ExprId`s, applying the recorded type substitution on lookup.

  **B. Renumbering.** The monomorphizer clones and substitutes as it
  does today, producing new `Expr` nodes with fresh `ExprId`s. A
  second `assign_expr_ids` pass runs after monomorphization. The
  analyzer's type table entries are carried forward by a mapping
  from old ID to new ID.

  **A is recommended.** It leaves the AST immutable after analysis,
  which is the property Phase 2 was designed to establish. B creates
  a second AST and a second numbering pass; it works, but it means
  two `ExprId` spaces coexist during IR construction, and every
  lookup has to know which space it is in.

- **Instantiation recording may be incomplete.** If the analyzer
  encounters a call to a generic function whose type arguments it
  cannot determine (e.g. `identity(none)` where `None` has no
  inferable element type), no `Instantiation` is recorded and the
  monomorphizer has nothing to specialize. Stage 3.4 will then
  correctly reject the resulting executable IR. This is the intended
  behavior: a program whose generic call cannot be resolved fails at
  the verifier, not at codegen. It does mean the analyzer needs a
  follow-up diagnostic ("cannot determine type arguments for
  `identity`") so the user sees a source-level error, not a
  verifier-internal one. That diagnostic is out of scope for this ADR
  and should be filed as a follow-up.

## Alternatives considered

**Extend `infer_expr_type` to handle more expression forms.**
Rejected. It would work for `identity(p)` and fail for
`identity(some_fn_that_returns_t())`. More importantly, it
institutionalizes a parallel type system. Phase 2 removed the last
of these; adding a new one defeats the point.

**Keep the generic template in the output and specialize per call
site in the IR builder.** Rejected. It defers the type-substitution
problem into lowering, where each backend would have to solve it
independently. LLVM and WASM would drift. It also requires the IR
builder to know, for every call site, the concrete type arguments —
which is exactly the information `Instantiation` records. If we're
going to record the fact, we may as well consume it before IR
construction.

**Do nothing; document the LLVM/WASM limitation and keep the
`#[ignore]`.** Rejected. The release-path `f64` fallback is a
correctness hazard: a user building with `cargo build --release`
gets a silently wrong program, not a compiler error. Fail-closed is
the whole point of the verifier.

**Fix only the panic, leave the f64 fallback.** Rejected. The panic
is the *visible* symptom. The f64 fallback is the *dangerous* one.
Both must go.

## Related

- `docs/decisions/0009-unsafe.md` — the other pending invariant at
  the executable-IR boundary.
- `docs/decisions/0012-implicit-deref-convention.md` — the type
  table conventions this ADR extends.
- `src/backends/llvm_codegen/types.rs:97` — the current panic site.
- `src/ir/monomorphize.rs::infer_expr_type` — the function this ADR
  deletes.
- `tests/backends/wasm_backend_test.rs::generic_function_reaches_backend_with_resolved_types`
  — the acceptance test.
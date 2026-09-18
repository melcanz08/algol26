# ALGOL26 — Architecture Direction

> Why the language is currently hard to extend, and the incremental path to making it easier.
> This document is descriptive, not prescriptive. It names the problem, proposes a direction, and points at the next concrete steps.

## TL;DR

Adding a feature to ALGOL26 currently requires touching roughly ten files across six directories.
That is not a design flaw — it is normal for compilers. But it makes each feature change
expensive, and every change risks regressing a different feature.

The path forward is not a rewrite. It is a set of small architectural commitments:

1. One canonical semantic representation.
2. A written contract per feature.
3. Fail-closed compilation.
4. Differential testing as an enforced invariant.
5. Verified IR as a typestate, not a convention.

Each one is independently useful. Together they turn feature work from a treasure hunt into a checklist.

## The observation

A single new language feature — say `Option<T>` — currently touches:

- lexer
- parser
- AST
- type checker
- ownership / borrow checker
- control-flow analysis
- type table
- semantic IR
- IR verifier
- optimizer
- LLVM backend
- WASM backend
- interpreter
- tests
- documentation

That is fifteen subsystems. It is not unique to ALGOL26 — every compiler with strong
static guarantees has this shape. But the cost is elevated because the same semantic
concept is represented independently in multiple subsystems.

## Where semantics currently live

### Types

The same "type" concept is expressed in six places:

| Representation | Location |
|---|---|
| AST type syntax | `src/frontend/` |
| Analyzer-internal types | `src/semantics/analyzer/` |
| `Type` enum | `src/common/types.rs` |
| `TypedIRValue` payload | `src/ir/semantic_ir.rs` |
| LLVM lowered type | `src/backends/llvm_codegen/types.rs` |
| `RuntimeValue` variant | `src/backends/interpreter/runtime.rs` |

Every one of these can drift independently. Adding `Result<T, E>` means teaching all six.

### Operations

The same "operation" concept is expressed in five places:

| Representation | Location |
|---|---|
| AST expression node | `src/frontend/ast.rs` |
| Analyzer rule | `src/semantics/analyzer/` |
| IR instruction | `src/ir/semantic_ir.rs` |
| LLVM lowering | `src/backends/llvm_codegen/` |
| Interpreter evaluation | `src/backends/interpreter/eval.rs` |

`SemanticBinOp::Add` is defined once, but its semantics are re-implemented in
`eval_binop` and in `llvm_codegen/binop.rs`. If one changes and the other does not,
tests catch it only if a differential test happens to exercise that case.

## The regression cycle

```
more features
    -> more cross-feature dependencies
    -> old assumptions become invalid
    -> patch feature A
    -> feature B regresses
    -> fix B
    -> feature C regresses
    -> ...
```

This is not a discipline problem. It is a coupling problem. The more independent
places a concept lives, the more chances there are for the copies to disagree.

## The target architecture

```
                    LANGUAGE FRONTEND
                          |
                     AST / HIR
                          |
                          v
                 +-----------------+
                 | Semantic Model  |
                 |                 |
                 | types           |
                 | ownership       |
                 | effects         |
                 | control flow    |
                 | lifetimes       |
                 +--------+--------+
                          |
                          v
                    Canonical IR
                          |
              +-----------+-----------+
              v           v           v
           LLVM         WASM     Interpreter
```

The key word is **canonical**: one authoritative representation of meaning, which
all backends consume. Backends answer "how do I implement this?" — they do not
each re-invent "what does this mean?".

## Feature pipeline contracts

Every feature should have one small specification file. Example:

```
Feature: Option<T>

Syntax:      Some(x), None
Typing:      Some(T) -> Option<T>
             None   -> Option<T>
Ownership:   payload follows normal ownership rules
IR:          OptionSome, OptionNone
Pattern:     Some(x), None
Backends:    LLVM        - partial
             WASM        - unsupported
             Interpreter - supported
Optimizer:   OptionSome(OptionNone) folds to None  (planned)
Safety:      payload cannot escape via Result boundaries
```

When adding a feature, this file is a checklist. If the compiler compiles but a
backend is unsupported, the compiler should emit a diagnostic that names the
specific missing support — not silently generate a fallback value.

Proposed location: `docs/features/<feature>.md`, one file per feature.

## Testing as architecture

Two test layers should become enforced invariants, not just suites that happen to exist.

### Conformance suite

```
tests/conformance/
    basics/
    types/
    ownership/
    borrowing/
    control_flow/
    generics/
    traits/
    memory/
    concurrency/
    defer/
    ffi/
```

Each feature directory contains `.gol` programs plus per-backend expected output:

```
tests/conformance/option_match/
    main.gol
    expected_interpreter.txt
    expected_llvm.txt
    expected_wasm.txt       (or "unsupported" marker)
    expected_diagnostics.txt
```

Then every compiler change runs the same suite. Currently ALGOL26 has flat
`tests/conformance/{valid,invalid}/` and `tests/soundness/` (12 programs). The
structure above is the target.

### Differential testing as a contract

```
        ALGOL26 program
              |
     +--------+--------+
     v        v        v
Interpreter  LLVM     WASM
     |        |        |
     +--------+--------+
              v
         same result
```

Current state:

- Interpreter vs LLVM: `tests/differential/differential_true.rs` (43 tests)
- WASM compile parity: `tests/differential/wasm_differential_test.rs`
- Backend independence: `tests/backends_tests.rs`

The missing invariant: *every new feature must be validated on every backend it
claims to support*, and a feature that is unsupported on a backend must produce
a compiler error, not silently fall through.

## Fail-closed compilation

Current situation: some backends still have fallback paths where an unsupported
operation becomes `0.0`, `null`, `Void`, or a silently-ignored no-op.

This is the single easiest architectural change with the largest payoff. Every
unsupported operation should become an explicit error:

```rust
return Err(CodegenError::UnsupportedOperation {
    operation: "Deref",
    backend: "llvm",
});
```

So a new feature that is not fully implemented fails at compile time, not at runtime.

**Progress this session:**

- Interpreter: removed the wildcard arm from `eval_value`; introduced `EvalError`
  with `TypeMismatch`, `Runtime`, and `Unsupported` variants; removed
  `unreachable!()` and `std::process::exit(1)` from `eval_binop`.
- CFG builder: replaced the `_ => Unsupported` catch-all with an exhaustive match —
  a new `Instruction` variant without a builder arm now fails the build rather
  than producing an `E-UNSUPPORTED-001` diagnostic at runtime.

**Still pending:**

- LLVM codegen has fallback paths not yet audited.
- WASM backend not audited.
- `CodegenError::UnsupportedOperation` as a first-class type does not exist yet.

## Verified IR as a typestate

Current state: `VerifiedIR` wraps a `SemanticProgram`, and `VerifiedIR::new()`
runs the verifier before returning. But `VerifiedIR::from_verify_pass()` bypasses
verification with a debug-only assertion, and `into_program()` is documented as
"any other call site is a bug" — a comment, not a type.

Target: transformations consume `VerifiedIR` by value and produce `VerifiedIR`
on success.

```rust
fn optimize(ir: VerifiedIR) -> Result<VerifiedIR, OptimizeError>;
```

Then running a transform on unverified IR is not possible — it is a type error.

Current pipeline stage names and the fact that `VerifyIrPass` returns `PassResult`
(unit) rather than `VerifiedIR` means this enforcement is not yet possible without
a trait change. That is the concrete next step.

## Module split

Long-term, the analyzer and the IR should be split along semantic axes.

### Analyzer

```
src/semantics/
    types/
        checker.rs
        inference.rs
        coercion.rs
    ownership/
        move_checker.rs
        borrow_checker.rs
        lifetime.rs
    control/
        flow.rs
        returns.rs
    generics/
        inference.rs
        constraints.rs
        monomorphize.rs
    concurrency/
        race.rs
        capture.rs
        channels.rs
    memory/
        escape.rs
        regions.rs
    traits/
        registry.rs
        resolution.rs
    mod.rs          (SemanticAnalyzer, orchestration only)
```

Current state: `src/semantics/analyzer/{expr, items, mod, ownership, scopes, stmt}.rs`
plus separate top-level `escape.rs`, `race/`, `control_flow.rs`, `flow_analyzer.rs`,
and `trait_registry/`. A partial split, but `SemanticAnalyzer` still orchestrates
everything.

### IR

```
src/ir/
    core.rs          (SemanticProgram, SemanticFunction, SemanticBlock)
    values.rs        (TypedIRValue, SemanticBinOp)
    control.rs       (Terminator)
    memory.rs        (Allocate, Free, RegionEnter, RegionExit)
    ownership.rs     (Borrow, MutBorrow, Deref, AddrOf)
    concurrency.rs   (Spawn, Fork, Send, Receive)
    patterns.rs      (SemanticPattern)
```

Current state: one file, `src/ir/semantic_ir.rs`, holds all of the above.

## Feature maturity model

Not every feature should be claimed "done" everywhere. A feature is at one of:

```
Experimental
    Parsed
        Type-checked
            Semantically validated
                IR supported
                    Verified
                        Interpreter supported
                            LLVM supported
                                WASM supported
                                    Optimized
                                        Stable
```

A feature can legitimately be:

```
Result<T,E>
    semantics:   Stable
    interpreter: supported
    llvm:        unsupported
    wasm:        unsupported
```

without implying the whole language is missing. The capability matrix
(`src/backends/capabilities/`, `src/compiler/capabilities.rs`) partially
implements this — extending it to full maturity tracking is the goal.

## Incremental path

No rewrite. Existing infrastructure is too valuable. The path is:

```
current ALGOL26
       |
       v
establish contracts          <- feature spec files, docs/features/
       |
       v
extract semantic modules     <- analyzer split, IR namespace split
       |
       v
strengthen canonical IR      <- one type universe, backend consumes it
       |
       v
make transforms fail-closed  <- LLVM/WASM backend audits, CodegenError
       |
       v
expand conformance suite     <- per-feature, per-backend fixtures
       |
       v
new features become cheaper
```

## Session progress (as of 2026-09-18)

Landed this session:

- Interpreter totality: `eval_value`, `eval_binop`, `eval_builtin_call`,
  and `eval_call` all return `Result<_, EvalError>`.
- `SemanticState::borrow` no longer relocates the borrower to the current
  region — fixes a false-negative in region-outlives diagnostics.
- CFG builder handles `Nop` and `IteratorInit` explicitly; the `_ => Unsupported`
  catch-all was replaced with an exhaustive match.
- `runtime_eq` handles mixed Int/Float equality; `1 == 1.0` is now `true` in the
  interpreter, matching LLVM.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` are enforced
  in CI (`.github/workflows/ci.yml`).

Outstanding from the observer's list:

1. Canonical IR as single authoritative meaning.
2. Per-feature pipeline contracts (`docs/features/`).
3. Conformance suite with per-backend expectations.
4. Differential testing enforced for every supported backend.
5. IR namespace split.
6. Feature maturity / capability matrix extended.
7. LLVM backend fail-closed audit.

## Bottom line

Regression is the symptom. Semantic coupling is the cause.

Fighting regression directly means adding more tests, more safety layers, more
guards. Reducing coupling means each feature change touches fewer subsystems
and has fewer chances to break its neighbours.

The five commitments above — canonical IR, feature contracts, fail-closed,
differential invariants, typestate IR — are the lever. Everything else is
detail.

## See also

- `docs/decisions/0005-ownership-model.md`
- `docs/decisions/0007-region-memory.md`
- `docs/decisions/0009-unsafe.md`
- `docs/ir-pass-contracts.md`
- `docs/test-organization.md`
- `docs/IMPLEMENTATION_STATUS.md`
# ALGOL26 — Architecture Direction

> Why the language was hard to extend, what changed, and the
> incremental path that remains. This document is descriptive, not
> prescriptive. It names the problem, records the direction that has
> been followed, and points at the next concrete steps.

## TL;DR

Adding a feature to ALGOL26 used to require touching roughly ten
files across six directories. That is not a design flaw — it is
normal for compilers. But it made each feature change expensive,
and every change risked regressing a different feature.

The direction, written down and then followed, is a set of small
architectural commitments:

1. One canonical semantic representation.
2. A written contract per feature.
3. Fail-closed compilation.
4. Differential testing as an enforced invariant.
5. Verified IR as a typestate, not a convention.

Four of the five have landed in part or in full. One — canonical
semantic representation across backends — is partially done. The
sections below record both the direction and its current status.

## The observation

A single new language feature — say `Option<T>` — touches:

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

That is fifteen subsystems. It is not unique to ALGOL26 — every
compiler with strong static guarantees has this shape. But the cost
is elevated because the same semantic concept is represented
independently in multiple subsystems.

## Where semantics live

### Types

The same "type" concept is expressed in six places:

| Representation | Location |
|---|---|
| AST type syntax | `src/frontend/ast.rs` (`TypeSyntax`) |
| Analyzer-internal types | `src/semantics/analyzer/` |
| `Type` enum | `src/common/types.rs` |
| `TypedIRValue` payload | `src/ir/semantic_ir/values.rs` |
| LLVM lowered type | `src/backends/llvm_codegen/types.rs` |
| `RuntimeValue` variant | `src/backends/interpreter/runtime.rs` |

Every one of these can drift independently. Adding `Result<T, E>`
means teaching all six.

**Partial progress.** `resolve_type_syntax` now exists in both the
analyzer and the builder as the single entry point for user-written
annotations. Every `to_type()` call site outside `resolve_type_syntax`
itself was swept and replaced (`5b07b27`, `c8c15fc`, `9ee2797`).
The six representations remain, but the annotation-to-`Type` path is
now uniform.

### Operations

The same "operation" concept is expressed in five places:

| Representation | Location |
|---|---|
| AST expression node | `src/frontend/ast.rs` |
| Analyzer rule | `src/semantics/analyzer/` |
| IR instruction | `src/ir/semantic_ir/instructions.rs` |
| LLVM lowering | `src/backends/llvm_codegen/` |
| Interpreter evaluation | `src/backends/interpreter/eval.rs` |

`SemanticBinOp::Add` is defined once, but its semantics are
re-implemented in `eval_binop` and in `llvm_codegen/binop.rs`. If one
changes and the other does not, tests catch it only if a differential
test happens to exercise that case.

### What is not covered by the above

Two additional subsystems became relevant with the records work:

| Representation | Location |
|---|---|
| Record declaration table | `SemanticAnalyzer::records` |
| Builder record-name set | `SemanticIRBuilder::record_names` |

These must be kept consistent. Every record-resolution bug found
during the CLI exercises traced to one of them not being consulted
at a call site that needed it.

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

This is not a discipline problem. It is a coupling problem. The more
independent places a concept lives, the more chances there are for
the copies to disagree.

### The CLI exercise validated this diagnosis

Between 2026-09-27 and 2026-10-01, four ALGOL26 programs were written
in `~/dev/algol26-cli/` — a CLI/config parser, a sales-report parser
with numeric aggregation, and two test modules. The exercise surfaced
thirteen compiler bugs. Every one was the same shape: a concept
represented in more than one place, with one copy not being consulted
at a call site that needed it.

The bugs, grouped by the coupling pattern:

**`to_type` vs `resolve_type_syntax` (five bugs).** User-written type
annotations flowed through a pure-syntax path in some call sites and
the record-aware path in others. Every call site had to be audited
individually. Five were missed on the first pass.

**`TypedIRValue::type_of` missing arms (three bugs).** The method had
a `_ => Type::Unknown` catch-all, so a variant without an arm silently
produced `Unknown` instead of failing to compile. `FieldAccess`,
`Array`, and `Range` were all missing. The catch-all was removed
(`b88efcc`); the method is now exhaustive.

**Callee resolution in the IR builder (two bugs).** Two push sites
re-resolved the callee from the raw source name instead of taking
`translate_expr`'s output. `list.length()` produced `list.length`
rather than `List.length` in the emitted IR, and the interpreter
couldn't dispatch it.

**Record table not consulted (three bugs).** `process_imports` didn't
merge imported records; the builder's signature loop didn't consult
the record set; `resolve_method_call` short-circuited for records
before trying the impl-mangled form.

The doc predicted that coupling produces regression cycles. The
exercise found thirteen concrete instances in three days of writing
real programs. That is not a criticism of the compiler; it is the
diagnosis being confirmed by practice.

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

The key word is **canonical**: one authoritative representation of
meaning, which all backends consume. Backends answer "how do I
implement this?" — they do not each re-invent "what does this
mean?".

### Status of the target

| Element | Status |
|---|---|
| One canonical pipeline | ✅ ADR 0018 — one `Compiler::run_pipeline` shared by all entry points |
| Semantic IR as the shape all backends consume | ✅ `SemanticProgram` is the sole input to interpreter, LLVM, and WASM |
| VerifiedIR as a distinct level | ✅ ADR 0017 — `IrState` enum replaces `Program::verified: bool` |
| Type unification across backends | ⚠️ Partial — `Type` is shared, but each backend still carries its own lowered representation |
| Operation semantics unified across backends | ⚠️ Partial — `SemanticBinOp` shared, but `eval_binop` and `llvm_codegen/binop.rs` re-implement the semantics |
| Transform consumes `VerifiedIR` by value | ⬜ Not yet — `VerifyIrPass` returns `PassResult` (unit) |

## Feature pipeline contracts

Every feature should have one small specification file. The
`docs/features/` directory now holds one per feature:

```
docs/features/
    alloc_free.md
    borrow.md
    channel.md
    defer.md
    ffi.md
    generic.md
    list.md
    map.md
    methods.md
    option.md
    range.md
    record.md
    region.md
    result.md
    spawn.md
    string.md
    trait.md
    unsafe.md
```

Each file follows the same shape:

```
Feature: Option<T>

Syntax:      Some(x), None
Typing:      Some(T) -> Option<T>
             None   -> Option<T>
Ownership:   payload follows normal ownership rules
IR:          TypedIRValue::Some, TypedIRValue::None
Pattern:     Some(x), None
Backends:    LLVM        - refused
             WASM        - refused
             Interpreter - supported
Capability:  Feature::Option, interpreter claims it
Tests:       src/backends/interpreter/tests.rs
```

When adding a feature, this file is a checklist. If the compiler
compiles but a backend is unsupported, the compiler emits a
diagnostic that names the specific missing support — not silently
generate a fallback value.

## Testing as architecture

Two test layers are enforced invariants, not just suites that happen
to exist.

### Capability matrix and maturity ladder

`tests/coverage_matrix.rs` holds one row per feature with its
per-backend support and its refusal tests. `tests/coverage_maturity.rs`
holds a hand-written expected maturity for every row. A feature
whose backend support changes without the maturity table being
updated fails a build.

Every `Refused` claim in the matrix is pinned by a named test in
`src/backends/capabilities/tests.rs`. `no_unclaimed_refusal_tests`
asserts every `*_rejects_*` test is claimed by some row, and
`refusal_tests_exist` asserts every named test actually exists.

The three checks together make it impossible for a feature's
capability claim to drift from the tests that verify it.

### Conformance suite

Currently: flat `tests/conformance/{valid,invalid}/` plus 39 programs
in `tests/corpus/`. The target structure — per-feature directories
with per-backend expected-output files — is not yet implemented. The
current layout is:

```
tests/conformance/
    valid/
        arithmetic/
        basics/
        concurrency/
        control_flow/
        defer/
        lists/
        method_call/
        option/
        result/
        strings/
    invalid/
        call_unknown_function.gol
        double_mut_borrow.gol
        method_on_wrong_type.gol
        type_mismatch.gol
        use_after_move.gol
```

The target is per-feature directories with backend markers. This
remains an open item.

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

- Interpreter vs LLVM: `tests/differential/differential_true.rs`
  (46 tests)
- WASM compile parity: `tests/differential/wasm_differential_test.rs`
- Backend independence: `tests/backends_tests.rs`
- Corpus differential: 39 programs compared across backends in
  `tests/corpus_diff.rs`

The missing invariant: *every new feature must be validated on every
backend it claims to support*, and a feature that is unsupported on
a backend must produce a compiler error, not silently fall through.
Partial coverage exists; systematic coverage does not.

## Fail-closed compilation

Current situation: some backends still have fallback paths where an
unsupported operation becomes `0.0`, `null`, `Void`, or a silently-
ignored no-op.

The single easiest architectural change with the largest payoff is
that every unsupported operation should become an explicit error:

```rust
return Err(CodegenError::UnsupportedOperation {
    operation: "Deref",
    backend: "llvm",
});
```

So a new feature that is not fully implemented fails at compile time,
not at runtime.

### Landed

- **Interpreter.** Removed the wildcard arm from `eval_value`;
  `EvalError` with `TypeMismatch`, `Runtime`, and `Unsupported`
  variants; no `unreachable!()` and no `std::process::exit(1)` in
  `eval_binop`.
- **CFG builder.** Exhaustive match on `Instruction` — a new variant
  without a builder arm fails the build.
- **Capability matrix.** LLVM and WASM refuse records, maps,
  `List.append`, `Option`, `Result`, conversions, and channels via
  `check_backend` rather than at codegen.
- **`type_of()` exhaustive.** The `_ => Type::Unknown` catch-all was
  removed; every `TypedIRValue` variant has an explicit arm.

### Still pending

- LLVM codegen has fallback paths not yet audited.
- WASM backend not audited.
- `CodegenError::UnsupportedOperation` as a first-class type does
  not exist yet.

## Verified IR as a typestate

Current state: `VerifiedIR` wraps a `SemanticProgram`, and
`VerifiedIR::new()` runs the verifier before returning.
`VerifyIrPass` promotes `IrState::Built` to `IrState::Verified`;
`ReVerifyPass` re-checks a `Verified` program after optimization.

Target: transformations consume `VerifiedIR` by value and produce
`VerifiedIR` on success.

```rust
fn optimize(ir: VerifiedIR) -> Result<VerifiedIR, OptimizeError>;
```

Then running a transform on unverified IR is not possible — it is a
type error.

Current pipeline stage names and the fact that `VerifyIrPass`
returns `PassResult` (unit) rather than `VerifiedIR` means this
enforcement is not yet possible without a trait change. That is the
concrete next step.

## Module split

Long-term, the analyzer and the IR should be split along semantic
axes.

### IR

The IR split has landed. `src/ir/semantic_ir/` is a directory:

```
src/ir/semantic_ir/
    core.rs           (SemanticProgram, SemanticFunction, SemanticBlock)
    values.rs         (TypedIRValue, SemanticBinOp)
    instructions.rs   (Instruction)
    terminators.rs    (Terminator)
    patterns.rs       (SemanticPattern)
    display.rs        (source-shaped rendering)
    mod.rs            (public surface)
```

### Analyzer

The analyzer is partially split. Current:

```
src/semantics/
    analyzer/
        expr.rs
        items.rs
        mod.rs           (SemanticAnalyzer, orchestration)
        ownership.rs     (borrow and move checks)
        scopes.rs
        stmt.rs
        tests.rs
    state/               (SemanticState, branch join)
    trait_registry/
        mod.rs
        register.rs
        resolve.rs
        validate.rs
    race/
    builder/             (IR construction)
    flow_result.rs
```

The target separates `types/`, `ownership/`, `control/`, `generics/`,
`concurrency/`, `memory/`, `traits/`, and leaves `mod.rs` as pure
orchestration. That split has not happened.

## Feature maturity model

Not every feature should be claimed "done" everywhere. The maturity
model proposed in the original version of this document has landed
as `tests/coverage_maturity.rs`.

`Maturity` variants:

```
Unfinished
InterpreterOnly
LlvmOnly
WasmOnly
InterpreterAndLlvm
InterpreterAndWasm
AllBackends      (with a conformance fixture)
Universal        (all backends, no fixture needed)
```

`maturity_of(row)` derives the label from the row's three backend
columns and whether a conformance directory exists. `EXPECTED_MATURITY`
is a hand-written table that must agree with the derived value.
Adding a feature to the matrix requires adding an entry to both.
The two-place update is deliberate: it forces a conscious decision
about the feature's maturity instead of letting it drift.

A feature can legitimately be:

```
map
    interpreter: Full
    llvm:        Refused
    wasm:        Refused
    maturity:    InterpreterOnly
```

without implying the whole language is missing. The capability matrix
(`src/backends/capabilities/`, `src/compiler/capabilities.rs`) is the
authoritative per-backend view; the maturity table is the
single-label summary.

## Incremental path

No rewrite. Existing infrastructure is too valuable. The path is:

```
current ALGOL26
       |
       v
establish contracts          ✅ docs/features/, one file per feature
       |
       v
extract semantic modules     ⚠️ IR split done; analyzer partial
       |
       v
strengthen canonical IR      ⚠️ Type unified; backend lowering separate
       |
       v
make transforms fail-closed  ⚠️ interpreter and CFG done; LLVM/WASM pending
       |
       v
expand conformance suite     ⬜ per-feature, per-backend fixtures not yet
       |
       v
new features become cheaper
```

## What landed

### Through 2026-09-18

- Interpreter totality: `eval_value`, `eval_binop`, `eval_builtin_call`,
  and `eval_call` all return `Result<_, EvalError>`.
- `SemanticState::borrow` no longer relocates the borrower to the current
  region — fixes a false-negative in region-outlives diagnostics.
- CFG builder handles `Nop` and `IteratorInit` explicitly; the `_ =>
  Unsupported` catch-all was replaced with an exhaustive match.
- `runtime_eq` handles mixed Int/Float equality; `1 == 1.0` is now `true`
  in the interpreter, matching LLVM.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`
  enforced in CI (`.github/workflows/ci.yml`).

### Through 2026-09-27

- **Records (ADR 0024).** `rec` declarations, literals, field access,
  field assignment, pattern destructuring. Interpreter-only.
- **Track B builtins.** `Int.to_string`, `String.to_int`, `String.trim`,
  `String.split`, `String.join`.
- **`affirm` (ADR 0022).** All backends.
- **`args()` (ADR 0023).** Interpreter-only.
- **Diagnostics.** `CompileError` gained `Option<String> file`;
  unresolvable named types produce a diagnostic rather than
  silently becoming `Type::Unknown`.
- **Convergence (ADRs 0013–0018).** Canonical pipeline,
  `VerifiedIR` typestate, per-function dataflow, generic
  instantiation closure.
- **Unsafe enforcement, references, channels, iterator metadata**
  (ADRs 0015, 0019, 0020, 0021).

### Through 2026-10-01

- **Structural `Copy` for records (ADR 0026).** `Point` copies;
  `Person` moves. Compile-time predicate on the type.
- **`Map<K, V>` (ADR 0027).** Literal syntax, `insert` / `get` /
  `contains` / `keys` / `values` / `length`, hashable-key
  restriction. Interpreter-only.
- **`List.append(x)` (ADR 0028).** Mutating method, `var` receiver
  required, static-length tracking invalidated on append.
  Interpreter-only.
- **Traits and generics in real programs.** `impl Trait for Type`
  with method-call syntax; `function first<T>(xs: List<T>)` with
  type-parameter binding that recurses into containers.
- **Module system.** Top-level imports work; imported files' own
  imports are followed recursively.
- **`TypedIRValue::type_of` is exhaustive.** Catch-all removed.
- **Thirteen compiler bugs** fixed, all the same coupling shape,
  all surfaced by the CLI/sales-report exercise.

## What is still outstanding

1. **Canonical IR as single authoritative meaning.** `Type` is shared,
   but each backend still carries its own lowered representation.
   Unifying them would be a real change; not yet scheduled.
2. **Per-feature conformance fixtures.** The `docs/features/`
   contracts exist; matching test fixtures under
   `tests/conformance/<feature>/` with per-backend expected output do
   not.
3. **Differential testing for every supported backend.** Partial.
   `differential_true.rs` covers 46 programs; new features landed
   since are covered by unit tests, not differential tests.
4. **Analyzer module split.** IR split is done; the analyzer's
   `types/`, `ownership/`, `control/`, `generics/`, `concurrency/`,
   `memory/` split has not happened.
5. **LLVM backend fail-closed audit.** Fallback paths exist that
   have not been individually confirmed to refuse rather than
   silently produce a value.
6. **`CodegenError::UnsupportedOperation` as a first-class type.**
   Does not exist.
7. **Transform consumes `VerifiedIR` by value.** Requires a
   `PassResult` trait change.
8. **Diagnostic file provenance.** Errors inside imported files
   render the importing file's name. Per-node file provenance
   does not exist in the AST or `Span`.
9. **Sum types.** Needed for recursive JSON; no ADR yet.
10. **Chained method calls on field accesses.** `x.field.method()`
    is rejected by the parser. Workaround documented in
    `docs/STATUS.md`.

## Adding a new language feature

When a feature ships, update four places or the test suite tells you which one you missed:

1. `tests/coverage_matrix.rs` — add a `FeatureRow` with the feature name, conformance_dir, per-backend support, and refusal tests.
2. `tests/coverage_maturity.rs` — add the matching `EXPECTED_MATURITY` entry (`AllBackends` if there's a conformance fixture and all backends are `Full`).
3. `docs/STATUS.md` — Feature Matrix row.
4. `docs/features/<name>.md` — the per-feature contract.

The compiler-driven workflow catches the rest (adding a `Type` or `ExprKind` variant forces you to fill every exhaustive match).

## Bottom line

Regression is the symptom. Semantic coupling is the cause.

Fighting regression directly means adding more tests, more safety
layers, more guards. Reducing coupling means each feature change
touches fewer subsystems and has fewer chances to break its
neighbours.

The five commitments — canonical IR, feature contracts, fail-closed,
differential invariants, typestate IR — are the lever. Four of them
have landed in part or in full. The remaining item is the deepest
one: making the canonical semantic representation actually canonical,
with backends consuming it rather than each building their own
lowered copy.

The CLI exercise that produced thirteen compiler bugs was, in
retrospect, a test of this document's central claim. The claim held.
Every bug was a place where a concept had been copied instead of
shared, and the copies disagreed. That is why the direction — fewer
copies, more contracts, more enforcement — is the right one to keep
following.

## See also

- `docs/decisions/0005-ownership-model.md`
- `docs/decisions/0007-region-memory.md`
- `docs/decisions/0009-unsafe.md`
- `docs/decisions/0013-executable-ir-generic-invariant.md`
- `docs/decisions/0017-verified-ir-typestate.md`
- `docs/decisions/0018-canonical-pipeline.md`
- `docs/ir-transformations.md`
- `docs/test-organization.md`
- `docs/STATUS.md`
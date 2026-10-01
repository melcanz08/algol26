# ALGOL26 Test Organization

**Version**: v0.8.0

This document maps the test suite to the layers of the compiler.
Every file listed here exists; every suite listed here runs in
`cargo test --all-targets`. Adversarial programs under
`tests/adversarial/` are run separately by `run_adversarial.sh`.

---

## Directory Layout

```
tests/
├── adversarial/          adversarial .gol programs + shell runner
├── backends/             backend-independence and trait tests
├── conformance/          conformance fixtures
│   ├── valid/            programs that must compile and run
│   └── invalid/          programs that must be rejected
├── corpus/               differential corpus — 39 programs
├── corpus_support/       shared helpers for corpus tests
├── differential/         LLVM vs interpreter oracle tests
├── frontend/             lexer/parser/FFI surface tests
├── integration/          end-to-end, diagnostics quality, stress
│   └── negative/         .gol programs that must be rejected
├── ir/                   CFG construction, verification, optimizer
├── programs/             additional valid/invalid program fixtures
│   ├── valid/
│   └── invalid/
├── semantics/            types, borrows, traits, escape
├── soundness/            soundness fixtures by category
│   ├── borrowing/
│   ├── escape/
│   ├── initialization/
│   └── ownership/

  Top-level runners (each declares mod entries for its
  subdirectory and contains no test bodies itself):

├── backend_diff.rs               differential, borrow removal
├── backends_tests.rs             runner for tests/backends/
├── compiler_pipeline_equiv.rs    pass vs direct-call equivalence
├── conformance_coverage.rs       conformance dir coverage checks
├── conformance_tests.rs          conformance fixture runner
├── corpus_diff.rs                differential corpus runner
├── coverage_matrix.rs            feature × backend matrix
├── coverage_maturity.rs          maturity ladder
├── differential_tests.rs         runner for tests/differential/
├── frontend_tests.rs             runner for tests/frontend/
├── fuzz_tests_runner.rs          fuzz harness (no-panic checks)
├── integration_tests.rs          runner for tests/integration/
├── ir_tests.rs                   runner for tests/ir/
├── pass_contracts.rs             pass contract metadata
├── pipeline_contract.rs          pipeline-level invariants
├── property_tests_runner.rs      property-based tests
├── semantics_tests.rs            runner for tests/semantics/
├── soundness_runner.rs           soundness fixture runner
└── verifier_invariants.rs        IR verifier invariant tests
```

Each top-level `*_tests.rs` file is a thin runner that declares
`mod` entries for the files in the corresponding subdirectory.
The actual test bodies live in the subdirectories. The single-file
suites (`pass_contracts.rs`, `pipeline_contract.rs`,
`coverage_matrix.rs`, `coverage_maturity.rs`,
`compiler_pipeline_equiv.rs`, `verifier_invariants.rs`,
`conformance_coverage.rs`, `conformance_tests.rs`,
`corpus_diff.rs`, `soundness_runner.rs`) hold their test bodies
directly.

---

## Suite → Layer Mapping

### Frontend

| Suite | What it tests |
|---|---|
| `frontend/ffi_test.rs` | FFI parsing and symbol renaming |
| `frontend/alloc_in_value_position.rs` | `alloc(x)` parses as an expression |
| `frontend/span_invariant_test.rs` | every AST node carries a real span |

### IR

| Suite | What it tests |
|---|---|
| `ir/borrow_deref_addrof_test.rs` | Borrow/deref/addrof expression lowering, method-call desugaring |
| `ir/defer_lowering_test.rs` | Defer LIFO chaining and return preservation |
| `ir/ir_verification_test.rs` | CFG verifier rejects malformed IR; generic specialization emission |
| `ir/optimization_safety_test.rs` | Optimizer preserves semantics across arithmetic, lists, strings |
| `ir/optimizer_test.rs` | Folding, DCE, branch simplification |
| `ir/short_circuit_test.rs` | `and`/`or` short-circuit evaluation in conditions and statements |
| `ir/try_catch_test.rs` | Result-based try/catch, ok and error paths |

### Semantics

| Suite | What it tests |
|---|---|
| `semantics/borrow_checker_test.rs` | Ownership, moves, borrow conflicts |
| `semantics/borrow_checker_extra_test.rs` | Borrow edge cases (loops, conditionals, defers, spawns) |
| `semantics/branch_join.rs` | State joins at if/match merge points |
| `semantics/semantic_validation.rs` | Types, bounds, immutability, race detection |
| `semantics/trait_bounds_enforcement.rs` | `where` clause enforcement |
| `semantics/trait_method_test.rs` | Trait registration and method resolution |
| `semantics/type_unification_test.rs` | Type unification and helpers |

### Backends

| Suite | What it tests |
|---|---|
| `backends/backend_independence.rs` | IR does not depend on a specific backend |
| `backends/backend_trait_test.rs` | Backend trait contract and registry |
| `backends/oracle_test.rs` | Interpreter as oracle for other backends |
| `backends/wasm_backend_test.rs` | WASM backend trait contract |

### Differential

| Suite | What it tests |
|---|---|
| `differential/differential_test.rs` | LLVM vs interpreter, small programs |
| `differential/differential_true.rs` | LLVM vs interpreter, extended corpus |
| `differential/wasm_differential_test.rs` | WASM backend isolation and codegen |
| `backend_diff.rs` | Borrow removal on the LLVM path |

### Integration

| Suite | What it tests |
|---|---|
| `integration/conformance_test.rs` | Valid and invalid conformance corpora |
| `integration/diagnostic_test.rs` | `CompileError` format, error codes |
| `integration/diagnostics_quality_test.rs` | Error messages carry expected fields |
| `integration/release_hardening.rs` | Stress tests and negative tests |
| `integration/safety_guarantees_test.rs` | End-to-end safety behavior |

### Compiler pipeline and contracts

| Suite | What it tests |
|---|---|
| `pass_contracts.rs` | Every registered pass declares non-empty contract metadata; lower/non-lower level rules |
| `pipeline_contract.rs` | Pipeline-level invariants: CFG presence, dataflow wiring, error dedup, transform-then-verify |
| `compiler_pipeline_equiv.rs` | Pass-driven pipeline produces the same result as direct calls |

### Coverage and capability

| Suite | What it tests |
|---|---|
| `coverage_matrix.rs` | Feature × backend matrix: names, uniqueness, refusal-test claim |
| `coverage_maturity.rs` | Maturity ladder agrees with the matrix |
| `conformance_coverage.rs` | Conformance directory coverage: every dir referenced, every referenced dir exists |
| `verifier_invariants.rs` | Verifier rejects `TypeVar` in executable IR |

### Corpus and conformance

| Suite | What it tests |
|---|---|
| `corpus_diff.rs` | 39-program differential corpus, run through all backends |
| `conformance_tests.rs` | Conformance fixture runner |
| `soundness_runner.rs` | Soundness fixtures by category |

### Adversarial

| Suite | What it tests |
|---|---|
| `adversarial/*.gol` | Boundary-case programs with an `// EXPECT:` tag |

### Capability tests (inside the crate)

`src/backends/capabilities/tests.rs` pins every `Refused` claim in
the coverage matrix. Adding a `Feature` variant without adding a
matching refusal test fails `no_unclaimed_refusal_tests`.

---

## Test Counts

Run `cargo test --all-targets` for the current numbers. The
specific count is not tracked here because it changes with every
commit; the source of truth is the test runner itself.

The adversarial suite reports a **pass / fail / review** tally.
`review` counts are programs whose expected behavior is a language
design decision that has not yet been resolved; they are not
failures.

---

## Inline Unit Tests

Each module with non-trivial logic carries its own `#[cfg(test)]`
block. These run under `cargo test --lib`. Modules with inline
tests include:

- `common/types.rs` — type constructors, coercion, parsing,
  substitution, mangling
- `common/span.rs` — span arithmetic
- `frontend/lexer/mod.rs` — tokenization, indentation, multiline
  bracket handling
- `frontend/parser/tests.rs` — parser entry points, record
  literals, map literals
- `frontend/module_loader.rs` — caching, cycle detection
- `frontend/ast_display.rs` — source-shaped AST rendering
- `ir/cfg_verifier.rs` — structural verification
- `ir/instantiation_plan.rs` — mangling, plan construction,
  transitive closure
- `ir/loop_desugar.rs` — unroll decisions
- `ir/optimizer.rs` — folding, DCE, branch simplification
- `ir/semantic_ir/display.rs` — IR rendering
- `ir/semantic_ir/terminators.rs` — successor computation
- `ir/verified_ir.rs` — gate type
- `ir/verifier/tests.rs` — instruction-level checks
- `semantics/analyzer/tests.rs` — records, maps, generics,
  traits, borrows, unsafe blocks
- `semantics/builder/mod.rs` — substitution and callee resolution
- `semantics/race/tests.rs` — access-conflict detection
- `semantics/state/mod.rs` — branch joins, region tracking
- `semantics/trait_registry/tests.rs` — trait registration and
  generic impl matching
- `backends/capabilities/tests.rs` — feature × backend refusal
  tests
- `backends/interpreter/tests.rs` — end-to-end interpreter
  behavior for records, maps, `List.append`, traits, generics

---

## Corpus Programs

`tests/corpus/` holds 39 programs exercised by `corpus_diff.rs`.
Each program is compiled and run through every backend that
supports it; the outputs are compared.

Header directives on the first lines of a corpus program:

- `// OUTPUT: <line>` — the expected stdout, one directive per
  line, in order.
- `// BACKEND: interpreter` — run this program only through the
  interpreter. Used for programs that exercise interpreter-only
  features (records, maps, `List.append`, `Option`, `Result`,
  `try/catch`).
- `// KNOWN_FAILURE: <short description>` — the program documents
  a known compiler bug. The runner records it as a known failure
  rather than a test failure.

Adding a corpus program:

1. Write the `.gol` file in `tests/corpus/`.
2. Add the `// OUTPUT:` directives matching what the interpreter
   prints.
3. If the program uses a feature LLVM refuses, add
   `// BACKEND: interpreter`.
4. Run `cargo test --test corpus_diff`. If it passes, commit.

---

## Adversarial Suite

Located at `tests/adversarial/`. Each `.gol` file begins with an
`// EXPECT:` directive:

- `REJECT` — the file must fail to compile.
- `ACCEPT` — the file must compile successfully.
- `RUNTIME-TRAP` — the file must compile, and its binary must
  exit with a non-zero status when executed.
- `REVIEW` — the file documents a design question that is not
  yet resolved; the runner records it but does not pass or fail.

Run with:

```sh
bash tests/adversarial/run_adversarial.sh
```

The runner looks for the `algol26` binary at
`target/debug/algol26` and falls back to `target/release/algol26`.
Set `ALGOL26_BIN` to override.

---

## Adding a New Test

Match the test to the layer it exercises, then follow the layout:

1. **Which layer?** Use the suite-to-layer table above.
2. **Which file?** Append to the existing file for that layer if
   the test fits its theme. Only create a new file if the theme
   is genuinely new.
3. **Name it.** `test_<feature>_<scenario>` — for example,
   `test_short_circuit_in_while_condition`.
4. **Add a comment** at the top of the test explaining the
   invariant it verifies. Future readers need the *why*.
5. **Prefer behavior over shape.** A test that runs a program
   through the interpreter and asserts on output is more durable
   than one that inspects the IR structure, because IR shapes
   change as the compiler evolves.
6. **Adversarial programs** go under `tests/adversarial/` with an
   `// EXPECT:` tag. Reserve them for cases that stress the
   safety machinery (borrow conflicts, escape, races, bounds,
   null deref, moves) rather than general correctness.
7. **If the feature adds a `Feature` variant**, add a matching
   row to `tests/coverage_matrix.rs` and an entry to
   `tests/coverage_maturity.rs`'s `EXPECTED_MATURITY`. Add the
   refusal test to `src/backends/capabilities/tests.rs`.
8. **If the feature is compile-time-only** (like traits or
   generics), a differential test would only exercise the same
   concrete function on both backends. Prefer interpreter
   unit tests for behavior and the coverage matrix for the
   capability claim.

---

## Running the Full Suite

```sh
cargo test --all-targets                       # all Rust suites
bash tests/adversarial/run_adversarial.sh      # adversarial programs
```

For a quick iteration on a single suite:

```sh
cargo test --test ir_tests
cargo test --test semantics_tests
cargo test --test corpus_diff
cargo test --lib                               # inline unit tests only
```

For a specific test within a suite:

```sh
cargo test --lib record_method_call_dispatches_to_impl
```

---

## Test Discipline

- A failing test is either a real bug or a stale expectation.
  Both are worth fixing; neither should be silenced.
- When a bug is fixed, add the test that would have caught it in
  the most specific suite. If a fix uncovered by a broader test
  (e.g. differential) belongs in a narrower one (e.g. IR), add a
  companion test there too.
- Adversarial cases that reveal a compiler bug are kept after the
  bug is fixed: they become regression guards.
- `REVIEW` cases are documented limitations. When a review case
  is resolved (either by fixing the compiler or by deciding the
  language semantics), update the `// EXPECT:` tag to a definite
  value and it becomes a passing test.
- **Two-place updates.** Adding a feature to `MATRIX` requires
  adding it to `EXPECTED_MATURITY`. A build fails if the two
  disagree. The deliberate duplication forces a conscious
  decision about a feature's maturity at the moment it is added,
  rather than letting it drift silently.
- **Refusal tests are named in the matrix.** Adding a
  `*_rejects_*` test in `src/backends/capabilities/tests.rs`
  without claiming it in some matrix row fails
  `no_unclaimed_refusal_tests`.
- **Empty contract fields fail the build.** Every registered
  pass must declare non-empty `requires`, `guarantees`,
  `may_change`, and `must_preserve` in its `PassContract`.
  See `docs/pass-contracts.md`.

---

## See also

- `docs/pass-contracts.md` — the pass contract model and pipeline
  rules.
- `docs/IMPLEMENTATION_STATUS.md` — the feature × backend matrix
  and known gaps.
- `docs/coverage-maturity.rs` (source) — the maturity ladder that
  `coverage_maturity.rs` enforces.
- `docs/no-panic-policy.md` — the fuzz and property test policy.
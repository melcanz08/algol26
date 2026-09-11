# ALGOL26 Test Organization

**Version**: v0.8.0

This document maps the test suite to the layers of the compiler.
Every file listed here exists; every suite listed here runs in
`cargo test`. Adversarial programs under `tests/adversarial/` are
run separately by `run_adversarial.sh`.

---

## Directory Layout

```
tests/
|-- adversarial/          adversarial .gol programs + runner
|-- backends/             backend-independence and trait tests
|-- differential/         LLVM vs interpreter oracle tests
|-- frontend/             lexer/parser/FFI surface tests
|-- integration/          end-to-end, diagnostics quality, stress
|   |-- negative/         .gol programs that must be rejected
|-- ir/                   CFG construction, verification, optimizer
|-- programs/             conformance corpus
|   |-- valid/            programs that must compile
|   |-- invalid/          programs that must be rejected
|-- semantics/            types, borrows, traits, escape
|-- backend_diff.rs       top-level runner for backend_diff
|-- backends_tests.rs     top-level runner for tests/backends/
|-- differential_tests.rs top-level runner for tests/differential/
|-- frontend_tests.rs     top-level runner for tests/frontend/
|-- fuzz_tests_runner.rs  fuzz harness (no-panic checks)
|-- integration_tests.rs  top-level runner for tests/integration/
|-- ir_tests.rs           top-level runner for tests/ir/
|-- property_tests_runner.rs  proptest-based property tests
|-- semantics_tests.rs    top-level runner for tests/semantics/
```

Each top-level `*_tests.rs` file is a thin runner that declares
`mod` entries for the files in the corresponding subdirectory.
The actual test bodies live in the subdirectories.

---

## Suite -> Layer Mapping

| Suite file                                       | Layer         | What it tests                              |
|--------------------------------------------------|---------------|--------------------------------------------|
| `ir/borrow_deref_addrof_test.rs`                 | IR            | Borrow/deref/addrof expression lowering    |
| `ir/defer_lowering_test.rs`                      | IR            | Defer LIFO chaining and return preservation |
| `ir/ir_verification_test.rs`                     | IR            | CFG verifier rejects malformed IR          |
| `ir/optimization_safety_test.rs`                 | IR            | Optimizer preserves semantics              |
| `ir/optimizer_test.rs`                           | IR            | Folding, DCE, branch simplification        |
| `ir/short_circuit_test.rs`                       | IR            | `and`/`or` short-circuit evaluation        |
| `ir/try_catch_test.rs`                           | IR            | Result-based try/catch, ok and error paths |
| `semantics/architecture_test.rs`                 | Semantics     | Flow analyzer termination check            |
| `semantics/borrow_checker_test.rs`               | Semantics     | Ownership, moves, borrow conflicts         |
| `semantics/escape_analysis_test.rs`              | Semantics     | Reference escape scoping                   |
| `semantics/semantic_validation.rs`               | Semantics     | Types, bounds, immutability, races         |
| `semantics/trait_bounds_enforcement.rs`          | Semantics     | `where` clause enforcement                 |
| `semantics/trait_method_test.rs`                 | Semantics     | Trait registration, method resolution      |
| `semantics/type_unification_test.rs`             | Semantics     | Type unification and helpers               |
| `backends/backend_independence.rs`               | Backends      | IR does not depend on a specific backend   |
| `backends/backend_trait_test.rs`                 | Backends      | Backend trait contract and registry        |
| `backends/oracle_test.rs`                        | Backends      | Interpreter as oracle for all backends     |
| `backends/wasm_backend_test.rs`                  | Backends      | WASM backend trait contract                |
| `differential/differential_test.rs`              | Differential  | LLVM vs interpreter, small programs        |
| `differential/differential_true.rs`              | Differential  | LLVM vs interpreter, extended corpus       |
| `differential/wasm_differential_test.rs`         | Differential  | WASM backend isolation and codegen         |
| `frontend/ffi_test.rs`                           | Frontend      | FFI parsing and symbol renaming            |
| `integration/conformance_test.rs`                | Integration   | Valid and invalid program corpora          |
| `integration/diagnostic_test.rs`                 | Integration   | CompileError format, error codes           |
| `integration/diagnostics_quality_test.rs`        | Integration   | Error messages carry expected fields       |
| `integration/release_hardening.rs`               | Integration   | Stress tests and negative tests            |
| `integration/safety_guarantees_test.rs`          | Integration   | End-to-end safety behavior                 |
| `adversarial/*.gol`                              | Adversarial   | Boundary-case programs, per-file EXPECT tag |

---

## Test Counts

Counts below are the current numbers from `cargo test`. They are
informational; the source of truth is the test runner itself.

| Suite                    | Tests | Type            |
|--------------------------|-------|-----------------|
| Inline unit tests (lib)  |   95  | `#[cfg(test)]`  |
| `ir_tests`               |   40  | Integration     |
| `semantics_tests`        |   49  | Integration     |
| `integration_tests`      |   27  | Integration     |
| `backends_tests`         |   20  | Integration     |
| `differential_tests`     |   16  | Differential    |
| `property_tests_runner`  |    5  | Property        |
| `fuzz_tests_runner`      |    4  | Fuzz harness    |
| `backend_diff`           |    2  | Differential    |
| `frontend_tests`         |    2  | Integration     |
| **Rust total**           | **260** |              |
| Adversarial `.gol`       |   31  | Programs        |

The adversarial suite reports a **pass / fail / review** tally.
`review` counts are programs whose expected behavior is a language
design decision that has not yet been resolved; they are not
failures.

---

## Inline Unit Tests

Each module with non-trivial logic carries its own `#[cfg(test)]`
block. These run under `cargo test --lib` and are counted as a
single `running 95 tests` block.

Modules with inline tests include:

- `common/types.rs`         -- type constructors, coercion, parsing
- `common/span.rs`          -- span arithmetic
- `frontend/lexer.rs`       -- tokenization, indentation
- `frontend/parser.rs`      -- parsing entry points
- `frontend/module_loader.rs` -- caching, cycle detection
- `ir/cfg_verifier.rs`      -- structural verification
- `ir/defer_lowering.rs`    -- legacy pass
- `ir/loop_desugar.rs`      -- unroll decisions
- `ir/optimizer.rs`         -- folding, DCE
- `ir/semantic_verifier.rs` -- instruction-level checks
- `ir/verified_ir.rs`       -- gate type
- `runtime/region.rs`       -- region stack
- `runtime/region_memory.rs` -- allocator
- `semantics/race.rs`       -- access-conflict detection
- `semantics/semantic.rs`   -- scope-borrow and null-deref tests
- `semantics/trait_registry.rs` -- generic impl matching

---

## Adversarial Suite

Located at `tests/adversarial/`. Each `.gol` file begins with an
`// EXPECT:` directive on its first non-comment line:

- `REJECT` -- the file must fail to compile.
- `ACCEPT` -- the file must compile successfully.
- `RUNTIME-TRAP` -- the file must compile, and its binary must
  exit with a non-zero status when executed.
- `REVIEW` -- the file documents a design question that is not
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
3. **Name it.** `test_<feature>_<scenario>` -- for example,
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

---

## Running the Full Suite

```sh
cargo test                                   # all Rust suites
bash tests/adversarial/run_adversarial.sh    # adversarial programs
```

For a quick iteration on a single suite:

```sh
cargo test --test ir_tests
cargo test --test semantics_tests
cargo test --lib                             # inline unit tests only
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
# ADR 0018: One canonical pipeline

Status: Proposed

## Context

Three public entry points produce executable IR:

    run_interpreter(source, filename)
        prepare_frontend
        type_check
        build_ir
        verify
        ──────────── Interpreter

    compile_to_wasm(source, filename, output_name)
        prepare_frontend
        type_check
        build_ir
        verify
        ──────────── WASM

    compile(source, filename, output_name, ...)
        prepare_frontend
        type_check
        type_table_complete     ← compile-only
        build_ir
        verify
        optimize                ← compile-only
        reverify
        ──────────── LLVM

The backend-specific work diverges only at the end. The pass
pipeline itself, however, diverges *before* the final VerifiedIR:
`compile` runs two passes the other two do not.

The divergence is not a semantic difference. There is no
target-specific lowering rule before the backend boundary. The
optimizer is a pure rewrite proved semantics-preserving by the
release-hardening suite. The completeness check emits warnings.
Neither changes what the program means — they change how much
work the compiler does before reaching the same VerifiedIR.

The divergence is pipeline configuration drift, not different
language semantics. The `compiler_pipeline_equiv.rs` test file
exists as an oracle precisely because the paths are capable of
diverging silently.

## Decision

One canonical pipeline. All compilation entry points run the same
sequence:

    type_check → type_table_complete → build_ir
        → verify → optimize → reverify

The pipeline is a single `Pipeline::builder()` chain run through
the scheduler. It is expressed once, as `Compiler::run_pipeline`.

    fn run_pipeline(
        &self,
        program: &mut Program,
        ctx: &mut CompilerContext,
    ) -> Result<(VerifiedIR, ScheduleOutcome)>

The three entry points collapse to: `prepare_frontend` →
`run_pipeline` → target-specific work. The target-specific work
is only the capability check, the backend selection, and the
output handling. No pass is added or removed per target.

`build_semantic_ir_for` (the `inspect --ir` path) stays partial.
It produces unverified IR deliberately, because its purpose is to
show the IR between construction and verification. It runs
`type_check → build_ir` and stops at `IrState::Built`.

## Rationale

The optimizer being applied in two paths and skipped in the
third is exactly the shape that produces "the interpreter and the
LLVM output disagree" bugs. Making all three paths run it means
the only remaining difference between them is which backend
consumes the IR — the correct place for divergence to live.

The completeness check is cheaper to run everywhere than to
reason about which targets need it. It is a warning-only pass.

The scheduler already enforces the pipeline's level transitions
(`Ast → SemanticIr → VerifiedIr`), the `Transform`-needs-following-
`Verification` rule, and the "no lowering before the current
level" rule. Putting the six passes in one chain means those
checks apply to the actual production pipeline, not to three
partial ones that each pass trivially.

## Consequences

**Positive.**

- One pipeline, one place to change. Adding a pass means adding
  it to `run_pipeline`, and every target gets it.

- The `compiler_pipeline_equiv.rs` oracle becomes stricter. Today
  it verifies "the pass wrapper agrees with the direct call" —
  after this ADR it also verifies "the production pipeline runs
  the full sequence" by construction.

- Per-pass timing is preserved. `ScheduleOutcome.timings` carries
  the duration of each stage; `compile`'s `--timing` output reads
  them by `PassId`.

- `run_verify_pass`, `run_optimize_pass`, and `OptimizeTimings`
  are deleted. They existed to assemble partial pipelines. With
  one canonical pipeline, they have no callers.

**Negative.**

- `compile_to_wasm` and `run_interpreter` now pay for the
  completeness check and the optimizer. Both are milliseconds on
  conformance-sized inputs. If a future measurement shows a
  real cost, an opt-out is a target-independent configuration
  knob, not a fork in the pipeline.

- `compile`'s `--timing` output changes shape. Six pass timings
  are reported instead of the old phase names. This is
  user-visible; the replacement is a strictly more informative
  output.

**Neutral.**

- `build_semantic_ir_for` remains a separate, partial path. It is
  not a compilation entry point; it is a diagnostic tool.

## Tests

Two new tests in `tests/compiler_pipeline_equiv.rs`:

1. `run_pipeline_for_reaches_verified_state` — the pipeline runs
   on a well-formed program and returns a `VerifiedIR`.

2. `run_pipeline_for_rejects_invalid_program` — a program with a
   type error fails before reaching verified IR.

The four existing per-pass equivalence tests are unaffected —
they compare each pass to its direct-call equivalent, which
`run_pipeline` does not change.

## Alternatives considered

**Keep three partial pipelines and rely on the oracle test to
detect divergence.**

   Rejected. The oracle detects drift after it happens. The ADR
   removes the possibility of drift.

**Parameterize the pipeline with a `PipelineConfig` describing
which passes to run.**

   Rejected. The only consumer of that parameter would be the
   three entry points, and their correct behavior is to pass the
   same config. A parameter that no caller should ever change
   differently is not a parameter.

**Move the optimizer behind a `--optimize` flag, off by default,
   and run it only in `compile`.**

   Rejected. Optimization is proved semantics-preserving by
   `release_hardening.rs`. If it is safe to run for LLVM, it is
   safe to run for the interpreter and WASM.
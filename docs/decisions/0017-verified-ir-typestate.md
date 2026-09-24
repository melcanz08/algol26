# ADR 0017: VerifiedIR as a typestate boundary

Status: Proposed

## Context

`VerifiedIR` (`src/ir/verified_ir.rs`) exists and is architecturally
correct in three respects:

1. Its constructor `new()` runs the full verifier.
2. Its `mutate()` method consumes `self`, applies a mutation,
   re-runs the verifier, and returns a fresh `VerifiedIR`. There is
   no way to get back an invalid wrapper.
3. Backend entry points take `&VerifiedIR`, so a backend cannot be
   called on unverified IR by accident.

The wrapper's `from_verify_pass` is `pub(crate)` — it is not
forgeable from outside the crate. The permitted call sites are
`run_verify_pass`, `run_optimize_pass`, and the interpreter/WASM/
compile paths in `compiler.rs`. Each call is `debug_assert`-guarded,
so debug builds re-verify; release builds trust the caller.

What remains unclosed is the pipeline's own model of "is this IR
verified."

### Hole 1 — `Program::verified: bool`

`Program` (`src/compiler/program.rs`) carries:

    pub semantic_ir: Option<SemanticProgram>,
    pub verified: bool,

`run_verify_pass` sets `program.verified = true` after a successful
verify (compiler.rs:399). `run_optimize_pass` sets it again after
the trailing `VerifyIrPass` (compiler.rs:451). `run_build_ir_pass`
sets it to `false` (compiler.rs:484). `lower_to_llvm` reads it
implicitly by unwrapping `semantic_ir` and constructing a
`VerifiedIR` (compiler.rs:864).

A bool is not a proof. Any future pass, helper, or test-only code
path can read `semantic_ir` and ignore the boolean. The boolean and
the actual verification state can drift.

### Hole 2 — `VerifyIrPass`'s contract declares the wrong level

`IrLevel` (`src/compiler/pass.rs`) already has `SemanticIr`,
`VerifiedIr`, and `OptimizedIr` variants. They are unused by the
actual passes:

    VerifyIrPass:        input: SemanticIr, output: SemanticIr
    OptimizePass:        input: SemanticIr, output: SemanticIr
    BuildSemanticIRPass: input: Ast,        output: SemanticIr

If `VerifyIrPass` declared `input: SemanticIr, output: VerifiedIr`,
`PipelineBuilder::validate_chain` would enforce that no lowering
pass could run before verification. The scheduler's ordering
invariants would then be structural rather than conventional.

### What the Convergence Map asks

Section 4.D names four bullets:

- remove `Program::verified`
- remove `from_verify_pass()`
- have verification return `VerifiedIR` or an equivalent typestate token
- make optimized verified IR remain inside the verified wrapper

The second is partially done (`from_verify_pass` is `pub(crate)`,
not `pub`). The other three are open.

---

## Decision

The pipeline carries verification state as a type, not a boolean.
`Program::verified` is deleted. `Program`'s IR representation
becomes a single enum, and every pass helper that currently sets or
reads the boolean is rewritten to transition between enum variants.

### Field substitution

Replace:

    pub semantic_ir: Option<SemanticProgram>,
    pub verified: bool,

with a single field:

    pub ir: IrState,

where:

    #[derive(Debug, Default)]
    pub enum IrState {
        #[default]
        Absent,
        Built(SemanticProgram),
        Verified(VerifiedIR),
    }

Passes at level `SemanticIr` match `IrState::Built`. Passes at level
`VerifiedIr` match `IrState::Verified`. A verification pass takes
`IrState::Built`, constructs a `VerifiedIR`, and stores
`IrState::Verified`. A lowering pass requires `IrState::Verified`
and fails with a named error if it sees `Built` or `Absent`.

The enum makes "the IR is in exactly one of three states" a
machine-checked property. It replaces a boolean plus a program with
an enum plus a payload.

### Why not two fields

Two fields (`Option<SemanticProgram>` and `Option<VerifiedIR>`)
allow inconsistent combinations — both present, or neither present
when one is expected. A single enum removes that possibility. This
is the same reasoning that justified the ADR 0016 per-function
`FunctionCfg` instead of a flat Cfg with disambiguating side
channels.

### `from_verify_pass` stays

The method remains `pub(crate)` and gains one caller: `VerifyIrPass`
itself. Every other call site in `compiler.rs` is rewritten to read
from `program.ir` rather than construct a fresh `VerifiedIR` from
an unwrapped `SemanticProgram`.

After the rewrite:

- `run_verify_pass` replaces `IrState::Built` with
  `IrState::Verified`.
- `run_build_ir_pass` replaces whatever was there with
  `IrState::Built(fresh_ir)`.
- `run_optimize_pass` requires `IrState::Verified`, runs
  `OptimizePass` then `VerifyIrPass`, and leaves `IrState::Verified`.
- Backend entry points read `program.ir`, require
  `IrState::Verified`, and borrow `&VerifiedIR`.

No production call site constructs `VerifiedIR` from raw
`SemanticProgram` other than `VerifyIrPass` and `VerifiedIR::new()`.

### `IrLevel` and the contract chain

`VerifyIrPass` declares:

    input:  IrLevel::SemanticIr,
    output: IrLevel::VerifiedIr,
    kind:   PassKind::Lowering,   // it advances the level

`OptimizePass` keeps its current shape:

    input:  IrLevel::VerifiedIr,
    output: IrLevel::VerifiedIr,
    kind:   PassKind::Transform,  // scheduler requires following Verification

The trailing `VerifyIrPass` in the optimize pipeline has
`input: VerifiedIr, output: VerifiedIr` as a Verification. The
scheduler's `require_verification_after_transforms` enforces the
pairing. This is the current pipeline shape; only the level
declarations change.

### The six sites in `compiler.rs`

Only `compiler.rs` writes the boolean or constructs a `VerifiedIR`:

| Line | Current | After |
|---|---|---|
| 399 | `program.verified = true;` | `program.ir = IrState::Verified(v);` |
| 451 | `program.verified = true;` | `program.ir = IrState::Verified(v);` |
| 484 | `program.verified = false;` | `program.ir = IrState::Built(p);` |
| 508 | `VerifiedIR::from_verify_pass(program.semantic_ir.take()...)` | `match &program.ir { IrState::Verified(v) => v, ... }` |
| 665 | same as 508 | same as 508 |
| 864 | same as 508 | same as 508 |

The three "construct from scratch" sites at 508/665/864 disappear
entirely; the IR is already verified at those points.

---

## Consequences

**Positive.**

- A backend cannot be called on unverified IR. `program.ir` in
  state `Built` does not expose a `&VerifiedIR` accessor.
  Lowering passes fail with a named error if they see `Built` or
  `Absent`.

- The pipeline's level model and `Program`'s field model agree.
  Both say "SemanticIr -> VerifiedIr" is a real transition, and
  both enforce it. Today they say different things, and the
  runtime bool papers over the gap.

- `Program::verified` is deleted. There is one canonical
  representation of "is this IR verified" — the `IrState` enum.

- `IrLevel::VerifiedIr` and `IrLevel::OptimizedIr` are used. They
  currently exist in the enum but no pass references them. After
  this ADR, at least `VerifyIrPass` references `VerifiedIr`.

**Negative.**

- Every pass helper in `compiler.rs` is rewritten. Three helpers
  (`run_verify_pass`, `run_optimize_pass`, `run_build_ir_pass`)
  each change shape. Three entry points (`run_interpreter`,
  `compile_to_wasm`, `compile`) change how they read IR.

- The `IrState` enum introduces a third state (`Absent`) that no
  current pass checks explicitly. Failing on `Absent` when a
  `Built` is expected is a new error path; existing helpers already
  fail in similar cases via `Option::expect`, so the diagnostic
  behavior is equivalent but the message changes.

- Tests that construct a `Program` and set
  `program.semantic_ir = Some(...)` need updating to set
  `program.ir = IrState::Built(...)`. Several test files build a
  `Program` this way; the fix is mechanical.

**Neutral.**

- `VerifiedIR::from_verify_pass` remains. Deleting it entirely
  would require `VerifyIrPass` to construct a `VerifiedIR` through
  `VerifiedIR::new()`, which re-runs the verifier — doubling
  verification cost for every program. The current shape (verify
  once, wrap without re-verifying, debug-assert on the wrapper) is
  correct.

- `VerifyIrPass`'s contract gains a real `output` field. Every
  other verification pass in the pipeline keeps its current
  contract until a similar case appears.

---

## Tests

Five new tests in `tests/pipeline_contract.rs`:

1. `build_ir_produces_built_state` — after `BuildSemanticIRPass`
   runs, `program.ir` is `IrState::Built`.
2. `verify_pass_produces_verified_state` — after `VerifyIrPass`
   runs, `program.ir` is `IrState::Verified`.
3. `lowering_without_verification_is_refused` — a synthetic pass
   that requires `IrState::Verified` fails with a named error when
   `program.ir` is `IrState::Built`.
4. `optimize_pass_requires_verified_input` — same shape, for
   `OptimizePass`.
5. `well_formed_pipeline_reaches_verified_state` — the existing
   `BuildSemanticIRPass + VerifyIrPass` sequence lands at
   `IrState::Verified`.

Plus one regression test:

6. `backend_cannot_be_called_on_unverified_ir` — attempts to call
   a lowering entry point with `program.ir = IrState::Built`.
   Asserts the call fails with the named error rather than
   proceeding.

---

## Alternatives considered

**Leave `Program::verified: bool` and delete `from_verify_pass`.**

   Rejected. The bool is the primary bypass: any code that reads
   `semantic_ir` and ignores `verified` gets unverified IR.
   Deleting the constructors without deleting the bool leaves the
   reader side unchecked.

**Replace `semantic_ir` with `verified_ir: Option<VerifiedIR>` and
keep `verified: bool`.**

   Rejected. Same shape as the current code with the field renamed.
   Two fields, no invariant preventing `verified_ir: Some(...)`
   from coexisting with `verified: false`.

**Make `VerifiedIR` a compile-time marker via a generic parameter
on `Program`.**

   Out of scope. Would require threading a type parameter through
   `Pass<Prog>`, `Scheduler`, `Pipeline`, and every pass impl. The
   runtime enum is a smaller change with the same fail-closed
   behavior; a compile-time version is a larger refactor that could
   land later if the runtime cost matters.

**Delete `IrLevel::VerifiedIr` and `IrLevel::OptimizedIr` since
they are unused.**

   Rejected. They are the model. The point of this ADR is to make
   them used.
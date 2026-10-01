# ALGOL26 Pass Contracts

**Effective**: v0.8.0 (post-VerifiedIR typestate, 2026-10-01)

## What a pass contract is

Every compiler pass implements `Pass<Prog>` and returns a
`PassContract` from `contract()`. The contract has two halves:
fields the compiler checks, and fields a human reads.

### Machine-checked

Three fields participate in pipeline construction and execution:

- **`id: PassId`** — stable identifier, used in logs, error
  messages, and the `inspect --passes` output. Format:
  `<level>.<name>`, e.g. `ir.build`, `ast.type_check`.
- **`kind: PassKind`** — one of `Analysis`, `Transform`,
  `Verification`, `Lowering`, `Annotation`. Determines the chain
  rules the pipeline enforces.
- **`input`, `output: IrLevel`** — the IR level consumed and
  produced. `Lowering` must strictly advance. All other kinds
  must not change level.

`PipelineBuilder::validate_chain` refuses to build a pipeline where
these three fields are inconsistent (`src/compiler/pipeline.rs`).
`Scheduler::run` refuses to run a `Transform` not immediately
followed (skipping `Analysis`/`Annotation`) by a `Verification` at
the same level (`src/compiler/scheduler.rs`).

### Human-read

Five fields are metadata: they document what the pass promises, in
prose. The compiler does not evaluate them at runtime.

- **`requires`** — preconditions the pass assumes. If a
  precondition is false, the pass should return `PassError::new`
  naming the violation, not proceed.
- **`guarantees`** — postconditions the pass establishes on
  success. Downstream passes may rely on these.
- **`may_change`** — the specific parts of `Program` the pass
  writes.
- **`must_preserve`** — invariants downstream passes rely on. A
  pass that changes something on this list is a bug.
- **`may_fail`** — whether failure is expected (user error) or a
  contract violation (compiler bug). Informational; the scheduler
  treats both as errors that stop the pipeline when
  `stop_on_error` is set (the default).

The compiler cannot check prose. A test
(`tests/pass_contracts.rs`) enforces that the prose is *present* —
every registered pass must declare non-empty `requires`,
`guarantees`, `may_change`, and `must_preserve` — but what the
prose *says* is a matter of code review.

## Pipeline-level rules

Enforced by `PipelineBuilder::validate_chain` and `Scheduler`,
regardless of individual pass contracts:

1. **Chain continuity.** Each pass's `input` must equal the
   previous pass's `output`, or the pass must be `Analysis` /
   `Annotation` (which read at the current level and produce no
   new level).
2. **Lowering advances.** `Lowering` passes must satisfy
   `output > input` in the `IrLevel` ordering:
   `Source < Ast < SemanticIr < VerifiedIr < OptimizedIr <
   Lowered < Backend`.
3. **Transform followed by Verification.** A `Transform` pass
   must be followed (skipping `Analysis` and `Annotation`) by a
   `Verification` pass at the same level. Set
   `Scheduler::require_verification_after_transforms = false` to
   disable.

Rules 1 and 2 are checked at pipeline build time. Rule 3 is checked
at run time, before the `Transform` executes.

## The canonical pipeline

`Compiler::run_pipeline` in `src/compiler.rs` builds the pipeline
that every entry point runs: `check`, `run`, `build`, `wasm`, and
`inspect --ir` / `--type-table`.

```
TypeCheckPass             ast.type_check         Ast → Ast
TypeTableCompletePass     ast.type_table_complete Ast → Ast
BuildSemanticIRPass       ir.build               Ast → SemanticIr
VerifyIrPass              ir.verify              SemanticIr → VerifiedIr
OptimizePass              ir.optimize            VerifiedIr → VerifiedIr
ReVerifyPass              ir.reverify            VerifiedIr → VerifiedIr
```

The ordering is deliberate. `VerifyIrPass` promotes the program from
`SemanticIr` to `VerifiedIr` (see ADR 0017). `OptimizePass` consumes
a `VerifiedIr` and produces one — it does not change the level.
`ReVerifyPass` re-checks the invariants after optimization without
pretending the level changed. Together the promote-then-recheck
split lets a `Transform` be verified both on entry and on exit,
using the same verification code at both points.

`inspect --passes` renders this pipeline along with each pass's
contract metadata.

## Registered passes

### `ast.type_check` — `TypeCheckPass`

**File**: `src/compiler/passes/type_check.rs`

| Field | Value |
|---|---|
| Kind | `Annotation` |
| Input → Output | `Ast` → `Ast` |
| Requires | parsed AST with traits and impls; span map |
| Guarantees | every expression reachable from a function body has a type in `type_table`; no data races detected between spawned functions |
| May change | `program.typed` |
| Must preserve | `program.ast`, source spans |
| May fail | yes (user errors) |

### `ast.type_table_complete` — `TypeTableCompletePass`

**File**: `src/compiler/passes/type_table_complete.rs`

| Field | Value |
|---|---|
| Kind | `Analysis` |
| Input → Output | `Ast` → `Ast` |
| Requires | typed AST with analyzer-produced type table |
| Guarantees | every reachable `Expr` node is checked for a `type_table` entry |
| May change | `diagnostics` |
| Must preserve | `program.ast`, `program.typed` |
| May fail | no (warnings only) |

The check runs even though every expression reachable from an
ordinary program has an entry. It is a safety net for the case
where a new AST node or a new analyzer path forgets to record a
type. A test in the pass's own module exercises both directions.

### `ir.build` — `BuildSemanticIRPass`

**File**: `src/compiler/passes/build_ir.rs`

| Field | Value |
|---|---|
| Kind | `Lowering` |
| Input → Output | `Ast` → `SemanticIr` |
| Requires | typed AST with analyzer-produced type table; a closed `InstantiationPlan` |
| Guarantees | every function is lowered to a `SemanticFunction`; every concrete generic specialization the plan records is emitted; every expression has an assigned block id |
| May change | `program.semantic_ir` |
| Must preserve | `program.ast`, `program.typed`, source spans |
| May fail | yes |

The CFG verifier runs as part of the `VerifyIrPass` immediately
after this pass, not inside it. This pass produces IR; the next
pass verifies that the IR is well-formed.

### `ir.verify` — `VerifyIrPass`

**File**: `src/compiler/passes/verify_ir.rs`

| Field | Value |
|---|---|
| Kind | `Verification` |
| Input → Output | `SemanticIr` → `VerifiedIr` |
| Requires | semantic IR built; types resolved; CFG valid |
| Guarantees | instruction operands are well-typed; branch conditions are `Bool`; returns coerce to function return type; calls resolve to known signatures; CFG structural invariants hold; no `Type::TypeVar` in executable IR |
| May change | `diagnostics`; `program.ir` state (promotes `Built` to `Verified`) |
| Must preserve | `program.ast`, `program.typed`, source spans |
| May fail | yes |

This pass **promotes** rather than checking in place. On success,
the program's `IrState` transitions from `Built` to `Verified`.
The `VerifiedIr` type ensures subsequent passes cannot run against
unverified IR.

### `ir.optimize` — `OptimizePass`

**File**: `src/compiler/passes/optimize.rs`

| Field | Value |
|---|---|
| Kind | `Transform` |
| Input → Output | `VerifiedIr` → `VerifiedIr` |
| Requires | verified IR |
| Guarantees | same observable behavior; IR remains well-formed |
| May change | instructions within blocks; block structure |
| Must preserve | program semantics; function signatures; types; source spans |
| May fail | no (pure rewrite) |

Six optimizer sub-passes run in order: unreachable-block removal,
constant folding, constant propagation, dead-code elimination,
branch simplification, and a second unreachable-block removal.
See `docs/ir-transformations.md` for what each does.

### `ir.reverify` — `ReVerifyPass`

**File**: `src/compiler/passes/verify_ir.rs` (same file as `VerifyIrPass`)

| Field | Value |
|---|---|
| Kind | `Verification` |
| Input → Output | `VerifiedIr` → `VerifiedIr` |
| Requires | verified IR that has been through an optimization pass |
| Guarantees | the same invariants as `ir.verify`, re-established after the optimizer has rewritten the program |
| May change | `diagnostics` |
| Must preserve | `program.ast`, `program.typed`, source spans |
| May fail | yes |

This pass exists because the optimizer is not trusted to preserve
verification invariants by construction. Running the verifier again
after optimization catches any transform that produced IR the
verifier would have rejected on the first pass. Without this, a
correctness bug in the optimizer would reach the backends.

The pass is a `Verification` at the same level as its input — it
does not promote, because the program is already `Verified`.
Re-running the check on already-verified IR is the point.

## Adding a new pass

1. Implement `Pass<Program>` with a non-empty `contract()`.
   Empty `requires`, `guarantees`, `may_change`, or
   `must_preserve` will fail `tests/pass_contracts.rs`.
2. Add the pass to the canonical pipeline in
   `Compiler::run_pipeline` (`src/compiler.rs`). Every entry point
   runs this pipeline; there is no separate registry to update.
3. If the pass is a `Transform`, ensure a `Verification` pass
   exists at the same level and follows it in the pipeline.
   `Scheduler::run` will refuse to run the pipeline otherwise.
4. Add a section to this document describing the pass's kind,
   levels, and contract fields.
5. Run `cargo test --test pass_contracts` to confirm.

## Enforcement summary

| Rule | Enforced by | When |
|---|---|---|
| Chain continuity | `PipelineBuilder::validate_chain` | pipeline build |
| Lowering advances | `PipelineBuilder::validate_chain` | pipeline build |
| Transform followed by Verification | `Scheduler::run` | before running the Transform |
| Non-empty contract fields | `tests/pass_contracts.rs` | test time |
| `Lowering` advances / non-`Lowering` stays | `tests/pass_contracts.rs` | test time |
| `VerifiedIr` promotion | `VerifyIrPass` returns `IrState::Verified` | pass run |

The first three are runtime invariants: they hold for every pipeline,
whether or not the test suite runs. The next two are test-time
assertions over the registered passes; they catch a bad contract at
commit time rather than at code generation. The last is the
typestate guarantee: a program cannot reach a backend without
having passed through `VerifyIrPass`.

## See also

- `docs/ir-transformations.md` — what each IR transformation does
  (loop desugaring, generic specialization, defer lowering, the
  optimizer's passes, IR verification, capability scanning).
- `docs/decisions/0017-verified-ir-typestate.md` — the promotion
  model this doc describes.
- `docs/decisions/0018-canonical-pipeline.md` — the single
  `run_pipeline` shared by every entry point.
- `src/compiler/pass.rs` — the `Pass` trait and `PassContract`
  struct.
- `src/compiler/pipeline.rs` — chain validation.
- `src/compiler/scheduler.rs` — Transform / Verification ordering.
# ALGOL26 Pass Contracts

**Effective**: v0.8.0 (post-Canonical-IR, 2026-09-20)

## What a pass contract is

Every compiler pass implements `Pass<Prog>` and returns a
`PassContract` from `contract()`. The contract has two halves: fields
the compiler checks, and fields a human reads.

### Machine-checked

Three fields participate in pipeline construction and execution:

- **`id: PassId`** — stable identifier, used in logs, error
  messages, and the `inspect --passes` output (planned). Format:
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

## Registered passes

The five passes registered today, in the order they appear in
`main.rs`. `TypeTableCompletePass` exists but is not in the default
registry — it is a diagnostic aid, not a required stage.

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

### `ir.build` — `BuildSemanticIRPass`

**File**: `src/compiler/passes/build_ir.rs`

| Field | Value |
|---|---|
| Kind | `Lowering` |
| Input → Output | `Ast` → `SemanticIr` |
| Requires | typed AST with analyzer-produced type table |
| Guarantees | every function is lowered to a `SemanticFunction`; every expression has an assigned block id; the resulting program passes CFG verification |
| May change | `program.semantic_ir` |
| Must preserve | `program.ast`, `program.typed`, source spans |
| May fail | yes |

### `ir.optimize` — `OptimizePass`

**File**: `src/compiler/passes/optimize.rs`

| Field | Value |
|---|---|
| Kind | `Transform` |
| Input → Output | `SemanticIr` → `SemanticIr` |
| Requires | semantic IR verified at least once |
| Guarantees | same observable behavior; IR remains well-formed |
| May change | instructions within blocks; block structure |
| Must preserve | program semantics; function signatures; types; source spans |
| May fail | no (pure rewrite) |

### `ir.verify` — `VerifyIrPass`

**File**: `src/compiler/passes/verify_ir.rs`

| Field | Value |
|---|---|
| Kind | `Verification` |
| Input → Output | `SemanticIr` → `SemanticIr` |
| Requires | semantic IR built; types resolved; CFG valid |
| Guarantees | instruction operands are well-typed; branch conditions are `Bool`; returns coerce to function return type; calls resolve to known signatures |
| May change | `diagnostics` |
| Must preserve | `program`; source spans |
| May fail | yes |

## Adding a new pass

1. Implement `Pass<Program>` with a non-empty `contract()`.
   Empty `requires`, `guarantees`, `may_change`, or
   `must_preserve` will fail `tests/pass_contracts.rs`.
2. Register it in the registry build site (`src/main.rs`,
   `build_registry` function).
3. If the pass is a `Transform`, ensure a `Verification` pass
   exists at the same level and follows it in the pipeline.
   `Scheduler::run` will refuse to run the pipeline otherwise.
4. Add a section to this document.
5. Run `cargo test --test pass_contracts` to confirm.

## Enforcement summary

| Rule | Enforced by | When |
|---|---|---|
| Chain continuity | `PipelineBuilder::validate_chain` | pipeline build |
| Lowering advances | `PipelineBuilder::validate_chain` | pipeline build |
| Transform followed by Verification | `Scheduler::run` | before running the Transform |
| Non-empty contract fields | `tests/pass_contracts.rs` | test time |
| `Lowering` advances / non-`Lowering` stays | `tests/pass_contracts.rs` | test time |

The first three are runtime invariants: they hold for every pipeline,
whether or not the test suite runs. The last two are test-time
assertions over the registry; they catch a bad contract at commit
time rather than at code generation.

## See also

- `docs/ir-transformations.md` — what each IR transformation
  does (loop desugaring, monomorphization, defer lowering, the
  optimizer's passes, IR verification). Renamed from
  `ir-pass-contracts.md`, which conflated transformations with
  pass contracts.
- `src/compiler/pass.rs` — the `Pass` trait and `PassContract`
  struct.
- `src/compiler/pipeline.rs` — chain validation.
- `src/compiler/scheduler.rs` — Transform / Verification ordering.
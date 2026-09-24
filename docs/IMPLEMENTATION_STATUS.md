# ALGOL26 Implementation Status

Last updated: 2026-09-24

This document records what actually works, verified by the differential
corpus in `tests/corpus/`. A feature is only listed as "works" if there
is at least one corpus program exercising it end-to-end.

## Changes since 2026-09-21

Convergence work: Phases 0–7 of the migration in
`ALGOL26_CONVERGENCE_MAP.md`.

- **Compiler pipeline.** One canonical pipeline
  (`Compiler::run_pipeline`) shared by `compile`, `run_interpreter`,
  and `compile_to_wasm`. See ADR 0018.
- **VerifiedIR typestate.** `Program::verified: bool` replaced by an
  `IrState` enum (`Absent` / `Built` / `Verified`). See ADR 0017.
- **Per-function dataflow.** Every function is analyzed;
  parameters seed the entry state; diagnostics are deduplicated.
  See ADR 0016.
- **Executable-IR invariant check.** `crate::ir::verifier::invariants`
  rejects `Type::TypeVar` in executable IR. See ADR 0014.
- **Generic instantiation closure.** `InstantiationPlan::close`
  materializes transitive specializations. The pipeline no longer
  calls a pre-typecheck `Monomorphizer`. See ADR 0013.
- **Unsafe enforcement.** Raw pointer dereference, `alloc`, and
  `free` are permitted only inside `unsafe` blocks. See ADR 0015.
- **Backend capability truth.** `Feature::References`,
  `Feature::Channels` refusal, and iterator-metadata fail-closed
  behavior. See ADRs 0019–0021.
- **Removed:** dormant `CaptureMode` field on `VariableInfo`.
- **Removed:** `escape.rs`, `flow_analyzer.rs`, `control_flow.rs`,
  `span_map`, `TerminatorKind`, `OptimizationReport`.
- **Docs:** `docs/features/region.md` and `docs/features/unsafe.md`
  corrected to describe what is enforced, not what was intended.

## Changes since 2026-09-14

This revision reflects a two-day session of compiler work:

- **Removed:** the "Deferred runtime modules" section. `src/runtime/region.rs`
  and `src/runtime/region_memory.rs` were deleted (they had no callers).
- **Added:** a "Compiler infrastructure" section describing the pass
  pipeline contracts, `--timing`, and the `inspect` subcommands.
- **Added:** the write-through-`&mut` backend gap, discovered during
  an ADR correction this session. See ADR 0010 Phase 2.
- **Added:** the region-boundary `break`/`continue` leak fix.
- **Added:** canonical IR variant names throughout. `Borrow` / `MutBorrow`
  → `BorrowShared` / `BorrowMutable`; `Deref` → `ReadReference`;
  `Send` / `ChannelSend` → `SendChannel`; `Receive` / `ChannelReceive`
  → `ReceiveChannel`.
- **Added:** CFG verifier rules enforcing the `Fork` shape (ADR 0011).
- **Removed:** the `eval_call` error-swallowing entry from the open
  bugs list. The interpreter propagates user-function errors; the
  entry described a state that no longer exists.

## Known divergences between backends

The interpreter and LLVM backends agree on observable behavior for
all corpus programs. The following non-corpus divergences are
documented so future work can close them:

- **Region auto-free does not track outer-pointer overwrite.**
  If a `region` block reassigns a `var` whose pointer was allocated
  *outside* the region, the outer allocation is not freed at
  region exit. Workaround: do not reuse an outer pointer variable
  as region-local scratch storage — declare a fresh `var` inside
  the block.
- **Write-through-`&mut` is accepted but not lowered.** Assigning
  to a variable whose declared type is `MutBorrow(T)` is the
  language's write-through syntax. The analyzer and verifier
  implement the rule; both backends do not. `Instruction::Assign`
  in LLVM codegen and the interpreter stores the value into the
  reference variable's slot instead of writing through the
  reference. See `docs/decisions/0010-canonical-ir.md` Phase 2
  for the corrected premise and the fix plan.
- **Variadic FFI argument *types* are not validated.** `extern "C"
  function printf(fmt: String, ...)` accepts any number of arguments
  at or above the fixed count, and the analyzer records every
  argument's type in the type table. The extra arguments' types are
  not checked against the format string — that is C-level undefined
  behavior, and no compiler catches it. Callers must match the
  format specifiers to the argument types themselves.
- **WASM execution requires the Node host shim.** The compiler
  links `.wasm` output via `wasm-ld`; the resulting module is
  runnable through `runtime/wasm/host.js`, which provides the C
  library imports. The shim's `malloc` is a bump allocator
  (no `free`); the C varargs ABI requires dereferencing by slot
  on the shim side, which is handled for the built-in `printf`
  format specifiers. A WASM-specific libc is out of scope.

## Compiler infrastructure (2026-09-20 / 2026-09-21)

The compiler's phase sequence is now visible and enforced through
the pass pipeline (`src/compiler/pipeline.rs`, `scheduler.rs`,
`registry.rs`, `pass.rs`).

- **Pass contracts.** Every pass declares a `PassContract`
  (`id`, `kind`, `input`/`output` IR level, and prose fields for
  `requires` / `guarantees` / `may_change` / `must_preserve`).
  See `docs/pass-contracts.md`.
- **Enforcement.** `PipelineBuilder::validate_chain` enforces
  chain continuity and level advancement at build time.
  `Scheduler::run` enforces Transform-must-be-followed-by-
  Verification at run time. `tests/pass_contracts.rs` enforces
  non-empty metadata for every registered pass.
- **Every compile path routes through the pipeline.** The LLVM
  path (`compile`), `run_interpreter`, `compile_to_wasm`, and
  `inspect --ir` / `--type-table` all invoke passes via the
  scheduler. Before this session, `run_optimize_pass` called
  `Optimizer` directly, so the Transform→Verification check
  never fired on the compiler's actual optimize path.
- **`--timing`.** Prints per-phase compile durations (lex,
  parse, imports, desugar, expand, mono, type_check, safety,
  ir_build, verify_pre, optimize, verify_post, lower,
  type_table_check). Previously only shown when total compile
  time exceeded one second.
- **`inspect` subcommands.** `--tokens`, `--ast`, `--ir`, `--cfg`,
  `--passes`, `--capabilities`, `--type-table`. `--ast` / `--ir`
  / `--cfg` render source-shaped output via `src/frontend/ast_display.rs`
  and `src/ir/semantic_ir/display.rs`.

## Legend

- ✅ **Works** — verified by corpus program(s)
- ⚠️ **Interpreter only** — works in the interpreter, LLVM refuses or miscompiles
- ❌ **Parser only** — parser and analyzer accept it, no backend runtime
- ⛔ **Not supported** — explicitly refused with a clear error
- 🔶 **Syntax only** — parses and analyzes; no backend executes it
- ❓ **Untested** — no corpus coverage yet

## Feature Matrix

| Feature | Parser | Analyzer | Interpreter | LLVM | Corpus |
|---------|--------|----------|-------------|------|--------|
| `while` | ✅ | ✅ | ✅ | ✅ | 18–22, 34–36 |
| `for ... in` | ✅ | ✅ | ✅ | ✅ | 01, 02, 07, 12, 17 |
| `if` / `else` | ✅ | ✅ | ✅ | ✅ | 02, 19 |
| `break` / `continue` | ✅ | ✅ | ✅ | ✅ | 19, 22 |
| `defer` | ✅ | ✅ | ✅ | ✅ | 06, 12, 33 |
| Functions / procedures | ✅ | ✅ | ✅ | ✅ | 03–05, 11, 16, 34–36 |
| Return values | ✅ | ✅ | ✅ | ✅ | 04, 05, 09 |
| Bare call statements | ✅ | ✅ | ✅ | ✅ | 33 (fixed 2026-09-14) |
| Lists + indexing | ✅ | ✅ | ✅ | ✅ | 01, 07, 15 |
| `List.length` / `List.sum` | ✅ | ✅ | ✅ | ✅ | 07, 15 |
| Strings | ✅ | ✅ | ✅ | ✅ | 08, 15 |
| `String.length` / `to_upper` / `to_lower` | ✅ | ✅ | ✅ | ⚠️ | 08 |
| `Math.*` | ✅ | ✅ | ✅ | ⚠️ | — |
| `Option` / `Some` / `None` | ✅ | ✅ | ✅ | ⛔ | 13 |
| `match` (literal patterns) | ✅ | ✅ | ✅ | ✅ | — |
| `match` (binding patterns) | ✅ | ✅ | ✅ | ⛔ | 13 |
| `Result` / `Ok` / `Error` | ✅ | ✅ | ✅ | ⛔ | 14 |
| `try` / `catch` | ✅ | ✅ | ✅ | ⛔ | 14 |
| Traits + impls | ✅ | ✅ | ✅ | ⚠️ | 28, 29, 37 |
| `region` | ✅ | ✅ | ✅ | ✅ | 30, 31 |
| `unsafe` | ✅ | ✅ | ✅ | ✅ | 32, 33 |
| `spawn` / `parallel` | ✅ | ✅ | ✅ | ⛔ | — |
| `channel` / `send` / `receive` | ✅ | ✅ | ⛔ | ⛔ | 23–26 |
| `alloc` in `var` position | ✅ | ❓ | ❓ | ❓ | — |
| `alloc(x)` as statement | ✅ | ✅ | ✅ | ❓ | — |
| `free` | ✅ | ✅ | ✅ | ❓ | — |
| `extern` (FFI) | ✅ | ✅ | ⛔ | ✅ | — |
| `import` | ✅ | ✅ | ✅ | ✅ | — |

**Note on canonical IR names.** The IR builder now emits `SendChannel`
and `ReceiveChannel` (not `Send` / `ChannelSend` / `Receive` /
`ChannelReceive`). User-facing syntax is unchanged.

**Note on `Option` / `Result` refusal markers.** Changed from ❌ to ⛔
to match the legend: the LLVM backend refuses these constructs via
the capability check, it does not silently fail. The ❌ marker means
"no backend runtime at all"; ⛔ means "explicitly refused."

## Known Gaps

Gaps are documented by `tests/corpus/*.gol` programs marked
`// KNOWN_FAILURE:` or `// BACKEND: interpreter`.

### Analyzer soundness gaps (accepted, not yet fixed)

These are cases where the analyzer accepts programs that a stricter
borrow system would reject. They are documented rather than silently
patched because fixing each one correctly requires design decisions
that belong in their own ADR, and past attempts have regressed real
programs.

- **`&mut x` in a call argument is not registered as a borrow.**
  Writing `f(&mut x)` does not mark `x` as mut-borrowed, so
  `f(&mut x); g(&mut x)` in the same scope compiles even though both
  functions may write through the same reference. A naive fix
  (marking the borrow in `Expr::MutBorrow`) makes
  `increment(&mut value); print(value);` fail, because the current
  borrow model is fully lexical and has no way to release a borrow
  when the statement that created it completes. A proper fix needs
  statement-scoped release (small, ~2 days) or full non-lexical
  lifetimes (large). Not a memory-safety hole in the current
  runtime — references are addresses, not aliased Rust-style
  references — but looser than Rust.

- **No general pointer-lifetime enforcement.** Region exit frees
  allocations, but no check rejects a pointer value that outlives
  its source region. Returning a region-allocated pointer, storing
  one in an outer-scope variable, or returning a reference inside
  an aggregate are all accepted. See `docs/features/region.md`,
  section "What is not enforced today".

### IR correctness

Fixed in the Phase 3 audit (2026-09-14):

- Constant propagation no longer leaks constants across branch
  blocks. Before the fix, a conditional assignment in one branch
  could rewrite a join-block use with the wrong value.
- Verifier recursively checks `Some`/`None`/`Ok`/`Error` payloads.
  Before the fix, an ill-typed payload inside an `Option` or
  `Result` constructor was silently accepted.
- Verifier rejects `Float` arguments for `Int` parameters. Now
  matches the analyzer's `can_coerce_to` rules.
- Optimizer skips folding large `Int` arithmetic where `f64`
  intermediates would lose precision (above 2^53).

Fixed in the Canonical IR session (2026-09-19 / 2026-09-20):

- Canonical IR variants landed: `Borrow` / `MutBorrow` → `BorrowShared`
  / `BorrowMutable`; `Deref` → `ReadReference`; `Send` / `ChannelSend`
  → `SendChannel`; `Receive` / `ChannelReceive` → `ReceiveChannel`.
  Eight variants reduced to five canonical operations.
- CFG verifier rules for `Fork` shape: each branch must be entered
  only from the fork block, must not contain nested concurrency or
  `return`, and the join block must not also appear as a branch.
  See ADR 0011.
- Region-boundary `break`/`continue` leak fixed at the analyzer
  level. Previously `break` or `continue` crossing a region
  boundary skipped `RegionExit`, leaking frames (and every
  allocation they held) until the enclosing function returned.

Deferred (design/cleanup, not correctness bugs):

- `builtins.rs` signature table is hand-synced with
  `builder/build.rs`; no test enforces the sync.
- Dominance-aware constant propagation would allow cross-block
  folding (currently disabled to preserve correctness).
- `Spawn`/`Fork` capture semantics are not verified. The
  `CaptureMode` field intended to carry this information was
  removed in this session. Adding real capture verification
  requires its own ADR. See `docs/decisions/0008-concurrency-model.md`.

### Backend audit

Fixed (cumulative through 2026-09-21):

- LLVM codegen: `NotEqual` on pointers now emits `NE` (was `EQ`).
- LLVM codegen: switch with no default emits `unreachable` (was an
  arbitrary block).
- LLVM codegen: list reassignment rebuilds the array instead of
  leaving `list_arrays` pointing at the old allocation.
- LLVM codegen: `Some`/`Ok`/`Error`/`None` are refused at capability
  check instead of silently unwrapped. The interpreter handles them.
- Production LLVM path now goes through `Backend::compile`, so
  `module.verify()` and the capability check run on the same path
  users exercise.
- Interpreter: iteration limit raised from 10_000 to 100_000_000;
  `Fork` runs every branch sequentially (was: only the first);
  Math builtins complete; `List.max`/`min`, `String.substring`
  added; Math builtins accept `Int` inputs.
- WASM: dead `validate_wasm_compatibility` replaced by the
  capability system; `module.verify()` runs before write.
- Capability: `scan_call_name` uses `builtin_signatures()` instead
  of a prefix match — user-defined `String.helper` is no longer
  misclassified.
- `BackendOutput` now carries real data (paths, stdout).
- Pipeline routing: `run_optimize_pass` now runs `OptimizePass` +
  `VerifyIrPass` through the scheduler (was: direct `Optimizer`
  call). `run_interpreter` and `compile_to_wasm` now use
  `run_verify_pass` (was: direct `semantic_ir.verify()`).
  Removes two redundant verifications and one program clone.

Open (not fixed):

- WASM output has unresolved C library imports (`printf`, `exit`,
  `sqrt`, `strlen`, `strcat`). Module is not executable without a
  host shim. Needs a design decision.
- `InterpreterBackend` clones the program per compile.
- Interpreter runtime errors are `Debug`-formatted.

### LLVM backend gaps (interpreter works)

- **`match` with pattern bindings** (`corpus_13`). LLVM refuses with a
  clear error. Root cause: the IR builder records bindings in its
  internal scope but does not emit `Declare` instructions for them,
  so the LLVM codegen cannot resolve the binding.
- **Iterating over a list parameter** (`corpus_10`). LLVM's iterator
  setup is only done for locally-created list literals. Lists passed
  as function arguments have no iterator state.
- **`try` / `catch` and `Result` values** (`corpus_14`). No LLVM
  lowering. Refused cleanly.

### Parser gaps

- *(resolved)* `alloc` in `var` position. `parse_primary` in
  `src/frontend/parser/expr.rs` now handles `Token::Alloc` in
  expression position, so `val p := alloc(8)` parses. End-to-end
  behavior through the analyzer / IR / backends has not been
  re-verified since the parser change.

### Unimplemented features

- **Channels** (`corpus_23`–`corpus_26`). Parser and analyzer
  support `channel c: T`, `send c, v`, and `receive c into x`.
  All three backends refuse channel programs at the capability
  boundary. The interpreter's channel instruction arms return
  `EvalError::Unsupported` as a defensive guard. See ADR 0020.

## Conventions

**Language design note:** methods are called as `x.method(x)` — the
receiver is passed explicitly as the first argument. Zero-argument
methods are called as `x.method()`. There is no implicit `self`.

**Backend selection:** programs that require interpreter-only features
declare `// BACKEND: interpreter` at the top of the file. The
`corpus_diff` harness runs them through the interpreter; others run
through LLVM by default.

## See also

- `docs/decisions/0010-canonical-ir.md` — the canonical IR migration.
- `docs/decisions/0011-phase4-task-model.md` — the `Fork` shape
  investigation and the CFG verifier rules.
- `docs/pass-contracts.md` — the pass contract model and pipeline
  enforcement rules.
- `README.md` — user-facing overview.

## How to add a feature to this document

1. Write a corpus program in `tests/corpus/` that exercises the feature.
2. Run `cargo test --test corpus_diff`.
3. If it passes, mark the feature ✅ in the matrix.
4. If it fails in a backend, mark it ⚠️ or ⛔, add a `// BACKEND:` or
   `// KNOWN_FAILURE:` header, and add a bullet under "Known Gaps."
5. Update the "Last updated" date.

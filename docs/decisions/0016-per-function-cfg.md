# ADR 0016: Per-function CFG and dataflow

Status: Proposed

## Context

`build_cfg_from_semantic_program` in `src/ir/cfg/builder.rs`
flattens every non-extern function's blocks into a single `Cfg`.
Block IDs are renumbered into a single flat namespace, `cfg.entry`
is set to `0`, and `add_edge` is called for every terminator
inside every function. The result is a disconnected graph: `main`
and `helper` sit in the same Cfg with no edge between them,
because a `CfgInstruction::Call` does not create a CFG edge.

`DataflowEngine::run` starts its worklist at `cfg.entry` and
expands only through predecessors and successors. It therefore
reaches exactly one connected component — the one containing
block `0`, which is the first non-extern function in
`program.functions` order. Every other function is silently
unanalyzed.

Two further defects compound this:

1. Function parameters are not given entry state. `run` accepts
   an `_initial: SemanticState` parameter but discards it. The
   entry block's incoming state is `SemanticState::new()` — an
   empty state. A parameter `p: Int` is not declared, so the
   first `CfgInstruction::Use { name: "p" }` inside the function
   sees no entry for `p` and produces no diagnostic.

2. Diagnostics accumulate across worklist iterations. A block
   re-processed after its incoming state changes re-emits every
   diagnostic it produced before. The result carries duplicates
   of the same message.

The consequence is that whole-program verification today is
whole-`main` verification. Functions called from `main` are
compiled but not dataflow-checked.

## Decision

The CFG is per-function. `build_cfg_from_semantic_program` returns
one `FunctionCfg` per non-extern function; the dataflow engine
runs each independently, with the function's parameters as the
entry state.

    pub struct FunctionCfg {
        /// The function's mangled name, for diagnostics.
        pub name: String,
        /// Block IDs are local to this function. The entry is 0.
        pub cfg: Cfg,
        /// Parameters of this function, in declaration order, as
        /// `(name, is_mutable)`. Used to seed the entry state.
        pub params: Vec<(String, bool)>,
    }

    pub fn build_cfgs_from_semantic_program(
        program: &SemanticProgram,
    ) -> Vec<FunctionCfg>

`DataflowEngine::run_all(&[FunctionCfg]) -> DataflowResult`
iterates the functions. For each, it constructs the entry state
from `params`, walks the worklist to fixpoint, and records
diagnostics tagged with the function name.

## Four changes

### 1. Per-function block namespaces

Each function's blocks keep their `SemanticProgram`-local IDs.
The old renumbering into a single flat namespace is deleted, as
is the `block_id_map`. `Cfg::new(0)` gives every function the
same entry block ID, which is fine because block IDs are only
compared within one `FunctionCfg`.

Edges are built by the same terminator walk, but scoped to a
single function's blocks.

### 2. Entry state from parameters

`DataflowEngine::run` currently takes `_initial: SemanticState`
and discards it. That parameter is deleted, replaced by
`FunctionCfg::params`. For each parameter:

    let mut entry = SemanticState::new();
    for (name, _mutable) in &func.params {
        entry.declare(name.clone(), VarState::available());
    }

`VarState` has no mutability dimension — mutability is enforced
by the analyzer at the AST level, not by dataflow. Declaring a
parameter as `available` is enough to prevent spurious
`E-INIT-001` diagnostics on the first use; it does not weaken
the ownership model.

### 3. Per-function diagnostics

`DataflowResult.diagnostics` gains a `function: String` field
per entry. `run_all` prefixes each diagnostic's message with
`"[{function}]"` before returning, so `VerifyIrPass`'s existing
error aggregation shows which function produced each problem.

### 4. Diagnostic deduplication

The worklist re-processes a block whenever its incoming state
changes. Each re-processing re-emits every diagnostic the block
produced before. `run_all` collects diagnostics into a
`HashSet<(String, usize, String)>` keyed by
`(function_name, block_id, message)` and returns the set as a
`Vec` at the end. Two diagnostics with the same message in the
same block of the same function are the same diagnostic; order
is not preserved, which is correct for a set of facts.

### What does not change

- `CfgInstruction`, `CfgBlock`, `Cfg` keep their shapes.
- The `Transfer` trait and `OwnershipTransfer::transfer` are
  unchanged. The transfer function already receives the block's
  incoming state and returns its outgoing state; per-function
  execution only changes who calls it and with what entry state.
- `VerifyIrPass` calls `build_cfgs_from_semantic_program`
  instead of `build_cfg_from_semantic_program`, and
  `DataflowEngine::run_all` instead of `run`. The surrounding
  error-handling is unchanged.
- `build_cfg_from_blocks` in `dataflow.rs` is a test-only helper
  and stays.

## Consequences

**Positive.**

- Every non-extern function is analyzed. Today, only the
  connected component containing block `0` is reached; after
  this change, each function has its own entry and its own
  worklist, and all are processed.

- Function parameters have defined state at entry. A `Use` of a
  parameter in the function's first block finds an initialized,
  owned entry instead of nothing.

- Diagnostics name the function. The aggregated error message
  from `VerifyIrPass` now shows which function each dataflow
  problem occurred in, rather than a bare block ID.

- Diagnostic duplicates are gone. The set-keyed collection
  collapses re-emissions from worklist iterations.

**Negative.**

- `DataflowEngine::run` gains a sibling `run_all`. The
  single-function form stays, because tests use it. Two entry
  points, but each has a clear caller.

- The `_initial: SemanticState` parameter on `run` is deleted.
  Any test constructing a non-empty initial state must instead
  build a `FunctionCfg` with the appropriate `params`. In
  practice no test does this today — the parameter was already
  ignored.

**Neutral.**

- Cross-function ownership analysis is still out of scope. A
  call does not transfer ownership of its arguments through the
  CFG; whether it should is a language question that this ADR
  does not settle. The interpreter and backends already handle
  call-argument borrow semantics via the analyzer's temporary
  borrows; dataflow agrees by not asserting the opposite.

## Tests

Four new tests in `tests/pipeline_contract.rs`:

1. `every_function_is_analyzed` — a program with `main` calling
   `helper`, where `helper` contains a use-after-move. Before
   this change, no diagnostic is produced. After, one is, and
   it names `helper`.

2. `parameter_is_initialized_at_entry` — a function
   `procedure print_arg(x: Int)` whose body is `print(x)`.
   Before, `x` is not in the entry state and no check fires;
   after, the check runs and passes (no spurious
   `E-INIT-001`).

3. `parameter_used_after_move_is_rejected` — a function whose
   body moves `x` and then uses it. Diagnostic names the
   function.

4. `no_duplicate_diagnostics` — a program whose function has a
   use-after-move inside a loop, forcing the worklist to
   revisit the containing block. Asserts the diagnostic appears
   exactly once in the result, not once per iteration.

Plus one positive test:

5. `well_formed_multi_function_program_passes` — the existing
   conformance corpus continues to pass. This is the regression
   check for the change, not a new assertion; if any of the 39
   corpus programs that involve calls fail, the change is wrong.

## Alternatives considered

**Keep the flat CFG and add explicit entry edges for every
function.**

   Rejected. A single CFG whose entry is `0` with additional
   unreachable entries has no formal meaning — dataflow on a
   disconnected graph is what produced the current bug. A
   `Vec<FunctionCfg>` is the correct data model for "one
   function, one analysis."

**Run dataflow only for functions reachable from `main`.**

   Rejected. The `SemanticProgram` contains every function
   because every function is emitted to IR. Reachability is a
   link-time question, not a verify-time one. A dead function
   with a use-after-move is still a compiler bug to report.

**Thread `ExprId`s into the CFG so diagnostics can point at
source.**

   Out of scope. It would require adding an ID field to
   `CfgInstruction`, threading it through every arm of the
   builder, and updating `SemanticInstruction` to carry one. The
   per-function `name` prefix is a smaller step in the same
   direction and can be extended later if the diagnostics need
   line numbers.
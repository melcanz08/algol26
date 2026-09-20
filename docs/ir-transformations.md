# ALGOL26 IR Transformations

**Effective**: v0.8.0

> **This file describes what each IR transformation *does*.**
> It is *not* about the compiler's pass contracts — for those,
> see [`pass-contracts.md`](pass-contracts.md), which describes
> the `PassContract` metadata every registered pass declares and
> the pipeline rules the scheduler enforces.
>
> The two files serve different purposes. A *transformation* is
> a change to the IR. A *pass contract* is a machine-readable
> declaration (id, kind, input/output level, prose fields) that
> the pipeline uses to schedule passes and refuse invalid chains.
> The same pass may run several transformations; the same
> transformation may be split across several passes.

## Where each transformation runs

| Transformation | Stage | Location |
|---|---|---|
| Loop Desugaring | Frontend (AST → AST) | `src/ir/loop_desugar.rs` |
| Monomorphization | Frontend (AST → AST) | `src/ir/monomorphize.rs` |
| Defer Lowering | IR build | `src/semantics/builder/control_flow.rs::translate_defer` |
| Constant Folding | IR optimize | `src/ir/optimizer.rs` |
| Constant Propagation | IR optimize | `src/ir/optimizer.rs` |
| Dead Code Elimination | IR optimize | `src/ir/optimizer.rs` |
| Branch Simplification | IR optimize | `src/ir/optimizer.rs` |
| IR Verification | IR build, IR optimize | `src/ir/cfg_verifier.rs` + `src/ir/verifier/` |

## Loop Desugaring

**Where**: `src/ir/loop_desugar.rs`

**Stage**: Frontend (runs before type checking).

Tracks list-valued variables in a per-function environment. A
`for x in <literal-list>` loop whose body has straight-line
control flow is **unrolled** — the loop variable is substituted
with each literal element, and the body is emitted once per
iteration. Everything else passes through:

- `while` loops are never unrolled.
- `for` loops over non-literal iterables are kept as `for` loops.
- `for` loops whose body contains `break` / `continue` /
  `return` / `defer`, or a nested loop with the same, are kept.
- `for` loops whose body contains `var y := x` where `y != x`
  are kept.

| Property | Description |
|---|---|
| Input | AST with `for` / `while` loops |
| Output | AST where eligible `for` loops are unrolled; other loops unchanged |
| Preserves | The observable behavior the source intends |
| May change | `for` loops become sequences of statements (only when unrolled) |
| Does NOT unroll | Bodies with `break` / `continue` / `return` / `defer`, or `var y := x` |
| Constraint | The desugared AST must be accepted by the analyzer |

**Why `var y := x` blocks unrolling.** If a loop is unrolled,
a move inside its body appears once per iteration in the
enclosing scope — but that scope has no notion of iteration,
so the analyzer's loop-aware move check never fires. Refusing
to unroll preserves the analyzer's ability to reject
moves-in-loops correctly. The check is conservative: it also
blocks unrolling for `Copy` values.

## Defer Lowering

**Where**: `src/semantics/builder/control_flow.rs::translate_defer`

**Stage**: IR build (part of `BuildSemanticIRPass`, not a
standalone pass).

A `defer` statement is not a terminator. The builder allocates
a cleanup block, translates the deferred statement into it, and
pushes the cleanup block onto a defer stack. When a `Return`
terminator is eventually emitted in the enclosing scope, it
chains the pending cleanup blocks LIFO before emitting the real
return.

| Property | Description |
|---|---|
| Input | IR instructions containing deferred bodies |
| Output | IR where deferred bodies live in cleanup blocks chained before `Return` |
| Preserves | Every defer executes before its scope exits |
| Preserves | Return semantics — an early `return` still runs pending defers LIFO |
| May change | Control flow structure — cleanup blocks are inserted before the return |

## The optimizer's passes

**Where**: `src/ir/optimizer.rs`

The optimizer runs six passes per function, in this order:

1. `remove_unreachable_blocks`
2. `constant_folding`
3. `constant_propagation` — skipped when the function's CFG
   has a cycle
4. `dead_code_elimination`
5. `simplify_branches`
6. `remove_unreachable_blocks` again

### Constant Folding

Folds `BinaryOp` nodes with constant operands, and `Cast`
nodes of constants. Refuses to fold `Int` arithmetic whose
operands exceed ±2^53 — routing that through `f64` would lose
precision.

| Property | Description |
|---|---|
| Input | IR values |
| Output | IR values with constants folded |
| Preserves | Program semantics |
| Preserves | Types |
| Skips | `Int` operations with operands above 2^53 |
| Does NOT fold | Division by zero |

### Constant Propagation

Replaces `Variable(x)` with a known constant when one is
available in the same block. **Not loop-aware, not
dominance-aware.** The pass is skipped entirely for functions
whose CFG has a cycle.

The pass clears its constant map at each block boundary, so
constants defined in one branch never leak into a sibling or
join.

| Property | Description |
|---|---|
| Input | IR with `Declare` / `Assign` instructions |
| Output | IR where same-block variable uses are replaced with constants |
| Preserves | Program semantics |
| Skips | Functions with cyclic CFGs |
| Scoping | Constants visible only within a single basic block |

### Dead Code Elimination

Removes **only** immutable `Declare` instructions whose declared
name is never used. Mutable declarations are always kept — they
may carry loop state, and the pass is not loop-aware.
Non-`Declare` instructions are never removed.

The pass relies on a coupling with the IR builder: initializer
side effects (e.g. a `Call`) are emitted as separate
instructions *immediately before* the `Declare`. If the builder
ever inlines side effects into `Declare` values, this pass must
be revised.

| Property | Description |
|---|---|
| Input | IR with `Declare` / `Assign` instructions |
| Output | IR with unused immutable declarations removed |
| Preserves | Program semantics |
| Preserves | All observable behavior |
| Does NOT remove | Mutable `Declare`; `Call`; any non-`Declare` instruction |
| Coupling | Assumes initializer side effects are separate instructions |

### Branch Simplification

Replaces `Terminator::Branch` with `Terminator::Jump` when the
condition is a literal `true` or `false`.

| Property | Description |
|---|---|
| Input | IR terminators |
| Output | IR with constant-condition branches collapsed |
| Preserves | Program semantics |
| May change | Block structure |

## IR Verification

**Where**: `src/ir/cfg_verifier.rs` (structural),
`src/ir/verifier/` (instruction-level)

**Stage**: IR build (once) and IR optimize (once). The
pipeline scheduler enforces that every `Transform` pass is
followed by a `Verification` pass at the same IR level.

The verifier runs in two layers:

1. **Structural** (`cfg_verifier.rs`): block IDs unique, entry
   block exists, every block has a terminator, every jump
   target resolves, no unreachable blocks, `Fork` shape
   (see ADR 0011).
2. **Instruction-level** (`verifier/`): operand types, branch
   conditions are `Bool`, returns coerce to the function return
   type, calls resolve to known signatures, `Option` / `Result`
   payloads are recursively checked, `Float` arguments are
   rejected for `Int` parameters.

| Property | Description |
|---|---|
| Input | Any `SemanticProgram` |
| Output | `Result<(), String>` |
| Checks | Structural (see above) |
| Checks | Semantic (see above) |
| Guarantees | If `Ok`, the IR is structurally and semantically valid |

## See also

- `pass-contracts.md` — the pass registry and pipeline rules.
- `decisions/0011-phase4-task-model.md` — the `Fork` shape rules.
- `IMPLEMENTATION_STATUS.md` — current state of each transformation.
- `type-table-addressing.md` — invariant any pass carrying AST-level
  data must preserve.

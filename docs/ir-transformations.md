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
| Generic Specialization | Analyzer + IR build | `src/ir/instantiation_plan.rs`, `src/semantics/builder/build.rs` |
| Defer Lowering | IR build | `src/semantics/builder/control_flow.rs::translate_defer` |
| Constant Folding | IR optimize | `src/ir/optimizer.rs` |
| Constant Propagation | IR optimize | `src/ir/optimizer.rs` |
| Dead Code Elimination | IR optimize | `src/ir/optimizer.rs` |
| Branch Simplification | IR optimize | `src/ir/optimizer.rs` |
| IR Verification | IR build, IR optimize | `src/ir/cfg_verifier.rs` + `src/ir/verifier/` |
| Capability Scanning | Before backend lowering | `src/backends/capabilities/scan.rs` |

## Loop Desugaring

**Where**: `src/ir/loop_desugar.rs`, invoked from
`Compiler::desugar` in `src/compiler.rs`.

**Stage**: Frontend (runs after imports, before impl expansion
and type checking).

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

## Generic Specialization

**Where**: `src/ir/instantiation_plan.rs` (the plan),
`src/semantics/analyzer/expr.rs` (instantiation recording),
`src/semantics/builder/build.rs` (specialization emission),
`src/semantics/builder/mod.rs::resolved_callee_name` (call-site rewriting).

**Stage**: Analyzer records; IR build specializes and rewrites.

Generic functions are monomorphized — one `SemanticFunction` is
emitted per concrete type-argument combination actually used. The
process is spread across three stages rather than a single
AST-to-AST pass:

1. **Recording (analyzer).** Each call to a function with non-empty
   `type_params` records an `Instantiation { call_site, function,
   type_params, type_args }`. The analyzer binds type parameters
   from the call's argument types via `unify_types`, which recurses
   into container types (`List<T>`, `Option<T>`, `Result<T,E>`,
   `Map<K,V>`, etc.).

2. **Planning (`InstantiationPlan`).** `from_instantiations` builds
   a plan with two tables: `call_sites` (keyed by the call site's
   stable `ExprId`) and `specializations` (keyed by mangled name).
   Symbolic call-site entries — those inside another generic's body,
   whose type args are still `TypeVar` or `Unknown` — are recorded
   but do not produce specializations yet. `close(functions)` walks
   each concrete specialization's body under that specialization's
   bindings, substitutes symbolic type args into concrete ones, and
   iterates to fixpoint. After `close`, every generic call reachable
   from any specialization has a concrete specialization in the plan
   (see ADR 0013).

3. **Emission (IR builder).** `build_impl` emits one
   `SemanticFunction` per specialization of each generic function,
   named by `mangled_name(function, type_args)`. Non-generic
   functions are emitted unchanged. `resolved_callee_name` rewrites
   every generic call site's callee to the mangled name of its
   matching specialization.

The original generic function declarations are not emitted as
executable IR — they are analyzer input, not compiler output.

| Property | Description |
|---|---|
| Input | AST with generic function declarations and generic call sites |
| Output | IR with one `SemanticFunction` per concrete specialization |
| Records | Instantiation facts in the analyzer's `instantiations` list |
| Closes | Symbolic call-site entries transitively via `close` |
| Rewrites | Generic call sites to mangled specialization names |
| Does NOT emit | The generic function templates themselves |

**Historical note.** An earlier design used a pre-typecheck
AST-to-AST monomorphizer (`src/ir/monomorphize.rs`). That pass is no
longer called by the pipeline; the analyzer-`.close()` design replaced
it because the pre-typecheck pass could not infer type arguments for
reference-typed arguments (`identity(&v)`), which the analyzer can.

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

The same chaining applies to the implicit `return` at the end of
a function body: if any defers are pending when control reaches
the end of `func.body`, they run before the fall-off-the-end
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

The pass relies on two couplings with the rest of the builder:

1. **Initializer side effects are separate instructions.** If a
   `val x := f()` where `f` has side effects is emitted, the
   `Call` instruction is pushed immediately before the `Declare`,
   and the `Declare`'s value is a `Variable` reference to the
   result binding. This lets DCE remove the unused `Declare`
   while preserving the `Call`'s side effect. If the builder
   ever inlines side effects into `Declare` values, this pass
   must be revised.

2. **The variable-collection walker recurses into composite
   values.** `collect_variables_from_value` descends into
   `FieldAccess { object }`, `Record { fields }`, and
   `Map { entries }` so that a variable used only as the receiver
   of a field read, or only as a value inside a record or map
   literal, is still counted as used. Without this, DCE removes
   the `Declare` for a record-typed variable read only via
   `p.x` and leaves a dangling `Variable` reference for the
   verifier to reject.

| Property | Description |
|---|---|
| Input | IR with `Declare` / `Assign` instructions |
| Output | IR with unused immutable declarations removed |
| Preserves | Program semantics |
| Preserves | All observable behavior |
| Does NOT remove | Mutable `Declare`; `Call`; any non-`Declare` instruction |
| Coupling | Initializer side effects are separate instructions |
| Coupling | Walker recurses into `FieldAccess` / `Record` / `Map` |

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

`TypedIRValue::type_of()` is exhaustive over the value enum:
there is no `_ => Type::Unknown` catch-all. A new variant without
an arm fails to compile, rather than silently producing `Unknown`
and letting a downstream operation type-check against it.

| Property | Description |
|---|---|
| Input | Any `SemanticProgram` |
| Output | `Result<(), String>` |
| Checks | Structural (see above) |
| Checks | Semantic (see above) |
| Guarantees | If `Ok`, the IR is structurally and semantically valid |

## Capability Scanning

**Where**: `src/backends/capabilities/scan.rs`

**Stage**: After IR verification, before backend lowering.

Walks the verified IR and reports which `Feature` variants the
program uses. `check_backend` compares that set against the
target backend's `supported` set and returns `E0002` naming the
missing features if any are refused.

The scan fires on:

- Value variants (`TypedIRValue::Record`, `FieldAccess`, `Some`,
  `None`, `Ok`, `Error`, `Map`)
- Instruction variants (`Instruction::FieldAssign`,
  `ChannelDecl`, `SendChannel`, `ReceiveChannel`, `Allocate`,
  `Free`)
- Terminator variants (`Spawn`, `Fork`, `Switch` on `Ok` /
  `Error`)
- Callee names (`String.*`, `File.*`, `List.append`,
  `List.sum` / `.max` / `.min`, `Map.*`, `Int.to_string`,
  `String.to_int`, `args`)
- Function signatures (reference types in parameters or the
  return type)

Records are not covered by the signature walk. `Feature::Records`
fires only when a record value appears in the IR — a
`TypedIRValue::Record` literal, a `TypedIRValue::FieldAccess`, or
an `Instruction::FieldAssign`. A program that declares a record,
mentions it in a function signature, and never constructs or reads
a value of that type does not fire the scan; that program has no
record values in its IR, so the capability claim is vacuously
correct.
The scan is the single mechanism by which a backend refuses a
feature. Codegen does not need per-feature refusal arms; the
capability check runs first.

| Property | Description |
|---|---|
| Input | `SemanticProgram` and a `BackendCapabilities` |
| Output | `Result<(), CompileError>` |
| Refuses | Programs using features not in the backend's supported set |
| Guarantees | A refused program never reaches codegen |
| Side effects | None |

## See also

- `pass-contracts.md` — the pass registry and pipeline rules.
- `decisions/0011-phase4-task-model.md` — the `Fork` shape rules.
- `decisions/0013-executable-ir-generic-invariant.md` — the
  instantiation plan and `close`.
- `decisions/0014-verifier-invariants.md` — the instruction-level
  verifier's guarantees.
- `decisions/0017-verified-ir-typestate.md` — the `IrState` enum
  and the promote-then-recheck split.
- `decisions/0018-canonical-pipeline.md` — one `run_pipeline`
  shared by every entry point.
- `IMPLEMENTATION_STATUS.md` — current state of each transformation.
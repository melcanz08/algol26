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

## Loop Desugaring

**Where**: `src/ir/loop_desugar.rs`

| Property | Description |
|----------|-------------|
| Input | AST containing `for` / `while` loops |
| Output | AST without loops (converted to lower-level control flow) |
| Preserves | Program semantics (same observable behavior) |
| May change | Control flow structure (loops become blocks) |
| Must NOT | Change variable types or ownership |

## Defer Lowering

**Where**: `src/semantics/builder/control_flow.rs::translate_defer`

Not a standalone module. The IR builder lowers `defer` to a
cleanup block, pushed onto a defer stack; the eventual `Return`
terminator chains the cleanup blocks LIFO before emitting the
real return.

| Property | Description |
|----------|-------------|
| Input | AST containing `defer` statements |
| Output | IR where `defer` bodies live in cleanup blocks chained before `Return` |
| Preserves | Every defer executes before scope exit |
| Preserves | Return semantics (early returns still run defers) |
| Preserves | Error semantics |
| May change | Control flow structure |

## Monomorphization

**Where**: `src/ir/monomorphize.rs`

| Property | Description |
|----------|-------------|
| Input | Generic AST with type parameters |
| Output | Concrete AST without type parameters |
| Preserves | Program semantics for each instantiation |
| Guarantees | No unresolved generic calls remain |
| Guarantees | All type parameters substituted |
| May change | Function names (specialized names) |

## Constant Folding

**Where**: `src/ir/optimizer.rs` (inside `OptimizePass`)

| Property | Description |
|----------|-------------|
| Input | Valid semantic IR |
| Output | Valid semantic IR with constants folded |
| Preserves | Program semantics (same output) |
| Preserves | Types (no type changes) |
| Preserves | Ownership (no ownership changes) |
| May change | Expression structure (constant replaces expression) |
| Known limit | Cross-block folding disabled — see `IMPLEMENTATION_STATUS.md` |

## Dead Code Elimination

**Where**: `src/ir/optimizer.rs` (inside `OptimizePass`)

| Property | Description |
|----------|-------------|
| Input | Valid semantic IR |
| Output | Valid semantic IR without unreachable code |
| Preserves | Program semantics |
| Preserves | All observable behavior |
| May change | Number of blocks/instructions |
| Must NOT | Remove code with side effects |

## IR Verification

**Where**: `src/ir/verifier/` (wrapped by the `ir.verify` pass)

`src/ir/semantic_ir.rs` has a `verify()` method that delegates to
`crate::ir::verifier::verify`. The pass wrapper is
`src/compiler/passes/verify_ir.rs::VerifyIrPass`.

| Property | Description |
|----------|-------------|
| Input | Any `SemanticProgram` |
| Output | `Result<(), String>` |
| Checks | Block IDs are unique |
| Checks | Jump targets exist |
| Checks | Entry block exists |
| Checks | Every block has a terminator |
| Checks | `Fork` shape (see ADR 0011) |
| Checks | Instruction-level semantics (types, calls, ownership dataflow) |
| Guarantees | If `Ok`, the IR is structurally and semantically valid |

The verifier runs twice in the compile pipeline: once after
`ir.build` and once after `ir.optimize`. The second run is
enforced by the scheduler (a `Transform` must be followed by a
`Verification` at the same IR level).

## See also

- `pass-contracts.md` — the pass registry and pipeline rules.
- `decisions/0011-phase4-task-model.md` — the `Fork` shape rules.
- `IMPLEMENTATION_STATUS.md` — current state of each transformation.
- `type-table-addressing.md` — invariant any pass carrying AST-level
  data must preserve.

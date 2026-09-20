# ADR 0011: Phase 4 — Task Model Investigation

## Status

**Proposed** (2026-09-19). Not yet implemented.

**Outcome: the task-model redesign is deferred.** The deliverable
of this ADR, if accepted, is a set of verifier rules that enforce
the shape `parallel` currently requires, plus documentation of that
shape. The redesign itself is a future option, not a current
commitment.

Responds to a discovery made during Phase 2 of ADR 0010
(`docs/decisions/0010-canonical-ir.md`). Phase 4 of that ADR
proposed canonicalizing `Terminator::Fork` to a pair of
`SpawnTask` / `JoinTask` terminators. Phase 2 found that this
is not a rename — it is a redesign of the interpreter's
concurrency model.

This ADR documents the discovery, states the constraint the
current mechanism imposes, and proposes how to close the gap
between what `parallel` can express and what the verifier
enforces.

## Summary

ADR 0010 assumed the interpreter's `parallel` implementation
was equivalent to spawning N tasks and joining them at a
single point. That assumption is wrong.

The interpreter implements `parallel` as a **continuation-
passing** mechanism. Branches do not run concurrently and do
not run as tasks. Instead: the first branch runs to completion;
when it jumps to the join block, that jump is intercepted and
the second branch runs instead; when the second branch reaches
the join, the third branch runs; and so on. Only the last
branch's jump to the join is allowed to proceed.

This is not task-based concurrency. It is a mechanism for
running a fixed list of branch bodies in source order and
then entering the join block. It is observationally equivalent
to concurrent execution for any program that respects the
language's independence rule (see ADR 0008). But it is not
expressible as `SpawnTask` + `JoinTask` without substantial
CFG restructuring.

## What `pending_forks` actually does

The interpreter's `execute_function` loop maintains:

```
pending_forks: Vec<(Vec<usize>, usize)>
```

Each entry is a pair: the list of *remaining* branch blocks
and the join block's id. The list is a worklist; the join block
is the target that branches jump to when their body completes.

When the interpreter reaches a `Terminator::Fork { blocks,
join_block }`:

1. Split `blocks` into `first` and `rest`.
2. If `rest` is non-empty, push `(rest, join_block)` onto
   `pending_forks`.
3. Set `current = first` and continue execution.

When the interpreter reaches a `Terminator::Jump { block:
target }`:

1. Look at the top of `pending_forks`.
2. If `target == join_block` of that entry:
   - If `rest` is non-empty, pop `rest[0]` and set
     `current = rest[0]`. The jump is *intercepted*.
   - If `rest` is empty, pop the entry and fall through:
     `current = target`.
3. Otherwise, `current = target` as normal.

The observable effect for `parallel` with branches `[A, B, C]`
and join `J`: the interpreter runs A's body up to its jump
to J, then runs B's body, then C's body, then enters J. Branch
bodies execute in source order, sequentially. The join block
runs exactly once, after all branches.

## Why this is not a rename

Canonical `SpawnTask` / `JoinTask` would express a different
control-flow shape. For the same `parallel` block, a canonical
representation would be:

```
entry:
    SpawnTask(A)
    SpawnTask(B)
    SpawnTask(C)
    JoinTask(A)
    JoinTask(B)
    JoinTask(C)
    Jump(J)

A:  ...body of A...
    Return_from_task

B:  ...body of B...
    Return_from_task

C:  ...body of C...
    Return_from_task

J:  ...
```

The differences:

1. **Task bodies return.** Under the current `Fork` model,
   branch bodies jump to the join. Under `SpawnTask`, they
   return from the task. The jump target is not the join —
   the join is a separate operation in the spawning block.

2. **The spawner continues.** Under `Fork`, control flow
   immediately leaves the spawning block and only returns at
   the join, after all branches have run. Under `SpawnTask`,
   the spawning block continues immediately and the actual
   join is a later instruction in the same block.

3. **Branches may have different joins.** Under `Fork`, every
   branch shares one join block. Under `SpawnTask` / `JoinTask`,
   each task can be joined separately.

## The constraint on `parallel`

The current mechanism is not buggy. It is a *specific reading*
of what a `parallel` block is, with constraints the CFG shape
does not state and the verifier does not enforce.

The constraints are:

1. Each branch block must be entered only from the spawning
   block. No other predecessor.
2. Each branch block must terminate directly with `Jump {
   block: join_block }`. No intermediate blocks, no early
   returns, no nested forks.
3. The join block must not appear in the branch list.

The CFG the IR builder emits today satisfies these constraints
for every program the parser accepts. But nothing enforces them.
If a future IR-builder change violates one — say by adding an
intermediate block between a branch body and its jump to the
join — the interpreter's jump-interception logic would not
fire, and control flow would not proceed as the source intended.

This is a **discontinuity**, not a bug. The current code does
not miscompile any program that exists today. But the constraint
is invisible: it is not stated in the CFG shape, not checked by
the verifier, and not documented in the language reference.

The right response to a discontinuity is to make the constraint
explicit — in the verifier and in the documentation. That is
what Option B below proposes.

## Scope of the redesign

A full task-model redesign would touch:

- `Terminator` (in `src/ir/semantic_ir/terminators.rs`).
- The CFG builder (`src/ir/cfg/builder.rs`).
- The dataflow engine (`src/ir/cfg/dataflow.rs`) — task edges
  have different reachability semantics than branch edges.
- The interpreter's execution loop (`src/backends/interpreter/
  mod.rs`).
- The race detector (`src/semantics/race/`) — task-level
  capture is what it already reasons about, so this may be
  unaffected.
- The capability scan (`src/backends/capabilities/scan.rs`) —
  LLVM and WASM already refuse `Fork`; they would refuse
  `SpawnTask` / `JoinTask` the same way.
- The differential tests for `spawn` and `parallel`.
- Every `parallel` fixture in the test suite.

Roughly the same footprint as Phase 1 or Phase 2 of ADR 0010,
plus a language-design decision about what `parallel` should
mean. That decision must be made before the work starts.

## Options

### Option A — Canonicalize to `SpawnTask` / `JoinTask`

Replace `Terminator::Fork` with a `SpawnTask` terminator and a
`JoinTask` terminator. The IR builder expands `parallel` blocks
into a sequence of `SpawnTask` calls, followed by `JoinTask`
calls, followed by the original join body.

**Pros:**

- Clean canonical model. Matches ADR 0010's proposal.
- Separable joins (`spawn a; spawn b; join a; ...; join b`)
  become expressible, if the language wants them.
- Removes the branch-shape constraint entirely. Branches can
  have arbitrary control flow.

**Cons:**

- **Migration cost.** Every `parallel` fixture in the test
  suite is rewritten, and the CFG shape for every program
  containing a `parallel` block changes.
- **The interpreter needs a task table.** The `pending_forks`
  worklist is replaced by an explicit task registry with spawn
  and join operations. This is new machinery, not a rename.
- **Design decisions are required.** Does `parallel` mean "run
  these branches concurrently" or "run these branches in some
  order and join"? Does the language need separable joins?
  These are language-level questions, not IR-level ones.

**Effort:** one design session plus one implementation session.

### Option B — Keep `Fork`, document and enforce its constraints

Leave `Terminator::Fork` as-is. Add CFG verifier rules that
reject the shapes it cannot handle. Document the constraint in
the language reference and the `spawn` feature contract.

**Pros:**

- Zero migration cost. Existing tests and programs unchanged.
- Honest about what the mechanism is.
- The verifier rules close the discontinuity: a malformed
  shape is rejected at verification time, with a diagnostic
  naming the constraint, rather than producing different
  control flow than the source implies.

**Cons:**

- `Fork` is not canonical. ADR 0010's Phase 4 goal is not met.
- The language has one concurrency form (`spawn`) that maps
  cleanly to tasks and one (`parallel`) that does not.
- A future user who wants to express `spawn a; spawn b; join
  a; do_work; join b` cannot. They must use `parallel` and
  accept the single-join constraint.

**Effort:** CFG verifier rules plus tests. One short session.

### Option C — Remove `parallel` from the language

Delete `Terminator::Fork`, the parser rule for `parallel`,
and every test fixture. Keep `spawn` only, which maps cleanly
to a task-spawn model.

**Pros:**

- Smallest language surface.
- Every remaining concurrency construct is task-shaped.
- Simplifies the eventual canonical IR (Phase 4 disappears).

**Cons:**

- Loses a language feature.
- Programs written against `parallel` break.
- `spawn` alone may be insufficient for users who want a
  fixed set of branches that all run before a join.

**Effort:** deletion, plus updating tests and documentation.
Half a session.

## Recommendation

**Option B**, for now.

Reasons:

1. **The discontinuity is the real problem.** Option A fixes
   it by rewriting everything; Option B fixes it by making the
   constraint explicit and enforced. The user-visible difference
   is small — a program shape that currently works by luck
   becomes a program shape that is verified to work — and the
   code change is a tenth the size.

2. **The canonical-IR migration does not need Phase 4.** ADR
   0010's success criterion 7 ("three files per new
   operation") applies to features that reach the IR. `Fork`
   is used by one terminator, exercised by one backend, and
   refused by the other two. Canonicalizing it is cleanup,
   not a prerequisite for anything else.

3. **The design question deserves a language-level answer.**
   Option A implicitly says "we want real task semantics."
   Option C says "we don't need `parallel`." Option B says
   "`parallel` is what it is; leave it alone." None of these
   is obviously right. Picking one now, in the middle of a
   migration, would be a decision made for the wrong reasons.

If the language later grows a feature that needs real task
semantics (a `select` statement over channels, or separable
joins), revisit Option A. Until then, verifier rules are the
right investment.

## Migration plan (Option B)

### Step 1 — Verify assumptions before touching anything

Two investigations, both required, before any code changes:

**(a) Re-read `src/backends/interpreter/mod.rs`.** Confirm the
`pending_forks` description above is still accurate. If the
code has changed since this ADR was written, correct the ADR
before proceeding.

**(b) Determine whether `spawn` has the same constraint.**
`spawn` is a one-way hand-off — the spawned block runs and
then control returns to the spawn's successor. If the spawned
block can `return`, the function's return value is set by the
spawn, not the spawner. Whether this is correct behavior or a
second instance of the same discontinuity is unresolved.

**If `spawn` has the same constraint**, the scope of this ADR
doubles. Revise it before proceeding.

**If `spawn` is free of the constraint**, record that fact in
this ADR — a one-line note under "The constraint on
`parallel`" — and continue to Step 2.

> **Completed 2026-09-19.** Findings: `pending_forks` description in
> this ADR is accurate; `spawn` does **not** have the parallel
> constraint (it has no worklist — the spawned block runs inline and
> continues); `cfg_verifier.rs` did not build a predecessor map, so
> the implementation adds one.

### Step 2 — Add CFG verifier rules for `Fork`

In `src/ir/cfg_verifier.rs`, for every `Fork { blocks,
join_block }` in the function's CFG:

- Every block in `blocks` has exactly one predecessor, and it
  is the block containing the `Fork`. Branches cannot be
  entered from elsewhere.
- Every block in `blocks` terminates with `Jump { block:
  join_block }` directly. No intermediate blocks, no early
  returns, no nested forks.
- `join_block` does not appear in `blocks`.

Each rule produces a distinct diagnostic with a code (see the
diagnostic convention in `common/diagnostics.rs`). The rules
close the discontinuity: a malformed `Fork` is rejected at
verification time instead of being silently misread by the
interpreter.

**Why `cfg_verifier.rs` and not `verifier/terminator.rs`:**
`verifier/terminator.rs` sees one terminator value and the
environment. It has no access to predecessor information.
The predecessor check requires the CFG structure, which is
what `cfg_verifier.rs` walks. The instruction-level verifier
handles semantic properties (types, environments); the CFG
verifier handles structural properties.

> **Completed 2026-09-19.** Rules implemented in `cfg_verifier.rs`,
> inside `verify_function`, after the switch-case check. Rule 2 as
> originally worded was too strict — see the refinement note below.

### Step 3 — Add tests for the new CFG verifier rules

In `src/ir/cfg_verifier.rs`'s test module:

- `fork_branch_with_early_return_is_rejected`
- `fork_branch_with_extra_block_is_rejected`
- `fork_branch_with_nested_fork_is_rejected`
- `fork_join_in_branches_is_rejected`
- `well_formed_fork_is_accepted` (positive case)

> **Completed 2026-09-19.** Five tests added to `cfg_verifier.rs`'s
> test module: `well_formed_fork_is_accepted` plus four rejection
> cases covering Rules 1 and 2.

### Step 4 — Document the constraint

Add a section to `docs/features/spawn.md` explaining that
`parallel` branches must be straight-line blocks ending in a
jump to the join. This is a real constraint on the language;
users should know it, and the CFG verifier will reject
programs that violate it.

### Step 5 — Add a conformance fixture

Add `tests/conformance/valid/concurrency/parallel_straightline.gol`
exercising the accepted shape. No fixture for the rejected
shapes — the shapes above are hard to write in the surface
language; the CFG verifier tests cover them directly.

### Step 6 — Note the outcome in ADR 0010

Update ADR 0010's Phase 4 section to point at this ADR and
record that Option B was chosen. This closes Phase 4 of ADR
0010 as "resolved by investigation, not by redesign."

### Rule 2 as implemented differs from the ADR's phrasing

The ADR's original Rule 2 said: *every block in `Fork.blocks`
terminates with `Jump { block: join_block }` directly.*

That is too strict. A branch body containing an `if` — perfectly
legal source like `parallel do\n  if x > 0 then print(1)\nand\n
print(2)` — is translated into multiple blocks. The branch's entry
terminates with `Branch`, not `Jump{join}`; only the inner block
does.

The implemented rule is: *no path through a branch exits the branch
except via a `Jump` to the join*. That means: walking the reachable
set from each branch entry, excluding the join, every block's
terminator must not be `Return`, `Spawn`, or `Fork`, and no block in
the set (other than the entry itself) may be another branch's entry.

This is what `cfg_verifier.rs` enforces. The ADR's original phrasing
would have rejected legal programs.

## Related finding from the same investigation

The investigation that produced this ADR — reading how the
interpreter maintains execution state across terminator
boundaries — also surfaced a second bug in a different
subsystem. `break` and `continue` that cross a region boundary
skip the region's `RegionExit`, leaving the interpreter's
`region_stack` (and the LLVM backend's `region_frames`) with an
unpopped frame. Inside a loop, frames accumulate linearly with
iterations, leaking every allocation the region made until the
enclosing function returns.

The fix is the same technique this ADR applies to `Fork`:
identify the shape the runtime assumes, reject the shapes it
cannot handle, at analysis time. The region check landed in
`src/semantics/analyzer/stmt.rs`; the fork check landed in
`src/ir/cfg_verifier.rs`. Different layers, same philosophy:
make the implicit constraint explicit.

Two paths for surfacing the region leak were considered:

- **Verifier rule** — reject the IR shape. Rejected because
  `break` and `continue` compile to `Jump` terminators, which
  are indistinguishable from any other jump at the CFG level.
  The information "this jump exits a region" lives in the AST,
  not the IR.
- **Analyzer rule** — reject the source shape. Adopted. The
  analyzer already tracks scope depth and can track region
  depth alongside it.

See `docs/decisions/0010-canonical-ir.md` Phase 5 for the full
region investigation.

## Open questions

The first open question of the previous draft — *does `spawn`
have the same constraint?* — has been promoted to Step 1 of
the migration plan, because it determines the scope of the
entire ADR. It is not an open question; it is a prerequisite.

Remaining open questions:

1. **Is `parallel` actually used?** If no program outside the
   test suite uses it, Option C becomes more attractive. A
   grep of `examples/`, `benchmarks/`, and any external
   programs would answer this. The ADR does not assume the
   answer.

2. **What does the race detector assume about `Fork`?**
   `src/semantics/race/analyze.rs` reasons about which
   variables each branch accesses. If it assumes the current
   `Fork` shape (branches jump to a shared join), the new
   CFG verifier rules should not conflict with its assumptions.
   Worth a read before Step 2.

3. **Should the CFG verifier rule reject or warn?** A branch
   that ends in `Return` rather than `Jump(join)` is a
   constraint violation. But some future programs may want
   "spawn and forget" semantics for a branch, which the
   current `Fork` cannot express. Rejecting is the right
   default; whether a warning mode makes sense is a
   language-level question.

4. **Does the CFG verifier have access to predecessor
   information today?** `cfg_verifier.rs` walks the CFG to
   check structural properties, so it should. But whether
   the current implementation builds a predecessor map or
   relies on forward-only traversal affects the shape of the
   new rules. A short read of the file before Step 2.

5. **What does `return` inside `spawn` mean?** Step 1(b) established
   that `spawn` does not have the `parallel` constraint. But the
   interpreter's handling of `Return` inside a spawn body is: the
   spawned task's return value becomes the *whole function's* return
   value, and the spawn's continuation block never runs. That is
   consistent with "spawn is an inline synchronous block that runs
   before the spawner continues," but not with "spawn is a task."
   A future language-semantics ADR should decide which reading is
   intended. Not a verifier concern.

## See also

- `docs/decisions/0010-canonical-ir.md` — the parent ADR;
  Phase 4 lives there.
- `docs/decisions/0008-concurrency-model.md` — the design
  decision behind `spawn` and `parallel`.
- `docs/features/spawn.md` — the feature contract for the
  constructs this ADR discusses.
- `src/backends/interpreter/mod.rs` — the `pending_forks`
  implementation.
- `src/ir/semantic_ir/terminators.rs` — `Terminator::Fork`.
- `src/ir/cfg_verifier.rs` — where the new rules go.
- `tests/differential/differential_true.rs` — the current
  tests for `spawn` and `parallel` refusal.

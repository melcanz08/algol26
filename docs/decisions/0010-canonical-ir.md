# ADR 0010: Canonical IR

## Status

**Partially implemented** (2026-09-19).

- Phase 1 (borrow canonicalization) — **landed**.
- Phase 2 (deref canonicalization) — **landed**.
- Phase 3 (channel canonicalization) — **landed**.
- Phase 5a (dead region code removal) — **landed**. 627 lines deleted.
- Phase 4 (fork shape) — **resolved by investigation** (ADR 0011).
- Phase 5b (region canonicalization) — **investigated and declined**.
  The current `Allocate` + `RegionEnter`/`RegionExit` split is coherent;
  `AllocateRegion`/`FreeRegion` would be an architectural repackaging,
  not a canonicalization. See the Phase 5 section below.
- Phase 6 (explicit Move) — **deferred indefinitely**.

Supersedes nothing. Superseded by nothing. This is the first
architectural decision that touches the IR representation as a whole.
## Summary

ALGOL26 currently represents the same semantic concepts in four
independent type universes:

1. `Type` in `src/common/types.rs` — the frontend's type universe.
2. `TypedIRValue` in `src/ir/semantic_ir/values.rs` — the IR's value
   universe.
3. `RuntimeValue` in `src/backends/interpreter/runtime.rs` — the
   interpreter's value universe.
4. `BasicValueEnum` from `inkwell` — the LLVM codegen's value
   universe.

Each backend re-derives the meaning of every operation from the IR
types it receives. Every new feature must be taught to all four.
Every semantics change ripples across all four. The sixteen feature
contracts under `docs/features/` document the ripple explicitly: each
one lists ten to fifteen files that a feature change touches.

This ADR proposes a single **canonical IR** — a small set of semantic
operations that every backend consumes — and a migration path from
the current four-universe state to a two-universe state: canonical
IR plus a per-backend lowering. It does not propose a rewrite.
Every step in the migration compiles, passes tests, and can be
reverted independently.

## Context

### The problem is coupling, not regression

When a feature is added, the same semantic concept is written down in
multiple places. When one of those places changes, the others drift.
The drift is discovered late — sometimes only at runtime, sometimes
only by a differential test — and fixing one place often breaks
another. This is the regression pattern the observer's document
describes:

```
more features
    -> more cross-feature dependencies
    -> old assumptions become invalid
    -> patch feature A
    -> feature B regresses
    -> fix B
    -> feature C regresses
```

The symptom is regression. The cause is semantic coupling. The fix is
canonicalization: one authoritative representation of meaning that
every consumer reads from.

### What we already have

The IR (in `src/ir/semantic_ir/`) is the closest thing to a canonical
representation today. It has:

- A value universe: `TypedIRValue`.
- An instruction universe: `Instruction`.
- A control-flow universe: `Terminator`.
- A pattern universe: `SemanticPattern`.

The capability matrix enforces that a backend either lowers a
construct or refuses the whole program. Nothing in between. That is
the fail-closed property established in Tier 2 of the roadmap.

### What we do not have

The current IR is not canonical. It has:

- **Variant duplication.** `Instruction::Send` and
  `Instruction::ChannelSend` have identical fields; same for
  `Receive` and `ChannelReceive`. Nothing distinguishes them except
  which code path produced them.

- **Operation-level ambiguity.** `TypedIRValue::Borrow` and
  `TypedIRValue::AddrOf` overlap. `TypedIRValue::Deref` covers both
  reads and (implicitly) writes through a `&mut`. The distinction is
  re-derived by each backend.

- **Missing operations.** There is no `Move` value or instruction;
  moves are implicit in `Declare` and `Assign` when the target type
  is not `Copy`. There is no `ReadReference` or `WriteReference`;
  both are expressed as `Deref` and a store, which the backend must
  recognize as a write pattern.

- **Backend-specific duplication.** `src/runtime/region.rs` and
  `src/runtime/region_memory.rs` implement a region model the
  interpreter does not use — the interpreter has its own
  `RegionFrame` in `src/backends/interpreter/mod.rs`. Two parallel
  implementations, one live.

### What the observer's document proposes

The observer's document (paraphrased in
`docs/architecture-direction.md`) lists canonical operations:

```
BorrowShared        BorrowMutable
ReadReference       WriteReference
Move
AllocateRegion      FreeRegion
SpawnTask           JoinTask
SendChannel         ReceiveChannel
```

This ADR adopts that proposal with two corrections:

1. **Rust enums cannot be split across files**, so the operations
   live in a small number of enum types, not one file each.
2. **Some operations are not appropriate for ALGOL26.** Details
   below.

## Decision

The canonical IR is `SemanticProgram` and its four constituent
universes (`TypedIRValue`, `Instruction`, `Terminator`,
`SemanticPattern`). Every other representation is derived:

| Representation | Role |
|---|---|
| `SemanticProgram` | **Canonical.** The single source of truth. |
| `Type` | **Input.** Produced by the analyzer, consumed by the IR builder. Not canonical for runtime semantics. |
| `RuntimeValue` | **Derived.** A per-backend lowering of `TypedIRValue`. |
| `BasicValueEnum` | **Derived.** A per-backend lowering of `TypedIRValue`. |

`Type` stays because it models compile-time-only concepts (generics,
traits) that never reach runtime and therefore have no
`TypedIRValue` representation. `RuntimeValue` and `BasicValueEnum`
stay because each backend needs its own value representation; the
point is that the *mapping* from canonical to derived is
mechanical, not interpretive.

## The canonical operations

### Borrow and reborrow

```
BorrowShared(place: Place, target_type: Type) -> TypedIRValue
BorrowMutable(place: Place, target_type: Type) -> TypedIRValue
```

**Replaces:** `TypedIRValue::Borrow`, `TypedIRValue::MutBorrow`,
and (partially) `TypedIRValue::AddrOf`. The distinction between
`Borrow` and `AddrOf` disappears; a raw pointer is a borrow whose
lifetime the analyzer cannot track, and the capability matrix
already refuses programs that use `AddrOf` in ways the analyzer
cannot justify.

**Why `Place` and not a bare `TypedIRValue`:** a borrow is of a
*place*, not a value. The current IR passes the place as a
`TypedIRValue` and relies on the backend to check that it is a
`Variable`. A dedicated `Place` type (introduced in a later step)
makes this a compile-time property of the IR.

### Reference reads and writes

```
ReadReference(reference: TypedIRValue, target_type: Type) -> TypedIRValue
WriteReference(reference: TypedIRValue, value: TypedIRValue) -> Instruction
```

**Replaces:** `TypedIRValue::Deref` in read position; the pair of
`Deref` plus a store instruction in write position. Today the write
case is not expressed as a first-class operation — the backend must
recognize `Assign { target: deref, value }` and lower it correctly.
Making the write explicit removes the recognition step.

### Move

```
Move(from: Place, to: Place) -> Instruction
```

**Replaces:** nothing, because there is no equivalent today. A move
is currently implicit: `Declare { name: x, value: Variable(y) }`
where `typeof(y)` is not `Copy` implicitly moves `y`. The
`Type::is_copy()` predicate is the sole arbiter, and both backends
must consult it to know whether to copy or move.

Making the move explicit means the decision is made *once* — by the
IR builder, at the point where the analyzer has already decided the
value is being moved. Backends see `Move` and lower it to whatever
their representation requires (a `RuntimeValue` reassignment in the
interpreter; a `memcpy` or a direct store in LLVM). The `is_copy`
predicate moves out of the backend and into the IR builder.

**Note:** introducing `Move` is the largest single change in this
ADR. It touches every `Declare` and `Assign` and requires the IR
builder to know about `Copy` types. It is proposed but flagged as
deferrable — the other canonicalizations can land first.

### Region allocation and deallocation

```
AllocateRegion(region: RegionId, size: TypedIRValue) -> Instruction
FreeRegion(region: RegionId) -> Instruction
```

**Replaces:** the current split between `Instruction::Allocate`
(raw `malloc`) and `Instruction::RegionEnter`/`RegionExit`. Today a
backend must infer from the region stack whether an allocation is
region-owned. Making the ownership explicit in the instruction
removes the inference.

**Why two operations, not four:** `RegionEnter` and `RegionExit` are
the boundaries of the region's *lifetime*, not separate semantic
operations. They become part of the lowering: the interpreter
pushes and pops a frame, LLVM emits no code (because the capability
check refuses region allocations today), WASM does the same as
LLVM. Making them explicit canonical operations would create
operations with no semantic content beyond their boundaries.

### Task spawning and joining

```
SpawnTask(entry: BlockId) -> Terminator
JoinTask(task: TaskId) -> Terminator
```

**Replaces:** `Terminator::Spawn` and `Terminator::Fork`. The
current pair is asymmetric — `Spawn` creates one task, `Fork`
creates many and joins them at once. A canonical model has one
operation (spawn) and one operation (join), with `Fork` becoming
sugar that the IR builder expands to a sequence of `SpawnTask`
followed by a sequence of `JoinTask`.

**This is a design improvement, not just a rename.** The current
`Fork` terminator carries a `Vec<BlockId>` and a `join_block` and
its semantics are defined by the interpreter's `pending_forks`
queue. A canonical spawn/join pair has semantics that the
capability matrix can verify directly.

### Channel send and receive

```
SendChannel(channel: Place, value: TypedIRValue) -> Instruction
ReceiveChannel(channel: Place, target: Place) -> Instruction
```

**Replaces:** `Instruction::Send`, `Instruction::ChannelSend`,
`Instruction::Receive`, `Instruction::ChannelReceive` — four
variants with two shapes. The canonical pair drops the
`Send`/`ChannelSend` distinction entirely.

**Why the distinction existed:** `Send` and `ChannelSend` were
produced by two different code paths in the IR builder.
Canonicalizing removes the distinction at the IR level, and the
two producer paths converge on the same instruction.

## What does not become canonical

### `Deref` and `AddrOf`

`TypedIRValue::Deref` and `TypedIRValue::AddrOf` do not survive as
canonical operations. They are replaced by:

- `ReadReference` for a deref in value position.
- `WriteReference` for a deref in target position of an assign.
- `BorrowShared` / `BorrowMutable` for address-taking.

This is a strict reduction: the current four borrow/deref variants
become four semantic operations with clear read/write roles.

### Composite types

`Type::List`, `Type::Option`, `Type::Result`, `Type::Channel`, and
the rest do not need canonical operations of their own. They are
container types; their construction (`Some`, `Ok`, list literals)
and their inspection (pattern matching) already have canonical
forms in `TypedIRValue` and `SemanticPattern`. The canonicalization
work in this ADR is about *operations*, not *containers*.

### Raw memory (alloc/free)

`Instruction::Allocate` and `Instruction::Free` remain. They model
raw `malloc`/`free`, which is a distinct concept from region-scoped
allocation. `AllocateRegion` and `Allocate` coexist because they
have different lifetimes: `AllocateRegion` is freed at region exit,
`Allocate` is freed by explicit `Free`.

## Consequences

### Positive

- **Backend symmetry.** Each backend implements the same small set
  of canonical operations. The LLVM codegen, the interpreter, and
  the WASM backend (which reuses `IRCodeGen`) cannot disagree about
  semantics because they see the same operations.

- **Feature contracts shrink.** The checklist at the bottom of each
  `docs/features/<feature>.md` currently has ten to fifteen entries.
  After canonicalization, the checklists reduce to: add the canonical
  operation, add the backend lowering, add the conformance fixture.
  The intermediate re-derivation steps disappear.

- **Coverage matrix becomes load-bearing.** The refusal tests in
  `src/backends/capabilities/tests.rs` currently test at the
  `Feature` level (`llvm_rejects_result_values`). After
  canonicalization they can test at the *operation* level, which is
  finer-grained and more honest.

- **The observer's document becomes true.** The map from
  `LANGUAGE FRONTEND -> Semantic Model -> Canonical IR -> {LLVM,
  WASM, Interpreter}` is currently aspirational. It becomes the
  actual shape of the code.

### Negative

- **Migration cost.** Every backend must be updated. This is
  expected and bounded by the migration plan below.

- **IR compatibility.** The `.ll` files that the LLVM backend emits
  will change shape. Golden fixtures that compare against `.ll`
  output will need regeneration. The conformance fixtures do not
  have `.ll` files any more (removed in Tier 4.1), so this is not
  currently a blocker, but it was a consideration when deciding
  whether to keep `.ll` goldens.

- **Performance.** Some canonical operations may lower less
  efficiently than the current backend-specific fast paths. The
  canonical `Move` in particular may force a copy where the current
  code avoids one. This is a real risk and is why `Move` is
  flagged as deferrable.

## Migration plan

**Principles:**

1. Every step compiles and passes the full test suite.
2. Every step is revertable in isolation.
3. New variants are added *alongside* old ones before old ones are
   removed.
4. Backends migrate first, the IR builder migrates last.

### Phase 0 — Design (this ADR)

Deliverable: this document, committed.

### Phase 1 — Canonical borrows

Add `BorrowShared` and `BorrowMutable` to `TypedIRValue`.
Do not remove `Borrow`, `MutBorrow`, or `AddrOf`.

- IR builder emits both old and new variants (the old ones
  temporarily ignored by backends).
- Interpreter implements the new variants; LLVM does the same;
  WASM inherits from LLVM.
- Differential tests verify the two forms produce identical output.
- After green: IR builder stops emitting old variants. Delete them
  from `TypedIRValue` and remove backend handling.

**Estimated scope:** one file each in the IR builder and the three
backends; one differential test.

### Phase 2 — ReadReference and WriteReference

> **Status: 2a (ReadReference) landed. 2b (WriteReference) deferred
> on a corrected premise.**
>
> `ReadReference` landed as described. `Deref` was removed; every
> dereference now lowers through `ReadReference`.
>
> `WriteReference` was originally deferred under the mistaken belief
> that write-through-`&mut` was dead code. It is not dead code.
> Assigning to a variable of declared type `MutBorrow(T)` is the
> language's write-through syntax. Both the analyzer
> (`src/semantics/analyzer/stmt.rs`, `Stmt::Assign` arm) and the
> verifier (`src/ir/verifier/instruction.rs`, `Instruction::Assign`
> arm) implement the rule: the assignment target's type is unwrapped
> from `MutBorrow(T)` to `T` for type-checking purposes. Two tests
> in the analyzer suite document the intended behavior, most
> directly `test_param_is_assignable`, whose comment names
> "functions like `increment(x: &mut float)` that write through
> their parameter."
>
> What the language does **not** have is backend support. When the
> IR builder emits `Instruction::Assign { target: "x", value }`
> for an assignment to a `MutBorrow(T)` variable, both backends
> store the value into `x`'s alloca — overwriting the *reference*
> rather than writing through it. LLVM (`llvm_codegen/instruction.rs`)
> and the interpreter (`interpreter/mod.rs`) both have this bug. No
> end-to-end test exercises the path, so the bug has been latent
> since the write-through rule was introduced.
>
> Fixing it is a phase, not a cleanup:
>
> 1. Add `Instruction::WriteReference { reference, value }`.
> 2. Have the IR builder emit it when the assignment target's
>    declared type is `MutBorrow(T)`, and emit plain `Assign`
>    otherwise.
> 3. Remove the `MutBorrow(inner) => inner` special cases from the
>    analyzer and verifier — with the IR now carrying the
>    distinction, the checkers can type-check both forms directly.
> 4. Implement lowerings in LLVM (`load` the pointer, `store`
>    through it) and the interpreter (`heap`/`variables` lookup,
>    write through the referent).
> 5. Add differential tests exercising write-through in both
>    backends.
>
> Estimated scope: one design session plus one implementation
> session — same as any of Phases 1, 3, or 5b.
>
> Original Phase 2 estimate (before the correction) applies to 2a,
> which shipped: adding `ReadReference` alongside `Deref`, then
> removing `Deref`, took the same shape as Phase 1.
### Phase 3 — Channel canonicalization

Collapse `Send`/`ChannelSend` into `SendChannel`; same for receive.

- Straight rename at the IR level; no new behavior.
- Update `dataflow.rs` and the CFG builder to emit the canonical
  forms.

**Estimated scope:** mostly mechanical.

### Phase 4 — Task canonicalization

> **Resolved by investigation, not by redesign.** See
> `docs/decisions/0011-phase4-task-model.md`. The canonical
> `SpawnTask` / `JoinTask` migration was considered and deferred
> in favor of enforcing the existing `Fork` shape via the CFG
> verifier. The rules landed 2026-09-19.


Replace `Terminator::Fork` with `SpawnTask` + `JoinTask` sequences.

- IR builder expands `parallel` to a `SpawnTask` per branch,
  followed by `JoinTask` for each.
- Interpreter's `pending_forks` queue is replaced by a task table.
- This is a substantive change to the concurrency model. Phase 4
  is the riskiest step and should be a session of its own.

**Estimated scope:** one IR file, one interpreter file, one
conformance fixture, one differential test.

### Phase 5 — Region canonicalization

> **Status: 5a landed. 5b investigated and declined.**
>
> 5a (deleting `runtime/region.rs` and `runtime/region_memory.rs`)
> landed 2026-09-19 — 627 lines removed. Those modules had no callers.
>
> 5b (introducing `AllocateRegion` / `FreeRegion`) was investigated
> 2026-09-20. The current design is:
>
> - `Instruction::Allocate { target, size, type }` — malloc
> - `Instruction::RegionEnter { name }` / `RegionExit { name }` —
>   push/pop a region frame; every allocation made while a frame is
>   active is freed on exit
>
> Two implementations — `Interpreter::RegionFrame` and
> `IRCodeGen::LRegionFrame` — both do the same thing: maintain a
> stack, register allocations that occur while a frame is on top,
> free them on pop.
>
> The ADR's proposal would move ownership tracking from an
> implicit "is a region frame on the stack?" lookup to an explicit
> `AllocateRegion(region_id, size)` instruction. It would rename an
> ambient-context lookup as an instruction field. It would not
> reduce the number of files a feature touches, remove a
> duplication, or fix a bug. It is an architectural repackaging,
> not a canonicalization.
>
> **The investigation did surface a real bug, fixed at the
> analyzer level:** `break` or `continue` that crosses a region
> boundary leaks the region frame until function return, and in a
> loop accumulates linearly. The fix rejects the shape at analysis
> time (`src/semantics/analyzer/stmt.rs`) using the same technique
> as ADR 0011's fork rules.


### Phase 6 — Explicit Move (deferrable)

> **Status:** Deferred indefinitely. See "When not to do this" below.


Add `Instruction::Move`. Make `Declare` and `Assign` always copy
when the value is not a fresh construction.

**This is the largest and least certain phase.** It changes the
observable behavior of every non-`Copy` value binding. It should
only be attempted after Phases 1–5 are stable for several weeks.

**Estimated scope:** unknown. Deferrable indefinitely.

## What is explicitly out of scope

- **Rewriting the frontend.** The parser, lexer, and AST are not
  affected. The canonicalization is at the IR level.

- **Changing the language.** No syntactic or semantic changes to
  ALGOL26 user-facing features. The canonical IR is an internal
  representation.

- **Removing the four type universes entirely.** The goal is not
  one type universe; it is one *canonical semantic* universe with
  mechanical lowerings. `Type`, `RuntimeValue`, and
  `BasicValueEnum` all stay.

- **Performance work.** Canonicalization may make some operations
  slower. Optimizing them is a separate project.

## Alternatives considered

### A. Keep the current IR, add tests

Rejected. More tests do not reduce coupling; they detect the
symptoms more reliably. The root cause — the same concept written
down in four places — remains.

### B. Rewrite the IR from scratch

Rejected. The current IR works. It has ~500 passing tests and a
verifier. A rewrite would discard that evidence. The migration
plan above is additive at every step.

### C. Make `Type` the canonical universe

Rejected. `Type` models compile-time concepts (generics, traits,
inference) that have no runtime representation. It is the right
shape for the analyzer, the wrong shape for the IR.

### D. Make `RuntimeValue` the canonical universe

Rejected. `RuntimeValue` is designed for a tree-walking interpreter.
It has no notion of basic blocks, control flow, or registers. It
is a lowering, not a canonical form.

## Open questions

These are decisions that will be made during implementation. They
are listed here so future sessions have a starting point.

1. **Should `Place` be a distinct type?** The ADR proposes
   `BorrowShared(place: Place, ...)`. Introducing `Place` as a
   distinct IR type makes borrows of non-variable places
   (e.g. `&list[0]`) representable. But it also adds a new type
   universe. Alternative: keep `place: TypedIRValue` and rely on
   the verifier to check that the value is a variable or a
   projection.

2. **What is the type of a `Move`?** In the current IR, a value
   has a type via `type_of()`. If `Move` is an `Instruction`, it
   has no type. If it is a `TypedIRValue`, its type is the type of
   the source place. The second option is more consistent with the
   rest of the IR; the first is closer to what most compilers do.

3. **How is `Copy` decided?** Currently `Type::is_copy()` is a
   predicate on `Type`. Phase 6 would move this decision into the
   IR builder. But `is_copy` will still exist for the analyzer to
   reason about moves. The two must be consistent.

4. **What happens to `TypedIRValue::Void`?** The current IR uses
   `Void` as a value that has no runtime representation. In a
   canonical IR, a function returning `Void` should not produce a
   value at all — its return is a terminator with no value, not a
   value of type `Void`. This is a small cleanup but it changes
   the signature of many functions.

5. **How do we test that a backend is not re-deriving semantics?**
   The coverage matrix tests *features*, not operations. After
   canonicalization we want a test that a backend does not
   contain pattern-matches on operation-kind beyond the canonical
   set. This is difficult to enforce mechanically. A code-review
   policy is the current answer.

6. **When do we delete the parallel implementations?**
   `src/runtime/region.rs` and `src/runtime/region_memory.rs` are
   live code with tests but no callers from the compiler. Phase 5
   proposes deleting them. This should happen in the same commit
   as the region canonicalization, not before, so we can compare
   the two implementations if the canonical one has a bug.

## Migration risk assessment

| Phase | Risk | Revertable? | Estimated sessions |
|---|---|---|---|
| 1. Borrows | Low | Yes | 1 |
| 2. ReadReference / WriteReference | Low | Yes | 1 |
| 3. Channels | Very low | Yes | 0.5 |
| 4. Tasks | Medium | Yes | 1 |
| 5. Regions | Medium | Yes (keeps old module) | 1 |
| 6. Move | High | Yes (revert the branch) | unknown |

The first three phases are compatible with the current architecture
and can land without disturbing anything that works today. Phase 4
and 5 are substantive but reversible. Phase 6 is the one to be
cautious about.

## Success criteria

Status as of 2026-09-19:

- [x] `TypedIRValue::Borrow` and `TypedIRValue::MutBorrow` no longer
      exist. *(Phase 1.)*
- [x] `TypedIRValue::Deref` no longer exists. *(Phase 2.)*
- [x] `Instruction::Send`, `Instruction::ChannelSend`,
      `Instruction::Receive`, and `Instruction::ChannelReceive` no
      longer exist. *(Phase 3.)*
- [x] `src/runtime/region.rs` and `src/runtime/region_memory.rs` are
      deleted. *(Phase 5a.)*
- [ ] `TypedIRValue::AddrOf` — **kept, deliberately.** Raw pointers
      (`Type::Ptr`) are semantically distinct from tracked borrows
      (`Type::Borrow(T)` / `Type::MutBorrow(T)`). Folding them would
      weaken the type discipline for no gain. Phase 1's design notes
      record this decision.
- [ ] `WriteReference` — **deferred.** Write-through-`&mut` is a real
      language feature: assigning to a variable whose declared type is
      `MutBorrow(T)` is defined as writing through the reference, and
      both the analyzer and the verifier implement that rule. What is
      missing is the *backend lowering*: `Instruction::Assign` in both
      LLVM codegen and the interpreter currently overwrites the
      reference variable instead of writing through it. Fixing this
      requires a new IR instruction, matching lowerings in both
      backends, and end-to-end tests — a phase the size of Phase 1 or
      Phase 3, not a cleanup. The earlier Phase 2 note claiming the
      construct "does not exist" was incorrect; see the corrected
      Phase 2 section below.
- [x] `Terminator::Fork` — **resolved by investigation.** See
      ADR 0011. The CFG verifier now enforces the shape the
      interpreter's `pending_forks` worklist assumes.
- [ ] `Move` — **not added.** Phase 6, deferred indefinitely.
- [x] **`break`/`continue` across a region boundary is rejected.**
      Surfaced during the Phase 5b investigation. Fixed in the
      analyzer, not the verifier — the leak is unconditional,
      not shape-dependent.

### Criterion 7 was wrong and is replaced

The original ADR claimed a new operation would require three files.
Reality, learned across Phases 1 and 3:

**An IR variant rename or addition touches roughly six files:**

1. The IR enum (`src/ir/semantic_ir/<family>.rs`).
2. The producer (IR builder, `src/semantics/builder/`).
3. The verifier (`src/ir/verifier/`).
4. The optimizer (`src/ir/optimizer.rs`).
5. One lowering per backend architecture — LLVM and interpreter;
   WASM inherits via `IRCodeGen`.
6. The capability scan (`src/backends/capabilities/scan.rs`).

Plus `src/ir/verifier/tests.rs` for verifier tests. The verifier and
optimizer arms are the two easiest to miss — both were missed in
Phase 1, and the verifier was missed again in Phase 3.

The realistic estimate is **six to eight files per operation**, not
three. Still better than the ten-to-fifteen per *feature* the current
architecture costs, but the original number was aspirational.

## Post-implementation notes

Two latent issues were documented during Phases 1–3 but not fixed.

### Write-through typing dead code

The analyzer (`src/semantics/analyzer/`) and the verifier
(`src/ir/verifier/instruction.rs`) contain type-checking code for writing
through a `&mut T`. There is no parser production that creates this
construct — `Stmt::Assign.target` is always a bare identifier — and
no backend lowering. The code has been dead since at least the
session that introduced the current parser.

Phase 2 revealed it: `WriteReference` was supposed to be the canonical
form, but there is nothing to canonicalize *from*.

**Resolution required:** either delete the dead type-checking (small,
low-risk) or implement write-through `&mut` end-to-end (a new
language feature with its own contract, own ADR, own tests). A future
session should pick one. Not urgent.

### `pending_forks` is CPS, not a task queue

The interpreter's concurrency model (`pending_forks` in
`src/backends/interpreter/mod.rs`) is a continuation-passing
implementation. When a `Fork` terminator is encountered, the branches
are stored and re-entered at each `Jump` to the join block. This is
not a task queue — it cannot be expressed by `SpawnTask` + `JoinTask`
without a CFG change to represent continuation capture.

Phase 4 is therefore a design project, not a mechanical migration.
The current `Fork` semantics are correct for what they do; the
question is whether the language wants those semantics, or whether
"spawn N tasks and join them" is the right model. That is a language
design question, not an IR cleanup question.
## When not to do this

This ADR proposes a multi-month migration. It is not the right
project for every point in the language's life. Concretely, do not
start (or continue) this work if:

- **You are not planning more features.** The value of canonical IR
  is measured in future feature velocity. If the current feature set
  is roughly what you intend to ship, the migration is a six-month
  project that buys you nothing.

- **You cannot dedicate sessions to it.** Partial completion is worse
  than not starting. A migration that gets through Phase 2 and stops
  leaves the codebase with both old and new variants, both needing
  maintenance. The ADR's "every phase is revertable" property makes
  each *step* safe; it does not make *stopping* safe.

- **You are tired or rushed.** Phase 4 is a concurrency redesign.
  Phase 6 changes the observable behavior of every non-`Copy`
  binding. Neither is a good fit for a session where you want to be
  done in an hour. Phases 1–3 worked because they were bounded and
  low-risk; Phases 4–6 are neither.

- **You have a live bug.** Bug fixes and architectural refactors use
  the same mental resources. Land the bug fix first.

**What to do instead if any of the above applies:**

- Keep the feature-contract discipline (`docs/features/*.md`). It is
  a poor substitute for canonical IR, but it is a *working*
  substitute at roughly 1/50th the cost.
- Add differential tests before fixing bugs, not after.
- Revisit this ADR when the next feature feels genuinely painful to
  add. That pain is the signal.

The goal of this ADR is not to be followed. It is to be *available* —
a written description of where the architecture would go if the
project's circumstances make that direction worth the trip.

## See also

- `docs/architecture-direction.md` — the observer's diagnosis this
  ADR responds to.
- `docs/decisions/0003-type-system.md` — the `Type` universe.
- `docs/decisions/0005-ownership-model.md` — the ownership rules
  the canonical borrow operations express.
- `docs/decisions/0007-region-memory.md` — the region model the
  canonical allocation operations express.
- `docs/decisions/0008-concurrency-model.md` — the concurrency
  model the canonical task operations express.
- `docs/decisions/0009-unsafe.md` — the unsafe boundary, which is
  the one place canonical operations do not apply.
- `docs/features/*.md` — the per-feature contracts whose
  checklists this ADR shortens.
- `tests/coverage_matrix.rs` — the current feature × backend
  matrix, which the canonicalization will refine.

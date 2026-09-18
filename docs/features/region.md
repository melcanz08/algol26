# Feature: Region (`region NAME ... `)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a region in ALGOL26, and where does it live?"

## Summary

A **region** is a lexical scope that owns the lifetime of every
allocation made inside it. When the region exits, all such
allocations are freed automatically. Regions give ALGOL26 a
structured, non-GC memory management model with predictable cost:
the cost of freeing a region is the number of live allocations
inside it, not the size of the heap.

Regions are also the semantic anchor for the borrow model. A
borrow's lifetime is often expressed as "the region it was created
in" (`BorrowLifetime::Region(name)`), and the analyzer uses the
region hierarchy to check whether a storage location outlives a
borrow that refers to it.

Regions are **lexical**, not dynamic. `region r { ... }` opens a
scope; the closing brace (or an early `return` from the enclosing
function) closes it. There is no way to reopen a region.

## Syntax

Basic form:

```gol
procedure main
    region scratch
        val p := alloc(64)
        // ... use p ...
    // p is freed here
```

Nested regions:

```gol
region outer
    val a := alloc(16)
    region inner
        val b := alloc(16)
        // ... use a and b ...
    // b freed here
    // ... use a only ...
// a freed here
```

The ADR `docs/decisions/0007-region-memory.md` describes the
motivation; the syntax is intentionally minimal — `region` is a
keyword, the name is a bare identifier, the body is a block.

## Typing rules

Regions do not have a type. A `region` block has type `Void`; it is
a statement, not an expression. Expressions inside it follow normal
typing rules.

There is no `Region<T>` type. The region's effect on typing is
**indirect**: a pointer or borrow created inside a region inherits
a lifetime bound to that region, and the analyzer uses that to
check outlives relations. The types themselves are unchanged —
`alloc(64)` is still `*Unknown` regardless of which region it is
in.

## Ownership

### What a region does

1. **Attributes allocations.** Every `alloc(n)` executed while a
   region is the innermost active region records its handle in that
   region's frame.

2. **Frees on exit.** When the region exits (normally or via
   `return`), every recorded handle is freed. Freeing is a batch
   operation, not a per-`free()` cost.

3. **Bounds pointer lifetime.** A pointer created in region `r`
   cannot be stored in a location that outlives `r`. The analyzer
   checks this via the region hierarchy — see the borrow contract
   for the `outlives_region` rule.

4. **Anchors borrow lifetimes.** A borrow whose borrower is declared
   inside region `r` gets `BorrowLifetime::Region("r")`. The analyzer
   uses that to check whether the borrowed storage outlives the
   borrow.

### What a region does not do

- **It does not scope ordinary variables.** A `val x := 42`
  declared inside a region is subject to the surrounding lexical
  scope, not the region. The region owns *allocations*, not *bindings*.
- **It does not stop moves.** A pointer can be moved out of a region
  as long as the analyzer can prove the destination's lifetime does
  not exceed the region's. Moving a region-local pointer to a
  function return value is a compile error unless the return type
  carries the region (which it cannot — see Open Questions).
- **It does not affect `free()` semantics.** An explicit `free(p)`
  inside a region is legal and removes the handle; the region exit
  later finds no handle to free. Freeing twice is not an error.

### Region hierarchy

Regions nest. `region_parent: HashMap<String, Option<String>>` in
`SemanticState` records each region's parent. The root regions (those
declared at function top level) have `Some(None)` as parent.

`SemanticState::is_ancestor(ancestor, descendant)` walks the parent
chain to test whether one region contains another. This is used by
`outlives_region` to determine whether a borrow's region outlives a
storage's region.

The stack discipline is enforced: `enter_region` pushes, `exit_region`
must pop the matching name. An `exit` with the wrong name is a
compiler error, not a runtime panic — see the diagnostic code
`E-REGION-001` and the interpreter's error path.

### Region lifetime

A region's lifetime ends when:

- The block closes normally (last statement executes).
- The enclosing function `return`s while the region is still open.
- A `break` or `continue` exits the enclosing loop, if the region
  was opened inside the loop body.

All three paths are handled by the interpreter: `execute_function`
drains `region_stack` before returning, freeing each frame's
allocations. Early-return cleanup is a single `while let Some(frame)
= self.region_stack.pop()` block at the top-level `Return` handler.

## IR representation

In `src/ir/semantic_ir.rs`:

| Concept | Variant |
|---|---|
| Region opening | `Instruction::RegionEnter { name: String }` |
| Region closing | `Instruction::RegionExit { name: String }` |

The IR comment on `RegionEnter` states:

> The interpreter pushes a new region frame; allocations between
> `RegionEnter` and the matching `RegionExit` are attributed to it
> and freed automatically on exit. LLVM treats both as no-ops — the
> capability check refuses any program that actually allocs, so a
> region without alloc has no runtime meaning.

The IR verifier does not have specific rules for regions beyond
checking that `RegionEnter`/`RegionExit` names are non-empty.
Region/alloc matching is enforced at the CFG dataflow level, not
the IR verifier.

## CFG representation

In `src/ir/cfg/dataflow.rs`:

```rust
CfgInstruction::RegionEnter { name } => { incoming.enter_region(name.clone()); }
CfgInstruction::RegionExit { name } => {
    let outliving = incoming.exit_region(name);
    for msg in outliving {
        diags.push(DataflowDiagnostic {
            message: format!("E-REGION-001: Reference outlives region '{}': {}", name, msg),
            block: block.id,
            is_error: true,
        });
    }
}
```

`SemanticState::exit_region(name)` does the work: it marks the region
`Freed`, pops it from the stack, and iterates over every live borrow,
checking whether the borrowed storage is in a region that is being
freed and whether the borrow outlives it. Any borrow that does is
returned as a diagnostic message.

### Diagnostic: `E-REGION-001`

The only region-specific diagnostic code currently emitted. Fires
when: a borrow's lifetime is bound to region `A`, the borrowed
storage is bound to region `B`, and `A` is an ancestor of `B` (or
`A == B` at the point where `B` is being freed).

Message format:

```
E-REGION-001: Reference outlives region 'NAME': PLACE (borrowed by BORROWER lives in LIFETIME but storage in NAME freed)
```

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported | `RegionFrame` in `src/backends/interpreter/mod.rs`; `interpreter_accepts_raw_memory` in capability tests |
| LLVM | **Unsupported** | Treats `RegionEnter`/`RegionExit` as no-ops; capability check refuses programs that actually alloc |
| WASM | **Unsupported** | No region lowering; capability check refuses (unverified which code path) |

### Interpreter

The interpreter models regions with `RegionFrame`:

```rust
pub(super) struct RegionFrame {
    pub name: String,
    pub allocations: Vec<usize>,
}
```

Each `RegionEnter` pushes a new frame; each `Allocate` records its
handle in the innermost frame; each `RegionExit` pops the frame and
frees every recorded handle. On early `return`, the interpreter
drains the entire `region_stack` before returning.

Mismatched names (`RegionExit{name: "a"}` when the top frame is
`"b"`) produce an `EvalError::Runtime` rather than a panic. This
is the fail-closed path added in the session that produced this
contract.

### LLVM

The LLVM backend treats `RegionEnter` and `RegionExit` as no-ops. The
IR comment above explains why: the capability check refuses any
program that actually uses `alloc`, so a region containing no
allocation has no observable behavior in the LLVM backend. If the
capability check ever relaxed to allow `alloc` on LLVM, regions
would need real lowering — likely to a `malloc`/`free` pair at the
region boundaries.

### WASM

Unverified. The WASM backend likely also refuses programs using
`alloc` or regions, but I have not read the capability scan code
for WASM.

## Runtime module

There are two runtime modules for regions:

- `src/runtime/region.rs` — a general region data structure with a
  parent/child hierarchy, tested via
  `test_create_and_enter_region`, `test_nested_regions`,
  `test_cannot_enter_inactive_region`, `test_cannot_deallocate_active_region`,
  `test_stack_discipline_enforced`, `test_child_regions_deallocated_with_parent`,
  `test_deallocate_region`.
- `src/runtime/region_memory.rs` — a region-aware allocator, tested
  via `test_create_and_allocate`, `test_double_create_fails`,
  `test_allocate_in_freed_region_fails`, `test_double_free_prevented`,
  `test_child_region_management`.

**These two modules are not currently used by the interpreter.** The
interpreter implements its own `RegionFrame`-based model directly in
`src/backends/interpreter/mod.rs`. The runtime modules exist as a
parallel implementation with richer semantics (parent/child
hierarchy at the allocator level, deallocation rules, etc.) but are
not wired into execution.

This is a duplication similar to the four `Send`/`Receive` variants
noted in the channel contract: two implementations of the same
concept, one used and one not. A Tier 7 (canonical IR) cleanup item
would either unify them or delete the unused one.

## Diagnostics

Region-related error codes currently emitted:

| Code | Meaning | Emitted from |
|---|---|---|
| `E-REGION-001` | Reference outlives region on exit | `dataflow.rs` |
| `E-ESCAPE-001` | Reference escapes via return | `dataflow.rs` (region-adjacent) |

**Codes that do NOT exist** (despite being mentioned in prior
reports): `E-REGION-002`, `E-REGION-003`. The current vocabulary
is exactly one code for regions.

Errors that could reasonably have distinct codes but fall through to
`E-REGION-001` or to a `Runtime` error:

- Opening a region inside a function whose return type carries a
  reference derived from it (region-pointer escape).
- Two `region` blocks with the same name at the same lexical level.
- A `region` name that shadows an existing region.

If any of these become distinguished diagnostics, the code inventory
will grow — that is a Tier 2 follow-up.

## Safety

- No garbage collector. Region exit is the sole reclamation
  mechanism for region-owned allocations.
- No use-after-free inside a region: allocations are freed only on
  region exit, and the region's scope is lexical, so no code path
  can reference a freed handle within the region.
- Across region boundaries, escape analysis enforces that a pointer
  or reference created in region `r` does not outlive `r`.
- No panics on bad region operations. Mismatched enters/exits,
  double-enters, and exits without matching enters all produce
  `EvalError` or `DataflowDiagnostic`, not panics.

The safety model is sound under the assumption that the analyzer's
`outlives_region` check is correct. It was strengthened this session
by fixing `SemanticState::borrow` to stop relocating the borrower
into the current region — see the borrow contract for that
discussion.

## Optimizer rules

None implemented. Candidates:

- **Region allocation coalescing.** Two adjacent `alloc(n)` calls
  inside a region could be lowered to one `malloc(n1 + n2)` with an
  offset. Not currently done.
- **Dead region elimination.** A region with no allocations could be
  deleted entirely. This would need to preserve the region's name in
  the symbol table for any name-reference errors to remain stable.

Any optimizer rule must preserve the correspondence between
`RegionEnter`/`RegionExit` and the allocations attributed to them.

## Test coverage

Current coverage across the tree:

**Semantics-level (`src/semantics/state/mod.rs::tests`):**

- `region_enter_exit` — basic enter/exit
- `region_borrow_outlives_inner_storage` — the positive case for E-REGION-001
- `same_region_borrow_ok` — same-region borrows do not outlive
- `local_outside_outlives_inner` — a local in an outer scope does not get flagged

**Runtime (`src/runtime/region.rs::tests` and `region_memory.rs::tests`):**

- Seven region tests, five region_memory tests, all passing
- But: these test the standalone runtime modules, not the interpreter's use of regions

**Interpreter (`src/backends/interpreter_backend.rs::tests`):**

- `test_interpreter_region_frees_allocation_on_exit` — verifies the interpreter frees on exit

**CFG level (`tests/pipeline_contract.rs`):**

- `dataflow_enforces_region_outlives` — constructs a CFG with nested regions and checks E-REGION-001 fires

**Soundness fixtures (`tests/soundness/escape/`):**

- `region_escape.gol` — the case E-REGION-001 should catch

**Corpus:**

- `corpus_30_region_simple.gol` — basic region
- `corpus_31_region_var.gol` — region with variables

### Gaps

- **No test for early return from inside a region.** The interpreter
  code path exists (`while let Some(frame) = self.region_stack.pop()`
  on `Return`), but I have not seen a test that exercises it.
- **No test for `break`/`continue` crossing a region boundary.**
- **No test for a region that allocates zero times.** In the LLVM
  backend, such a region is a no-op; in the interpreter, it is a
  push/pop pair with no handles. Both should be fine, but no test
  pins this.
- **No test for the region-name mismatch error.** The interpreter
  has the error path; no test constructs the scenario.
- **No test for `region r1 { region r1 { ... } }`** (same name at
  two levels). Is this allowed? Is it rejected? Not tested.
- **No differential test.** Regions and the interpreter's region
  model are unverified against LLVM (which treats them as no-ops).
  Any differential test would need to use programs where the
  region has no observable effect in LLVM.
- **No test that exercises the `runtime/region.rs` and
  `runtime/region_memory.rs` modules against the interpreter.**
  The two implementations have never been compared.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
region NAME
    semantics:   Stable (with one open question on region-pointer returns)
    parsed:      yes
    typed:       yes
    validated:   yes (analyzer + CFG dataflow)
    IR:          yes
    verified:    yes
    interpreter: supported
    LLVM:        unsupported (treats as no-op; capability refuses allocs)
    WASM:        unverified
    optimized:   no rules
```

Regions are the most self-contained feature in the language: their
semantics are clear, their implementation is small, and their only
interaction with other features is through pointer and borrow
lifetimes (which are themselves well-defined).

## Checklist for related features

If you are adding a feature *like* regions (a lexical scope that
owns the lifetime of something), you need to touch:

1. `src/ir/semantic_ir.rs` — new `Instruction` variants for the
   scope's enter/exit if this is not already covered by
   `RegionEnter`/`RegionExit`.
2. `src/semantics/state/mod.rs` — new `SemanticState` fields if the
   scope needs its own tracking map or stack.
3. `src/ir/cfg/dataflow.rs` — dataflow handling for the new
   `CfgInstruction` variants.
4. `src/ir/cfg/builder.rs` — translation from IR to CFG instruction.
5. `src/semantics/analyzer/` — any analyzer-level rules (usually
   none — the semantics are structural).
6. `src/backends/interpreter/mod.rs` — runtime handling.
7. `src/backends/interpreter/runtime.rs` — new `RuntimeValue` variant
   if the scope produces values (unlike regions, which do not).
8. `src/backends/capabilities/scan.rs` — declare backend support.
9. `src/backends/capabilities/tests.rs` — accept/reject per backend.
10. `tests/conformance/valid/<feature>.gol`.
11. `tests/soundness/<family>/` — fixtures for escape-related cases.
12. `docs/features/<feature>.md` — this file.

## Open questions

- **Can a pointer into a region be returned from the enclosing
  function?** The natural answer is no — the region's lifetime ends
  at function return, so any pointer into it becomes dangling. The
  analyzer's `E-ESCAPE-001` / `E-REGION-001` checks are meant to
  catch this, but I have not seen the specific test. This should be
  confirmed with a small fixture.

- **Should regions be first-class values?** Currently they are
  purely lexical. A `Region` value type with methods like
  `freeze()`, `leak()`, or `merge()` would enable richer patterns,
  but would be a major language addition. Not currently planned.

- **Should there be a `defer` inside a region?** `defer` runs on
  scope exit; whether that scope exit also triggers region cleanup
  is a design question. Currently, region cleanup is handled by the
  `RegionExit` instruction, not by `defer` — the two mechanisms are
  independent.

- **Why do `runtime/region.rs` and `runtime/region_memory.rs` exist
  if the interpreter does not use them?** They implement a richer
  model (parent/child deallocation, allocator discipline) than the
  interpreter needs. Either they are the future of the region model
  and the interpreter will migrate to them, or they are leftover
  from an earlier design and should be deleted. This should be
  resolved before Tier 7 (canonical IR), because the answer affects
  where region state lives in the new IR.

- **What is the cost model for region exit?** In the current
  interpreter, exit iterates over the frame's allocation list and
  calls `heap.remove(handle)` for each. That is O(n) in the number
  of live allocations, not O(1). A real implementation might prefer
  an arena allocator with O(1) free, at the cost of a more complex
  `alloc` path. Not currently a bottleneck, but worth noting.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/decisions/0007-region-memory.md` — the design decision
- `docs/features/borrow.md` — borrow lifetimes are expressed in region terms
- `docs/features/alloc_free.md` — (to be written) explicit allocation
- `src/ir/semantic_ir.rs` — `RegionEnter`, `RegionExit`
- `src/semantics/state/mod.rs` — `region_stack`, `region_parent`, `var_region`
- `src/ir/cfg/dataflow.rs` — `RegionExit` handling and `E-REGION-001`
- `src/backends/interpreter/mod.rs` — `RegionFrame`
- `src/runtime/region.rs`, `src/runtime/region_memory.rs` — parallel runtime implementation
- `tests/soundness/escape/region_escape.gol` — the escape case
- `tests/corpus/corpus_30_region_simple.gol`, `corpus_31_region_var.gol`

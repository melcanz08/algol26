\
# ADR 0050: Region cleanup for heap-backed lists

Status: Proposed (design; implementation deferred)

## Context

ADR 0042 introduced heap-backed lists. A list's buffer moves
from stack to heap on three paths:

  - **stack-to-heap conversion** — the first `.append` on a
    stack-backed list (`capacity == 0`) mallocs a fresh buffer
    and memcpys the elements in. See `emit_list_append`.
  - **realloc growth** — `.append` on a full heap-backed list
    (`length == capacity`) reallocs to `2 * capacity`. Same
    site.
  - **escape on return** — a stack-backed list returned from a
    proc is copied to a fresh heap buffer so the caller
    receives a buffer that outlives the callee's frame. See
    `emit_list_return`.

And on one more path added by ADR 0049:

  - **by-value parameter copy** — a `List<T>` parameter is
    copied into a fresh heap buffer so callee mutation does
    not reach the caller. See the list-param branch in
    `compile_function`.

None of these frees the buffer. The compiler knowingly leaks.

The region machinery (ADR 0007) already frees raw `alloc(n)`
pointers at `RegionExit`:

- `LRegionFrame` carries `tracked_vars` (variables whose
  current value is a raw allocation) and `saved_slots`
  (snapshots of overwritten allocations, so
  `p := alloc(8); p := alloc(16)` releases both).
- `RegionExit` pops the frame and emits a guarded
  `free(load(var))` per entry, via `emit_free_if_non_null`.
- `Terminator::Return` runs the same cleanup first, so early
  returns do not leak region-managed allocations.

This ADR extends that machinery to list buffers.

## Decision

A list variable whose descriptor has `capacity > 0` at
`RegionExit` has its buffer freed.

**Scope, stated plainly.** This ADR covers lists created
*inside* a `region` block, and only when control reaches
`RegionExit` or an early `return` from inside the region.
Every other heap-backed list still leaks:

- a list grown with no `region` anywhere (the common case);
- a list declared outside a region and grown inside one;
- a list-parameter copy (ADR 0049) in a callee whose body is
  not itself inside a region;
- a list returned from a `proc` (escape-to-heap).

General scope-exit cleanup — treating every lexical block as
an allocation scope, not just explicit `region` blocks — is
ADR 0051.

The mechanism mirrors `tracked_vars`:

- `LRegionFrame` gains `tracked_lists: Vec<String>`, a list
  of variable names whose descriptor allocas live inside the
  region and whose buffers were heap-allocated after entry.
- A list is added to the innermost frame's `tracked_lists`
  the first time its descriptor's `capacity` field is written
  to a nonzero value after region entry. The relevant sites:
  `emit_list_append`'s stack and grow paths; the by-value
  parameter copy in `compile_function` when the callee's
  list param is inside a region.
- `RegionExit` emits, for each tracked name, a runtime guard:
  `if descriptor.capacity > 0 { free(descriptor.buffer) }`.
  After the free the capacity field is not cleared — the
  descriptor's alloca dies with the frame, so a second free
  cannot happen through it.
- `Terminator::Return` runs the same per-frame list cleanup
  before the raw-pointer cleanup, so early returns inside a
  region release heap-backed lists too.

## Open questions

- **List declared outside, grown inside.** The descriptor's
  alloca is outside the region; the buffer is inside. Neither
  frame currently tracks it. Out of scope here; addressed by
  ADR 0051's lexical-scope cleanup.
- **`break` out of a region.** A `break` inside a `region`
  block jumps to the loop exit and skips the `RegionExit`
  instruction, so region-scoped lists are not freed. ADR 0051
  closes this as a side effect — scope-exit cleanup frees what
  `RegionExit` would have freed, through the same helper.
- **List grown through a parameter inside a region.** The
  callee's descriptor is a copy (ADR 0049), so the caller's
  descriptor is unchanged. The callee's own `tracked_lists`
  entry handles its copy. Fine — no cross-frame case.
- **Free on callee return for by-value parameter copies.**
  The parameter's descriptor is destroyed when the callee's
  frame is torn down, so the buffer leaks unless the callee's
  body itself runs inside a region. A future ADR could add
  scope-exit cleanup for parameters.
- **Realloc failure.** `realloc` can return null. The current
  growth path stores the null into the descriptor and
  continues — a subsequent element write segfaults. Fail-
  closed handling (print + exit) is a small follow-up, not
  part of this ADR.

## Implementation plan

Five small commits, each buildable:

1. Add `tracked_lists: Vec<String>` to `LRegionFrame`.
   Initialise empty in `RegionEnter`.
2. Register a list in the innermost frame's `tracked_lists`
   at the three heap-allocation sites when inside a region.
3. `RegionExit` emits `if capacity > 0 { free(buffer) }`
   per tracked list, after the raw-pointer frees.
4. `Terminator::Return` calls the same per-frame list
   cleanup.
5. Tests: list grown inside a region, list grown and freed
   with an early return, list grown outside a region (leaks,
   documented as known).

## See also

- ADR 0007 — region memory
- ADR 0042 — LLVM dynamic lists (introduced heap-backed
  lists)
- ADR 0049 — list parameters are by value (introduced the
  parameter-copy heap allocation)
- `src/backends/llvm_codegen/mod.rs:138` — `LRegionFrame`
- `src/backends/llvm_codegen/instruction.rs:1832` —
  `RegionExit` handler

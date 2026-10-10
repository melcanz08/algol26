\
# ADR 0051: Scope-exit cleanup for heap-backed lists

Status: Proposed (design; implementation multi-phase)

## Context

ADR 0050 frees heap-backed list buffers at `RegionExit`. Its
scope is explicit: only lists whose descriptor is created
inside a `region` block, and only when control reaches
`RegionExit` or an early `return` from inside the region.

Everything else leaks:

- **No region anywhere.** `var xs := [1]; xs.append(2)` outside
  any `region` never frees the malloc'd buffer. This is the
  common case — `region` is the exception, not the rule.
- **List declared outside, grown inside.** The descriptor's
  alloca is outside the region, so no frame tracks it.
- **List parameter copy.** The ADR 0049 by-value copy mallocs a
  fresh buffer per call. It is freed only if the callee's body
  is itself inside a region containing the parameter binding.
- **Escape-on-return.** A list returned from a `proc` has its
  buffer copied to a fresh heap allocation (or, if already
  heap-backed, handed off directly) and never freed by either
  side.

The gap between ADR 0050 and "list buffers do not leak" is the
gap between *region scoping* and *lexical scoping*. A `region`
is a user-written, delimited allocation window. Lexical scoping
is implicit: every block is a scope, whether the user names it
or not. List lifetimes follow lexical scoping in every other
language with garbage collection or RAII. This ADR makes them
follow it here.

## Decision

Emit a `free(buffer)` for a heap-backed list when its binding
leaves lexical scope. This is the same operation ADR 0050 emits
at `RegionExit`, generalized to every scope-exit path.

The mechanism: the IR builder already tracks lexical scopes via
`push_scope` / `pop_scope`. Extend it to record, per scope, the
list variables declared in it. On scope exit — however the scope
is exited — emit an `Instruction::FreeList { name }` for each
tracked list, LIFO. The LLVM codegen lowers `FreeList` to the
existing `emit_free_list_if_heap` helper (guarded by
`capacity > 0`, idempotent via the `capacity = 0` store after
free).

The interpreter treats `FreeList` as a no-op: its
`RuntimeValue::List` holds an owned `Vec`, and Rust's drop
handles the memory. The verifier checks that the named
descriptor exists and is list-typed.

## Scope-exit paths

A scope can be exited four ways. All four need cleanup.

1. **Fall-through.** The block's last statement runs and control
   moves to the next block. Trivial case: emit `FreeList`
   instructions after the last statement, before the block's
   terminator.
2. **Return.** A `return` inside the scope exits that scope and
   every enclosing scope up to the function boundary. Emit
   `FreeList` for every list in every scope between the return
   site and the function, LIFO.
3. **Break.** A `break` exits the innermost loop's body scope
   and every scope between. Same walk as `return`, stopping at
   the loop boundary.
4. **Continue.** A `continue` exits the current iteration's
   body scope, stopping at the loop's condition block. Same
   walk, different boundary.

`exit(1)` (the OOB error path) skips cleanup: the process is
tearing down and the OS reclaims everything.

## The hard case: returns and ownership

`emit_list_return` (ADR 0042 phase 1b, ext) currently escapes a
stack-backed buffer to the heap on return. When scope-exit
cleanup is added, it must also handle the heap-backed case:

    proc make() -> List<Int>
        var xs := [1]
        xs.append(2)       // xs is now heap-backed
        return xs          // does the callee free xs.buffer?

If the callee frees it at scope exit, the caller receives a
dangling pointer. If it does not, ownership simply transfers —
the caller owns the buffer, the callee's binding goes dead, no
leak.

The rule: **on return, ownership of the returned list's buffer
transfers to the caller.** The callee does not free it. The IR
builder omits `FreeList` for a list whose value is the return
expression.

This generalizes: any list whose value is *consumed* by the
return expression — not just a bare variable, but a list passed
to a function call that itself escapes, or a list-typed field of
a returned record — transfers ownership. The analyzer's
move-checking already tracks these; the IR builder reads the
same state to decide which lists to free and which to hand off.

**Escape-to-heap on return becomes unconditional.** Today
`emit_list_return` only escapes when `capacity == 0`
(stack-backed). Under this ADR, a heap-backed list returned from
a function whose scope-exit would free it must also be copied —
the callee keeps its buffer, the caller gets a fresh copy. The
alternative (transfer ownership and skip the free) means
annotating the escape path with "this buffer is owned by the
caller now." Both work; the unconditional-copy version is
simpler and preserves the existing `emit_list_return` shape.
Defer the ownership-transfer optimization to a follow-up ADR.

## Interaction with regions

A list inside a region is freed by `RegionExit` (ADR 0050) and,
under this ADR, also by its binding's scope exit if the scope
closes before `RegionExit`. Two mechanisms can free the same
buffer. To avoid double-frees:

- The `capacity = 0` store after free makes a second cleanup a
  no-op. An inner scope-exit followed by a `RegionExit` is
  safe.
- A `break` out of a region currently skips the `RegionExit`
  instruction entirely. ADR 0050 has this gap. ADR 0051 closes
  it as a side effect: scope-exit cleanup frees what
  `RegionExit` would have freed, both going through
  `emit_free_list_if_heap`.

After this ADR, ADR 0050's region machinery is redundant for
list buffers but not for raw `alloc(n)` pointers, which have no
lexical-scope story yet.

## Why not "every block is an implicit region"

The obvious simplification: reuse the region machinery verbatim
by treating every `{ ... }` block as a hidden region. Push a
frame on block entry, pop on exit. Then scope-exit cleanup *is*
region-exit cleanup, and ADR 0050's code handles it.

Rejected because:

- Every block would emit cleanup code, including the vast
  majority with no lists. The overhead is per-block, not
  per-list.
- Regions and blocks have different lifetimes under `break` and
  `continue`. A region block broken out of has cleanup
  semantics the region machinery does not currently handle
  correctly. Reusing it hides that bug rather than fixing it.
- A region's cleanup runs on `return`; a block's cleanup runs
  on `break` / `continue` too. The paths are similar but not
  identical, and merging them muddles the design.

Explicit scope tracking keeps the two mechanisms separate and
each honest about its own paths.

## Migration plan

Four phases, each independently verifiable.

### Phase 1 — IR scaffolding

Add `Instruction::FreeList { name: String }` to the IR. Wire
through:

- Verifier: accept, check the named list's descriptor exists in
  `env.variables` and is list-typed.
- Interpreter: no-op arm.
- LLVM codegen: `emit_free_list_if_heap` on the descriptor from
  `list_structs[name]`.

No IR builder emission yet. Run the full suite; expect no
behavior change.

### Phase 2 — Fall-through cleanup

Track list variables per scope in the IR builder. On scope exit
via normal fall-through, emit `FreeList` for each. Handles the
simplest case: a list declared inside a nested block, used, and
left.

    proc main
        if true
            var xs := [1]
            xs.append(2)
            print(xs[1])
        print(99)
        // xs's scope exited here; buffer freed

Run the region probes from ADR 0050 plus a new
`no_region_scope_exit` probe that fails today.

### Phase 3 — Return / break / continue cleanup

Extend the scope walk to the three early-exit terminators. For
`Terminator::Return`, collect lists from every open scope; for
`Break` and `Continue`, walk up to the loop boundary. Same LIFO
emission.

Handle the return-ownership rule (skip `FreeList` for lists
whose value is the return expression) in this phase. Without
it, `make() -> List<Int>` would double-free.

### Phase 4 — Escape-on-return reconciliation

Make `emit_list_return` unconditional-copy (currently
stack-only). This pairs with the ownership rule so the callee
never hands out a buffer it is about to free.

Update ADR 0042's `emit_list_return` doc comment to note the
change.

## Open questions

- **Interpreter `FreeList` semantics.** Rust's drop already
  frees the `Vec` at scope exit, so `FreeList` is a no-op in
  the interpreter. Confirmed by inspection; a test asserting
  the no-op shape is cheap.
- **Lists inside `spawn` / `parallel`.** Each thread or
  parallel branch has its own scope. Cleanup at the end of the
  block is the natural rule; whether the runtime waits for the
  spawned body before freeing is a concurrency question.
  Defer; spawn is interpreter-only today.
- **List as a record field.** A `rec` containing a `List<T>`
  field owns that list when the record is owned. Freeing the
  record should free the field's buffer. Same mechanism, one
  level down. Defer — out of scope for the initial phases.
- **List returned by a call bound to a local.** `val c :=
  make()` where `c` is not itself inside a region. The escape
  in `make` transfers ownership; `c`'s scope exit frees the
  buffer. Covered by phase 2 once phase 3's ownership rule is
  in place, but the ordering matters — verify with a probe.

## See also

- ADR 0007 — region memory
- ADR 0042 — LLVM dynamic lists (introduced heap-backed lists)
- ADR 0049 — list parameters are by value (parameter-copy
  allocations)
- ADR 0050 — region cleanup for heap-backed lists (this ADR
  generalizes it)
- `src/backends/llvm_codegen/mod.rs` — `LRegionFrame`,
  `emit_free_list_if_heap`, `register_region_list`
- `src/semantics/builder/control_flow.rs` — scope tracking,
  terminator emission
- `src/ir/semantic_ir/instructions.rs` — `Instruction` enum

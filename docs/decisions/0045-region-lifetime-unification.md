# ADR 0045: Unify region lifetimes with borrow lifetimes

Status: Proposed (design)

## Context

ALGOL26 has two independent notions of "when does a reference to
memory stop being valid":

1. **Borrow lifetimes.** `&x` and `&mut x` live for the duration
   of the enclosing scope (today; see ADR 0043 for the NLL
   proposal). The analyzer tracks them.

2. **Region lifetimes.** A `region r ... end` frees every
   allocation made inside it on exit. The analyzer tracks which
   variables hold region-scoped pointers via
   `SemanticState::region_stack` and `LRegionFrame`.

These systems **do not talk to each other**. A pointer into a
region is caught on region exit (see `B5_region_ptr_to_outer`,
which was rejected). A reference into a region is not (see
`B5_region_ref_outer`, which was accepted and returned `42` from
memory that should have been freed).

Confirmed soundness hole:

    proc main
        var leaked: &Int := &0
        region r
            var x := 42
            leaked := &x
        print(*leaked)         // accepted; reads a variable whose
                               // scope has ended

The issue this ADR addresses is not merely that this specific case
is uncaught, but that the two tracking systems are structurally
separate. Every new interaction between them adds another spot
that can be forgotten.

## Decision

Unify region lifetimes and borrow lifetimes into **one lifetime
system**.

A lifetime is a label attached to a value or a place, indicating
until when it is valid. Two lifetime classes exist:

- **Scope lifetime** — valid until the enclosing lexical scope
  ends (the default for local variables).
- **Region lifetime** — valid until the named region exits.

A borrow `&x` or `&mut x` has the same lifetime as `x`. A
reference stored in a variable outlives its referent iff the
referent's lifetime is shorter than the enclosing scope's.

Two new static rules follow:

1. **A reference may not be assigned to a binding whose scope
   outlives the referent's lifetime.** This catches
   `B5_region_ref_outer` directly: `leaked` lives at function
   scope, but `&x` has region lifetime.

2. **A region's exit may not happen while a live reference into
   it exists.** This catches the case where a reference is passed
   into a callee that outlives the region (through function
   summaries; see ADR 0044 §Function call summaries).

## Consequences

**Positive.**

- The B5 soundness hole is closed by construction: any reference
  into a region has region lifetime, and the assignment rule
  rejects writes to a longer-lived binding.
- No new analysis is needed at region exit — the region's
  allocations are already freed by the existing cleanup. Only
  the lifetime-check that *permits* the assignment is new.
- Users get one mental model, not two: "how long does this
  reference live" is answered by its scope, and region exits
  shorten that scope.

**Negative.**

- The region analysis and the borrow analysis currently live in
  separate modules (`ownership.rs`, `state/mod.rs`). They must be
  refactored into one. This is a substantial change to
  `SemanticState`.
- Every existing test that mixes `region` with `&` needs to be
  re-run and possibly rewritten.
- The lifetime annotation must appear in diagnostics: "the
  reference has region lifetime `r`, the binding has scope
  lifetime of function `main`".

## Implementation sketch

1. **Extend `Place` with lifetime.** Each `Place` carries a
   `Lifetime` enum, either `Scope(depth)` or `Region(name)`.
   Populated by the existing scope-tracking machinery.

2. **Add assignment check.** In `Stmt::Assign` and
   `Instruction::Declare`, when the value's type contains a
   `Borrow(_)` / `MutBorrow(_)`, verify the value's lifetime is
   `>=` the target's lifetime in the scoping order.

3. **Region exit check.** When `RegionExit` is analyzed, verify
   no live borrow is into the region's storage. With (1) and (2)
   this should be a no-op for well-formed programs, but keeping
   the check catches a class of producer bug.

4. **Rewrite `SemanticState`.** Merge `region_stack` and
   `borrows`. Lifetime becomes a first-class field of every
   borrow state entry.

5. **Deprecate `LRegionFrame`'s saved_slots mechanism.** With
   regions and borrows speaking the same language, the snapshot
   logic that currently lives in the LLVM codegen (`region_saved_*`
   allocas) may no longer be needed for correctness — but it will
   still be needed for the interpreter to free on the right
   schedule, so this step is code cleanup only.

## What is deliberately out of scope

- **Full escape analysis.** This ADR handles *statically
  tracked* references. Values reaching `unsafe` blocks and FFI
  are outside the lifetime system by definition. ADR 0015 covers
  the boundary.

- **Region subtyping.** Whether a region can outlive another
  region is a question about region hierarchy. The current
  language has no nesting rule, and this ADR does not add one.

## Alternatives considered

### A. Keep the two systems, add cross-checks

Every time a new way to construct a region-scoped reference
appears, add a check that it isn't escaping. This is what the code
does today for raw pointers, and it is why references were missed.

**Rejected.** The failure mode is exactly what we're trying to
eliminate: a new feature adds a new escape shape, and someone
forgets to add a check. One unified system is more robust than N
ad-hoc checks.

### B. Runtime-checked regions

Make the region frees happen on the heap, and rely on the
allocator to detect use-after-free.

**Rejected.** This trades a compile-time guarantee for a runtime
one. ALGOL26's pitch is compile-time safety.

## See also

- ADR 0043 — non-lexical lifetimes
- ADR 0044 — place-based borrows
- `docs/decisions/0007-region-memory.md` — the region design
- `docs/decisions/0005-ownership-model.md` — the borrow design
- `src/semantics/state/mod.rs` — the merged state this ADR
  envisions
- Session commit that surfaced the B5 repro: this ADR is
  motivated in part by the probe `B5_region_ref_outer`, which
  currently returns `42` from a freed stack slot

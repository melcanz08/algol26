# ADR 0037 — WASM Records and References

## Status

Proposed. Requires investigation before any decision is frozen.

> **Status note (2026-10-06).** The WASM backend refuses both
> records and references at the capability boundary. This ADR
> frames the design problem and proposes a direction, but
> freezes no concrete layout or codegen decisions — the WASM
> backend's current value model is not documented, and the
> first implementation step is investigation.
>
> This ADR is deliberately less concrete than ADR 0036 (LLVM
> records). LLVM's memory model is standardized; WASM's is not,
> and the ALGOL26 WASM backend may or may not already have a
> linear-memory convention for lists and strings.

## Context

The WASM backend is the least mature of the three. The
capability matrix refuses two features that LLVM and the
interpreter support to varying degrees:

- **Records.** `Type::Record` in any position is refused. Same
  as LLVM (ADR 0036).
- **References.** `&x`, `&mut x`, `*r`, and `AddrOf` are
  refused. Unique to WASM among the three backends.

Parity today:

| Backend | Records | References | Methods |
|---|---|---|---|
| Interpreter | Yes | Read-only, pass-through | All three modes |
| LLVM | No | Yes | Blocked on records |
| WASM | No | No | Blocked on both |

What the WASM backend *does* support, per the capability scan:
scalars (Int, Float, Bool, String), lists, control flow,
function calls, some builtins, and the `Result`/`Spawn`/`Fork`
features. Any of those with compound representations implies
the backend already has *some* memory model for values — a
list of variable length cannot live on the WASM value stack
alone. But the shape of that model is undocumented, and it is
the first thing this ADR's investigation step must answer.

## Decision

**Direction, not decisions.** This ADR commits to the following
framing and defers concrete design to investigation and a
follow-up revision:

### Records and references share a representation

Both are addresses into WASM linear memory. A record value is
the address of its first field. A reference (`&x`, `&mut x`) is
the address of the referenced value. `*r` is a load from that
address; `u.name := x` on a record with a reference receiver is
a store through the address.

This is the same shape as ADR 0036's LLVM proposal (records as
pointers, references as pointers) — WASM and LLVM differ in
how pointers are represented (offset into linear memory vs.
native address) but not in what they point at.

### Memory allocation is a new concern

The interpreter has no allocation model; values are owned
Rust values. LLVM has stack allocas and the language's existing
`alloc`/`free`. WASM has linear memory and no stack; every
value must be placed at a linear-memory offset.

This ADR does not decide how that placement works. It is the
central open question.

### Records and references are separate problems

Records can land first. Once record values have addresses,
references to records follow naturally. References to scalars,
lists, and other value kinds are a further step.

The implementation plan therefore splits into two phases, each
with its own review gate.

## Open questions

The following must be answered before any code lands. They are
listed here so the investigation has a target.

**Q1. What is the WASM backend's current value model?** Does it
place lists in linear memory (pointer + length), or are lists
also unsupported in practice? Does the backend have a bump
allocator or similar for allocating memory?

**Q2. How do scalars and aggregates coexist?** WASM's value
stack holds `i32`, `i64`, `f32`, `f64`. A record cannot fit on
the stack. Either all compound values become `i32` offsets into
linear memory, or a hybrid model is used with an explicit
tagged pointer scheme.

**Q3. What handles allocation?** Each record literal, list
concatenation, and string operation that produces a new value
needs memory. Options: (a) a bump allocator in the runtime
with a well-known global, (b) host-provided `malloc`-style
imports, (c) a real allocator compiled to WASM. The choice
affects the module's imports and its interaction with the
host environment (see `runtime/wasm/host.js`).

**Q4. How does the memory grow?** WASM linear memory starts at
a fixed size and can grow with `memory.grow`. The backend needs
a policy: allocate a large initial region and error on
overflow, or grow on demand.

**Q5. What is a reference's representation in the presence of
WASM's multi-value returns and the reference-types proposal?**
WASM 2.0 has typed reference types; the backend may or may not
want to use them. `i32` offsets are simpler but lose type
safety at the WASM level.

**Q6. How do records interact with WASM's function ABI?**
Multi-value returns are supported; a record returned by value
could be returned as an `i32` offset, or as multiple stack
values (one per field). The latter is more efficient but
harder to generalize.

**Q7. Does the existing WASM backend align with the LLVM
record layout, or does it need its own?** Cross-backend
agreement is the differential testing goal. If WASM and LLVM
choose different layouts, the same program may produce
observably different results on byte-level operations (hashing,
serialization).

## Scope boundary

**In scope (eventually):**

- Records and references on WASM, backed by linear memory.
- Records with primitive, string, list, and record fields.
- Methods on WASM (which follow once records do).

**Explicitly out of scope:**

- Replacing the interpreter or LLVM as the primary
  correctness backends. WASM is a portability target, not a
  reference implementation.
- WASM GC or the reference-types proposal. Linear memory is
  the baseline.
- Any performance target. Correctness first.

## Implementation order

Three gates, each preceded by investigation:

```
W0. Investigation. Read src/backends/wasm_backend.rs and the
    interpreter's list handling. Answer Q1 and Q3. If the WASM
    backend has no memory model at all, this ADR is premature
    and should be replaced by a broader 'WASM value model' ADR.

W1. Design pass. Answer Q2, Q4, Q5, Q6, Q7. Revise this ADR
    with concrete decisions and a review record entry. Only
    then does implementation begin.

W2. Records phase. Type lowering, literal codegen, field
    access, field assignment. Same shape as ADR 0036's L1–L5.

W3. References phase. Borrow, deref, and mutable-borrow
    lowering. Capability matrix gains Feature::References
    for WASM.

W4. Fixtures. Enable record and reference fixtures on WASM.
    Differential tests against the interpreter.
```

W0 and W1 are the same work ADR 0036's L0 does for LLVM, plus
the extra questions WASM raises about memory management. This
ADR cannot be implemented before W1 completes and revises it.

## Consequences

### Positive

- If successful, all three backends agree on records and
  references. The parity table has no gaps.
- The WASM backend becomes usable for browser-hosted ALGOL26
  programs that use anything more complex than scalars.
- The `runtime/wasm/host.js` file, which currently must be
  minimal because the language surface is minimal, gains a
  meaningful role: providing the allocation primitives the
  compiled module imports.

### Negative

- The WASM backend has to invent a memory model that the other
  two backends do not need. There is no existing convention to
  follow.
- The linear-memory allocator becomes a permanent part of the
  compiled module. Its behavior under memory pressure, its
  interaction with `memory.grow`, and its effect on module
  size are all new concerns.
- If the WASM layout diverges from LLVM's, cross-backend
  differential tests cannot compare byte-level representations.
  They can still compare observable output, but the tests are
  weaker.

### Neutral

- The IR is unchanged. The codegen does the work.
- The interpreter is unaffected. It has no memory model and
  does not need one.

## Alternatives considered

### Skip WASM records; leave WASM as a scalar-only target

**Rejected.** The `runtime/wasm/` directory already exists;
the backend is live; the capability scan is the only thing
preventing records. Leaving it scalar-only would mean every
non-trivial program is LLVM-only or interpreter-only, which
undermines the portability motivation for having a WASM target
at all.

### Use WASM reference types (externref / funcref)

**Rejected for v1.** Reference types are for host objects, not
for in-module data structures. Records and references into
them are naturally linear-memory concerns. A future ADR could
use externref for host interop if that becomes important; it is
orthogonal to this one.

### Use WASM GC (the typed-function-references proposal)

**Rejected for v1.** GC is a browser-runtime feature with
uneven support and a very different programming model. Linear
memory is the portable baseline. If a future version of WASM
makes GC ubiquitous and useful, revisiting is fine.

### Match LLVM's layout exactly, byte for byte

**Preferred if achievable, but not yet decided.** See Q7. If
the two backends can agree on layout without contortions, do
it. If LLVM's alignment rules and WASM's alignment rules
diverge in ways that make agreement expensive, accept the
divergence and weaken the differential tests to observable
output only.

## Review record

No outside review yet. This ADR is a framing document. It is
expected to be substantially revised after W0 and W1. The
review record should be updated with the answers to Q1–Q7
before W2 begins.

## References

- ADR 0024 (Records) — the source-level feature.
- ADR 0005 (Ownership Model) — Copy, move, and borrow at the
  semantic level.
- ADR 0010 (Canonical IR) — why records are not a special IR
  shape.
- ADR 0033 (Methods) — the feature blocked on this ADR.
- ADR 0036 (LLVM Record Support) — the parallel problem for
  the other unfinished backend.
- `src/backends/wasm_backend.rs` — the codegen module this
  ADR would extend.
- `runtime/wasm/host.js` — the host-provided runtime the
  compiled module interacts with.
- `runtime/wasm/run.sh` — the toolchain invocation.

## See also

- `docs/features/methods.md` — the feature contract; records
  and references are prerequisites for methods on WASM.
- `docs/features/record.md` — the record feature contract.
- `docs/decisions/README.md` — the ADR index and convention.
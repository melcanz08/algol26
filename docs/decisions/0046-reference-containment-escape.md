# ADR 0046: Reference containment in escape analysis

Status: Proposed (design)

## Context

The escape analysis catches a reference that escapes directly:

    function escape() -> &Int
        var x := 42
        return &x           // E-ESCAPE-001: Reference 'x' escapes via return

It does not catch a reference that escapes **inside a composite
value**:

    rec Holder
        p: &Int

    function escape() -> Holder
        var x := 42
        val h := Holder { p: &x }
        return h            // ACCEPTED today; caller gets a
                            // reference to a dead stack slot
                            // when it dereferences h.p

The same hole exists for `List<&T>` and any other type that can
contain a reference. Confirmed by probe (this session's B2
suite): `B2_escape_via_record` and `B2_escape_via_list` both
return `42` from memory that the referent's scope has already
left.

## Why the current check misses it

The escape check lives in `src/ir/cfg/dataflow.rs`, not in the
AST analyzer. It operates on two IR-level instructions:

    CfgInstruction::ReturnRef { place }   →  E-ESCAPE-001
    CfgInstruction::Escape { from, to }    →  E-ESCAPE-002

The IR builder emits `ReturnRef` when the returned expression is
itself a reference (`return &x`). It has no equivalent for a
returned value that *contains* a reference. The dataflow pass
consumes the emitted instructions; it cannot invent a `ReturnRef`
for a reference it doesn't know is there.

## Decision

Extend the IR builder to track **reference containment** for
variables and values, and emit `Escape` when a value that
contains a reference to a local leaves the function.

Concretely:

1. **Add a "contains-reference-to" set to the IR builder's value
   model.** For each variable currently in scope, the builder
   records a set of local names whose references are reachable
   from it. Initially empty; populated at construction sites.

2. **Populate the set at every composite constructor:**
   - `RecordLiteral { fields }` — union the containment sets of
     the field values.
   - `List { elements }` — union across elements.
   - `Some` / `Ok` / `Error` — the inner value's set.
   - `Array` — union across elements.
   - `Assign` to a variable — replace the target's set with the
     source's set.
   - `Declare` — same.
   - `FieldAssign` / `ArrayAssign` — union into the target's set.

3. **Emit `Escape` at every escape boundary** for each name in
   the containment set:
   - `Return` of a variable whose set is non-empty.
   - Assignment to a binding whose scope outlives the referent.
   - Passing a value to a function whose parameter scope outlives
     the referent's scope.

4. **The dataflow pass is unchanged.** It already consumes
   `Escape { from, to }` and produces the diagnostic; the builder
   is what needs new information.

## Consequences

**Positive.**

- Closes the B2 hole: `return h` where `h` contains `&x` produces
  `E-ESCAPE-002: Reference 'x' escapes to return`.
- The mechanism generalizes: any future composite type just needs
  to be added to the containment-union list in the builder; the
  dataflow side needs no change.
- The containment relation is a natural side-effect of building
  a value; it doesn't require a new analysis pass.

**Negative.**

- Every composite constructor in the IR builder gets a new step.
  The pattern is mechanical (walk fields, union their sets), but
  there are many sites.
- Containment is transitive across assignments: `val h2 := h`
  where `h` contains `&x` must copy the set. Every move/copy site
  must be audited.
- The containment set can be large in pathological programs (a
  list of records each referencing many locals). The cost is
  bounded by the size of the IR, but a large program may see
  measurable slowdown in the builder. Not a correctness concern.

## What is deliberately out of scope

- **Interprocedural containment.** A function returning a
  `Holder` that contains a reference to one of *its own*
  parameters (not a local) is not an escape — the caller owns the
  parameter. Distinguishing "reference to a local" from
  "reference to a parameter" needs the containment set to carry
  origin information. This ADR tracks containment of locals
  only; parameter references flow through as non-escaping.

- **Reference cycles.** A record that contains a reference to
  itself (through a mutable field) would make the containment
  set unbounded. The language does not currently permit direct
  cycles (`ADR 0036` restricts record recursion); if it ever
  does, this ADR needs revisiting.

- **Region-contained references.** A reference into a region is
  B4/B5 territory (ADR 0045), not B2. This ADR handles local
  scope escape only.

## Alternatives considered

### A. Extend the dataflow pass to inspect returned values

The pass could walk the returned record's fields looking for
references. But the pass operates on `CfgInstruction`, not on IR
value structure — it has no representation for "record field
containing a reference." Making it work requires new
`CfgInstruction` variants, which is equivalent to Option 1 but
with the containment logic in a worse place (a flow abstraction
rather than the builder that has the value in hand).

**Rejected** for architectural reasons.

### B. Restrict composite types to forbid references

`rec Holder { p: &Int }` could be a type error, and `List<&T>`
could too.

**Rejected** because it removes useful programs. `&Int` fields
are a common way to express "this record borrows the value it
refers to", and the language should support them; the analysis
just needs to be precise enough to reject the escaping cases.

### C. Runtime reference-checking

Emit a length or generation count alongside each reference and
check on dereference.

**Rejected.** ALGOL26's pitch is compile-time safety; a runtime
check trades the guarantee for overhead and a class of
loud-but-late failures.

## See also

- ADR 0005 — ownership model (the source of the escape rules)
- ADR 0043 — NLL borrow lifetimes
- ADR 0044 — place-based borrows
- ADR 0045 — region lifetime unification
- `src/ir/cfg/dataflow.rs` — the current check
- `src/semantics/builder/` — where the containment tracking
  would live
- This session's B2 probes:
  `B2_escape_via_record` and `B2_escape_via_list`, both
  ACCEPT-then-print-42 today

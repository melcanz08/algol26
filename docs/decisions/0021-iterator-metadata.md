# ADR 0021 — Iterator metadata

## Status

Accepted. Implemented. Backfilled 2026-11-02.

## Context

`for x in xs { ... }` desugars to `Instruction::IteratorInit` +
`Instruction::IteratorNext`. Both need to know the *array type*
of `xs` (element type + length) so the iterator knows when to
stop. In an earlier version, the iterator guessed from context —
which worked for list literals but silently misbehaved for lists
passed as function parameters, lists returned from other
functions, or lists stored in record fields.

## Decision

**`Declare` records the array type.** Every `Instruction::Declare`
whose value is a `TypedIRValue::List` or `TypedIRValue::Array`
carries the full `Type::List(T)` / `Type::Array(T, N)` in a side
table (`list_array_types`) on the IR builder. The iterator
instructions consult that table.

**Fail closed on missing metadata.** If `IteratorInit` or
`IteratorNext` is reached for a binding whose array type isn't
recorded, both refuse with `EvalError::Unsupported` rather than
guessing. This surfaces IR-construction bugs at the moment they
happen, not as silent misbehavior later.

**Metadata is keyed by binding name.** `Declare { name, ... }`
inserts `list_array_types[name] = type`. A subsequent `Assign`
to the same binding that changes the type invalidates the entry;
the iterator then fails closed. The analyzer is the primary gate
for "the type is stable"; the metadata is a defense-in-depth
check.

## Consequences

### Positive

- `for x in xs` works when `xs` is a parameter, a field, or a
  return value — the metadata travels with the binding rather
  than being inferred from context.
- A missing metadata entry is a loud error, not a silent
  miscompile.
- The metadata mechanism is reusable: future passes that need
  "what is this binding's list type?" consult the same table.

### Negative

- Every `Declare` that produces a list must be paired with a
  metadata insert. The IR builder enforces this by construction;
  a hand-built IR (tests) must remember to insert.
- The metadata table is per-function, not per-block. Two
  bindings with the same name in different scopes share an
  entry. The analyzer ensures this can't happen (a scope shadow
  is a rename in IR), but the invariant is implicit.

## References

- `ADR 0010` — canonical IR
- `ADR 0016` — per-function CFG (block ordering)
- `ADR 0042` — LLVM dynamic lists (which made array types
  load-bearing for codegen)

## Backfill note

Backfilled 2026-11-02. The decision predates its ADR; STATUS.md
referenced "ADR 0021" without the file existing.

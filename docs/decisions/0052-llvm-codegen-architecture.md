\
# ADR 0052: LLVM codegen — structural gaps and a phased architecture

Status: Proposed (design; implementation phased)

## Context

`tests/codegen_probes/` (31 short programs, run by
`tools/probe_codegen.py`) exercises the LLVM backend along six
axes: list literal positions, receiver shapes, function
boundaries, control flow with non-scalar values, ownership, and
composite shapes.

Current distribution (interpreter vs LLVM):

    total 31 probes
      OK          18
      DIVERGE     3
      LLVM-GAP    7
      BOTH-FAIL   3

    LLVM gaps by class:
      missing_compile_value    3
      ice                      2
      value_route              1
      non_variable_receiver    1

The distribution is not random. Three structural problems
account for every failure.

## Problem 1 — `compile_value` is incomplete

`compile_value` is the single entry point for lowering a
`TypedIRValue` to an LLVM value. It handles most variants, but
three probes (`a3`, `a4`, `a5`) fail because the `List` variant
returns `Err(unsupported)`. Two more (`c6`, `f7`) hit
`unreachable!()` on `Option`-returning `if`/`match` bodies.

The `List` gap is deliberate — the arm was changed from
"silently return null" to "fail closed" during an earlier
cleanup. That was the right call at the time; the arm was never
implemented. `Option` is worse: the ICE means codegen reached
a path it believed unreachable, so a `compile_value` arm *or*
an upstream decision is missing.

**Fix.** Every `TypedIRValue` variant must have a
`compile_value` arm. For `List`: allocate an `[N x elem]`
alloca in the current function, store each element, build the
`{ptr, i64, i64}` descriptor struct, return it. For `Option`:
the `Some`/`None` arms exist already; the ICE is in the
*consumer* — a `match` on an `Option` value returned from a
function goes through a path that assumes the scrutinee is a
variable. Diagnose, then either extend the consumer or move
the ICE to a real error.

## Problem 2 — List operations are keyed on variable names

Every LLVM list operation looks up a `HashMap<String, _>` by
the variable's *name*:

    list_arrays:      HashMap<String, PointerValue>
    list_array_types: HashMap<String, BasicTypeEnum>
    list_lengths:     HashMap<String, usize>
    list_structs:     HashMap<String, PointerValue>

Three of the seven LLVM gaps (`f4`, and the `side_table_missing`
failures from earlier sessions) come from this: any list that
isn't bound to a variable — a field access, an array element, a
call result — has no key, so the operation either fails or
reads a stale entry.

ADR 0042 phases 1–4 introduced a `{ptr, i64, i64}` descriptor
*struct* that carries buffer, length, and capacity as runtime
values. The struct is the natural key: every list operation can
take a pointer to the descriptor instead of a variable name.
The side tables become *caches* of information that the
descriptor already carries.

**Fix (ADR 0051 phase 1).** Migrate list operations from
name-keyed to descriptor-keyed:

- `emit_list_append(descriptor_ptr, value)` — already this shape.
- `list_length_value`: returns `IntValue`, sourced from the
  descriptor when present.
- `list_buffer_value`: returns `PointerValue`, sourced from the
  descriptor when present.
- Every consumer of `list_arrays[name]`, `list_lengths[name]`
  migrates to a helper that takes the value, not the name.

After migration, `list_arrays` and `list_lengths` are deleted
or become pure caches. `list_array_types` remains for GEP
strides but can be computed from the element type.

## Problem 3 — Iterate-then-use loses type info

Three probes (`f1`, `f2`, `f6`) *diverge* silently — both
backends run, outputs differ. All three iterate a list whose
element type is non-scalar. The interpreter produces the
correct value; LLVM produces something else.

The likely site is `IteratorNext` (in `terminator.rs`). It
loads the element from the array, stores it into the loop
variable's alloca, and records the element type in
`iterator_elem_types`. But the loop variable is then *used*
in the body through `compile_value(Variable)`, which reads
`var_types[name]`. If those two sources disagree — for example
because `IteratorInit` recorded `List<Int>` as the element
type rather than the concrete `List<Record>`, or because the
GEP stride uses the array's `[0 x elem]` type with a
zero-sized element — the loaded value is wrong.

**Fix.** Diagnose with a minimal probe (extend
`tests/codegen_probes/` with single-element cases for each
non-scalar element type), then unify the source of truth: the
loop variable's `var_types` entry is set from
`iterator_elem_types`, and `IteratorNext` uses the same type
for its GEP that the array was built with.

## What's not a structural problem

`BOTH-FAIL` cases split:

- `c7_return_list` — my probe is wrong: `fn` with a
  statement body is a syntax error under ADR 0048. Fix the
  probe.
- `a2_list_lit_assign` and `b4_list_append_field` — real
  shared-layer gaps. The first is a `Declare`-with-annotation
  followed by `Assign` of a list literal; the second is
  `.append` on a field. Both are one-off bugs, not instances
  of a class. Fix as normal bugs.

## Proposed sequence

Four phases, each independently verifiable.

### Phase 1 — Close the `compile_value` gaps

Implement `compile_value(TypedIRValue::List)`. Diagnose and
fix the `Option` ICE. Extend `tools/probe_codegen.py` to
report a `missing_compile_value` gap as a named regression if
it reappears.

Exit criteria: probes `a3`, `a4`, `a5`, `c6`, `f7` pass; no new
`missing_compile_value` or `ice` verdicts.

### Phase 2 — Descriptor-keyed list operations

Migrate list operations from name-keyed to descriptor-keyed.
Remove `list_arrays` and `list_lengths` as authoritative
sources; they become caches or are deleted. This is the
ADR 0051 phase 1 work, applied to the full backend.

Exit criteria: probes `f4` passes; no `side_table_missing` or
`non_variable_receiver` verdicts.

### Phase 3 — Unify the loop variable source of truth

Diagnose and fix the three DIVERGE cases. The loop variable's
type comes from `iterator_elem_types`, and its GEP stride uses
the same element type the array was built with.

Exit criteria: probes `f1`, `f2`, `f6` produce identical
output on both backends. **No DIVERGE verdicts remain.**
Divergence is a soundness bug — it should be the top priority,
because a wrong-answer silently is worse than a
compile-error loudly.

### Phase 4 — Fix the two shared-layer bugs

`a2_list_lit_assign` and `b4_list_append_field` fail on both
backends; they belong to the analyzer / builder, not to the
LLVM codegen. Fix as ordinary bugs once phases 1–3 have
cleared the LLVM noise.

Exit criteria: full probe suite is OK except for a small set of
documented capability gaps (Result, Map, spawn — interpreter-
only features).

## Why this order

Phase 1 before phase 2: the `compile_value(List)` implementation
needs to construct a descriptor, which is cleaner once phase 2's
descriptor-keyed helpers exist. But phase 1 is smaller, so it
lands first as a warm-up.

Phase 3 before phase 4: DIVERGE cases are soundness bugs;
shared-layer bugs are convenience bugs. Soundness first.

## Probe suite as a permanent instrument

`tests/codegen_probes/` and `tools/probe_codegen.py` are added
to the repo. Every future codegen change runs the suite; every
new failure mode becomes a new probe. The suite is the answer
to "how complete is the LLVM backend" — a question that was
previously answered by whichever error surfaced next.

## See also

- ADR 0042 — LLVM dynamic lists (introduced the descriptor
  struct)
- ADR 0049 — list parameters are by value
- ADR 0050 — region cleanup for heap-backed lists
- ADR 0051 — scope-exit cleanup for heap-backed lists
  (descriptor-authoritative design lives there)
- `tools/probe_codegen.py` — the instrument
- `tests/codegen_probes/` — the probes

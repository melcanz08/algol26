# ADR 0044: Place-based borrow tracking

Status: Proposed (design)

## Context

The current borrow checker tracks conflicts by **variable name**.
Two borrows of the same variable name conflict; two borrows of
different variable names do not, regardless of what they actually
reference at the memory level.

This produces both false positives and false negatives:

    // False positive: disjoint fields of one record
    rec Pair
        x: Int
        y: Int
    var p := Pair { x: 1, y: 2 }
    val rx := &p.x
    val ry := &p.y       // E0007: p is already borrowed
                         // — but `rx` and `ry` never overlap

    // False positive: disjoint list elements
    var arr := [1, 2, 3]
    val r0 := &arr[0]
    val r1 := &arr[1]    // E0007: arr is already borrowed
                         // — but indices differ

    // False negative (through a function)
    function modify(r: &mut Int)
        *r := 99
    var p := Pair { x: 1, y: 2 }
    val rx := &p.x
    modify(&mut p.y)     // accepted — different variable name
                         // but this is fine, x and y are disjoint

The analyzer is neither consistently conservative nor consistently
precise. Whether it accepts or rejects depends on whether the
programmer wrote the same name twice, not on whether the memory
actually overlaps.

## Decision

Move borrow tracking from "variable name" to **memory place**.

A **place** is a path expression rooted at a variable:

    x            — a variable
    x.f          — a record field
    x.f.g        — a nested record field
    x[i]         — a list / array element
    *r           — a dereference
    r.f          — a field through a reference
    (place)      — any composition of the above

Two borrows conflict when their places **may overlap** at runtime.
The analysis is over-approximate when it cannot prove disjointness
(e.g. two indices of the same list with unknown values) and precise
when it can (e.g. distinct literal field names).

## Consequences

**Positive.**

- `&mut p.x` and `&p.y` no longer conflict; the code above is
  accepted.
- `&mut arr[0]` and `&arr[1]` no longer conflict; same.
- The diagnostic for a genuine conflict is precise: the analyzer
  can name the exact place ("`p.x` is borrowed and `p.y` is being
  mutated" rather than "`p` is borrowed").
- Function calls that take a `&mut` to one field while another
  field is borrowed are correctly handled, because the argument
  expression is itself a place.

**Negative.**

- Every existing borrow-tracking site must be rewritten. Today
  the analyzer keys off `String` variable names;
  `SemanticState::borrows` becomes `HashMap<Place, BorrowState>`.
- Aliases through function parameters must be resolved to their
  call sites. `modify(&mut x)` inside `modify` becomes
  `&mut <call-site place>` in the summary. This is a form of
  interprocedural analysis and can be expensive.
- Diagnostic wording and error codes (`E0007`) need to change to
  reflect places rather than names.

## Implementation sketch

1. **Define `Place`.** A small algebraic type: `Var(name)`,
   `Field(box Place, field_name)`, `Index(box Place, ExprId)`,
   `Deref(box Place)`, `Ref(box Place)`. Lives in
   `src/semantics/analyzer/place.rs`.

2. **Place resolution.** A pass that lowers every `&x.f` /
   `&mut arr[i]` / `*r` into a `Place`. This is a *forward* pass
   over the AST; result is a `HashMap<ExprId, Place>` cached for
   the borrow checker to consume.

3. **Overlap check.** Two places conflict iff one is a prefix of
   the other, or their roots are the same variable and one has an
   unknown index at the point of conflict. A "may-overlap"
   analysis over indices uses the literal value when known,
   falls back to "conflicts with anything" otherwise.

4. **Rewrite `SemanticState::borrows`.** Key by `Place` rather
   than `String`. Update every site that reads it — most of them
   are in `ownership.rs` and `expr.rs`.

5. **Function call summaries.** When a function is called with a
   `&mut place`, the callee's `modify` body must be analyzed
   under that place binding. In practice this means the analyzer
   runs per-call-site on generic `&mut` parameters (or the
   analysis carries a substitution table).

6. **Test migration.** The soundness suite gains new positive
   cases (disjoint-field borrows, disjoint-index borrows) and
   keeps all existing negative cases.

## What is deliberately out of scope

- **Borrow lifetimes.** This ADR is about *what* a borrow refers
  to. *When* a borrow is live is ADR 0043.

- **Region lifetimes.** ADR 0045.

- **Full alias analysis.** Overlap through pointers is a "may
  alias" problem in the general case. This ADR's proposal is a
  place-based approximation, not a full points-to analysis.

## Alternatives considered

### A. Keep variable-name granularity, document the imprecision

Simplest. Users write disjoint-field borrows by splitting fields
into separate variables. Awkward but possible.

**Rejected** as the long-term plan. The false positives are common
enough that users will complain.

### B. Interval-based memory tracking

Track byte ranges rather than symbolic places. More precise for
`arr[0]` vs `arr[1]`, less useful for record fields (layout-
dependent).

**Rejected** because record-field layout is not finalized in the
frontend, and "place" is closer to the source-level concept the
user is reasoning about.

## See also

- ADR 0043 — non-lexical lifetimes
- ADR 0045 — region lifetime unification
- `docs/decisions/0005-ownership-model.md` — original borrow rules
- `src/semantics/analyzer/ownership.rs` — current variable-name
  tracking

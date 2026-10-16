# ADR 0044: Place-based borrow tracking

Status: Current (analyzer side).

- **Stage 1** (2026-10-11, commit `50d9e80`). `Place` and
  `Projection` introduced; `BorrowState.place` changed from
  `String` to `Place`. Every constructor site wraps in
  `Place::simple(name)` — no behavior change, all 437 lib tests
  pass.
- **Stage 2 partial** (2026-10-11, commit `3cc0af9`). Shared
  borrows (`ExprKind::Borrow`) resolve through `place_of_expr`, so
  `&p.x` and `&xs[0]` register as places instead of falling
  through to the value path. `pop_scope` releases shared borrows
  whose base variable exits, closing the synthetic-id leak.
- **Stage 2 complete on the analyzer side**
  (2026-10-12, commits `8db4559`, `44076dc`, `377bfeb`, `21bad18`).
  - `MutBorrow` resolves its operand through `place_of_expr`;
    `check_borrow_rules_place` honors its `mutable` parameter;
    `register_mutable_borrow` takes a `Place`.
  - `Stmt::FieldAssign` rejects writes to a field whose place
    overlaps a live borrow; releases only borrows whose place
    overlaps the written place.
  - `Stmt::ArrayAssign` does the same for list elements.
    Non-literal indices fall back to the conservative "any
    borrow of the base array conflicts" policy.
  - Four regression tests in
    `tests/semantics/borrow_checker_extra_test.rs` pin the
    reject-for-same-place and accept-for-disjoint-place
    behavior on both fields and elements.
- **Stage 3: read-side place granularity** (this commit).
  The read-side check at `ExprKind::Var` used
  `Place::simple(name)`, losing any field or index projection
  the leaf sat inside. Any field-level mutable borrow triggered
  a conflict on the base name. `SemanticAnalyzer::in_place_expr`
  is now set by the three place-producing arms (FieldAccess,
  ArrayAccess, Deref) around their recursion into the object;
  the leaf `Var` arm skips its check when set, and the enclosing
  arm runs `check_borrow_rules_place` on the full place.
  - Three new fixtures under `tests/adr_examples/0044_places/`:
    `pass_disjoint_field_read.gol`,
    `fail_read_same_field.gol`,
    `pass_disjoint_element_read.gol`.
- **Remaining.** Interpreter reference semantics (ADR 0053).
  The analyzer accepts and rejects the motivating programs
  correctly at compile time, but the interpreter's runtime does
  not model places — `&p.x` is a value snapshot. ADR 0053
  replaces this with a `RuntimeValue::Reference(Place)` that
  reads through the current storage. That ADR is the one that
  finally closes ADR 0044's "interpreter side" line; the
  analyzer-side work here is complete.

### Shipped shape vs. original sketch

The implementation converged on a simpler form than the
implementation sketch below proposed:

- **`Place` is `{ base: String, projections: Vec<Projection> }`**,
  not a recursive enum (`Var | Field | Index | Deref | Ref`).
  Vec-of-projections makes prefix checking for overlap a slice
  comparison rather than recursive tree-walking, and `Deref`/`Ref`
  drop out of the grammar because a dereference produces no
  projection — `*r` is `Place::simple("r")`.
- **`borrows` stays `HashMap<String, BorrowState>`**, keyed on
  the *borrower* (a synthetic id for shared borrows, the
  variable name for mutable borrows). `Place` lives inside
  `BorrowState`, not in the map key. Places are not unique
  identifiers — two `&x` from different scopes need distinct
  entries with the same place — so the key must remain the
  borrower.

The sketch below is preserved as the design intent. The shipped
form is what the code actually does.

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

5. **Test migration.** The soundness suite gains new positive
   cases (disjoint-field borrows, disjoint-index borrows) and
   keeps all existing negative cases.

   (Interprocedural summaries — carrying a `&mut place` argument
   into the callee's body — are **out of scope** for this ADR;
   see "What is deliberately out of scope" below.)

## What is deliberately out of scope

- **Borrow lifetimes.** This ADR is about *what* a borrow refers
  to. *When* a borrow is live is ADR 0043.

- **Region lifetimes.** ADR 0045.

- **Full alias analysis.** Overlap through pointers is a "may
  alias" problem in the general case. This ADR's proposal is a
  place-based approximation, not a full points-to analysis.

- **Interprocedural place resolution.** Carrying a `&mut place`
  argument into the callee's body (so that `modify(&mut p.y)`
  analyses `modify`'s body under the specific place `p.y` rather
  than as a symbolic parameter) requires a call-graph analysis
  and summary caching. The current analyzer is local — a call
  `f(&mut place)` is treated as a use of that place, and
  conflicts are checked against the caller's borrow state at the
  call site. That is sound and cheap; interprocedural refinement
  is deferred to a future ADR on function summaries.

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
- ADR 0053 — interpreter reference semantics (proposed; three
  sessions; needed before `g1_borrow_field` flips green on the
  interpreter)
- `docs/decisions/0005-ownership-model.md` — original borrow rules
- `src/semantics/analyzer/ownership.rs` — current variable-name
  tracking

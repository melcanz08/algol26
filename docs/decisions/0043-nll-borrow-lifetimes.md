# ADR 0043: Non-lexical borrow lifetimes

Status: Proposed (design)

## Context

Borrows in ALGOL26 end at the end of the enclosing lexical scope.
A `&x` taken in an inner block keeps `x` immutably borrowed until
the block exits, even if the reference is never used again:

    var x := 42
    val r := &x
    print(*r)
    x := 99          // E0007: cannot mutate `x` while `&x` is live
                     // — even though `r` is not used after this line

Rust removed this rule in 2018 by switching to **non-lexical
lifetimes** (NLL): a borrow ends at its last use, not at scope end.
The current analyzer is *sound* — it never accepts a program that
could observe a conflict — but it is *incomplete*: it rejects some
programs a more precise analysis would accept.

## Decision

Move borrow lifetime tracking from "ends at scope exit" to
"ends at last use", preserving soundness.

The rule becomes:

> A borrow is live from the point it is created to the point of
> its **last use** on every path from creation to scope exit. After
> that point the borrow is dead, and the place it references is no
> longer considered borrowed.

## Consequences

**Positive.**

- Valid programs that reassign a variable after the last use of a
  borrowed reference to it are accepted.
- Valid programs that take a second reference after the first is
  dead are accepted.
- The analyzer's behavior aligns with the mental model most users
  bring from Rust, C++, and modern systems languages.

**Negative.**

- The `SemanticState::borrows` map currently records a borrow
  until scope exit; every read of that map (mutability checks,
  region cleanup, the race analyzer) must be re-read against the
  new lifetime rules.
- Diagnostics that today read "borrow is live for the rest of the
  scope" must be rewritten to describe "borrow is live up to line
  N".
- `defer` captures complicate last-use: a reference captured in a
  `defer` block is used at scope exit regardless of textual
  position. The capture itself must count as a use.

## Implementation sketch

1. **Add a use-site index.** Track every `BorrowShared` /
   `BorrowMutable` along with the `ExprId`s of the expressions
   that dereference it. This requires an AST walk — a new pass in
   `src/semantics/analyzer/`.

2. **Compute live intervals per borrow.** A borrow is live over
   the CFG interval from its creation to its last use. In a
   branching program this becomes a dataflow problem — the same
   worklist pattern the IR verifier just adopted (see ADR 0014 and
   `0547963`).

3. **Replace `SemanticState::borrows`.** Instead of a set of live
   borrows, carry a map `BorrowId -> Interval`. A conflict is a
   borrow whose interval overlaps a mutation of the same place
   (see ADR 0044 for "place").

4. **Preserve the ownership-transfer analysis.** Moves and borrows
   interact; the current `ownership.rs` module computes both.
   NLL changes the borrow side only.

5. **Migrate tests.** `tests/soundness/borrowing/*.gol` and
   `tests/soundness/escape/*.gol` need to be re-run. Some
   currently-rejected programs may become accepted; that is the
   point.

## What is deliberately out of scope

- **Place-based aliasing.** This ADR refines *when* a borrow is
  live; it does not change *what* a borrow refers to. `&mut p.x`
  still conflicts with `&p.y` under the current variable-granular
  model. That is ADR 0044.

- **Region lifetimes.** This ADR changes borrow-to-variable
  lifetimes; region-to-allocation lifetimes remain lexical.
  Unifying the two is ADR 0045.

## Alternatives considered

### A. Keep lexical lifetimes and document the conservatism

The current behavior is sound. The cost is a class of valid
programs rejected for no good reason. This has been the standing
answer since the analyzer was first written, and it remains a
defensible one if NLL is deferred indefinitely.

**Rejected** as the long-term plan; may be the correct short-term
choice if ADR 0044 or 0045 is prioritized first.

### B. Full borrow-checker rewrite (Polonius-style)

Track the borrow graph as a set of origin constraints and solve
with Datalog-style rules. This is what Rust is migrating to
internally.

**Rejected** as a first step because it requires a different
analysis framework and is much larger than necessary for the
lifetime-length problem alone.

## See also

- ADR 0044 — place-based borrows
- ADR 0045 — region lifetime unification
- `docs/decisions/0005-ownership-model.md` — original borrow rules
- `src/semantics/analyzer/ownership.rs` — where the current
  lexical-lifetime checks live
- Rust RFC 2094 (non-lexical lifetimes) — the design this ADR
  borrows from

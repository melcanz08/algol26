# ADR 0043: Non-lexical borrow lifetimes

Status: Accepted

## Context

The analyzer tracks shared borrows in `SemanticState::borrows`, but
nothing consulted the map to reject a write to a borrowed place.
Scope exit (`pop_scope`) pruned the map, `move_out` pruned it when
the borrowed variable moved, and the mutable-borrow release sites
pruned it when a write went through a reference. But a write to the
variable itself was not checked against a live shared borrow of that
variable:

    var x := 1
    val r := &x
    x := 2          // accepted before this ADR — no check ran
    print(*r)       // reads a reference to a value that was written

This is a **soundness hole**, not a completeness gap. The program
above is accepted today, but the write at `x := 2` invalidates the
reference held in `r`; `print(*r)` then observes the mutated value
through a shared borrow that was live across the mutation.

The hole was not visible in tests because no fixture exercised it.
`tests/adr_examples/0043_nll/pass_borrow_ends_at_last_use.gol`
compiled (correctly), and
`tests/adr_examples/0043_nll/fail_mutate_while_borrow_live.gol`
also compiled (incorrectly) — the fixture's `// EXPECT: REJECT`
comment was aspirational. The runner reported `has_error=false`
for the reject-fixture, which is how the bug was found.

## Decision

Shared borrows are live from their creation expression through
their last use. A write to a place inside that interval produces
`E0007`.

The rule becomes:

> A shared borrow is live from the point it is created to the
> point of its **last use**. A write to any place the borrow
> overlaps, at a position strictly after creation and at or
> before the last use, is a conflict.

Reads are unaffected: a shared borrow permits concurrent reads of
its referent. Only writes are rejected. Mutable borrows keep their
existing rule — exclusive from creation until released, either by
a write through the reference (ADR 0056) or by scope exit.

## Consequences

**Positive.**

- The shared-borrow soundness hole is closed:
  `val r := &x; x := 2; print(*r)` now rejects at `x := 2`.
- Valid programs that write after the last use of a borrow are
  still accepted: `val r := &x; print(*r); x := 2` compiles.
- The analyzer's behavior aligns with the mental model most users
  bring from Rust, C++, and modern systems languages.

**Negative.**

- The analysis is syntactic, not CFG-based. Uses are recorded by
  walking the AST; liveness is "max `ExprId` of any use." Two
  conservative approximations result:
  - A use inside a loop body promotes the borrow's last use to
    the loop's exit, because iteration N+1 re-enters the body
    after iteration N's write. This over-rejects some
    loop-with-write-after-use programs that a per-iteration
    analysis would accept.
  - A use inside a `defer` body will promote to the deferring
    scope's exit, once the `defer_depth` guard lands. (See
    "Known limitations" below — this part is not yet
    implemented.)
- Diagnostics that today read "borrow is live for the rest of the
  scope" (from `E0007` sites that predate this ADR) must be
  reworded if they turn out to be reachable alongside the new
  check. No such diagnostic has been observed in the test suite.

## Implementation

- `SemanticState::BorrowState` carries `created_expr: ExprId`,
  the AST identity of the `&x` expression. `created_at` (a
  `NodeId`) is used for synthetic key generation and is not
  comparable with AST positions.
- `SemanticAnalyzer` holds three side tables that survive
  `pop_scope`'s prune of `state.borrows`:
  - `shared_borrow_meta: HashMap<String, (Place, ExprId)>` —
    key to (place, creation site).
  - `shared_borrow_uses: HashMap<String, Vec<ExprId>>` — key to
    use positions.
  - `binding_to_shared_borrow: HashMap<String, String>` —
    `val r := &x` binds `"r"` to the borrow's synthetic key.
- Uses are recorded by `record_borrow_use` from three sites in
  the expression walk:
  - `ExprKind::Deref { expr: Var(name) }` — `*r` uses `r`.
  - `ExprKind::Var(name)` — a bare pass `f(r)` uses `r`.
  - `ExprKind::FunctionCall { name: "r.method" }` — a dotted
    call's receiver is folded into the name string and is not
    walked as a child `Expr`; the use is recorded explicitly.
- Writes are deferred: `defer_shared_borrow_check(place, at)`
  pushes onto a per-function queue, and `flush_shared_borrow_checks`
  runs at the end of `analyze_function`. The deferral is required
  because the top-down walk has not yet seen uses later in the
  source when it reaches a write site.
- `LAST_USE_SENTINEL = ExprId(u32::MAX - 1)` marks a borrow whose
  last use is the enclosing scope's exit. Used for unused borrows
  (safe default) and for uses inside loops.

## Known limitations

- **No per-path liveness.** A borrow used in one `if` branch and
  a conflicting write in the other branch reject, even if the
  two paths are mutually exclusive. Refining this requires a
  per-function CFG and a dataflow pass; the same infrastructure
  ADR 0014's verifier worklist anticipates.

- **Place-based alias precision is ADR 0044.** This ADR refines
  *when* a borrow is live; it does not change *what* a borrow
  refers to. `&p.x` still conflicts with `&p.y` at the current
  variable-granularity of the conflict check for the shared-borrow
  read path. The write-side checks are place-aware (added in the
  ADR 0044 stage-2 work); the read side is not, yet.

## See also

- ADR 0044 — place-based borrows
- ADR 0045 — region lifetime unification
- ADR 0056 — write-through a mutable reference
- `docs/decisions/0005-ownership-model.md` — original borrow rules
- `src/semantics/analyzer/ownership.rs` — `is_live_shared_borrow_of`,
  `record_borrow_use`, `defer_shared_borrow_check`,
  `flush_shared_borrow_checks`
- Rust RFC 2094 (non-lexical lifetimes) — the design this ADR
  borrows from
- Commit `6819517` — the implementation

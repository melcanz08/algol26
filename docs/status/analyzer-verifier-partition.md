# Analyzer / verifier partition

> Who owns which safety rule, and why the overlap is deliberate.
> Companion to [safety-guarantees.md](safety-guarantees.md) (what
> the language claims) and ADR 0014 (verifier invariants).

## Why a partition is needed

Ownership, borrow, race, and type reasoning all appear in two
places: the AST analyzer (`src/semantics/analyzer/`) and the IR
verifier (`src/ir/verifier/`). Without a written rule for who
owns what, a safety check can live in both places with subtly
different logic, or in neither place because each stage assumes
the other enforces it. Both are real bugs waiting to happen.

This document states the partition, names the rules that fall
into each category, and explains why the overlap exists.

## The principle

**The analyzer owns language semantics. The verifier owns IR
consistency.**

- The **analyzer** decides whether a *program* is well-formed.
  It runs on the AST, sees source constructs, and produces the
  diagnostics a user reads. It is the authoritative source for
  every rule about what ALGOL26 accepts.
- The **verifier** decides whether the analyzer produced sound
  IR. It runs on the semantic IR, sees only the lowered form,
  and produces internal-error diagnostics. It is the
  authoritative source for every rule about IR well-formedness.

When the two disagree, they are not "both right from different
angles" — one of them has a bug. If the verifier rejects IR the
analyzer accepted, either the analyzer produced malformed IR
(analyzer bug) or the verifier's rule is wrong (verifier bug).
The disagreement itself is the signal.

## Category 1 — Analyzer-only rules

These rules are enforced by the analyzer and nowhere else. Two
reasons a rule lives here:

### 1a. The rule is about source constructs that do not survive to IR

Name resolution, trait declarations, visibility, field
existence, enum variants, subrange declarations, FFI extern
bindings. These are resolved during analysis; the IR carries the
resolved forms. There is nothing left for the verifier to check.

*Examples*: "Variable 'x' already declared", "Unknown record
'Point'", "Impl of trait T for U does not define constant C",
"`x::y` is private to module m".

### 1b. The rule requires data that the IR does not carry

Borrow lifetime conflicts, ownership move tracking, region
boundary rules. The analyzer tracks these in `SemanticState`
(borrow sets, move states, region stack). None of that state is
materialized in the IR — the IR has the lowered values, not the
analysis state that produced them. Re-checking these in the
verifier would require carrying the state through the pipeline,
which is a cost with no compensating benefit.

*Examples*: "Cannot mutably borrow 'x' while immutably borrowed",
"Cannot use 'x' after it was captured by defer", "Cannot `break`
across a region boundary".

## Category 2 — Verifier-only rules

These rules are enforced by the verifier and not by the analyzer,
because they are about IR shape, not program semantics. The
analyzer has no reason to check them — a well-formed program
naturally produces well-formed IR at this level.

*Examples*:

- "No `TypeVar` / `Generic` / `Associated` in executable IR"
  (invariants.rs) — the analyzer eliminates these before IR; the
  verifier confirms.
- "Function 'f' has block N with no terminator" — a CFG
  structural property, invisible at the source level.
- "Switch value cannot be Void" — the analyzer never produces
  a `Void` switch, but the IR builder could.
- "BoundsCheck has low > high" — the analyzer only constructs
  valid bounds checks; the verifier confirms.
- "VirtualCall has empty method_name" — an IR-builder
  invariant.

## Category 3 — Overlap rules (deliberate redundancy)

Some rules are enforced by both stages. These are the rules the
language's safety guarantees directly depend on, where a silent
failure would be dangerous. The analyzer produces the
user-facing diagnostic; the verifier is the backstop for
analyzer and IR-builder bugs.

The overlap is a policy, not an accident. For each rule below,
both stages enforce it *by design*:

| Rule | Analyzer site | Verifier site |
|---|---|---|
| Declare type matches value | `stmt.rs` | `instruction.rs`, `value.rs` |
| Assign type matches declared type | `stmt.rs` | `instruction.rs` |
| Cannot assign to immutable variable | `stmt.rs` | `instruction.rs` |
| Array index must be Int | `stmt.rs` | `instruction.rs`, `value.rs` |
| Array assignment element type | `stmt.rs` | `instruction.rs` |
| Field assignment on record only | `stmt.rs` | `instruction.rs` |
| Call arity | `expr.rs` | `instruction.rs`, `value.rs` |
| Call argument type | `expr.rs` | `instruction.rs`, `value.rs` |
| Function return type | `stmt.rs` | `terminator.rs` |
| Branch condition must be Bool | `expr.rs`, `stmt.rs` | `terminator.rs` |
| Cast legality | `expr.rs` | `value.rs` |
| Binary op operand types | `expr.rs` | `value.rs` |
| Borrow value claims match operand | `expr.rs` | `value.rs` |

### Why these rules

The criterion for double-checking is: **does the language's
safety guarantee depend on this rule, and would a silent failure
produce wrong code?**

- "If condition must be Bool" — G2 (type safety). A non-Bool
  branch condition is malformed control flow.
- "Call arity / argument type" — G2. A mis-typed call to a
  user function or builtin silently changes program meaning.
- "Return type mismatch" — G2. A return whose IR type disagrees
  with the declared signature is a miscompile waiting to happen.
- "Borrow value claims match" — G1 (memory safety). A
  `BorrowShared` whose operand type doesn't match its claim
  means the analyzer and IR builder disagree about what is being
  borrowed.

Rules that are *not* double-checked either (a) require state the
IR doesn't carry (Category 1b), (b) are about source-level
constructs with no IR form (Category 1a), or (c) are IR-only
invariants (Category 2).

## The asymmetry to note

The verifier is deliberately **more permissive on `Type::Unknown`
than the analyzer**. The analyzer uses `Unknown` as "no opinion"
inside composites (`Result<Int, Unknown>`, `List<Unknown>`);
those are valid programs. The verifier treats `Unknown` as a
wildcard in compatibility checks (`types_compatible_for_call`),
accepting any concrete type against it.

This is not a divergence — it is the verifier honoring the
analyzer's contract. The verifier's job is to enforce *what the
analyzer promises to produce*, not to be stricter than the
analyzer. See commit `29f9cca` and the module doc in
`src/ir/verifier/invariants.rs` for the reasoning.

## What the verifier does not verify

Two categories to be aware of:

1. **Borrow lifetime and ownership state.** The verifier sees
   the lowered `BorrowShared` / `BorrowMutable` values but not
   the borrow sets, move states, or region stacks that gave
   rise to them. A borrow-conflict bug that the analyzer misses
   is not caught here. See Category 1b.

2. **Data-flow at CFG joins.** The verifier's worklist pass
   (`verify_function` in `src/ir/verifier/mod.rs`) joins
   predecessor environments at merge points, but only for the
   facts it tracks — variable types, mutability, iterator
   element types. Broader dataflow (initialization, aliasing,
   race analysis) is not in scope. See ADR 0044 and ADR 0047 for
   what the analyzer covers.

## Where to add a new rule

When adding a new safety rule, the question is: **where is the
authoritative check?**

- If the rule is about what the *programmer wrote* — resolve it
  in the analyzer, produce a user diagnostic. Do not add a
  verifier check unless the rule is in the "both" table above.
- If the rule is about *IR well-formedness* — put it in the
  verifier. Do not add an analyzer check unless the rule is
  about a source construct the user can produce.
- If the rule is safety-critical and a silent failure would
  produce wrong code — put it in both, with the analyzer
  producing the diagnostic and the verifier catching producer
  bugs.

When in doubt, put the authoritative check where the data lives.
The analyzer has the state; the verifier has the IR. The rule
belongs where its inputs are.

## See also

- `docs/status/safety-guarantees.md` — the claims this partition
  serves
- ADR 0014 — verifier invariants
- ADR 0017 — the `VerifiedIR` typestate
- `src/semantics/analyzer/` — Category 1 rules
- `src/ir/verifier/` — Categories 2 and 3
- Commit `29f9cca` — the `Unknown`-permissiveness decision
- Commit `0547963` — the worklist verifier that joins
  predecessor environments at merge points

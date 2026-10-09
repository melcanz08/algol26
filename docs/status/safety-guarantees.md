# Safety guarantees

> What "safe" means for ALGOL26: the claims, what enforces each,
> and where they end. Companion to [STATUS.md](../STATUS.md) (what
> works) and [architecture-direction.md](../architecture-direction.md)
> (why the codebase is shaped the way it is).

## The guarantees

ALGOL26 makes four safety claims. Each is scoped to programs the
compiler accepts that do not use `unsafe` and do not cross an FFI
boundary.

### G1 — Memory safety

A well-typed ALGOL26 program does not read or write outside an
allocation, use a pointer after free, double-free, dereference
null, or use a reference whose referent has been dropped.

*Enforcement*: analyzer (borrow checker, ownership tracker,
region lifetimes), verifier (typed operands, control flow),
interpreter (runtime bounds checks on list access).

*Known gaps*: reference escape through composite values (B2, ADR
0046); reference escape across a region exit (B5, ADR 0045);
LLVM list bounds are not checked at runtime (see
`docs/features/list_llvm.md`).

### G2 — Type safety

A well-typed ALGOL26 program does not get stuck. If the analyzer
accepts a program, every expression produces a value of its
declared type, and execution reaches an explicit `return` or
falls off the end of a `Void` function.

*Enforcement*: analyzer (inference, coercion, traits), verifier
(operand type consistency, signature matching, branch condition
typing), capability check (refuses features a backend cannot
lower).

*Known gap*: the analyzer/verifier boundary is not documented
(E3 in the safety-theory review, follow-up planned).

### G3 — Ownership and borrow safety

A well-typed ALGOL26 program does not use a variable after
moving out of it, does not mutate through a `&`, and never has
two conflicting live borrows of the same variable.

*Enforcement*: analyzer (ownership tracking, borrow lifetimes,
scope exit).

*Known gaps*: borrow lifetimes are lexical, not NLL (B1, ADR
0043); conflicts are tracked by variable name, not memory place
(B3, ADR 0044).

### G4 — Data-race freedom

A well-typed ALGOL26 program that uses `spawn`, `parallel`, or
`channel` and does not use `unsafe` has no data races, per the
model in ADR 0047. Programs with data races have undefined
behavior.

*Enforcement*: analyzer (race detector); capability check
(refuses `spawn` on LLVM/WASM today).

*Known gaps*: detector is name-based and per-function (C4, C5);
the interpreter runs `parallel` sequentially, which is a valid
implementation of the model but not real concurrency.

## What enforces each guarantee

| Guarantee | Analyzer | Verifier | Capability | Runtime |
|---|---|---|---|---|
| G1 memory safety | yes | yes | — | bounds check on list access |
| G2 type safety | yes | yes | refuses unsupported features | — |
| G3 ownership | yes | — | — | — |
| G4 data-race freedom | yes (conservative) | — | refuses `spawn` on LLVM/WASM | — |

The verifier runs after the analyzer and before any backend. Its
job is to catch analyzer and IR-builder bugs, not to re-check
user-facing semantics (ADR 0014). The capability check is
orthogonal — it refuses features a backend cannot lower, which
is a coverage question, not a safety one.

## Where the guarantees end

The four claims hold only for programs that:

- avoid `unsafe` blocks (operations inside are outside the
  guarantee — ADR 0015, `docs/features/unsafe.md`)
- do not cross an FFI boundary with mismatched signatures (the C
  ABI has a different memory model; no runtime type information
  flows in either direction — see `docs/status/llvm-support.md`
  §D8)
- use only features the target backend supports (see
  `docs/status/llvm-support.md`)

### Backend opacity

LLVM's opaque-pointer mode collapses `&T`, `*T`, and `&mut T`
into a single `ptr` type. The borrow distinction lives entirely
in the analyzer and verifier; the backend cannot enforce it at
codegen time. Safety depends on the earlier stages having run —
this is by design (D10 in the review) and means the analyzer and
verifier are load-bearing.

## What is not proven

There is no type-soundness proof for ALGOL26. No progress or
preservation theorem, no mechanized proof, no paper proof. The
guarantees above are empirically validated by the corpus,
conformance, differential, and soundness test suites; they are
not proven.

Closing this gap is a research-scale project: formal operational
semantics, a formal type system matching the analyzer's rules, a
progress/preservation theorem, and either a mechanization in
Coq/Lean or a paper proof. That work is deliberately out of
scope. Users who need a formal guarantee should treat ALGOL26 as
a proof of concept, not a certified compiler.

## What "VerifiedIR" means

The IR typestate `VerifiedIR` means *the IR verifier's checks ran
and passed*. It does **not** mean *the program satisfies the
language's safety properties*.

The verifier is a second check: its job is to catch bugs in the
analyzer and IR builder, not to establish the language's
guarantees from first principles. Those are established by the
analyzer, the verifier, and the capability check together, each
doing a different job. Holding a `VerifiedIR` value means one
stage's checks succeeded; it does not mean the program is "safe"
in the language sense.

See ADR 0014 for the verifier's invariants and
`docs/decisions/0017-verified-ir-typestate.md` for the typestate
design.

## See also

- `docs/vision.md` §Safety Philosophy — the design intent
- `docs/STATUS.md` — what works today, corpus-verified
- `docs/no-panic-policy.md` — a separate guarantee about user
  source and Rust panics
- ADR 0005, 0007, 0014, 0015, 0017 — current safety-relevant
  decisions
- ADR 0043, 0044, 0045, 0046, 0047 — planned work
- `docs/status/llvm-support.md` — per-feature backend coverage

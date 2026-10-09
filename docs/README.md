# ALGOL26 Documentation

This directory contains documentation for the ALGOL26 compiler.

## Read these first

- **[../README.md](../README.md)** — project overview, build, usage
- **[STATUS.md](STATUS.md)** — what works today,
  corpus-verified, with known gaps listed
- **[architecture-direction.md](architecture-direction.md)** — how the
  codebase is structured and why, plus the incremental path to less coupling
- **[language-reference.md](language-reference.md)** — canonical language
  specification (being audited against the parser + corpus section by section)

## Document categories

Every `.md` file in `docs/` belongs to exactly one category. The
category determines how the file is maintained.

### Reference — audited against the code

Describes current behavior. Must be accurate. Updated whenever the
code changes.

| File | Purpose |
|------|---------|
| `STATUS.md` | Feature matrix + known gaps, corpus-verified |
| `language-reference.md` | Canonical language specification |
| `architecture-direction.md` | Structure of the codebase and why |
| `ir-transformations.md` | What each IR transformation does |
| `test-organization.md` | What lives in each `tests/` subdirectory |
| `no-panic-policy.md` | Rules about panics, unwraps, and errors |
| `status/safety-guarantees.md` | The language’s safety claims, what enforces each, where they end |
| `status/analyzer-verifier-partition.md` | Which safety rules the analyzer owns vs. the verifier, and why the overlap is deliberate |

Reference docs that drift are worse than no docs. If a Reference doc
makes a claim that is not backed by code or a corpus program, remove
the claim or back it.

### Feature contracts

One file per language feature, under `features/`. Each describes the
feature's syntax, typing, ownership, IR representation, backend
support, and test coverage. They are the checklist for adding or
modifying a feature — see `architecture-direction.md` for the rationale.

There are currently 21 contracts; see the directory for the full list.

Contract files are updated alongside the code they describe. A
feature change that does not update its contract is incomplete.

### Subsystem investigations

**Live, in-progress.** Focused notes about a single subsystem,
narrower than `STATUS.md`. When an investigation concludes, its
outcome is folded into `STATUS.md` (and the corresponding
`features/<name>.md` or `decisions/NNNN-*.md` updated if the
decision changed), and the file here is either updated or archived.

| File | Subsystem | State |
|------|-----------|-------|
| `status/diagnostics.md` | Diagnostics & spans | Investigation (Phase 0) |
| `status/ffi-boundary.md` | FFI static boundary | Gap documented, fix pending |

### ADR — Architecture Decision Records

**Frozen.** Never updated. Each captures reasoning at a point in time.
If a decision changes, write a new ADR that supersedes it; do not edit
the old one.

See [`decisions/README.md`](decisions/README.md) for the full index.
The range is 0001–0032, with a few gaps where a planned decision was
folded into another.

### Release notes

**Frozen.** Historical record of each release.

| File | Release |
|------|---------|
| `releases/5.5-5.6.md` | 5.5–5.6 |
| `releases/5.6-backend.md` | 5.6 backend work |
| `releases/5.7.md` | 5.7 |
| `releases/v0.8.0.md` | 0.8.0 |

### Archive — superseded

**Frozen.** Kept for history. Do not read these as descriptions of the
current code. They were accurate at some point in the past. Every
file under `archive/` carries a banner naming its replacement.

Files in `archive/` include:

- Old status reports (`current-status.md`, `testing-report.md`)
- Old specifications (`formal-specification-v0.1.md`,
  `language-specification.md`, `language-reference-v0.1.md`)
- Old roadmaps (`safety-roadmap.md`, `frontend-hardening-backlog.md`)
- Old architecture writeups (`architecture-inventory.md`,
  `architecture-maturity.md`)
- Old design docs that have since been superseded

## Rules

1. **If a doc makes a factual claim about code, it is Reference and must
   be audited against the code.** Reference docs that drift are worse
   than no docs.
2. **If a doc explains reasoning at a point in time, it is an ADR or
   Release note.** Freeze it. Never edit it.
3. **When a doc is superseded, move it to `archive/` — never delete.**
   Historical docs explain why past decisions were made.
4. **When a feature ships, update three things:** its feature contract
   under `features/`, a corpus program in `tests/corpus/`, and the
   row for it in `STATUS.md`. The contract without a
   corpus program is a claim; the corpus program without an updated
   contract is an undiscoverable feature.

## Adding a new doc

Before writing a new doc, ask:

- Is this describing current behavior? → Reference. It joins the audit
  rotation.
- Is this describing a single feature? → `features/<name>.md`, using
  `features/option.md` as the template.
- Is this recording a decision? → ADR. Number it `NNNN-title.md`.
- Is this release notes? → `releases/`.
- Is it explaining why, not what? → Design. Keep it short and timeless.
- Is it a rewrite of something already in `archive/`? → Reference,
  and note in the new doc which archived file it supersedes.

## Docs that are needed but not yet written

| Doc | Why it matters |
|-----|---------------|
| `features/*.md` (14 more) | Every feature besides `Option` and `Result` still needs a contract. `list.md` and `channel.md` are the next candidates. |
| `stdlib.md` | Built-in functions (`String.*`, `Math.*`, `List.*`) have no reference doc. Currently discoverable only by reading `builtin_signatures()` in `src/ir/verifier/builtins.rs`. |
| `errors.md` | The nine current diagnostic codes (`E-BORROW-004` through `E-UNSUPPORTED-001`) have no public documentation. Users only see them in compiler output. |

Each of these is a small, self-contained writing project. The
`features/option.md` file is the model for `features/*.md`.


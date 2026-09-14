# ALGOL26 Documentation

This directory contains documentation for the ALGOL26 compiler.

## Read these first

- **[../README.md](../README.md)** — project overview, build, usage
- **[IMPLEMENTATION_STATUS.md](IMPLEMENTATION_STATUS.md)** — what works today,
  corpus-verified, with known gaps listed
- **[archive/](archive/)** — superseded docs, kept for history

## Document categories

Every `.md` file in `docs/` belongs to exactly one category. The
category determines how the file is maintained.

### Reference — audited against the code

Describes current behavior. Must be accurate. Updated whenever the
code changes.

| File | Purpose |
|------|---------|
| `IMPLEMENTATION_STATUS.md` | Feature matrix + known gaps, corpus-verified |

Only one doc is currently in this category. As the language reference
and architecture docs are rewritten from code, they will join it.

### ADR — Architecture Decision Records

**Frozen.** Never updated. Each captures reasoning at a point in time.
If a decision changes, write a new ADR that supersedes it; do not edit
the old one.

| File | Decision |
|------|----------|
| `decisions/0001-significant-indentation.md` | Indentation as syntax |
| `decisions/0002-file-extension.md` | `.gol` file extension |
| `decisions/0003-type-system.md` | Static types with inference |
| `decisions/0004-memory-model.md` | No garbage collector |
| `decisions/0005-ownership-model.md` | Move and borrow semantics |
| `decisions/0006-immutability.md` | `val` by default |
| `decisions/0007-region-memory.md` | Region-based allocation |
| `decisions/0008-concurrency-model.md` | `spawn` and `parallel` |
| `decisions/0009-unsafe.md` | Unsafe blocks and FFI |

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
current code. They were accurate at some point in the past.

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
4. **When a feature ships, update two things:** `IMPLEMENTATION_STATUS.md`
   and a corpus program in `tests/corpus/`. The status doc without a
   corpus program is just a claim; the corpus program without an update
   to the status doc is an undiscoverable feature.

## Adding a new doc

Before writing a new doc, ask:

- Is this describing current behavior? → Reference. It joins the audit
  rotation.
- Is this recording a decision? → ADR. Number it `NNNN-title.md`.
- Is this release notes? → `releases/`.
- Is it explaining why, not what? → Design. Keep it short and timeless.
- Is it a rewrite of something already in `archive/`? → Reference,
  and note in the new doc which archived file it supersedes.

## Docs that are needed but not yet written

| Doc | Why it matters |
|-----|---------------|
| `language-reference.md` | The actual spec. Currently in `archive/` but
  stale. Must be rewritten from the parser + corpus. |
| `architecture.md` | High-level overview. Currently in `archive/` but
  references deleted paths. Must be rewritten from `src/`. |

Both are large projects. They will be written section by section
against the code, and every claim will be backed by either a parser
rule or a corpus program.
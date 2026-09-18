# Archived Documentation

This directory contains superseded ALGOL26 documents, kept for
historical context. **They do not describe the current code.**

Every file here carries a `Superseded` banner at the top naming
its replacement. If you are looking for current information, follow
that banner; do not read the file itself as authoritative.

## Where current docs live

| Category | Location |
|----------|----------|
| Language specification | [`../language-reference.md`](../language-reference.md) |
| Architecture and direction | [`../architecture-direction.md`](../architecture-direction.md) |
| Implementation status | [`../IMPLEMENTATION_STATUS.md`](../IMPLEMENTATION_STATUS.md) |
| Feature contracts | [`../features/`](../features/) |
| Architecture decisions (ADRs) | [`../decisions/`](../decisions/) |
| Release notes | [`../releases/`](../releases/) |
| Compiler pass contracts | [`../ir-pass-contracts.md`](../ir-pass-contracts.md) |
| Test layout | [`../test-organization.md`](../test-organization.md) |
| Panic policy | [`../no-panic-policy.md`](../no-panic-policy.md) |

## Why keep archived docs at all

Three reasons:

1. **Rationale.** Old roadmaps and status reports explain *why* past
   decisions were made. When a design looks arbitrary in the current
   code, the archive often shows the reasoning that produced it.
2. **Historical record.** Release-adjacent notes, when written, are
   the only record of what shipped when.
3. **Rewrite source.** When rewriting a canonical doc from scratch,
   the archived version is often the closest starting point — even
   if most of it is out of date, the sections that were accurate are
   usually worth reading.

## What was archived and when

The banner on each file gives the exact replacement target. Grouped
by family:

- **Spec family** → `../language-reference.md`
  (`language-specification.md`, `language-reference-v0.1.md`,
  `formal-specification-v0.1.md`, `language-freeze.md`,
  `contextual-typing-status.md`, `type-table-addressing.md`)
- **Architecture family** → `../architecture-direction.md`
  (`architecture.md`, `architecture-inventory.md`,
  `architecture-maturity.md`, `design-principles.md`)
- **Status / roadmap family** → `../IMPLEMENTATION_STATUS.md`
  (`current-status.md`, `semantics-hardening-complete.md`,
  `semantics-hardening-progress.md`, `frontend-hardening-backlog.md`,
  `semantic-verifier-backlog.md`, `safety-roadmap.md`,
  `backend-refactoring-roadmap.md`, `list-ops-status.md`,
  `ffi-isolation-status.md`, `testing-report.md`)
- **Meta** → `../README.md`
  (`documentation-hierarchy.md`)

## Files without a banner

These archive files do **not** carry a Superseded banner because
they are still current or are intentionally historical:

- `historical-lineage.md` — describes ALGOL 58/60 lineage; not
  superseded by anything
- `memory-model.md` — the memory model reference; still current
  unless and until it is folded into `../language-reference.md`
- `list-printing.md` — behavior of list printing; still current
- `versioning.md` — versioning policy; still current
- `vision.md` — mission statement; still current
- `wasm-differential-testing.md` — the WASM test plan; still
  current, referenced from `../architecture-direction.md`
- `algol26-contract.md` — the original language contract; kept
  as a historical anchor

## Rules

1. **Do not edit archived files.** They are frozen snapshots. If
   content needs updating, update the current doc and note the
   change there; the archive is not maintained.
2. **Do not move files out of `archive/` unless the file is genuinely
   current.** Four files were moved out on 2026-09-18
   (`language-reference.md`, `ir-pass-contracts.md`,
   `test-organization.md`, `no-panic-policy.md`) because they are
   referenced from `../architecture-direction.md` and describe
   current behavior.
3. **When a current doc is superseded, move it here — never delete.**
   Prepend the same Superseded banner, naming the replacement.


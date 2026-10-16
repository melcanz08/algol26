# Architecture Decision Records

This directory contains ALGOL26's Architecture Decision Records (ADRs).

## Convention

ADRs are **frozen**. Each records reasoning at a point in time. When
a decision changes, the original is preserved and a **status note** is
added right after the title pointing to the current state. A new ADR
supersedes an old one when the decision itself changed; a status note
suffices when the decision is unchanged but the implementation drifted.

Do not edit an ADR's body to reflect new decisions. Either:

- Add a status note (implementation drift, no decision change), or
- Write a new ADR that supersedes it (decision change).

## Status vocabulary

- **Current** — decision holds, implementation matches the ADR.
- **Drifted** — decision holds, implementation has moved; the ADR's
  body has a status note describing the drift.
- **Accepted** — the decision is settled but the implementation is
  partial or in progress.
- **Proposed** — decision not yet finalized.
- **Superseded** — a later ADR replaces this one; the replacement is
  named in the status column.
- **Deferred** — decision made (often 'do not implement yet'), but
  no implementation is planned until a stated precondition holds.
- **Not started** — accepted as the direction, no code yet.

## Index

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-significant-indentation.md) | Significant Indentation | Current |
| [0002](0002-file-extension.md) | File Extension `.gol` | Current |
| [0003](0003-type-system.md) | Type System | Drifted (see status note) |
| [0004](0004-memory-model.md) | Memory Model | Drifted (see status note) |
| [0005](0005-ownership-model.md) | Ownership Model | Drifted (see status note) |
| [0006](0006-immutability.md) | Immutability | Current |
| [0007](0007-region-memory.md) | Region Memory | Drifted (see status note) |
| [0008](0008-concurrency-model.md) | Concurrency Model | Drifted (see status note) |
| [0009](0009-unsafe.md) | Unsafe Boundary | Drifted (see status note) |
| [0010](0010-canonical-ir.md) | Canonical IR | Accepted |
| [0011](0011-phase4-task-model.md) | Phase 4 Task Model | Proposed |
| [0012](0012-implicit-deref-convention.md) | Implicit Deref Convention | Proposed |
| [0013](0013-executable-ir-generic-invariant.md) | Executable IR Generic Invariant | Current |
| [0014](0014-verifier-invariants.md) | Verifier Invariants | Current |
| [0015](0015-unsafe-enforcement.md) | Unsafe Enforcement | Current |
| [0016](0016-per-function-cfg.md) | Per-Function CFG | Current |
| [0017](0017-verified-ir-typestate.md) | Verified IR Typestate | Current |
| [0018](0018-canonical-pipeline.md) | Canonical Pipeline | Current |
| [0022](0022-affirm.md) | `affirm` | Current |
| [0023](0023-args.md) | `args()` | Current |
| [0024](0024-record.md) | Records | Current |
| [0025](0025-trait-bounds-status.md) | Trait Bounds | Accepted |
| [0026](0026-structural-copy.md) | Structural `Copy` | Current |
| [0027](0027-map.md) | `Map<K, V>` | Current |
| [0028](0028-list-append.md) | `List.append` | Current |
| [0029](0029-nominal-types.md) | Nominal Types | Current |
| [0030](0030-enum-types.md) | Enum Types | Current |
| [0031](0031-subrange-types.md) | Subrange Types | Current |
| [0032](0032-set-types.md) | Set Types | Current |
| [0033](0033-methods-and-self.md) | Methods, Receivers, and the OOP Direction | Accepted |
| [0034](0034-generic-impls.md) | Generic Impls | Current |
| [0035](0035-interpreter-aliasing.md) | Interpreter Write-Through for `&mut self` Receivers | Current |
| [0036](0036-llvm-records.md) | LLVM Record Support | Current |
| [0037](0037-wasm-records-references.md) | WASM Records and References | Superseded by 0036 |
| [0038](0038-dynamic-dispatch.md) | Dynamic Dispatch (`dyn Trait`) | Current |
| [0039](0039-visibility.md) | Visibility Modifiers | Current |
| [0040](0040-function-value-surface.md) | Function-Value Surface | Accepted (three subjects are deferrals) |
| [0041](0041-associated-types.md) | Associated Types | Current |
| [0042](0042-llvm-dynamic-lists.md) | LLVM Dynamic Lists | Accepted |
| [0043](0043-nll-borrow-lifetimes.md) | Non-Lexical Lifetimes | Accepted |
| [0044](0044-place-based-borrows.md) | Place-Based Borrows | Current (analyzer); interpreter side via 0053 |
| [0045](0045-region-lifetime-unification.md) | Region Lifetime Unification | Current |
| [0046](0046-reference-containment-escape.md) | Reference Containment / Escape | Reject-only |
| [0047](0047-concurrency-memory-model.md) | Concurrency Memory Model | Deferred (no concurrent runtime) |
| [0048](0048-procedure-function-split.md) | Procedure / Function Split | Current |
| [0049](0049-list-parameter-semantics.md) | List Parameter Semantics | Current |
| [0050](0050-region-cleanup-heap-lists.md) | Region Cleanup for Heap Lists | Current |
| [0051](0051-scope-exit-list-cleanup.md) | Scope-Exit List Cleanup | Current |
| [0052](0052-llvm-codegen-architecture.md) | LLVM Codegen Architecture | Current |
| [0053](0053-interpreter-reference-semantics.md) | Interpreter Reference Semantics | Current (interpreter); cross-fn write-through deferred |
| [0054](0054-alternate-brace-block-syntax.md) | Alternate Brace Block Syntax | Current (formatter deferred) |
| [0055](0055-sum-and-recursive-types.md) | Sum Types and Recursive Types | Proposed |
| [0056](0056-reference-writes.md) | Reference Writes (`*r := v`) | Current (Form A + cross-fn deferred) |
| [0057](0057-statement-boundaries.md) | Statement Boundaries | Current |
| [0058](0058-algol26-fmt.md) | `algol26 fmt` | Proposed |

## Numbering notes

- **0019–0021** are unused. No ADRs were written under those
  numbers; the sequence skips from 0018 to 0022.
- **0055** is Reserved. **0056** (reference writes) and
  **0057** (statement boundaries) are accepted and implemented.
  **0058** (`algol26 fmt`) is proposed; implementation is the
  next session-sized item. **0059+** are reserved for further
  work on ADR 0044's interpreter side and anything else.

## Current state

For what is actually implemented today, see
[`STATUS.md`](../STATUS.md) — the corpus-verified feature matrix.

## Naming

Files are named `NNNN-title.md`. The ADRs refer to themselves as
`DNNN` in their headings; both forms identify the same record.

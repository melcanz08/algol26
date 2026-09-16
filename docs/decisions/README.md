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

## Current state

For what is actually implemented today, see
[`IMPLEMENTATION_STATUS.md`](../IMPLEMENTATION_STATUS.md) — the
corpus-verified feature matrix.

## Naming

Files are named `NNNN-title.md`. The ADRs refer to themselves as
`DNNN` in their headings; both forms identify the same record.

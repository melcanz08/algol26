#!/usr/bin/env python3
"""
Phase 7: sync ADRs to current implementation state.

For each drifted ADR, insert a status note right after the title.
The frozen body is untouched — the note points readers to the
current state. Also creates docs/decisions/README.md explaining the
convention.

Notes are only added to ADRs whose implementation has drifted since
the ADR was written. D001, D002, D006 are accurate and are left alone.
"""

import sys
from pathlib import Path

DATE = "2026-09-16"

NOTES = {
    "docs/decisions/0003-type-system.md": {
        "anchor": "# D003: Type System",
        "note": (
            "> **Status note (2026-09-16)**: All items in the \"Planned\"\n"
            "> list below (algebraic data types, Option, Result, generics,\n"
            "> pattern matching) are implemented. \"No implicit conversions\"\n"
            "> is not accurate \u2014 `Int` coerces to `Float` at call sites\n"
            "> and in arithmetic. See `IMPLEMENTATION_STATUS.md` for the\n"
            "> current feature matrix."
        ),
    },
    "docs/decisions/0004-memory-model.md": {
        "anchor": "# D004: Memory Model",
        "note": (
            "> **Status note (2026-09-16)**: Borrowing is implemented,\n"
            "> not \"future.\" The syntax shown below predates the current\n"
            "> parser: `region r` takes an indented body (no `do`/`end\n"
            "> region`), `alloc(n)` / `free(p)` are the memory builtins,\n"
            "> and move happens implicitly on assignment (there is no\n"
            "> `move(...)` call). See `IMPLEMENTATION_STATUS.md` for the\n"
            "> current memory model and known limitations."
        ),
    },
    "docs/decisions/0005-ownership-model.md": {
        "anchor": "# D005: Ownership Model",
        "note": (
            "> **Status note (2026-09-16)**: Borrowing is implemented,\n"
            "> not \"future.\" The syntax in the example matches the\n"
            "> current parser. Known limitation: `&mut x` passed as a\n"
            "> call argument is not registered as a borrow (see\n"
            "> `IMPLEMENTATION_STATUS.md`)."
        ),
    },
    "docs/decisions/0007-region-memory.md": {
        "anchor": "# D007: Region Memory",
        "note": (
            "> **Status note (2026-09-16)**: Regions are implemented\n"
            "> end-to-end. `RegionExit` auto-frees region-scoped\n"
            "> allocations on both the interpreter and LLVM backends\n"
            "> (tags `step3-done`, `step6-done`). The syntax shown below\n"
            "> predates the current parser: use `region r` + indented\n"
            "> body, and `alloc(n)` (not `allocate(n)`)."
        ),
    },
    "docs/decisions/0008-concurrency-model.md": {
        "anchor": "# D008: Concurrency Model",
        "note": (
            "> **Status note (2026-09-16)**: Data-race detection is\n"
            "> implemented (see `src/semantics/race/`), though it is\n"
            "> deliberately conservative and per-function only. `spawn`\n"
            "> and `parallel` execute through the interpreter and are\n"
            "> refused by the LLVM backend. Channels parse and analyze\n"
            "> but have no backend runtime. The syntax shown below\n"
            "> predates the current parser: `spawn` takes an indented\n"
            "> body (no `do`/`end`)."
        ),
    },
    "docs/decisions/0009-unsafe.md": {
        "anchor": "# D009: Unsafe Boundary",
        "note": (
            "> **Status note (2026-09-16)**: `unsafe` blocks and FFI are\n"
            "> both implemented. `extern \"C\"` supports `as \"symbol\"`\n"
            "> renaming and `from \"library\"` linking (tag\n"
            "> `step4b-done`). The syntax shown below predates the current\n"
            "> parser: `unsafe` takes an indented body (no `do`/`end\n"
            "> unsafe`)."
        ),
    },
}

DECISIONS_README = """\
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
[`IMPLEMENTATION_STATUS.md`](../IMPLEMENTATION_STATUS.md) \u2014 the
corpus-verified feature matrix.

## Naming

Files are named `NNNN-title.md`. The ADRs refer to themselves as
`DNNN` in their headings; both forms identify the same record.
"""


def insert_note(text, anchor, note):
    """
    Insert `note` right after the line containing `anchor`. The
    insertion is: anchor line + blank line + note. If a note is
    already present (idempotent re-run), return the original text.
    """
    idx = text.find(anchor)
    if idx == -1:
        return None
    # Find the end of the anchor line.
    line_end = text.find("\n", idx)
    if line_end == -1:
        return None
    # Check for a pre-existing status note immediately after.
    after = text[line_end + 1 : line_end + 100]
    if "Status note" in after:
        return text  # already inserted, idempotent
    replacement = text[: line_end + 1] + "\n" + note + "\n" + text[line_end + 1 :]
    return replacement


def main():
    repo = Path.cwd()
    if not (repo / "Cargo.toml").exists():
        print("ERROR: run from repo root", file=sys.stderr)
        return 1

    # Write the decisions README (idempotent — always overwrite).
    readme = repo / "docs" / "decisions" / "README.md"
    readme.write_text(DECISIONS_README)
    print("  wrote: docs/decisions/README.md")

    # Insert status notes into drifted ADRs.
    for rel, spec in NOTES.items():
        path = repo / rel
        if not path.exists():
            print(f"ERROR: {rel} not found", file=sys.stderr)
            return 1
        text = path.read_text()
        new_text = insert_note(text, spec["anchor"], spec["note"])
        if new_text is None:
            print(
                f"ERROR: {rel}: anchor not found ({spec['anchor']!r})",
                file=sys.stderr,
            )
            return 1
        if new_text == text:
            print(f"  skipped (already noted): {rel}")
        else:
            path.write_text(new_text)
            print(f"  added status note: {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
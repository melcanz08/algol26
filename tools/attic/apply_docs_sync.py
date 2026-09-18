#!/usr/bin/env python3
"""
Step 1 of the "sync everything" plan: make the documentation honest
about features that parse and analyze but are not yet wired into a
backend.

This script touches docs only. No code changes.

- README.md: add a clarifying note after "Language at a Glance".
- docs/IMPLEMENTATION_STATUS.md: add a "Deferred subsystems" section
  and a new "syntax only" legend tier.

Usage:
    python3 tools/apply_docs_sync.py --dry-run
    python3 tools/apply_docs_sync.py
"""

import argparse
import sys
from pathlib import Path

# ─── README edit ────────────────────────────────────────────────────────
README_ANCHOR = "- **`region`** blocks and `alloc` / `free` for manual memory"

README_REPLACEMENT = """\
- **`region`** blocks and `alloc` / `free` for manual memory

> **Note:** three features in the list above are **parse-and-analyze
> only** — the syntax parses, the analyzer checks it, but no backend
> currently executes it:
>
> - **`region` blocks** — `region r` introduces a lexical scope in the
>   analyzer. A working region allocator exists at
>   `src/runtime/region*.rs` but is not driven by the compiler
>   pipeline yet.
> - **`alloc` / `free`** — refused at the capability layer for both
>   LLVM and the interpreter (no runtime heap exists). Earlier
>   versions silently no-op'd them.
> - **`extern "C"` FFI** — `extern` declarations parse and reach the
>   LLVM backend through the AST's `ExternDecl`; the richer FFI
>   registry at `src/ffi/*` is not consulted by the driver.
>
> The WASM backend produces a module, but the module has unresolved
> C-library imports (`printf`, `exit`, `sqrt`, `strlen`, `strcat`)
> and is not directly executable. See
> `docs/IMPLEMENTATION_STATUS.md` for the corpus-verified state of
> every feature."""

# ─── IMPLEMENTATION_STATUS edit: Deferred subsystems section ────────────
STATUS_ANCHOR = "## Legend"

STATUS_REPLACEMENT = """\
## Deferred subsystems (built, not wired in)

The following modules exist, are unit-tested, and compile — but
nothing in the pipeline drives them. They are documented here so a
reader does not assume they are active.

- **`src/runtime/region.rs` and `src/runtime/region_memory.rs`** —
  a working region allocator using `std::alloc::alloc` / `dealloc`
  with parent/child cascade on free, LIFO stack discipline, and
  pointer validity tracking. The `region` keyword parses and the
  analyzer treats it as a lexical scope, but neither backend
  actually allocates or frees through these types.

- **`src/ffi/c.rs` and `src/ffi/lowering.rs`** — a C ABI type
  model (`CType`, `CFunctionSignature`), an FFI registry
  (`FFIRegistry`), and a type-compatibility validator. The
  compiler does not construct an `FFIRegistry` or consult one
  during compilation. `extern` declarations currently reach
  codegen through the AST's `ExternDecl`; the richer registry is
  not wired in.

Wiring these in is a roadmap item, not a bug fix. Both subsystems
are complete in isolation; what is missing is the driver code that
constructs them from source and routes through them.

## Legend"""

# ─── IMPLEMENTATION_STATUS edit: new legend tier ────────────────────────
LEGEND_ANCHOR = "- ❓ **Untested**"

LEGEND_REPLACEMENT = """\
- 🔶 **Syntax only** — parses and analyzes; no backend executes it
- ❓ **Untested**"""

FIXES = [
    ("README.md", README_ANCHOR, README_REPLACEMENT, 1),
    ("docs/IMPLEMENTATION_STATUS.md", STATUS_ANCHOR, STATUS_REPLACEMENT, 1),
    ("docs/IMPLEMENTATION_STATUS.md", LEGEND_ANCHOR, LEGEND_REPLACEMENT, 1),
]


def apply_fix(text, find, replace, occurrence):
    if occurrence == "all":
        return text.replace(find, replace) if find in text else None
    start = 0
    idx = -1
    for _ in range(occurrence):
        idx = text.find(find, start)
        if idx == -1:
            return None
        start = idx + len(find)
    return text[:idx] + replace + text[idx + len(find):]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    repo = Path.cwd()
    if not (repo / "Cargo.toml").exists():
        print("ERROR: run from repo root", file=sys.stderr)
        return 1

    from collections import defaultdict
    by_file = defaultdict(list)
    for i, (rel, find, replace, occ) in enumerate(FIXES, 1):
        by_file[rel].append((i, find, replace, occ))

    planned = []
    for rel, fixes in by_file.items():
        path = repo / rel
        if not path.exists():
            print(f"ERROR: {rel} not found", file=sys.stderr)
            return 1
        text = path.read_text()
        for fix_i, find, replace, occ in fixes:
            new_text = apply_fix(text, find, replace, occ)
            if new_text is None:
                print(
                    f"ERROR: fix {fix_i} in {rel}: anchor not found.\n"
                    f"  Looking for: {find[:80]!r}",
                    file=sys.stderr,
                )
                return 1
            if new_text == text:
                print(
                    f"ERROR: fix {fix_i} in {rel}: no change produced.",
                    file=sys.stderr,
                )
                return 1
            text = new_text
        planned.append((path, text, rel))

    if args.dry_run:
        print("Dry run — would edit:")
        for _, _, rel in planned:
            print(f"  {rel}")
        return 0

    for path, new_text, rel in planned:
        path.write_text(new_text)
        print(f"  edited: {rel}")
    print()
    print("Review the diff with:")
    print("  git diff README.md docs/IMPLEMENTATION_STATUS.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())
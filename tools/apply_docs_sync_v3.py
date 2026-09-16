#!/usr/bin/env python3
"""
Docs sync v3 — reflect Step 6 (region auto-free on both backends).

Regex-anchored so internal content edits don't invalidate the match.
"""

import re
import sys
from pathlib import Path

# ─── README: replace the region bullet ───
README_PATTERN = re.compile(
    r"> - \*\*`region` blocks\*\* work end-to-end\. The interpreter auto-frees\n"
    r">   a region's allocations on `RegionExit` \(Step 3 wiring\); the\n"
    r">   LLVM backend treats `region` as a lexical hint with no\n"
    r">   auto-free — programs relying on auto-free must run through the\n"
    r">   interpreter, or free their allocations explicitly\. This\n"
    r">   asymmetry is documented in `docs/IMPLEMENTATION_STATUS\.md`\.",
)

README_REPLACE = """\
> - **`region` blocks** work end-to-end with auto-free on both
>   backends. `RegionExit` frees the region's allocations in the
>   interpreter (Step 3 wiring) and in the LLVM backend (Step 6
>   wiring, tag `step6-done`). Explicit `free(p)` inside a region
>   is idempotent — LLVM nulls the pointer after freeing so
>   auto-free skips it. The one remaining asymmetry (reassigning a
>   `var` pointer inside a region leaks the earlier allocation in
>   LLVM but not in the interpreter) is documented in
>   `docs/IMPLEMENTATION_STATUS.md`."""

# ─── IMPLEMENTATION_STATUS: replace the region divergence bullet ───
STATUS_PATTERN = re.compile(
    r"- \*\*Region auto-free is interpreter-only\.\*\* A `region r` block\n"
    r"  frees its allocations on exit in the interpreter\. The LLVM\n"
    r"  backend treats `region` as a lexical hint and does not free\n"
    r"  region-scoped allocations implicitly — programs relying on\n"
    r"  auto-free must call `free\(p\)` explicitly or run through the\n"
    r"  interpreter\.",
)

STATUS_REPLACE = """\
- **Region `var` reassignment leaks in LLVM.** If a `var p` inside a\n"
  "  `region r` is reassigned from one `alloc` result to another\n"
  "  (`p := alloc(8); p := alloc(16)`), the LLVM backend frees only\n"
  "  the value visible at region exit. The interpreter tracks the\n"
  "  allocation handle and frees both. Programs that reassign a\n"
  "  pointer variable inside a region should free explicitly before\n"
  "  reassigning, or run through the interpreter."""

FIXES = [
    ("README.md", README_PATTERN, README_REPLACE),
    ("docs/IMPLEMENTATION_STATUS.md", STATUS_PATTERN, STATUS_REPLACE),
]


def main():
    repo = Path.cwd()
    if not (repo / "Cargo.toml").exists():
        print("ERROR: run from repo root", file=sys.stderr)
        return 1

    for rel, pattern, replacement in FIXES:
        path = repo / rel
        if not path.exists():
            print(f"ERROR: {rel} not found", file=sys.stderr)
            return 1
        text = path.read_text()
        if not pattern.search(text):
            print(
                f"ERROR: {rel}: pattern did not match.\n"
                f"  Pattern: {pattern.pattern[:120]!r}",
                file=sys.stderr,
            )
            return 1
        new_text = pattern.sub(replacement, text, count=1)
        if new_text == text:
            print(f"ERROR: {rel}: replacement made no change", file=sys.stderr)
            return 1
        path.write_text(new_text)
        print(f"  edited: {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
#!/usr/bin/env python3
"""
Final docs sync (v2) — uses regex anchors so internal content
edits don't invalidate the match.

- README: replace the entire "Note:" block with updated copy.
- IMPLEMENTATION_STATUS: replace the "Deferred subsystems" section
  with "Known divergences" + "Deferred runtime modules".
"""

import re
import sys
from pathlib import Path

# ─── README: match from "> **Note:" up to (but not including)
#     the blank line before "## Backends". ───
README_PATTERN = re.compile(
    r"> \*\*Note:.*?(?=\n\n## Backends)",
    re.DOTALL,
)

README_REPLACE = """\
> **Note on execution coverage:**
>
> - **`alloc` / `free`** work end-to-end through both backends.
>   The interpreter uses a simulated heap; LLVM lowers to libc
>   `malloc` / `free` (Step 5 wiring, tag `step5-done`).
> - **`region` blocks** work end-to-end. The interpreter auto-frees
>   a region's allocations on `RegionExit` (Step 3 wiring); the
>   LLVM backend treats `region` as a lexical hint with no
>   auto-free — programs relying on auto-free must run through the
>   interpreter, or free their allocations explicitly. This
>   asymmetry is documented in `docs/IMPLEMENTATION_STATUS.md`.
> - **`extern "C"` FFI** works through LLVM. `as "symbol"` renaming
>   and `from "library"` linking are honored (Step 4b wiring, tag
>   `step4b-done`). Variadic externs (`...`) parse but do not
>   validate argument types — a known gap.
> - The WASM backend produces a module, but the module has
>   unresolved C-library imports (`printf`, `exit`, `sqrt`,
>   `strlen`, `strcat`, `malloc`, `free`) and is not directly
>   executable. See `docs/IMPLEMENTATION_STATUS.md` for the
>   corpus-verified state of every feature."""

# ─── IMPLEMENTATION_STATUS: replace from "## Deferred subsystems"
#     heading up to (but not including) "## Legend". ───
STATUS_PATTERN = re.compile(
    r"## Deferred subsystems.*?(?=## Legend)",
    re.DOTALL,
)

STATUS_REPLACE = """\
## Known divergences between backends

The interpreter and LLVM backends agree on observable behavior for
all corpus programs. The following non-corpus divergences are
documented so future work can close them:

- **Region auto-free is interpreter-only.** A `region r` block
  frees its allocations on exit in the interpreter. The LLVM
  backend treats `region` as a lexical hint and does not free
  region-scoped allocations implicitly — programs relying on
  auto-free must call `free(p)` explicitly or run through the
  interpreter.
- **Variadic FFI arguments are not validated.** `extern "C"
  function printf(...)` parses, registers, and calls through to
  libc, but the analyzer does not check the argument count or
  types against a variadic signature.
- **WASM output requires a host shim.** The generated `.wasm`
  module imports `printf`, `exit`, `malloc`, `free`, and the C
  math library; it cannot execute without a host that provides
  those symbols.

## Deferred runtime modules (wired but not shared)

The modules `src/runtime/region.rs` and `src/runtime/region_memory.rs`
implement a `std::alloc`-based region allocator with parent/child
cascade on free. They are not used by the current pipeline: the
interpreter has its own heap and the LLVM backend relies on libc.
These modules are the seed of a shared runtime that a future
backend could consume.

"""

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
                f"  Pattern: {pattern.pattern[:80]!r}",
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
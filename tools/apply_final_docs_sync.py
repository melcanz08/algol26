#!/usr/bin/env python3
"""
Final docs sync after Steps 2–5.

- README: replace the "three features are parse-and-analyze only"
  note with the actual current state.
- IMPLEMENTATION_STATUS: update region/alloc/free/extern rows,
  remove stale "deferred subsystems" entries, add the region
  auto-free asymmetry as a known divergence.
"""

import sys
from collections import defaultdict
from pathlib import Path

# ─── README replacement ────────────────────────────────────────────────
README_FIND = """\
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

README_REPLACE = """\
> **Note on execution coverage:**
>
> - **`alloc` / `free`** work end-to-end through both backends. The
>   interpreter uses a simulated heap; LLVM lowers to libc
>   `malloc` / `free`.
> - **`region` blocks** work end-to-end. The interpreter auto-frees
>   a region's allocations on `RegionExit`; the LLVM backend treats
>   `region` as a lexical hint with no auto-free — programs relying
>   on auto-free must run through the interpreter, or free their
>   allocations explicitly. This asymmetry is documented in
>   `docs/IMPLEMENTATION_STATUS.md`.
> - **`extern "C"` FFI** works through LLVM. `as "symbol"` renaming
>   and `from "library"` linking are honored. Variadic externs
>   (`...`) parse but do not validate argument types — a known gap.
> - The WASM backend produces a module, but the module has
>   unresolved C-library imports (`printf`, `exit`, `sqrt`,
>   `strlen`, `strcat`, `malloc`, `free`) and is not directly
>   executable. See `docs/IMPLEMENTATION_STATUS.md` for the
>   corpus-verified state of every feature."""

# ─── IMPLEMENTATION_STATUS: replace deferred-subsystems section ────────
STATUS_FIND = """\
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

## Legend"""

# ─── IMPLEMENTATION_STATUS: update legend tier description ─────────────
LEGEND_FIND = """\
- 🔶 **Syntax only** — parses and analyzes; no backend executes it
- ❓ **Untested**"""

LEGEND_REPLACE = """\
- 🔶 **Syntax only** — parses and analyzes; no backend executes it
- ⚠️ **Backend divergence** — supported by one backend but not the other
- ❓ **Untested**"""

FIXES = [
    ("README.md", README_FIND, README_REPLACE, 1),
    ("docs/IMPLEMENTATION_STATUS.md", STATUS_FIND, STATUS_REPLACE, 1),
    ("docs/IMPLEMENTATION_STATUS.md", LEGEND_FIND, LEGEND_REPLACE, 1),
]


def apply_fix(text, find, replace, occurrence):
    start = 0
    idx = -1
    for _ in range(occurrence):
        idx = text.find(find, start)
        if idx == -1:
            return None
        start = idx + len(find)
    return text[:idx] + replace + text[idx + len(find):]


def main():
    repo = Path.cwd()
    if not (repo / "Cargo.toml").exists():
        print("ERROR: run from repo root", file=sys.stderr)
        return 1

    by_file = defaultdict(list)
    for i, (rel, find, replace, occ) in enumerate(FIXES, 1):
        by_file[rel].append((i, find, replace, occ))

    for rel, fixes in by_file.items():
        path = repo / rel
        if not path.exists():
            print(f"ERROR: {rel} not found", file=sys.stderr)
            return 1
        text = path.read_text()
        for fix_i, find, _, occ in fixes:
            if apply_fix(text, find, find, occ) is None:
                print(
                    f"ERROR: fix {fix_i} in {rel}: anchor not found.\n"
                    f"  First 120 chars: {find[:120]!r}",
                    file=sys.stderr,
                )
                return 1

    for rel, fixes in by_file.items():
        path = repo / rel
        text = path.read_text()
        for fix_i, find, replace, occ in fixes:
            new_text = apply_fix(text, find, replace, occ)
            if new_text is None:
                print(f"ERROR: fix {fix_i} chaining failed on {rel}", file=sys.stderr)
                return 1
            if new_text == text:
                print(f"ERROR: fix {fix_i} in {rel}: no change", file=sys.stderr)
                return 1
            text = new_text
        path.write_text(text)
        print(f"  applied {len(fixes)} fix(es): {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
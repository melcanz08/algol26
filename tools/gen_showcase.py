#!/usr/bin/env python3
"""Regenerate examples/showcase/ from tests/fixtures/.

Copies every fixture whose `// supported:` header includes
`interpreter llvm wasm`, prefixed with a number for stable order.
Writes a README indexing them and listing the gaps.
"""
import pathlib, shutil, sys

repo = pathlib.Path(__file__).resolve().parent.parent
fixtures_dir = repo / "tests/fixtures"
showcase_dir = repo / "examples/showcase"

if showcase_dir.exists():
    shutil.rmtree(showcase_dir)
showcase_dir.mkdir(parents=True, exist_ok=True)

portable, gaps = [], []
for path in sorted(fixtures_dir.glob("*.gol")):
    first = path.read_text().split("\n", 1)[0]
    if "interpreter llvm wasm" in first:
        portable.append(path)
    else:
        gaps.append(path)

for i, path in enumerate(portable, 1):
    shutil.copy2(path, showcase_dir / f"{i:02d}_{path.name}")

lines = [
    "# Algol26 Generated Showcase", "",
    f"{len(portable)} programs, each runs identically on interpreter, LLVM, and WASM.",
    "", "## Programs", "",
]
for i, path in enumerate(portable, 1):
    lines.append(f"{i}. `{path.name}`")
lines.append("")
if gaps:
    lines += ["## Not in the showcase", "",
              "Interpreter-only fixtures. Each has a `// supported:` header.",
              ""]
    for path in gaps:
        lines.append(f"- `{path.name}`")
    lines.append("")
(showcase_dir / "README.md").write_text("\n".join(lines))
print(f"{len(portable)} portable, {len(gaps)} gaps")

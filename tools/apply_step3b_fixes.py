#!/usr/bin/env python3
"""
Step 3b: use RegionFrame::name in RegionExit to detect IR-level
mismatches. Fixes the dead-code warning and adds a real invariant.
"""

import sys
from pathlib import Path

FIXES = [
    (
        "src/backends/interpreter/mod.rs",
        "            Instruction::RegionExit { .. } => {\n"
        "                if let Some(frame) = self.region_stack.pop() {\n"
        "                    for handle in frame.allocations {\n"
        "                        self.heap.remove(&handle);\n"
        "                    }\n"
        "                }\n"
        "            }\n",
        "            Instruction::RegionExit { name } => {\n"
        "                // The frame's name must match the exit's name.\n"
        "                // A mismatch means the IR builder emitted an\n"
        "                // enter/exit pair out of sync — a compiler bug,\n"
        "                // not a user error. Report it loudly rather than\n"
        "                // silently freeing the wrong region's heap.\n"
        "                match self.region_stack.pop() {\n"
        "                    Some(frame) if frame.name == *name => {\n"
        "                        for handle in frame.allocations {\n"
        "                            self.heap.remove(&handle);\n"
        "                        }\n"
        "                    }\n"
        "                    Some(frame) => {\n"
        "                        return Err(format!(\n"
        "                            \"region exit mismatch: expected '{}', found '{}'\",\n"
        "                            name, frame.name\n"
        "                        ));\n"
        "                    }\n"
        "                    None => {\n"
        "                        return Err(format!(\n"
        "                            \"region exit '{}' with no matching enter\",\n"
        "                            name\n"
        "                        ));\n"
        "                    }\n"
        "                }\n"
        "            }\n",
        1,
    ),
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

    for i, (rel, find, replace, occ) in enumerate(FIXES, 1):
        path = repo / rel
        if not path.exists():
            print(f"ERROR: fix {i}: {rel} not found", file=sys.stderr)
            return 1
        text = path.read_text()
        new_text = apply_fix(text, find, replace, occ)
        if new_text is None:
            print(
                f"ERROR: fix {i}: {rel}: anchor not found.\n"
                f"  First 120 chars: {find[:120]!r}",
                file=sys.stderr,
            )
            return 1
        path.write_text(new_text)
        print(f"  applied fix {i}: {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
#!/usr/bin/env python3
"""
A5b fixes:
1. runtime_kind not in scope in interpreter/mod.rs — use full path.
2. CfgInstruction arm in src/ir/cfg/builder.rs.
"""

from pathlib import Path

INTERP = Path("src/backends/interpreter/mod.rs")
CFG = Path("src/ir/cfg/builder.rs")


def patch(path, edits):
    src = path.read_text()
    for old, new, label in edits:
        n = src.count(old)
        if n != 1:
            print(f"FAIL: {path} — {label} matched {n} times")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            path.write_text(src)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
        path.write_text(src)
        print(f"OK: {path} — {label}")


# 1. Use fully-qualified path for runtime_kind in mod.rs.
patch(INTERP, [
    (
        """                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "BoundsCheck",
                            left: runtime_kind(&other),
                            right: "Int",
                        });
                    }""",
        """                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "BoundsCheck",
                            left: super::runtime::runtime_kind(&other),
                            right: "Int",
                        });
                    }""",
        "runtime_kind full path",
    ),
])

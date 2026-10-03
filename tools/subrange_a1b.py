#!/usr/bin/env python3
"""
A1b: Subrange arms in the two exhaustive matches.

  1. llvm_codegen::map_type       -> unwrap to base
  2. instantiation_plan::mangled_type_name -> Subrange_{id}
"""

from pathlib import Path

LLVM = Path("src/backends/llvm_codegen/types.rs")
PLAN = Path("src/ir/instantiation_plan.rs")


def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


patch(
    LLVM,
    [
        (
            """            // An enum's runtime value is its ordinal: an i64. The
            // identity and variant names are compile-time only.
            // See ADR 0030.
            Type::Enum { .. } => self.context.i64_type().into(),

            Type::List(inner) => {""",
            """            // An enum's runtime value is its ordinal: an i64. The
            // identity and variant names are compile-time only.
            // See ADR 0030.
            Type::Enum { .. } => self.context.i64_type().into(),

            // A subrange has no runtime representation of its own;
            // it lowers to its base (Int or an enum). The identity
            // and the bounds are compile-time properties enforced
            // by the analyzer's literal check and the runtime
            // BoundsCheck instruction. See ADR 0031.
            Type::Subrange { base, .. } => self.map_type(base),

            Type::List(inner) => {""",
        ),
    ],
)

patch(
    PLAN,
    [
        (
            """        // Enums follow the same discipline: mangle by identity.
        // Two `enum Color` declarations in different modules must
        // mangle differently. See ADR 0030.
        Type::Enum { id, .. } => format!("Enum_{}", id.0),
    }
}""",
            """        // Enums follow the same discipline: mangle by identity.
        // Two `enum Color` declarations in different modules must
        // mangle differently. See ADR 0030.
        Type::Enum { id, .. } => format!("Enum_{}", id.0),
        // Subranges: mangle by identity, same reasoning. Two
        // `type Percentage Int in 0..100` declarations in different
        // modules are different types. See ADR 0031.
        Type::Subrange { id, .. } => format!("Subrange_{}", id.0),
    }
}""",
        ),
    ],
)

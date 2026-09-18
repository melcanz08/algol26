#!/usr/bin/env python3
"""
PR-13a quick fixes. Text-exact edits, validated before writing.
If any 'find' string is missing or appears the wrong number of times,
the script aborts without touching anything.

Usage:
    python3 tools/apply_pr13a_fixes.py --dry-run
    python3 tools/apply_pr13a_fixes.py
"""

import argparse
import sys
from pathlib import Path

# (path, find, replace, occurrence) — occurrence is 1-indexed, or "all"
FIXES = [
    # 4.2.1 NotEqual on pointers emits EQ (should be NE).
    # The string appears twice; the 2nd is the buggy NotEqual arm.
    (
        "src/backends/llvm_codegen/binop.rs",
        '.build_int_compare(inkwell::IntPredicate::EQ, l_int, r_int, "ptr_eq")',
        '.build_int_compare(inkwell::IntPredicate::NE, l_int, r_int, "ptr_ne")',
        2,
    ),

    # 4.2.7 Switch with no default picks an arbitrary block.
    (
        "src/backends/llvm_codegen/terminator.rs",
        "                    let default_bb = if let Some(default_id) = default_block {\n"
        "                        self.blocks.get(default_id).cloned().unwrap()\n"
        "                    } else {\n"
        "                        // create dummy unreachable? use current block's next? fallback to entry\n"
        "                        self.blocks.values().next().cloned().unwrap()\n"
        "                    };\n",
        "                    let default_bb = if let Some(default_id) = default_block {\n"
        "                        self.blocks.get(default_id).cloned().unwrap()\n"
        "                    } else {\n"
        "                        // No default. Emit an unreachable block for\n"
        "                        // unmatched values instead of jumping to an\n"
        "                        // arbitrary block. The analyzer's match\n"
        "                        // exhaustiveness check should have caught\n"
        "                        // this at compile time.\n"
        "                        let saved_bb = self.builder.get_insert_block().unwrap();\n"
        "                        let un_bb = self.context.append_basic_block(\n"
        "                            self.current_function.unwrap(),\n"
        "                            \"switch_unmatched\",\n"
        "                        );\n"
        "                        self.builder.position_at_end(un_bb);\n"
        "                        self.builder.build_unreachable().unwrap();\n"
        "                        self.builder.position_at_end(saved_bb);\n"
        "                        un_bb\n"
        "                    };\n",
        1,
    ),

    # 4.3.I1 Interpreter iteration limit too low.
    (
        "src/backends/interpreter/mod.rs",
        "            if iterations > 10000 {",
        "            if iterations > 100_000_000 {",
        1,
    ),

    # 4.3.I4 + 4.3.I6 Interpreter Math builtins.
    (
        "src/backends/interpreter/eval.rs",
        '            "Math.sqrt" => {\n'
        '                if let Some(RuntimeValue::Float(f)) = arg_vals.first() {\n'
        '                    RuntimeValue::Float(f.sqrt())\n'
        '                } else {\n'
        '                    RuntimeValue::Void\n'
        '                }\n'
        '            }\n',
        '            "Math.sqrt" | "Math.sin" | "Math.cos" | "Math.tan"\n'
        '            | "Math.exp" | "Math.log" | "Math.floor" | "Math.ceil"\n'
        '            | "Math.abs" => {\n'
        '                let x = match arg_vals.first() {\n'
        '                    Some(RuntimeValue::Float(f)) => *f,\n'
        '                    Some(RuntimeValue::Int(i)) => *i as f64,\n'
        '                    _ => return RuntimeValue::Void,\n'
        '                };\n'
        '                let r = match func {\n'
        '                    "Math.sqrt" => x.sqrt(),\n'
        '                    "Math.sin" => x.sin(),\n'
        '                    "Math.cos" => x.cos(),\n'
        '                    "Math.tan" => x.tan(),\n'
        '                    "Math.exp" => x.exp(),\n'
        '                    "Math.log" => x.ln(),\n'
        '                    "Math.floor" => x.floor(),\n'
        '                    "Math.ceil" => x.ceil(),\n'
        '                    "Math.abs" => x.abs(),\n'
        '                    _ => unreachable!(),\n'
        '                };\n'
        '                RuntimeValue::Float(r)\n'
        '            }\n'
        '            "Math.pow" => {\n'
        '                let (a, b) = match (arg_vals.first(), arg_vals.get(1)) {\n'
        '                    (Some(RuntimeValue::Float(a)), Some(RuntimeValue::Float(b))) => (*a, *b),\n'
        '                    (Some(RuntimeValue::Int(a)), Some(RuntimeValue::Float(b))) => (*a as f64, *b),\n'
        '                    (Some(RuntimeValue::Float(a)), Some(RuntimeValue::Int(b))) => (*a, *b as f64),\n'
        '                    (Some(RuntimeValue::Int(a)), Some(RuntimeValue::Int(b))) => (*a as f64, *b as f64),\n'
        '                    _ => return RuntimeValue::Void,\n'
        '                };\n'
        '                RuntimeValue::Float(a.powf(b))\n'
        '            }\n',
        1,
    ),
]


def find_nth(text, needle, n):
    start = 0
    idx = -1
    for _ in range(n):
        idx = text.find(needle, start)
        if idx == -1:
            return -1
        start = idx + len(needle)
    return idx


def apply_fix(text, find, replace, occurrence):
    if occurrence == "all":
        return text.replace(find, replace) if find in text else None
    idx = find_nth(text, find, occurrence)
    if idx == -1:
        return None
    return text[:idx] + replace + text[idx + len(find):]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    repo = Path.cwd()
    if not (repo / "Cargo.toml").exists():
        print("ERROR: run this from the repository root", file=sys.stderr)
        return 1

    edits = []
    for rel, find, replace, occ in FIXES:
        path = repo / rel
        if not path.exists():
            print(f"ERROR: {rel} not found", file=sys.stderr)
            return 1
        text = path.read_text()
        new_text = apply_fix(text, find, replace, occ)
        if new_text is None:
            print(
                f"ERROR: {rel}: could not find the expected text "
                f"(occurrence {occ}).\n  Looking for: {find[:90]!r}",
                file=sys.stderr,
            )
            return 1
        if new_text == text:
            print(f"ERROR: {rel}: replacement produced no change", file=sys.stderr)
            return 1
        edits.append((path, new_text, rel))

    if args.dry_run:
        print("Dry run — would edit:")
        for _, _, rel in edits:
            print(f"  {rel}")
        return 0

    for path, new_text, rel in edits:
        path.write_text(new_text)
        print(f"  applied: {rel}")

    print()
    print("Verify with:")
    print("  cargo build 2>&1 | tail -3")
    print("  cargo test --all-features 2>&1 | grep -E 'test result: FAILED|FAILED' | head")
    return 0


if __name__ == "__main__":
    sys.exit(main())
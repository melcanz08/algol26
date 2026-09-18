#!/usr/bin/env python3
"""
PR-13b quick fixes. Text-exact edits, validated before writing.

- 4.2.4 : LLVM Assign for a list rebuilds the array and updates list_arrays.
- 4.3.I4b : interpreter List.max / List.min
- 4.3.I4c : interpreter String.substring

Usage:
    python3 tools/apply_pr13b_fixes.py --dry-run
    python3 tools/apply_pr13b_fixes.py
"""

import argparse
import sys
from pathlib import Path

FIXES = [
    # ─── 4.2.4 ─── List reassignment
    (
        "src/backends/llvm_codegen/instruction.rs",
        "            Instruction::Assign { target, value } => {\n"
        "                let ptr = self.variables.get(target).cloned().ok_or_else(|| {\n"
        "                    CompileError::simple(\n"
        "                        &format!(\"var {} not found\", target),\n"
        "                        0,\n"
        "                        0,\n"
        "                        \"\",\n"
        "                        ErrorCode::E0004,\n"
        "                    )\n"
        "                })?;\n"
        "                let val = self.compile_value(value)?;\n"
        "                self.builder.build_store(ptr, val).unwrap();\n"
        "                if let TypedIRValue::List(elems, _) = value {\n"
        "                    self.list_lengths.insert(target.clone(), elems.len());\n"
        "                }\n"
        "                Ok(())\n"
        "            }\n",
        "            Instruction::Assign { target, value } => {\n"
        "                // A list assignment must mirror `Declare`'s list\n"
        "                // path: allocate a fresh array, populate it, and\n"
        "                // update `list_arrays`, `list_array_types`, and\n"
        "                // `list_lengths` together. Updating only\n"
        "                // `list_lengths` left `list_arrays[target]`\n"
        "                // pointing at the *old* array — subsequent\n"
        "                // indexing read from stale memory.\n"
        "                if let TypedIRValue::List(elems, elem_ty) = value {\n"
        "                    let len = elems.len();\n"
        "                    let elem_llvm_ty = self.map_type(elem_ty);\n"
        "                    let array_ty = elem_llvm_ty.array_type(len as u32);\n"
        "                    let arr_alloca = self.create_entry_alloca(\n"
        "                        &format!(\"{}_data\", target),\n"
        "                        &Type::Array(Box::new(elem_ty.clone()), len),\n"
        "                    );\n"
        "                    for (i, elem) in elems.iter().enumerate() {\n"
        "                        let ev = self.compile_value(elem)?;\n"
        "                        let idx = self.context.i32_type().const_int(i as u64, false);\n"
        "                        let ptr = unsafe {\n"
        "                            self.builder\n"
        "                                .build_gep(\n"
        "                                    array_ty,\n"
        "                                    arr_alloca,\n"
        "                                    &[self.context.i32_type().const_zero(), idx],\n"
        "                                    &format!(\"{}_assign_gep_{}\", target, i),\n"
        "                                )\n"
        "                                .unwrap()\n"
        "                        };\n"
        "                        self.builder.build_store(ptr, ev).unwrap();\n"
        "                    }\n"
        "                    self.list_arrays.insert(target.clone(), arr_alloca);\n"
        "                    self.list_array_types.insert(target.clone(), array_ty);\n"
        "                    self.list_lengths.insert(target.clone(), len);\n"
        "                    self.variables.insert(target.clone(), arr_alloca);\n"
        "                    return Ok(());\n"
        "                }\n"
        "                let ptr = self.variables.get(target).cloned().ok_or_else(|| {\n"
        "                    CompileError::simple(\n"
        "                        &format!(\"var {} not found\", target),\n"
        "                        0,\n"
        "                        0,\n"
        "                        \"\",\n"
        "                        ErrorCode::E0004,\n"
        "                    )\n"
        "                })?;\n"
        "                let val = self.compile_value(value)?;\n"
        "                self.builder.build_store(ptr, val).unwrap();\n"
        "                Ok(())\n"
        "            }\n",
        1,
    ),

    # ─── 4.3.I4b + 4.3.I4c — interpreter builtins ───
    # Insert List.max/min and String.substring right after the List.sum arm.
    # Anchor on the List.sum arm's closing brace.
    (
        "src/backends/interpreter/eval.rs",
        '            "List.sum" | "sum" => {\n'
        '                if let Some(RuntimeValue::List(list)) = arg_vals.first() {\n'
        '                    let sum: f64 = list\n'
        '                        .iter()\n'
        '                        .map(|v| match v {\n'
        '                            RuntimeValue::Int(i) => *i as f64,\n'
        '                            RuntimeValue::Float(f) => *f,\n'
        '                            _ => 0.0,\n'
        '                        })\n'
        '                        .sum();\n'
        '                    RuntimeValue::Float(sum)\n'
        '                } else {\n'
        '                    RuntimeValue::Float(0.0)\n'
        '                }\n'
        '            }\n',
        '            "List.sum" | "sum" => {\n'
        '                if let Some(RuntimeValue::List(list)) = arg_vals.first() {\n'
        '                    let sum: f64 = list\n'
        '                        .iter()\n'
        '                        .map(|v| match v {\n'
        '                            RuntimeValue::Int(i) => *i as f64,\n'
        '                            RuntimeValue::Float(f) => *f,\n'
        '                            _ => 0.0,\n'
        '                        })\n'
        '                        .sum();\n'
        '                    RuntimeValue::Float(sum)\n'
        '                } else {\n'
        '                    RuntimeValue::Float(0.0)\n'
        '                }\n'
        '            }\n'
        '            "List.max" => {\n'
        '                if let Some(RuntimeValue::List(list)) = arg_vals.first() {\n'
        '                    let max = list\n'
        '                        .iter()\n'
        '                        .filter_map(|v| match v {\n'
        '                            RuntimeValue::Int(i) => Some(*i as f64),\n'
        '                            RuntimeValue::Float(f) => Some(*f),\n'
        '                            _ => None,\n'
        '                        })\n'
        '                        .fold(f64::NEG_INFINITY, f64::max);\n'
        '                    RuntimeValue::Float(max)\n'
        '                } else {\n'
        '                    RuntimeValue::Float(0.0)\n'
        '                }\n'
        '            }\n'
        '            "List.min" => {\n'
        '                if let Some(RuntimeValue::List(list)) = arg_vals.first() {\n'
        '                    let min = list\n'
        '                        .iter()\n'
        '                        .filter_map(|v| match v {\n'
        '                            RuntimeValue::Int(i) => Some(*i as f64),\n'
        '                            RuntimeValue::Float(f) => Some(*f),\n'
        '                            _ => None,\n'
        '                        })\n'
        '                        .fold(f64::INFINITY, f64::min);\n'
        '                    RuntimeValue::Float(min)\n'
        '                } else {\n'
        '                    RuntimeValue::Float(0.0)\n'
        '                }\n'
        '            }\n'
        '            "String.substring" => {\n'
        '                let s = match arg_vals.first() {\n'
        '                    Some(RuntimeValue::String(s)) => s.clone(),\n'
        '                    _ => return RuntimeValue::Void,\n'
        '                };\n'
        '                let start = match arg_vals.get(1) {\n'
        '                    Some(RuntimeValue::Int(i)) => (*i).max(0) as usize,\n'
        '                    _ => return RuntimeValue::Void,\n'
        '                };\n'
        '                let length = match arg_vals.get(2) {\n'
        '                    Some(RuntimeValue::Int(i)) => (*i).max(0) as usize,\n'
        '                    _ => return RuntimeValue::Void,\n'
        '                };\n'
        '                let chars: Vec<char> = s.chars().collect();\n'
        '                let end = (start + length).min(chars.len());\n'
        '                if start >= chars.len() {\n'
        '                    RuntimeValue::String(String::new())\n'
        '                } else {\n'
        '                    RuntimeValue::String(chars[start..end].iter().collect())\n'
        '                }\n'
        '            }\n',
        1,
    ),
]


def apply_fix(text, find, replace, occurrence):
    if occurrence == "all":
        if find not in text:
            return None
        return text.replace(find, replace)
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
                f"ERROR: {rel}: could not find the expected text.\n"
                f"  First 90 chars: {find[:90]!r}",
                file=sys.stderr,
            )
            return 1
        if new_text == text:
            print(f"ERROR: {rel}: replacement made no change", file=sys.stderr)
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
    return 0


if __name__ == "__main__":
    sys.exit(main())
#!/usr/bin/env python3
"""
PR-13c: capability refusals for Option and RawMemory.

- Add Feature::Option and Feature::RawMemory.
- Scan Some/None as Feature::Option; Allocate/Free as Feature::RawMemory.
- Refuse them for LLVM (which silently unwrapped Some/None and no-op'd
  alloc/free). The interpreter still accepts Option.
- Add defensive error arms in LLVM codegen for Some/None/Ok/Error.
- Add capability tests for the new refusals.

Usage:
    python3 tools/apply_pr13c_fixes.py --dry-run
    python3 tools/apply_pr13c_fixes.py
"""

import argparse
import sys
from pathlib import Path

FIXES = [
    # ─── 1. Add Feature::Option and Feature::RawMemory to the enum ───
    (
        "src/backends/capabilities/mod.rs",
        "    /// `print(x)` where `x` has a list type. The interpreter formats\n"
        "    /// lists as `[a, b, c]`; the LLVM backend has no lowering for it\n"
        "    /// (it would need a per-element printf loop).\n"
        "    ListPrint,\n"
        "}\n",
        "    /// `print(x)` where `x` has a list type. The interpreter formats\n"
        "    /// lists as `[a, b, c]`; the LLVM backend has no lowering for it\n"
        "    /// (it would need a per-element printf loop).\n"
        "    ListPrint,\n"
        "    /// `Option<T>` values: `Some(x)` and `None`. The LLVM backend\n"
        "    /// has no tag+payload representation, so it silently unwrapped\n"
        "    /// `Some(x)` to `x` and `None` to null — producing wrong code\n"
        "    /// for any program that stored or tested an Option value. The\n"
        "    /// interpreter handles both correctly.\n"
        "    Option,\n"
        "    /// `alloc(n)` / `free(p)`. Neither backend has a heap model;\n"
        "    /// both were silently no-op'ing these instructions. Refuse\n"
        "    /// rather than pretend.\n"
        "    RawMemory,\n"
        "}\n",
        1,
    ),

    # ─── 2. Add to Feature::all() ───
    (
        "src/backends/capabilities/mod.rs",
        "            Feature::ListAggregates,\n"
        "            Feature::ListPrint,\n"
        "        ]\n",
        "            Feature::ListAggregates,\n"
        "            Feature::ListPrint,\n"
        "            Feature::Option,\n"
        "            Feature::RawMemory,\n"
        "        ]\n",
        1,
    ),

    # ─── 3. Add name() arms ───
    (
        "src/backends/capabilities/mod.rs",
        '            Feature::ListPrint => "print(list)",\n'
        '        }\n',
        '            Feature::ListPrint => "print(list)",\n'
        '            Feature::Option => "option",\n'
        '            Feature::RawMemory => "raw-memory",\n'
        '        }\n',
        1,
    ),

    # ─── 4. Add description() arms ───
    (
        "src/backends/capabilities/mod.rs",
        '            Feature::ListPrint => "printing a list value",\n'
        '        }\n',
        '            Feature::ListPrint => "printing a list value",\n'
        '            Feature::Option => "Option<T>: Some(x) and None",\n'
        '            Feature::RawMemory => "alloc / free (raw memory)",\n'
        '        }\n',
        1,
    ),

    # ─── 5. Interpreter supports Option (it has RuntimeValue::Option) ───
    (
        "src/backends/capabilities/mod.rs",
        "        supported.insert(Feature::ListAggregates);\n"
        "        supported.insert(Feature::ListPrint);\n"
        "        // FFI is not supported by the interpreter.\n",
        "        supported.insert(Feature::ListAggregates);\n"
        "        supported.insert(Feature::ListPrint);\n"
        "        supported.insert(Feature::Option);\n"
        "        // FFI and raw memory are not supported by the interpreter.\n",
        1,
    ),

    # ─── 6. Scan Some/None as Feature::Option ───
    (
        "src/backends/capabilities/scan.rs",
        "        TypedIRValue::Some(inner) => scan_value(inner, extern_fns, used),\n"
        "        TypedIRValue::Cast { value, .. } => scan_value(value, extern_fns, used),\n",
        "        TypedIRValue::Some(inner) => {\n"
        "            used.insert(Feature::Option);\n"
        "            scan_value(inner, extern_fns, used);\n"
        "        }\n"
        "        TypedIRValue::None { .. } => {\n"
        "            used.insert(Feature::Option);\n"
        "        }\n"
        "        TypedIRValue::Cast { value, .. } => scan_value(value, extern_fns, used),\n",
        1,
    ),

    # ─── 7. Scan Allocate/Free as Feature::RawMemory ───
    (
        "src/backends/capabilities/scan.rs",
        "        Instruction::Allocate { size, .. } => scan_value(size, extern_fns, used),\n"
        "        Instruction::Free { ptr } => scan_value(ptr, extern_fns, used),\n",
        "        Instruction::Allocate { size, .. } => {\n"
        "            used.insert(Feature::RawMemory);\n"
        "            scan_value(size, extern_fns, used);\n"
        "        }\n"
        "        Instruction::Free { ptr } => {\n"
        "            used.insert(Feature::RawMemory);\n"
        "            scan_value(ptr, extern_fns, used);\n"
        "        }\n",
        1,
    ),

    # ─── 8. Defensive error in LLVM codegen for Some/None/Ok/Error ───
    (
        "src/backends/llvm_codegen/value.rs",
        "            TypedIRValue::Some(v) => self.compile_value(v)?,\n"
        "            TypedIRValue::None { .. } => self\n"
        "                .context\n"
        "                .ptr_type(AddressSpace::default())\n"
        "                .const_null()\n"
        "                .into(),\n"
        "            TypedIRValue::Ok { value, .. } => self.compile_value(value)?,\n"
        "            TypedIRValue::Error { value, .. } => self.compile_value(value)?,\n",
        "            // These four variants have no LLVM lowering — the\n"
        "            // LLVM backend does not model Option<T> or Result<T,E>\n"
        "            // as tagged unions. The capability scan refuses\n"
        "            // programs that would produce them, so reaching this\n"
        "            // code means the scan was bypassed or the IR builder\n"
        "            // emitted something the capability system missed.\n"
        "            //\n"
        "            // Error out defensively instead of silently unwrapping\n"
        "            // (which is what the code did before PR-13c and was the\n"
        "            // source of a real wrong-code bug).\n"
        "            TypedIRValue::Some(_) => {\n"
        "                return Err(CompileError::simple(\n"
        "                    \"LLVM codegen: Some(...) has no LLVM lowering; \\\n"
        "                     the capability scan should have refused this program\",\n"
        "                    0, 0, \"\", ErrorCode::E0002,\n"
        "                ));\n"
        "            }\n"
        "            TypedIRValue::None { .. } => {\n"
        "                return Err(CompileError::simple(\n"
        "                    \"LLVM codegen: None has no LLVM lowering; \\\n"
        "                     the capability scan should have refused this program\",\n"
        "                    0, 0, \"\", ErrorCode::E0002,\n"
        "                ));\n"
        "            }\n"
        "            TypedIRValue::Ok { .. } => {\n"
        "                return Err(CompileError::simple(\n"
        "                    \"LLVM codegen: Ok(...) has no LLVM lowering; \\\n"
        "                     the capability scan should have refused this program\",\n"
        "                    0, 0, \"\", ErrorCode::E0002,\n"
        "                ));\n"
        "            }\n"
        "            TypedIRValue::Error { .. } => {\n"
        "                return Err(CompileError::simple(\n"
        "                    \"LLVM codegen: Error(...) has no LLVM lowering; \\\n"
        "                     the capability scan should have refused this program\",\n"
        "                    0, 0, \"\", ErrorCode::E0002,\n"
        "                ));\n"
        "            }\n",
        1,
    ),

    # ─── 9. Capability test for Option refusal ───
    (
        "src/backends/capabilities/tests.rs",
        "#[test]\n"
        "fn interpreter_accepts_result_values() {\n",
        "#[test]\n"
        "fn llvm_rejects_option_values() {\n"
        "    let program = program_with(\n"
        "        Instruction::Declare {\n"
        "            name: \"m\".to_string(),\n"
        "            mutable: false,\n"
        "            type_: Type::option(Type::Int),\n"
        "            value: TypedIRValue::Some(Box::new(TypedIRValue::Int(1))),\n"
        "        },\n"
        "        simple_return(),\n"
        "    );\n"
        "    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();\n"
        "    assert!(err.message.contains(\"Option\"), \"{}\", err.message);\n"
        "}\n"
        "\n"
        "#[test]\n"
        "fn interpreter_accepts_option_values() {\n"
        "    let program = program_with(\n"
        "        Instruction::Declare {\n"
        "            name: \"m\".to_string(),\n"
        "            mutable: false,\n"
        "            type_: Type::option(Type::Int),\n"
        "            value: TypedIRValue::Some(Box::new(TypedIRValue::Int(1))),\n"
        "        },\n"
        "        simple_return(),\n"
        "    );\n"
        "    assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());\n"
        "}\n"
        "\n"
        "#[test]\n"
        "fn llvm_rejects_raw_memory() {\n"
        "    let program = program_with(\n"
        "        Instruction::Allocate {\n"
        "            target: \"p\".to_string(),\n"
        "            size: TypedIRValue::Int(8),\n"
        "            type_: Type::pointer(Type::Unknown),\n"
        "        },\n"
        "        simple_return(),\n"
        "    );\n"
        "    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();\n"
        "    assert!(err.message.contains(\"raw memory\"), \"{}\", err.message);\n"
        "}\n"
        "\n"
        "#[test]\n"
        "fn interpreter_rejects_raw_memory() {\n"
        "    let program = program_with(\n"
        "        Instruction::Free { ptr: TypedIRValue::NullPtr },\n"
        "        simple_return(),\n"
        "    );\n"
        "    let err = check_backend(&program, &BackendCapabilities::interpreter()).unwrap_err();\n"
        "    assert!(err.message.contains(\"raw memory\"), \"{}\", err.message);\n"
        "}\n"
        "\n"
        "#[test]\n"
        "fn interpreter_accepts_result_values() {\n",
        1,
    ),
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

    edits = []
    for i, (rel, find, replace, occ) in enumerate(FIXES, 1):
        path = repo / rel
        if not path.exists():
            print(f"ERROR: fix {i}: {rel} not found", file=sys.stderr)
            return 1
        text = path.read_text()
        new_text = apply_fix(text, find, replace, occ)
        if new_text is None:
            print(
                f"ERROR: fix {i}: {rel}: could not find expected text.\n"
                f"  First 100 chars: {find[:100]!r}",
                file=sys.stderr,
            )
            return 1
        if new_text == text:
            print(f"ERROR: fix {i}: {rel}: no change", file=sys.stderr)
            return 1
        edits.append((path, new_text, rel, i))

    if args.dry_run:
        print(f"Dry run — {len(edits)} fixes across:")
        seen = set()
        for _, _, rel, _ in edits:
            if rel not in seen:
                print(f"  {rel}")
                seen.add(rel)
        return 0

    # Apply grouped by file to avoid double-writing the same file.
    from collections import defaultdict
    by_file = defaultdict(list)
    for path, new_text, rel, i in edits:
        by_file[rel].append((path, new_text, i))

    for rel, fixes in by_file.items():
        # Only the last write matters per file — but each fix was
        # computed against the *original* file. To apply multiple
        # fixes to the same file, we need to chain them.
        path = fixes[0][0]
        text = path.read_text()
        for _, _, fix_i in fixes:
            find, replace, occ = FIXES[fix_i - 1][1], FIXES[fix_i - 1][2], FIXES[fix_i - 1][3]
            new_text = apply_fix(text, find, replace, occ)
            if new_text is None:
                print(
                    f"ERROR: fix {fix_i} failed when chained against {rel}",
                    file=sys.stderr,
                )
                return 1
            text = new_text
        path.write_text(text)
        print(f"  applied {len(fixes)} fix(es): {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
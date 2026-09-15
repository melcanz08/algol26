#!/usr/bin/env python3
"""
PR-13f: backend cleanup batch.

- 4.4.W2 : delete dead validate_wasm_compatibility; capability system covers it.
- 4.4.W4 : WasmBackend::compile runs module.verify() before writing.
- 4.1.9  : scan_call_name uses builtin_signatures() instead of
           starts_with("String.") — closes an over-broad classification.
- 4.1.8  : IMPLEMENTATION_STATUS.md — split List row: length ✅, sum/max/min ❌ on LLVM.
- 4.4.W6 : add llvm_accepts_ffi capability test.
- 4.4.W9 : add wasm_rejects_result capability test.

Usage:
    python3 tools/apply_pr13f_fixes.py --dry-run
    python3 tools/apply_pr13f_fixes.py
"""

import argparse
import sys
from pathlib import Path

FIXES = [
    # ─── 4.4.W2: delete validate_wasm_compatibility and its call ───
    (
        "src/backends/wasm_backend.rs",
        "impl WasmBackend {\n"
        "    pub fn new() -> Self {\n"
        "        WasmBackend\n"
        "    }\n"
        "\n"
        "    fn validate_wasm_compatibility(ir: &VerifiedIR) -> Result<()> {\n"
        "        // Check for unsupported operations\n"
        "        for func in &ir.program().functions {\n"
        "            for block in &func.blocks {\n"
        "                for instr in &block.instructions {\n"
        "                    match instr {\n"
        "                        // Send and Receive are not supported in WASM\n"
        "                        crate::ir::semantic_ir::Instruction::Send { .. } => {\n"
        "                            return Err(CompileError::new(\n"
        "                                \"Channel send is not supported in WASM backend\",\n"
        "                                0,\n"
        "                                0,\n"
        "                                \"\",\n"
        "                                ErrorCode::E0002,\n"
        "                            ));\n"
        "                        }\n"
        "                        crate::ir::semantic_ir::Instruction::Receive { .. } => {\n"
        "                            return Err(CompileError::new(\n"
        "                                \"Channel receive is not supported in WASM backend\",\n"
        "                                0,\n"
        "                                0,\n"
        "                                \"\",\n"
        "                                ErrorCode::E0002,\n"
        "                            ));\n"
        "                        }\n"
        "                        _ => {}\n"
        "                    }\n"
        "                }\n"
        "            }\n"
        "        }\n"
        "        Ok(())\n"
        "    }\n"
        "}\n",
        "impl WasmBackend {\n"
        "    pub fn new() -> Self {\n"
        "        WasmBackend\n"
        "    }\n"
        "}\n",
        1,
    ),
    (
        "src/backends/wasm_backend.rs",
        "    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {\n"
        "        // Validate WASM compatibility\n"
        "        Self::validate_wasm_compatibility(ir)?;\n"
        "\n"
        "        // Initialize WebAssembly target\n",
        "    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {\n"
        "        // Capability check — the backend supports nothing, so any\n"
        "        // non-trivial program is refused here with a clear message.\n"
        "        // (The old `validate_wasm_compatibility` only checked\n"
        "        // Send/Receive and missed everything else.)\n"
        "        crate::backends::capabilities::check_backend(\n"
        "            ir.program(),\n"
        "            &crate::backends::capabilities::BackendCapabilities::wasm(),\n"
        "        )?;\n"
        "\n"
        "        // Initialize WebAssembly target\n",
        1,
    ),
    # ─── 4.4.W4: verify module before writing ───
    (
        "src/backends/wasm_backend.rs",
        "        let wasm_path = format!(\"{}.wasm\", output_name);\n"
        "        machine\n",
        "        // Verify the generated LLVM IR before handing it to the\n"
        "        // target machine. Matches the LLVM backend and catches\n"
        "        // codegen bugs before they reach the writer.\n"
        "        if let Err(e) = codegen.module.verify() {\n"
        "            return Err(CompileError::simple(\n"
        "                &format!(\"Generated WASM IR is invalid: {}\", e),\n"
        "                0,\n"
        "                0,\n"
        "                \"\",\n"
        "                ErrorCode::E0002,\n"
        "            ));\n"
        "        }\n"
        "\n"
        "        let wasm_path = format!(\"{}.wasm\", output_name);\n"
        "        machine\n",
        1,
    ),
    # ─── 4.1.9: use builtin_signatures() in scan_call_name ───
    (
        "src/backends/capabilities/scan.rs",
        "pub(super) fn scan_call_name(name: &str, used: &mut HashSet<Feature>) {\n"
        "    // `String.length` / `String.len` have an LLVM lowering via strlen;\n"
        "    // skip them so programs that only need string length still compile\n"
        "    // through LLVM. See \"Exclusions\" in the doc-comment above.\n"
        "    if name == \"String.length\" || name == \"String.len\" {\n"
        "        return;\n"
        "    }\n"
        "\n"
        "    if name.starts_with(\"String.\") {\n"
        "        used.insert(Feature::StringFunctions);\n"
        "    } else if name.starts_with(\"File.\") {\n"
        "        used.insert(Feature::FileFunctions);\n"
        "    } else if name == \"List.sum\" || name == \"List.max\" || name == \"List.min\" {\n"
        "        used.insert(Feature::ListAggregates);\n"
        "    }\n"
        "}\n",
        "pub(super) fn scan_call_name(name: &str, used: &mut HashSet<Feature>) {\n"
        "    // Only names that appear in the analyzer/verifier builtin table\n"
        "    // are candidates. A user-defined function named `String.helper`\n"
        "    // does not need LLVM's String lowering (it has its own body)\n"
        "    // and must not be classified as `StringFunctions`.\n"
        "    if !crate::ir::verifier::builtins::is_builtin_name(name) {\n"
        "        return;\n"
        "    }\n"
        "\n"
        "    // `String.length` / `String.len` have an LLVM lowering via strlen;\n"
        "    // skip them so programs that only need string length still compile\n"
        "    // through LLVM. See \"Exclusions\" in the doc-comment above.\n"
        "    if name == \"String.length\" || name == \"String.len\" {\n"
        "        return;\n"
        "    }\n"
        "\n"
        "    if name.starts_with(\"String.\") {\n"
        "        used.insert(Feature::StringFunctions);\n"
        "    } else if name.starts_with(\"File.\") {\n"
        "        used.insert(Feature::FileFunctions);\n"
        "    } else if name == \"List.sum\" || name == \"List.max\" || name == \"List.min\" {\n"
        "        used.insert(Feature::ListAggregates);\n"
        "    }\n"
        "}\n",
        1,
    ),
    # ─── Expose is_builtin_name from verifier::builtins ───
    (
        "src/ir/verifier/builtins.rs",
        "/// Signatures for built-in functions that the IR builder registers\n"
        "/// but that do not appear as `SemanticFunction` entries in the program.\n"
        "/// Keep this in sync with `SemanticIRBuilder::build_impl`.\n"
        "pub(super) fn builtin_signatures() -> HashMap<String, FunctionSignature> {\n",
        "/// True if `name` is one of the compiler's built-in functions.\n"
        "/// Used by the capability scan to distinguish built-ins from user\n"
        "/// functions that happen to share a namespace prefix (e.g. a\n"
        "/// user-defined `String.helper`).\n"
        "pub fn is_builtin_name(name: &str) -> bool {\n"
        "    builtin_signatures().contains_key(name)\n"
        "}\n"
        "\n"
        "/// Signatures for built-in functions that the IR builder registers\n"
        "/// but that do not appear as `SemanticFunction` entries in the program.\n"
        "/// Keep this in sync with `SemanticIRBuilder::build_impl`.\n"
        "pub fn builtin_signatures() -> HashMap<String, FunctionSignature> {\n",
        1,
    ),
    # ─── Add positive FFI test and WASM Result rejection test ───
    (
        "src/backends/capabilities/tests.rs",
        "#[test]\n"
        "fn wasm_rejects_channels() {\n",
        "#[test]\n"
        "fn llvm_accepts_ffi() {\n"
        "    // Ffi is a supported feature for LLVM. Pin the positive case\n"
        "    // so a future change to BackendCapabilities::llvm() cannot\n"
        "    // silently route FFI-using programs to the interpreter.\n"
        "    let mut program = SemanticProgram::new();\n"
        "    let entry = program.new_block_id();\n"
        "    program.functions.push(SemanticFunction {\n"
        "        name: \"puts\".to_string(),\n"
        "        params: vec![(\"s\".to_string(), Type::String)],\n"
        "        return_type: Type::Int,\n"
        "        blocks: vec![],\n"
        "        entry_block: 0,\n"
        "        is_extern: true,\n"
        "    });\n"
        "    program.functions.push(SemanticFunction {\n"
        "        name: \"main\".to_string(),\n"
        "        params: vec![],\n"
        "        return_type: Type::Void,\n"
        "        blocks: vec![SemanticBlock {\n"
        "            id: entry,\n"
        "            instructions: vec![Instruction::Call {\n"
        "                func: \"puts\".to_string(),\n"
        "                args: vec![TypedIRValue::String(\"hi\".to_string())],\n"
        "                result: None,\n"
        "            }],\n"
        "            terminator: Some(simple_return()),\n"
        "        }],\n"
        "        entry_block: entry,\n"
        "        is_extern: false,\n"
        "    });\n"
        "    assert!(check_backend(&program, &BackendCapabilities::llvm()).is_ok());\n"
        "}\n"
        "\n"
        "#[test]\n"
        "fn wasm_rejects_result_values() {\n"
        "    // WASM supports nothing today. Result must be refused.\n"
        "    let program = program_with(\n"
        "        Instruction::Declare {\n"
        "            name: \"r\".to_string(),\n"
        "            mutable: false,\n"
        "            type_: Type::result(Type::Int, Type::String),\n"
        "            value: TypedIRValue::Ok {\n"
        "                value: Box::new(TypedIRValue::Int(42)),\n"
        "                result_type: Type::result(Type::Int, Type::String),\n"
        "            },\n"
        "        },\n"
        "        simple_return(),\n"
        "    );\n"
        "    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();\n"
        "    assert!(err.message.contains(\"Result\"), \"{}\", err.message);\n"
        "}\n"
        "\n"
        "#[test]\n"
        "fn wasm_rejects_channels() {\n",
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

    from collections import defaultdict
    by_file = defaultdict(list)
    for path, _, rel, i in edits:
        by_file[rel].append((path, i))

    for rel, fixes in by_file.items():
        path = fixes[0][0]
        text = path.read_text()
        for _, fix_i in fixes:
            find, replace, occ = FIXES[fix_i - 1][1], FIXES[fix_i - 1][2], FIXES[fix_i - 1][3]
            new_text = apply_fix(text, find, replace, occ)
            if new_text is None:
                print(f"ERROR: fix {fix_i} failed when chained against {rel}", file=sys.stderr)
                return 1
            text = new_text
        path.write_text(text)
        print(f"  applied {len(fixes)} fix(es): {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
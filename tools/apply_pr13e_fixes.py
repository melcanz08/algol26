#!/usr/bin/env python3
"""
PR-13e: unify the LLVM production path through the Backend trait.

- Enrich BackendOutput to carry real data (paths, stdout).
- LlvmBackend::compile now:
    * runs check_backend (moved from Compiler::lower_to_llvm)
    * verifies IR BEFORE writing to disk
    * uses .with_extension("ll") to match the old path convention
    * returns BackendOutput::LlvmIr { path }
- Compiler::lower_to_llvm becomes a thin wrapper around
  LlvmBackend::compile + linking/running.
- InterpreterBackend and WasmBackend updated to the new variant
  shapes.

Usage:
    python3 tools/apply_pr13e_fixes.py --dry-run
    python3 tools/apply_pr13e_fixes.py
"""

import argparse
import sys
from pathlib import Path

FIXES = [
    # ─── 1. Enrich BackendOutput ───
    (
        "src/backends/backend.rs",
        "use crate::common::diagnostics::Result;\n"
        "use crate::ir::verified_ir::VerifiedIR;\n"
        "\n"
        "/// Represents the output of a backend compilation\n"
        "#[derive(Debug, Clone)]\n"
        "pub enum BackendOutput {\n"
        "    /// LLVM IR was generated\n"
        "    LlvmIr,\n"
        "    /// Native executable was produced\n"
        "    NativeExecutable,\n"
        "    /// Interpreter execution completed\n"
        "    InterpreterOutput,\n"
        "    /// WASM module was generated\n"
        "    WasmModule,\n"
        "}\n",
        "use crate::common::diagnostics::Result;\n"
        "use crate::ir::verified_ir::VerifiedIR;\n"
        "use std::path::PathBuf;\n"
        "\n"
        "/// Represents the output of a backend compilation.\n"
        "///\n"
        "/// Each variant carries the data the compiler driver needs to\n"
        "/// continue: the path to the emitted `.ll` or `.wasm` file,\n"
        "/// the interpreter's captured stdout, and so on. Before PR-13e\n"
        "/// these were unit variants and callers reconstructed the path\n"
        "/// or output out-of-band — which is why the trait was not the\n"
        "/// real integration point.\n"
        "#[derive(Debug, Clone)]\n"
        "pub enum BackendOutput {\n"
        "    /// LLVM IR was written to `path`.\n"
        "    LlvmIr { path: PathBuf },\n"
        "    /// Native executable was produced at `path`.\n"
        "    NativeExecutable { path: PathBuf },\n"
        "    /// Interpreter execution completed with this stdout.\n"
        "    InterpreterOutput { stdout: String },\n"
        "    /// WASM module was written to `path`.\n"
        "    WasmModule { path: PathBuf },\n"
        "}\n",
        1,
    ),

    # ─── 2. LlvmBackend::compile — full impl replacement ───
    (
        "src/backends/llvm_backend.rs",
        "impl Backend for LlvmBackend {\n"
        "    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {\n"
        "        let context = Context::create();\n"
        "        let mut codegen = IRCodeGen::new(&context, \"algol26_module\");\n"
        "\n"
        "        codegen.compile(ir.program()).map_err(|e| {\n"
        "            let error_msg = format!(\"Codegen failed: {}\", e);\n"
        "            e.display();\n"
        "            CompileError::simple(&error_msg, 0, 0, \"\", ErrorCode::E0002)\n"
        "        })?;\n"
        "\n"
        "        let ir_path = format!(\"{}.ll\", output_name);\n"
        "        codegen.module.print_to_file(&ir_path).map_err(|e| {\n"
        "            CompileError::simple(\n"
        "                &format!(\"Failed to emit LLVM IR to {}: {}\", ir_path, e),\n"
        "                0,\n"
        "                0,\n"
        "                \"\",\n"
        "                ErrorCode::E0001,\n"
        "            )\n"
        "        })?;\n"
        "\n"
        "        // Verify the generated IR is valid\n"
        "        if let Err(e) = codegen.module.verify() {\n"
        "            return Err(CompileError::simple(\n"
        "                &format!(\"Generated LLVM IR is invalid: {}\", e),\n"
        "                0,\n"
        "                0,\n"
        "                \"\",\n"
        "                ErrorCode::E0002,\n"
        "            ));\n"
        "        }\n"
        "\n"
        "        Ok(BackendOutput::LlvmIr)\n"
        "    }\n"
        "\n"
        "    fn name(&self) -> &str {\n"
        "        \"llvm\"\n"
        "    }\n"
        "    fn description(&self) -> &str {\n"
        "        \"LLVM IR from SemanticProgram with validation\"\n"
        "    }\n"
        "    fn can_execute(&self) -> bool {\n"
        "        true\n"
        "    }\n"
        "}\n",
        "impl Backend for LlvmBackend {\n"
        "    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {\n"
        "        // Capability check: refuse programs that use features\n"
        "        // LLVM cannot lower. Moved here from\n"
        "        // `Compiler::lower_to_llvm` so every caller of the trait\n"
        "        // gets the check and the trait becomes the enforcement\n"
        "        // point rather than an optional wrapper.\n"
        "        crate::backends::capabilities::check_backend(\n"
        "            ir.program(),\n"
        "            &crate::backends::capabilities::BackendCapabilities::llvm(),\n"
        "        )?;\n"
        "\n"
        "        let context = Context::create();\n"
        "        let mut codegen = IRCodeGen::new(&context, \"algol26_module\");\n"
        "\n"
        "        codegen.compile(ir.program()).map_err(|e| {\n"
        "            let error_msg = format!(\"Codegen failed: {}\", e);\n"
        "            e.display();\n"
        "            CompileError::simple(&error_msg, 0, 0, \"\", ErrorCode::E0002)\n"
        "        })?;\n"
        "\n"
        "        // Verify BEFORE writing. If verification fails, the `.ll`\n"
        "        // file never lands on disk and a user's `clang bad.ll`\n"
        "        // follow-up cannot accidentally run against invalid IR.\n"
        "        if let Err(e) = codegen.module.verify() {\n"
        "            return Err(CompileError::simple(\n"
        "                &format!(\"Generated LLVM IR is invalid: {}\", e),\n"
        "                0,\n"
        "                0,\n"
        "                \"\",\n"
        "                ErrorCode::E0002,\n"
        "            ));\n"
        "        }\n"
        "\n"
        "        // `.with_extension(\"ll\")` so `foo.bar` produces `foo.ll`,\n"
        "        // matching what `Compiler::lower_to_llvm` used to do.\n"
        "        let ir_path = std::path::PathBuf::from(output_name).with_extension(\"ll\");\n"
        "        codegen.module.print_to_file(&ir_path).map_err(|e| {\n"
        "            CompileError::simple(\n"
        "                &format!(\"Failed to emit LLVM IR to {}: {}\", ir_path.display(), e),\n"
        "                0,\n"
        "                0,\n"
        "                \"\",\n"
        "                ErrorCode::E0001,\n"
        "            )\n"
        "        })?;\n"
        "\n"
        "        println!(\"[Generated LLVM IR: {}]\", ir_path.display());\n"
        "\n"
        "        Ok(BackendOutput::LlvmIr { path: ir_path })\n"
        "    }\n"
        "\n"
        "    fn name(&self) -> &str {\n"
        "        \"llvm\"\n"
        "    }\n"
        "    fn description(&self) -> &str {\n"
        "        \"LLVM IR from SemanticProgram with validation\"\n"
        "    }\n"
        "    fn can_execute(&self) -> bool {\n"
        "        true\n"
        "    }\n"
        "}\n",
        1,
    ),

    # ─── 3. InterpreterBackend — return the stdout in the enum ───
    (
        "src/backends/interpreter_backend.rs",
        "        // Store output\n"
        "        let mut buffer = self.output_buffer.lock().expect(\"interpreter output lock poisoned\");\n"
        "        if output.is_empty() {\n"
        "            buffer.clear();\n"
        "        } else {\n"
        "            *buffer = format!(\"{}\\n\", output).into_bytes();\n"
        "        }\n"
        "\n"
        "        Ok(BackendOutput::InterpreterOutput)\n",
        "        let stdout = if output.is_empty() {\n"
        "            String::new()\n"
        "        } else {\n"
        "            format!(\"{}\\n\", output)\n"
        "        };\n"
        "\n"
        "        // Keep the legacy buffer in sync so existing callers\n"
        "        // using `get_output()` continue to work; the returned\n"
        "        // enum now also carries the same string for new callers.\n"
        "        let mut buffer = self.output_buffer.lock().expect(\"interpreter output lock poisoned\");\n"
        "        buffer.clear();\n"
        "        buffer.extend_from_slice(stdout.as_bytes());\n"
        "\n"
        "        Ok(BackendOutput::InterpreterOutput { stdout })\n",
        1,
    ),

    # ─── 4. WasmBackend — return the path in the enum ───
    (
        "src/backends/wasm_backend.rs",
        "        println!(\"[Generated WASM: {}]\", wasm_path);\n"
        "\n"
        "        Ok(BackendOutput::WasmModule)\n",
        "        println!(\"[Generated WASM: {}]\", wasm_path);\n"
        "\n"
        "        Ok(BackendOutput::WasmModule {\n"
        "            path: std::path::PathBuf::from(wasm_path),\n"
        "        })\n",
        1,
    ),

    # ─── 5. Compiler::lower_to_llvm — thin wrapper around the trait ───
    (
        "src/compiler.rs",
        "    fn lower_to_llvm(\n"
        "        &self,\n"
        "        verified: &VerifiedIR,\n"
        "        filename: &str,\n"
        "        output_name: &str,\n"
        "        emit_llvm: bool,\n"
        "        run_after_compile: bool,\n"
        "    ) -> Result<()> {\n"
        "        let program = verified.program();\n"
        "\n"
        "        crate::backends::capabilities::check_backend(\n"
        "            program,\n"
        "            &crate::backends::capabilities::BackendCapabilities::llvm(),\n"
        "        )?;\n"
        "\n"
        "        let context = Context::create();\n"
        "        let mut codegen = IRCodeGen::new(&context, \"algol26_module\");\n"
        "\n"
        "        codegen.compile(program).map_err(|e| {\n"
        "            e.display();\n"
        "            CompileError::simple(\"Code generation failed\", 0, 0, \"\", ErrorCode::E0002)\n"
        "        })?;\n"
        "\n"
        "        let ir_path = PathBuf::from(output_name).with_extension(\"ll\");\n"
        "        codegen.module.print_to_file(&ir_path).map_err(|e| {\n"
        "            let err = CompileError::simple(\n"
        "                &format!(\"Failed to emit LLVM IR: {}\", e),\n"
        "                0, 0, \"\", ErrorCode::E0001,\n"
        "            );\n"
        "            err.display();\n"
        "            err\n"
        "        })?;\n"
        "\n"
        "        println!(\"[Generated LLVM IR: {}]\", ir_path.display());\n"
        "\n"
        "        if emit_llvm {\n"
        "            return Ok(());\n"
        "        }\n"
        "\n"
        "        let output_path = crate::toolchain::link_llvm_ir(&ir_path, output_name)?;\n"
        "        println!(\"[Successfully compiled to {}]\", output_path.display());\n"
        "\n"
        "        if run_after_compile {\n"
        "            crate::toolchain::run_binary(&output_path)?;\n"
        "        }\n"
        "\n"
        "        Ok(())\n"
        "    }\n",
        "    fn lower_to_llvm(\n"
        "        &self,\n"
        "        verified: &VerifiedIR,\n"
        "        _filename: &str,\n"
        "        output_name: &str,\n"
        "        emit_llvm: bool,\n"
        "        run_after_compile: bool,\n"
        "    ) -> Result<()> {\n"
        "        use crate::backends::backend::{Backend, BackendOutput};\n"
        "        use crate::backends::llvm_backend::LlvmBackend;\n"
        "\n"
        "        // Delegate LLVM emission to the backend trait. The same\n"
        "        // code path is exercised by `tests/backends/`, so\n"
        "        // `module.verify()` and the capability check both run on\n"
        "        // the production path. Before PR-13e this function\n"
        "        // reimplemented the codegen inline and never called\n"
        "        // `verify()`.\n"
        "        let backend = LlvmBackend::new();\n"
        "        let ir_path = match backend.compile(verified, output_name)? {\n"
        "            BackendOutput::LlvmIr { path } => path,\n"
        "            other => {\n"
        "                return Err(CompileError::simple(\n"
        "                    &format!(\"LlvmBackend returned unexpected output: {:?}\", other),\n"
        "                    0,\n"
        "                    0,\n"
        "                    \"\",\n"
        "                    ErrorCode::E0009,\n"
        "                ));\n"
        "            }\n"
        "        };\n"
        "\n"
        "        if emit_llvm {\n"
        "            return Ok(());\n"
        "        }\n"
        "\n"
        "        let output_path = crate::toolchain::link_llvm_ir(&ir_path, output_name)?;\n"
        "        println!(\"[Successfully compiled to {}]\", output_path.display());\n"
        "\n"
        "        if run_after_compile {\n"
        "            crate::toolchain::run_binary(&output_path)?;\n"
        "        }\n"
        "\n"
        "        Ok(())\n"
        "    }\n",
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
                f"  First 120 chars: {find[:120]!r}",
                file=sys.stderr,
            )
            return 1
        edits.append((path, new_text, rel, i))

    if args.dry_run:
        print(f"Dry run — {len(edits)} fixes across:")
        for _, _, rel, _ in edits:
            print(f"  {rel}")
        return 0

    for path, new_text, rel, i in edits:
        path.write_text(new_text)
        print(f"  applied fix {i}: {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
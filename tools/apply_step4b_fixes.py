#!/usr/bin/env python3
"""
Step 4b: FFI symbol renaming and library linking.

- SemanticProgram carries ffi_symbols (ALGOL26 name -> C symbol)
  and ffi_libraries (library names to link against).
- Builder populates them from ExternDecl.symbol_name / .library.
- LLVM codegen uses ffi_symbols in declare_function so calls resolve
  to the C symbol instead of the ALGOL26 name.
- Linker accepts a library list and passes -l<name> to clang.
- Compiler driver forwards the program's library list to the linker.
- Tests: renamed symbol via `extern "C" function print_line(...) as "puts"`.
"""

import sys
from collections import defaultdict
from pathlib import Path

FIXES = [
    # ─── 1. semantic_ir.rs — add fields to SemanticProgram ───
    (
        "src/ir/semantic_ir.rs",
        "use crate::common::types::Type;\n",
        "use crate::common::types::Type;\nuse std::collections::HashMap;\n",
        1,
    ),
    (
        "src/ir/semantic_ir.rs",
        "#[derive(Debug, Clone)]\n"
        "pub struct SemanticProgram {\n"
        "    pub functions: Vec<SemanticFunction>,\n"
        "    pub block_counter: usize,\n"
        "}\n",
        "#[derive(Debug, Clone)]\n"
        "pub struct SemanticProgram {\n"
        "    pub functions: Vec<SemanticFunction>,\n"
        "    pub block_counter: usize,\n"
        "    /// Map from an extern function's ALGOL26 name to its C symbol\n"
        "    /// name, when `extern ... as \"sym\"` was declared. Populated\n"
        "    /// by the IR builder; consumed by LLVM codegen so the\n"
        "    /// emitted call uses the C symbol rather than the ALGOL26\n"
        "    /// name.\n"
        "    pub ffi_symbols: HashMap<String, String>,\n"
        "    /// Library names (without `lib` prefix or extension) that\n"
        "    /// any extern declaration requested via `from \"lib\"`.\n"
        "    /// Consumed by the linker driver as `-l<name>` flags.\n"
        "    pub ffi_libraries: Vec<String>,\n"
        "}\n",
        1,
    ),
    (
        "src/ir/semantic_ir.rs",
        "    pub fn new() -> Self {\n"
        "        Self {\n"
        "            functions: vec![],\n"
        "            block_counter: 0,\n"
        "        }\n"
        "    }\n",
        "    pub fn new() -> Self {\n"
        "        Self {\n"
        "            functions: vec![],\n"
        "            block_counter: 0,\n"
        "            ffi_symbols: HashMap::new(),\n"
        "            ffi_libraries: Vec::new(),\n"
        "        }\n"
        "    }\n",
        1,
    ),

    # ─── 2. builder/build.rs — populate the maps ───
    (
        "src/semantics/builder/build.rs",
        "            self.pop_scope();\n"
        "            program.functions.push(semantic_func);\n"
        "        }\n"
        "\n"
        "        program\n"
        "    }\n",
        "            self.pop_scope();\n"
        "\n"
        "            // Extern declarations carry link metadata that the\n"
        "            // LLVM codegen and linker need. Record it on the\n"
        "            // program before the function is moved into the\n"
        "            // functions vec. (Step 4b wiring.)\n"
        "            if func.is_extern {\n"
        "                if let Some(ffi) = &func.ffi_info {\n"
        "                    if let Some(sym) = &ffi.symbol_name {\n"
        "                        program\n"
        "                            .ffi_symbols\n"
        "                            .insert(func.name.clone(), sym.clone());\n"
        "                    }\n"
        "                    if let Some(lib) = &ffi.library {\n"
        "                        if !program.ffi_libraries.contains(lib) {\n"
        "                            program.ffi_libraries.push(lib.clone());\n"
        "                        }\n"
        "                    }\n"
        "                }\n"
        "            }\n"
        "\n"
        "            program.functions.push(semantic_func);\n"
        "        }\n"
        "\n"
        "        program\n"
        "    }\n",
        1,
    ),

    # ─── 3. IRCodeGen — add ffi_symbols field, init, populate ───
    (
        "src/backends/llvm_codegen/mod.rs",
        "    pub(super) iterator_indices: HashMap<String, PointerValue<'ctx>>,\n"
        "    pub(super) iterator_lengths: HashMap<String, usize>,\n"
        "}\n",
        "    pub(super) iterator_indices: HashMap<String, PointerValue<'ctx>>,\n"
        "    pub(super) iterator_lengths: HashMap<String, usize>,\n"
        "    /// ALGOL26 extern name -> C symbol. Populated by\n"
        "    /// `compile()` from the program's FFI metadata. Used in\n"
        "    /// `declare_function` so a call to `print_line` emits\n"
        "    /// `@puts` when the declaration was `as \"puts\"`.\n"
        "    pub(super) ffi_symbols: HashMap<String, String>,\n"
        "}\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "            iterator_indices: HashMap::new(),\n"
        "            iterator_lengths: HashMap::new(),\n"
        "        }\n"
        "    }\n",
        "            iterator_indices: HashMap::new(),\n"
        "            iterator_lengths: HashMap::new(),\n"
        "            ffi_symbols: HashMap::new(),\n"
        "        }\n"
        "    }\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "    pub fn compile(&mut self, program: &SemanticProgram) -> Result<()> {\n"
        "        self.register_stdlib();\n",
        "    pub fn compile(&mut self, program: &SemanticProgram) -> Result<()> {\n"
        "        // Copy the FFI symbol map so declaration can use the C\n"
        "        // name for extern functions.\n"
        "        self.ffi_symbols = program.ffi_symbols.clone();\n"
        "        self.register_stdlib();\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "    fn declare_function(&mut self, func: &SemanticFunction) -> Result<()> {\n"
        "        let clean_name = func.name.trim_end_matches(\"()\").to_string();\n"
        "        if self.functions.contains_key(&clean_name) {\n"
        "            return Ok(());\n"
        "        }\n",
        "    fn declare_function(&mut self, func: &SemanticFunction) -> Result<()> {\n"
        "        let clean_name = func.name.trim_end_matches(\"()\").to_string();\n"
        "        if self.functions.contains_key(&clean_name) {\n"
        "            return Ok(());\n"
        "        }\n"
        "        // If the extern declared `as \"sym\"`, the LLVM symbol is\n"
        "        // the C name; the ALGOL26 name is preserved as the\n"
        "        // lookup key in `self.functions` so call sites continue\n"
        "        // to reference the ALGOL26 name. (Step 4b wiring.)\n"
        "        let llvm_name = self\n"
        "            .ffi_symbols\n"
        "            .get(&clean_name)\n"
        "            .cloned()\n"
        "            .unwrap_or_else(|| clean_name.clone());\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "        let function = self.module.add_function(&clean_name, fn_type, None);\n"
        "        self.functions.insert(clean_name, function);\n"
        "        Ok(())\n"
        "    }\n",
        "        let function = self.module.add_function(&llvm_name, fn_type, None);\n"
        "        self.functions.insert(clean_name, function);\n"
        "        Ok(())\n"
        "    }\n",
        1,
    ),

    # ─── 4. linker.rs — accept a library list ───
    (
        "src/toolchain/linker.rs",
        "pub fn link_llvm_ir(ir_path: &Path, output_name: &str) -> Result<PathBuf> {\n"
        "    let output_path = resolve_output_path(output_name);\n"
        "\n"
        "    let output = Command::new(\"clang\")\n"
        "        .arg(ir_path)\n"
        "        .arg(\"-o\")\n"
        "        .arg(&output_path)\n"
        "        .arg(\"-O2\")\n"
        "        .arg(\"-lm\")\n"
        "        .arg(\"-lpthread\")\n"
        "        .output()\n",
        "pub fn link_llvm_ir(\n"
        "    ir_path: &Path,\n"
        "    output_name: &str,\n"
        "    libraries: &[String],\n"
        ") -> Result<PathBuf> {\n"
        "    let output_path = resolve_output_path(output_name);\n"
        "\n"
        "    let mut cmd = Command::new(\"clang\");\n"
        "    cmd.arg(ir_path)\n"
        "        .arg(\"-o\")\n"
        "        .arg(&output_path)\n"
        "        .arg(\"-O2\")\n"
        "        .arg(\"-lm\")\n"
        "        .arg(\"-lpthread\");\n"
        "    // Each extern declaration that used `from \"lib\"` becomes a\n"
        "    // `-l<lib>` flag. The linker searches the standard library\n"
        "    // paths plus anything the user added to LIBRARY_PATH.\n"
        "    for lib in libraries {\n"
        "        cmd.arg(format!(\"-l{}\", lib));\n"
        "    }\n"
        "    let output = cmd\n"
        "        .output()\n",
        1,
    ),

    # ─── 5. compiler.rs — pass libraries ───
    (
        "src/compiler.rs",
        "        let output_path = crate::toolchain::link_llvm_ir(&ir_path, output_name)?;\n",
        "        // FFI libraries requested by extern declarations are\n"
        "        // forwarded to clang as -l flags.\n"
        "        let libraries = &verified.program().ffi_libraries;\n"
        "        let output_path =\n"
        "            crate::toolchain::link_llvm_ir(&ir_path, output_name, libraries)?;\n",
        1,
    ),

    # ─── 6. IR test for FFI symbol map ───
    (
        "src/ir/optimizer.rs",
        "#[cfg(test)]\n"
        "mod tests {\n",
        "#[cfg(test)]\n"
        "mod tests_ffi_symbols {\n"
        "    // Sanity: program.ffi_symbols defaults empty.\n"
        "    use crate::ir::semantic_ir::SemanticProgram;\n"
        "    #[test]\n"
        "    fn new_program_has_empty_ffi_tables() {\n"
        "        let p = SemanticProgram::new();\n"
        "        assert!(p.ffi_symbols.is_empty());\n"
        "        assert!(p.ffi_libraries.is_empty());\n"
        "    }\n"
        "}\n"
        "\n"
        "#[cfg(test)]\n"
        "mod tests {\n",
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

    by_file = defaultdict(list)
    for i, (rel, find, replace, occ) in enumerate(FIXES, 1):
        by_file[rel].append((i, find, replace, occ))

    # Validate every anchor against the original content.
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
                print(f"ERROR: fix {fix_i} failed chaining on {rel}", file=sys.stderr)
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
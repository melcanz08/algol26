#!/usr/bin/env python3
"""
Variadic FFI args: allow extern "C" functions declared with `...`
to be called with more arguments than the fixed parameter list.

Four layers:
  1. Analyzer tracks which functions are variadic and relaxes the
     arity check.
  2. SemanticProgram carries the flag so downstream layers can see it.
  3. LLVM codegen declares the extern function as variadic so LLVM
     accepts the extra args at the call site.
  4. IR verifier relaxes its own arity check for variadic functions.
"""

import sys
from collections import defaultdict
from pathlib import Path

FIXES = [
    # ─── 1. SemanticAnalyzer: variadic set field ───
    (
        "src/semantics/analyzer/mod.rs",
        "    deferred_captures: Vec<HashSet<String>>,\n"
        "    null_bindings: Vec<HashSet<String>>,\n",
        "    deferred_captures: Vec<HashSet<String>>,\n"
        "    null_bindings: Vec<HashSet<String>>,\n"
        "    /// Function names declared variadic via `extern \"C\" ...(...)`.\n"
        "    /// Used to relax the arity check from \"exactly N\" to \"at\n"
        "    /// least N\" for those functions. (Variadic FFI PR.)\n"
        "    variadic_functions: HashSet<String>,\n",
        1,
    ),
    (
        "src/semantics/analyzer/mod.rs",
        "            deferred_captures: vec![HashSet::new()],\n"
        "            null_bindings: vec![HashSet::new()],\n",
        "            deferred_captures: vec![HashSet::new()],\n"
        "            null_bindings: vec![HashSet::new()],\n"
        "            variadic_functions: HashSet::new(),\n",
        1,
    ),

    # ─── 2. register_user_functions: populate the set ───
    (
        "src/semantics/analyzer/items.rs",
        "            let clean_name = func.name.trim_end_matches(\"()\").to_string();\n"
        "            self.functions.insert(clean_name, FunctionInfo { params, return_type });\n"
        "        }\n"
        "    }\n",
        "            let clean_name = func.name.trim_end_matches(\"()\").to_string();\n"
        "            if func.ffi_info.as_ref().is_some_and(|f| f.variadic) {\n"
        "                self.variadic_functions.insert(clean_name.clone());\n"
        "            }\n"
        "            self.functions.insert(clean_name, FunctionInfo { params, return_type });\n"
        "        }\n"
        "    }\n",
        1,
    ),

    # ─── 3. Analyzer FunctionCall: relax arity check ───
    (
        "src/semantics/analyzer/expr.rs",
        "                if args.len() != func_info.params.len() {\n"
        "                    return Err(CompileError::simple(\n"
        "                        &format!(\n"
        "                            \"Function '{}' expects {} arguments, got {}\",\n"
        "                            name, func_info.params.len(), args.len()\n"
        "                        ),\n"
        "                        0, 0, \"\", ErrorCode::E0002,\n"
        "                    ).with_suggestion(&format!(\n"
        "                        \"Provide exactly {} argument(s) to '{}'\",\n"
        "                        func_info.params.len(), name\n"
        "                    )));\n"
        "                }\n",
        "                // Variadic extern functions accept any number of\n"
        "                // arguments at or above the fixed count. Extra\n"
        "                // arguments are the variadic tail — their types\n"
        "                // are not checked (matching C). Non-variadic\n"
        "                // functions still require exact arity.\n"
        "                let is_variadic = self.variadic_functions.contains(clean_name);\n"
        "                let arity_ok = if is_variadic {\n"
        "                    args.len() >= func_info.params.len()\n"
        "                } else {\n"
        "                    args.len() == func_info.params.len()\n"
        "                };\n"
        "                if !arity_ok {\n"
        "                    let expected_msg = if is_variadic {\n"
        "                        format!(\"at least {} argument(s)\", func_info.params.len())\n"
        "                    } else {\n"
        "                        format!(\"exactly {} argument(s)\", func_info.params.len())\n"
        "                    };\n"
        "                    return Err(CompileError::simple(\n"
        "                        &format!(\n"
        "                            \"Function '{}' expects {}, got {}\",\n"
        "                            name, expected_msg, args.len()\n"
        "                        ),\n"
        "                        0, 0, \"\", ErrorCode::E0002,\n"
        "                    ).with_suggestion(&format!(\n"
        "                        \"Provide {} to '{}'\", expected_msg, name\n"
        "                    )));\n"
        "                }\n",
        1,
    ),

    # ─── 4. SemanticProgram: variadic set field ───
    (
        "src/ir/semantic_ir.rs",
        "    /// Library names (without `lib` prefix or extension) that\n"
        "    /// any extern declaration requested via `from \"lib\"`.\n"
        "    /// Consumed by the linker driver as `-l<name>` flags.\n"
        "    pub ffi_libraries: Vec<String>,\n"
        "}\n",
        "    /// Library names (without `lib` prefix or extension) that\n"
        "    /// any extern declaration requested via `from \"lib\"`.\n"
        "    /// Consumed by the linker driver as `-l<name>` flags.\n"
        "    pub ffi_libraries: Vec<String>,\n"
        "    /// Names of extern functions declared variadic\n"
        "    /// (`extern \"C\" function f(a: T, ...)`). Consumed by LLVM\n"
        "    /// codegen so the declared function type is variadic, and\n"
        "    /// by the IR verifier to relax its arity check.\n"
        "    pub variadic_functions: std::collections::HashSet<String>,\n"
        "}\n",
        1,
    ),
    (
        "src/ir/semantic_ir.rs",
        "            ffi_symbols: HashMap::new(),\n"
        "            ffi_libraries: Vec::new(),\n"
        "        }\n"
        "    }\n",
        "            ffi_symbols: HashMap::new(),\n"
        "            ffi_libraries: Vec::new(),\n"
        "            variadic_functions: std::collections::HashSet::new(),\n"
        "        }\n"
        "    }\n",
        1,
    ),

    # ─── 5. Builder: populate the set ───
    (
        "src/semantics/builder/build.rs",
        "                    if let Some(lib) = &ffi.library {\n"
        "                        if !program.ffi_libraries.contains(lib) {\n"
        "                            program.ffi_libraries.push(lib.clone());\n"
        "                        }\n"
        "                    }\n",
        "                    if let Some(lib) = &ffi.library {\n"
        "                        if !program.ffi_libraries.contains(lib) {\n"
        "                            program.ffi_libraries.push(lib.clone());\n"
        "                        }\n"
        "                    }\n"
        "                    if ffi.variadic {\n"
        "                        program.variadic_functions.insert(func.name.clone());\n"
        "                    }\n",
        1,
    ),

    # ─── 6. LLVM codegen: declare variadic externs correctly ───
    (
        "src/backends/llvm_codegen/mod.rs",
        "    /// ALGOL26 extern name -> C symbol. Populated by\n"
        "    /// `compile()` from the program's FFI metadata. Used in\n"
        "    /// `declare_function` so a call to `print_line` emits\n"
        "    /// `@puts` when the declaration was `as \"puts\"`.\n"
        "    pub(super) ffi_symbols: HashMap<String, String>,\n",
        "    /// ALGOL26 extern name -> C symbol. Populated by\n"
        "    /// `compile()` from the program's FFI metadata. Used in\n"
        "    /// `declare_function` so a call to `print_line` emits\n"
        "    /// `@puts` when the declaration was `as \"puts\"`.\n"
        "    pub(super) ffi_symbols: HashMap<String, String>,\n"
        "    /// Names of variadic extern functions. Used in\n"
        "    /// `declare_function` so the LLVM function type is variadic\n"
        "    /// and accepts the call's extra arguments.\n"
        "    pub(super) variadic_functions: std::collections::HashSet<String>,\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "            ffi_symbols: HashMap::new(),\n"
        "            region_frames: Vec::new(),\n"
        "        }\n"
        "    }\n",
        "            ffi_symbols: HashMap::new(),\n"
        "            variadic_functions: std::collections::HashSet::new(),\n"
        "            region_frames: Vec::new(),\n"
        "        }\n"
        "    }\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "        // Copy the FFI symbol map so declaration can use the C\n"
        "        // name for extern functions.\n"
        "        self.ffi_symbols = program.ffi_symbols.clone();\n",
        "        // Copy the FFI symbol map so declaration can use the C\n"
        "        // name for extern functions.\n"
        "        self.ffi_symbols = program.ffi_symbols.clone();\n"
        "        self.variadic_functions = program.variadic_functions.clone();\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "        let llvm_name = self\n"
        "            .ffi_symbols\n"
        "            .get(&clean_name)\n"
        "            .cloned()\n"
        "            .unwrap_or_else(|| clean_name.clone());\n",
        "        let llvm_name = self\n"
        "            .ffi_symbols\n"
        "            .get(&clean_name)\n"
        "            .cloned()\n"
        "            .unwrap_or_else(|| clean_name.clone());\n"
        "        // Variadic externs must be declared with LLVM's variadic\n"
        "        // bit set, otherwise LLVM rejects the extra call args.\n"
        "        let is_variadic = self.variadic_functions.contains(&clean_name);\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "        let fn_type = match func.return_type {\n"
        "            Type::Void => self.context.void_type().fn_type(&param_types, false),\n"
        "            Type::Int => self.context.i64_type().fn_type(&param_types, false),\n"
        "            Type::Bool => self.context.bool_type().fn_type(&param_types, false),\n"
        "            Type::String => self\n"
        "                .context\n"
        "                .ptr_type(AddressSpace::default())\n"
        "                .fn_type(&param_types, false),\n"
        "            _ => self.context.f64_type().fn_type(&param_types, false),\n"
        "        };\n",
        "        let fn_type = match func.return_type {\n"
        "            Type::Void => self.context.void_type().fn_type(&param_types, is_variadic),\n"
        "            Type::Int => self.context.i64_type().fn_type(&param_types, is_variadic),\n"
        "            Type::Bool => self.context.bool_type().fn_type(&param_types, is_variadic),\n"
        "            Type::String => self\n"
        "                .context\n"
        "                .ptr_type(AddressSpace::default())\n"
        "                .fn_type(&param_types, is_variadic),\n"
        "            _ => self.context.f64_type().fn_type(&param_types, is_variadic),\n"
        "        };\n",
        1,
    ),

    # ─── 7. Verifier FunctionSignature: variadic field ───
    (
        "src/ir/verifier/mod.rs",
        "#[derive(Debug, Clone)]\n"
        "pub(super) struct FunctionSignature {\n"
        "    params: Vec<(String, Type)>,\n"
        "    return_type: Type,\n"
        "}\n",
        "#[derive(Debug, Clone)]\n"
        "pub(super) struct FunctionSignature {\n"
        "    params: Vec<(String, Type)>,\n"
        "    return_type: Type,\n"
        "    /// True for `extern \"C\"` functions declared with `...`.\n"
        "    /// Callers may pass more arguments than `params.len()`;\n"
        "    /// the extra args are untyped (matching C's variadic ABI).\n"
        "    variadic: bool,\n"
        "}\n",
        1,
    ),
    (
        "src/ir/verifier/mod.rs",
        "    for func in &program.functions {\n"
        "        signatures.insert(\n"
        "            func.name.clone(),\n"
        "            FunctionSignature {\n"
        "                params: func.params.clone(),\n"
        "                return_type: func.return_type.clone(),\n"
        "            },\n"
        "        );\n"
        "    }\n",
        "    for func in &program.functions {\n"
        "        signatures.insert(\n"
        "            func.name.clone(),\n"
        "            FunctionSignature {\n"
        "                params: func.params.clone(),\n"
        "                return_type: func.return_type.clone(),\n"
        "                variadic: program.variadic_functions.contains(&func.name),\n"
        "            },\n"
        "        );\n"
        "    }\n",
        1,
    ),

    # ─── 8. builtins.rs: builtin signatures are non-variadic ───
    (
        "src/ir/verifier/builtins.rs",
        "    let mut reg = |name: &str, params: Vec<(&str, Type)>, ret: Type| {\n"
        "        m.insert(\n"
        "            name.to_string(),\n"
        "            FunctionSignature {\n"
        "                params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),\n"
        "                return_type: ret,\n"
        "            },\n"
        "        );\n"
        "    };\n",
        "    let mut reg = |name: &str, params: Vec<(&str, Type)>, ret: Type| {\n"
        "        m.insert(\n"
        "            name.to_string(),\n"
        "            FunctionSignature {\n"
        "                params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),\n"
        "                return_type: ret,\n"
        "                variadic: false,\n"
        "            },\n"
        "        );\n"
        "    };\n",
        1,
    ),

    # ─── 9. verify_instruction Call: relax arity for variadic ───
    (
        "src/ir/verifier/instruction.rs",
        "            let sig = env.function_sigs.get(callee).ok_or_else(|| {\n"
        "                format!(\n"
        "                    \"Function '{}': Call to undefined function '{}'\",\n"
        "                    func.name, callee\n"
        "                )\n"
        "            })?;\n"
        "\n"
        "            if arg_types.len() != sig.params.len() {\n"
        "                return Err(format!(\n"
        "                    \"Function '{}': Call to '{}' expects {} args, found {}\",\n"
        "                    func.name, callee, sig.params.len(), arg_types.len()\n"
        "                ));\n"
        "            }\n",
        "            let sig = env.function_sigs.get(callee).ok_or_else(|| {\n"
        "                format!(\n"
        "                    \"Function '{}': Call to undefined function '{}'\",\n"
        "                    func.name, callee\n"
        "                )\n"
        "            })?;\n"
        "\n"
        "            // Variadic functions accept any number of args at\n"
        "            // or above the fixed count. Non-variadic functions\n"
        "            // must match exactly.\n"
        "            let arity_ok = if sig.variadic {\n"
        "                arg_types.len() >= sig.params.len()\n"
        "            } else {\n"
        "                arg_types.len() == sig.params.len()\n"
        "            };\n"
        "            if !arity_ok {\n"
        "                let expected = if sig.variadic {\n"
        "                    format!(\"at least {}\", sig.params.len())\n"
        "                } else {\n"
        "                    format!(\"{}\", sig.params.len())\n"
        "                };\n"
        "                return Err(format!(\n"
        "                    \"Function '{}': Call to '{}' expects {} args, found {}\",\n"
        "                    func.name, callee, expected, arg_types.len()\n"
        "                ));\n"
        "            }\n",
        1,
    ),

    # ─── 10. verify_value Call: same relaxation ───
    (
        "src/ir/verifier/value.rs",
        "            let sig = env.function_sigs.get(function).ok_or_else(|| {\n"
        "                format!(\"Call to undefined function '{}' in verified IR\", function)\n"
        "            })?;\n"
        "\n"
        "            if arg_types.len() != sig.params.len() {\n"
        "                return Err(format!(\n"
        "                    \"Call to '{}' expects {} args, found {}\",\n"
        "                    function,\n"
        "                    sig.params.len(),\n"
        "                    arg_types.len()\n"
        "                ));\n"
        "            }\n",
        "            let sig = env.function_sigs.get(function).ok_or_else(|| {\n"
        "                format!(\"Call to undefined function '{}' in verified IR\", function)\n"
        "            })?;\n"
        "\n"
        "            // Variadic functions accept any number of args at or\n"
        "            // above the fixed count.\n"
        "            let arity_ok = if sig.variadic {\n"
        "                arg_types.len() >= sig.params.len()\n"
        "            } else {\n"
        "                arg_types.len() == sig.params.len()\n"
        "            };\n"
        "            if !arity_ok {\n"
        "                let expected = if sig.variadic {\n"
        "                    format!(\"at least {}\", sig.params.len())\n"
        "                } else {\n"
        "                    format!(\"{}\", sig.params.len())\n"
        "                };\n"
        "                return Err(format!(\n"
        "                    \"Call to '{}' expects {} args, found {}\",\n"
        "                    function, expected, arg_types.len()\n"
        "                ));\n"
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

    by_file = defaultdict(list)
    for i, (rel, find, replace, occ) in enumerate(FIXES, 1):
        by_file[rel].append((i, find, replace, occ))

    # Validate all anchors first.
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

    # Apply.
    for rel, fixes in by_file.items():
        path = repo / rel
        text = path.read_text()
        for fix_i, find, replace, occ in fixes:
            new_text = apply_fix(text, find, replace, occ)
            if new_text is None:
                print(f"ERROR: fix {fix_i} chaining failed on {rel}", file=sys.stderr)
                return 1
            text = new_text
        path.write_text(text)
        print(f"  applied {len(fixes)} fix(es): {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
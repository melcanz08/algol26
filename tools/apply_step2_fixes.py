#!/usr/bin/env python3
"""
Step 2: wire alloc/free end-to-end through the interpreter.

- Builder: emit Instruction::Allocate / Free for alloc/free calls.
- Interpreter: simulated heap (HashMap<usize, Vec<u8>>).
- Capability matrix: interpreter accepts RawMemory.
- Tests updated / added.

Usage:
    python3 tools/apply_step2_fixes.py --dry-run
    python3 tools/apply_step2_fixes.py
"""

import argparse
import sys
from collections import defaultdict
from pathlib import Path

FIXES = [
    # ─── FIX 1: intercept `val p := alloc(n)` in Stmt::VarDecl ───
    (
        "src/semantics/builder/expr.rs",
        "                if let Expr::List(elements, _) = value {\n"
        "                    self.list_values.insert(name.clone(), elements.clone());\n"
        "                }\n"
        "                let typed_value = self.translate_expr(program, func, current_block, value);\n",
        "                // `val p := alloc(n)` is a memory operation, not a\n"
        "                // generic call. Emit an `Allocate` instruction and\n"
        "                // declare the pointer in one step. (Step 2 wiring.)\n"
        "                if let Expr::FunctionCall { name: fn_name, args, .. } = value {\n"
        "                    if fn_name == \"alloc\" && args.len() == 1 {\n"
        "                        let size = self.translate_expr(\n"
        "                            program, func, current_block, &args[0],\n"
        "                        );\n"
        "                        let ptr_ty = Type::pointer(Type::Unknown);\n"
        "                        self.declare_var(name, ptr_ty.clone(), *mutable);\n"
        "                        self.safe_push_instruction(\n"
        "                            func,\n"
        "                            current_block,\n"
        "                            SemanticInstruction::Allocate {\n"
        "                                target: name.clone(),\n"
        "                                size,\n"
        "                                type_: ptr_ty,\n"
        "                            },\n"
        "                        );\n"
        "                        return FlowResult::Reachable(current_block);\n"
        "                    }\n"
        "                }\n"
        "\n"
        "                if let Expr::List(elements, _) = value {\n"
        "                    self.list_values.insert(name.clone(), elements.clone());\n"
        "                }\n"
        "                let typed_value = self.translate_expr(program, func, current_block, value);\n",
        1,
    ),

    # ─── FIX 2: intercept alloc(n)/free(p) as statements ───
    (
        "src/semantics/builder/expr.rs",
        "            Stmt::Expression(expr) => {\n"
        "                // A discarded function call must still execute its side\n"
        "                // effects. `translate_expr` for FunctionCall returns the\n"
        "                // value without pushing an instruction — the caller pushes\n"
        "                // it. For discarded calls, push with `result: None`.\n",
        "            Stmt::Expression(expr) => {\n"
        "                // alloc(n) / free(p) in statement position are memory\n"
        "                // operations, not generic calls. Intercept before the\n"
        "                // discarded-call path below. (Step 2 wiring.)\n"
        "                if let Expr::FunctionCall { name: fn_name, args, .. } = expr {\n"
        "                    if fn_name == \"alloc\" && args.len() == 1 {\n"
        "                        let size = self.translate_expr(\n"
        "                            program, func, current_block, &args[0],\n"
        "                        );\n"
        "                        let temp = format!(\"__alloc_{}\", self.iter_counter);\n"
        "                        self.iter_counter += 1;\n"
        "                        let ptr_ty = Type::pointer(Type::Unknown);\n"
        "                        self.declare_var(&temp, ptr_ty.clone(), false);\n"
        "                        self.safe_push_instruction(\n"
        "                            func,\n"
        "                            current_block,\n"
        "                            SemanticInstruction::Allocate {\n"
        "                                target: temp,\n"
        "                                size,\n"
        "                                type_: ptr_ty,\n"
        "                            },\n"
        "                        );\n"
        "                        if let Some(merge) = self.pending_merge.take() {\n"
        "                            return FlowResult::Reachable(merge);\n"
        "                        }\n"
        "                        return FlowResult::Reachable(current_block);\n"
        "                    }\n"
        "                    if fn_name == \"free\" && args.len() == 1 {\n"
        "                        let ptr = self.translate_expr(\n"
        "                            program, func, current_block, &args[0],\n"
        "                        );\n"
        "                        self.safe_push_instruction(\n"
        "                            func,\n"
        "                            current_block,\n"
        "                            SemanticInstruction::Free { ptr },\n"
        "                        );\n"
        "                        if let Some(merge) = self.pending_merge.take() {\n"
        "                            return FlowResult::Reachable(merge);\n"
        "                        }\n"
        "                        return FlowResult::Reachable(current_block);\n"
        "                    }\n"
        "                }\n"
        "\n"
        "                // A discarded function call must still execute its side\n"
        "                // effects. `translate_expr` for FunctionCall returns the\n"
        "                // value without pushing an instruction — the caller pushes\n"
        "                // it. For discarded calls, push with `result: None`.\n",
        1,
    ),

    # ─── FIX 3: interpreter heap field ───
    (
        "src/backends/interpreter/mod.rs",
        "pub struct Interpreter {\n"
        "    pub(super) variables: HashMap<String, RuntimeValue>,\n"
        "    pub(super) output: Vec<String>,\n"
        "    pub(super) program: SemanticProgram,\n"
        "    pub(super) return_value: Option<RuntimeValue>,\n"
        "}\n"
        "\n"
        "impl Interpreter {\n"
        "    pub fn new(program: SemanticProgram) -> Self {\n"
        "        Self {\n"
        "            variables: HashMap::new(),\n"
        "            output: Vec::new(),\n"
        "            program,\n"
        "            return_value: None,\n"
        "        }\n"
        "    }\n",
        "pub struct Interpreter {\n"
        "    pub(super) variables: HashMap<String, RuntimeValue>,\n"
        "    pub(super) output: Vec<String>,\n"
        "    pub(super) program: SemanticProgram,\n"
        "    pub(super) return_value: Option<RuntimeValue>,\n"
        "    /// Simulated heap for `alloc` / `free`. The key is an opaque\n"
        "    /// pointer handle (exposed to the program as\n"
        "    /// `RuntimeValue::Int`); the value is the allocated byte\n"
        "    /// buffer. Real pointers are not meaningful in a tree-walker,\n"
        "    /// so the handle indirection gives the runtime the same\n"
        "    /// observable behavior without FFI.\n"
        "    pub(super) heap: HashMap<usize, Vec<u8>>,\n"
        "    pub(super) next_ptr: usize,\n"
        "}\n"
        "\n"
        "impl Interpreter {\n"
        "    pub fn new(program: SemanticProgram) -> Self {\n"
        "        Self {\n"
        "            variables: HashMap::new(),\n"
        "            output: Vec::new(),\n"
        "            program,\n"
        "            return_value: None,\n"
        "            heap: HashMap::new(),\n"
        "            next_ptr: 1, // start at 1 so 0 means \"null\"\n"
        "        }\n"
        "    }\n",
        1,
    ),

    # ─── FIX 4: interpreter Allocate / Free implementations ───
    (
        "src/backends/interpreter/mod.rs",
        "            Instruction::IteratorInit { iterator, iterable } => {\n"
        "                let val = self.eval_value(iterable);\n"
        "                self.variables.insert(iterator.clone(), val);\n"
        "                self.variables.insert(\n"
        "                    format!(\"{}_idx\", iterator),\n"
        "                    RuntimeValue::Int(0),\n"
        "                );\n"
        "            }\n"
        "            _=> {}\n",
        "            Instruction::IteratorInit { iterator, iterable } => {\n"
        "                let val = self.eval_value(iterable);\n"
        "                self.variables.insert(iterator.clone(), val);\n"
        "                self.variables.insert(\n"
        "                    format!(\"{}_idx\", iterator),\n"
        "                    RuntimeValue::Int(0),\n"
        "                );\n"
        "            }\n"
        "            Instruction::Allocate { target, size, .. } => {\n"
        "                let requested = match self.eval_value(size) {\n"
        "                    RuntimeValue::Int(i) if i > 0 => i as usize,\n"
        "                    RuntimeValue::Float(f) if f > 0.0 => f as usize,\n"
        "                    _ => 0,\n"
        "                };\n"
        "                let handle = self.next_ptr;\n"
        "                self.next_ptr += 1;\n"
        "                self.heap.insert(handle, vec![0u8; requested]);\n"
        "                self.variables\n"
        "                    .insert(target.clone(), RuntimeValue::Int(handle as i64));\n"
        "            }\n"
        "            Instruction::Free { ptr } => {\n"
        "                let handle = match self.eval_value(ptr) {\n"
        "                    RuntimeValue::Int(h) if h > 0 => Some(h as usize),\n"
        "                    _ => None,\n"
        "                };\n"
        "                if let Some(h) = handle {\n"
        "                    self.heap.remove(&h);\n"
        "                }\n"
        "            }\n"
        "            _=> {}\n",
        1,
    ),

    # ─── FIX 5: capability matrix — interpreter accepts RawMemory ───
    (
        "src/backends/capabilities/mod.rs",
        "        supported.insert(Feature::ListPrint);\n"
        "        supported.insert(Feature::Option);\n"
        "        // FFI and raw memory are not supported by the interpreter.\n",
        "        supported.insert(Feature::ListPrint);\n"
        "        supported.insert(Feature::Option);\n"
        "        supported.insert(Feature::RawMemory);\n"
        "        // FFI is not supported by the interpreter (a tree-walker\n"
        "        // cannot call into C).\n",
        1,
    ),

    # ─── FIX 6: capability test — flip rejection to acceptance ───
    (
        "src/backends/capabilities/tests.rs",
        "#[test]\n"
        "fn interpreter_rejects_raw_memory() {\n"
        "    let program = program_with(\n"
        "        Instruction::Free { ptr: TypedIRValue::NullPtr },\n"
        "        simple_return(),\n"
        "    );\n"
        "    let err = check_backend(&program, &BackendCapabilities::interpreter()).unwrap_err();\n"
        "    assert!(err.message.contains(\"raw memory\"), \"{}\", err.message);\n"
        "}\n",
        "#[test]\n"
        "fn interpreter_accepts_raw_memory() {\n"
        "    // The interpreter has a simulated heap for alloc/free\n"
        "    // (added in Step 2 wiring). Refusal is only for LLVM.\n"
        "    let program = program_with(\n"
        "        Instruction::Free { ptr: TypedIRValue::NullPtr },\n"
        "        simple_return(),\n"
        "    );\n"
        "    assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());\n"
        "}\n",
        1,
    ),

    # ─── FIX 7: interpreter backend test — alloc + free + print ───
    (
        "src/backends/interpreter_backend.rs",
        "    #[test]\n"
        "    fn test_interpreter_captures_output() {\n",
        "    #[test]\n"
        "    fn test_interpreter_allocate_and_free() {\n"
        "        // Step 2 wiring: the interpreter now handles\n"
        "        // `Instruction::Allocate` and `Instruction::Free`\n"
        "        // against a simulated heap.\n"
        "        use crate::common::types::Type;\n"
        "        use crate::ir::semantic_ir::{\n"
        "            Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator,\n"
        "            TypedIRValue,\n"
        "        };\n"
        "        use crate::ir::verified_ir::VerifiedIR;\n"
        "\n"
        "        let mut program = SemanticProgram::new();\n"
        "        let entry = program.new_block_id();\n"
        "\n"
        "        let func = SemanticFunction {\n"
        "            name: \"main\".to_string(),\n"
        "            params: vec![],\n"
        "            return_type: Type::Void,\n"
        "            blocks: vec![SemanticBlock {\n"
        "                id: entry,\n"
        "                instructions: vec![\n"
        "                    Instruction::Allocate {\n"
        "                        target: \"p\".to_string(),\n"
        "                        size: TypedIRValue::Int(8),\n"
        "                        type_: Type::pointer(Type::Unknown),\n"
        "                    },\n"
        "                    Instruction::Free {\n"
        "                        ptr: TypedIRValue::Variable(\n"
        "                            \"p\".to_string(),\n"
        "                            Type::pointer(Type::Unknown),\n"
        "                        ),\n"
        "                    },\n"
        "                    Instruction::Print {\n"
        "                        value: TypedIRValue::String(\"ok\".to_string()),\n"
        "                    },\n"
        "                ],\n"
        "                terminator: Some(Terminator::Return {\n"
        "                    value: None,\n"
        "                    type_: Type::Void,\n"
        "                }),\n"
        "            }],\n"
        "            entry_block: entry,\n"
        "            is_extern: false,\n"
        "        };\n"
        "        program.functions.push(func);\n"
        "\n"
        "        let verified = VerifiedIR::new(program).expect(\"IR verification failed\");\n"
        "        let backend = InterpreterBackend::new();\n"
        "        let result = backend.compile(&verified, \"test\");\n"
        "        assert!(result.is_ok(), \"interpreter should handle alloc/free: {:?}\", result);\n"
        "        assert_eq!(backend.get_output(), \"ok\\n\");\n"
        "    }\n"
        "\n"
        "    #[test]\n"
        "    fn test_interpreter_captures_output() {\n",
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

    by_file = defaultdict(list)
    for i, (rel, find, replace, occ) in enumerate(FIXES, 1):
        by_file[rel].append((i, find, replace, occ))

    # Validate every anchor against the original file content first.
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

    if args.dry_run:
        print(f"Dry run — {len(FIXES)} fixes across:")
        for rel in by_file:
            print(f"  {rel}")
        return 0

    for rel, fixes in by_file.items():
        path = repo / rel
        text = path.read_text()
        for fix_i, find, replace, occ in fixes:
            new_text = apply_fix(text, find, replace, occ)
            if new_text is None:
                print(f"ERROR: fix {fix_i} failed when chained against {rel}", file=sys.stderr)
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
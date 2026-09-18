#!/usr/bin/env python3
"""
Step 3: wire region blocks end-to-end through the interpreter.

- IR: add Instruction::RegionEnter / RegionExit.
- IR verifier: accept the new instructions.
- Capability scan: no features added (regions are scoping, not
  memory-model features); new arms are no-ops.
- Builder: Stmt::RegionBlock emits enter/exit around the body.
- Interpreter: region stack, allocations attributed to innermost
  region, auto-free on exit, cleanup on function return, save/restore
  across user-function calls.
- LLVM codegen: no-op arms (LLVM refuses any program with alloc/free,
  so a region containing only non-memory code becomes a lexical
  hint with zero cost).

Usage:
    python3 tools/apply_step3_fixes.py --dry-run
    python3 tools/apply_step3_fixes.py
"""

import argparse
import sys
from collections import defaultdict
from pathlib import Path

FIXES = [
    # ─── 1. IR: add RegionEnter/RegionExit after Free ───
    (
        "src/ir/semantic_ir.rs",
        "    Free {\n"
        "        ptr: TypedIRValue,\n"
        "    },\n"
        "}\n",
        "    Free {\n"
        "        ptr: TypedIRValue,\n"
        "    },\n"
        "\n"
        "    /// Enter a `region NAME` block. The interpreter pushes a new\n"
        "    /// region frame; allocations between `RegionEnter` and the\n"
        "    /// matching `RegionExit` are attributed to it and freed\n"
        "    /// automatically on exit. LLVM treats both as no-ops — the\n"
        "    /// capability check refuses any program that actually allocs,\n"
        "    /// so a region without alloc has no runtime meaning.\n"
        "    RegionEnter { name: String },\n"
        "    /// Exit a `region NAME` block.\n"
        "    RegionExit { name: String },\n"
        "}\n",
        1,
    ),

    # ─── 2. IR verifier: accept the new instructions ───
    (
        "src/ir/verifier/instruction.rs",
        "        Instruction::Free { ptr } => {\n"
        "            let ptr_ty = verify_value(ptr, env)?;\n"
        "            match ptr_ty {\n"
        "                Type::Pointer(_) | Type::Ptr | Type::Unknown => Ok(()),\n"
        "                other => Err(format!(\n"
        "                    \"Function '{}': Free on non-pointer type {:?}\",\n"
        "                    func.name, other\n"
        "                )),\n"
        "            }\n"
        "        }\n",
        "        Instruction::Free { ptr } => {\n"
        "            let ptr_ty = verify_value(ptr, env)?;\n"
        "            match ptr_ty {\n"
        "                Type::Pointer(_) | Type::Ptr | Type::Unknown => Ok(()),\n"
        "                other => Err(format!(\n"
        "                    \"Function '{}': Free on non-pointer type {:?}\",\n"
        "                    func.name, other\n"
        "                )),\n"
        "            }\n"
        "        }\n"
        "        // Region enter/exit carry no operands to verify. The\n"
        "        // name is metadata; scope discipline is enforced by the\n"
        "        // interpreter at runtime (or by the LLVM codegen treating\n"
        "        // both as no-ops).\n"
        "        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => Ok(()),\n",
        1,
    ),

    # ─── 3. Capability scan: nothing to insert ───
    (
        "src/backends/capabilities/scan.rs",
        "        Instruction::Free { ptr } => {\n"
        "            used.insert(Feature::RawMemory);\n"
        "            scan_value(ptr, extern_fns, used);\n"
        "        }\n",
        "        Instruction::Free { ptr } => {\n"
        "            used.insert(Feature::RawMemory);\n"
        "            scan_value(ptr, extern_fns, used);\n"
        "        }\n"
        "        // Region enter/exit are scoping hints. If the body\n"
        "        // contains alloc/free, those instructions insert\n"
        "        // `Feature::RawMemory` on their own — regions themselves\n"
        "        // do not need a feature gate.\n"
        "        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => {}\n",
        1,
    ),

    # ─── 4. Builder: RegionBlock emits enter/exit ───
    (
        "src/semantics/builder/control_flow.rs",
        "                Stmt::RegionBlock { name: _, body, .. } => {\n"
        "                    self.push_scope();\n"
        "                    let flow = self.translate_block(program, func, current_block, body);\n"
        "                    self.pop_scope();\n"
        "                    flow\n"
        "                }\n",
        "                Stmt::RegionBlock { name, body, .. } => {\n"
        "                    // Region blocks now emit RegionEnter before the\n"
        "                    // body and RegionExit after, so the interpreter\n"
        "                    // can free every allocation made between them.\n"
        "                    // (Step 3 wiring.)\n"
        "                    self.push_scope();\n"
        "                    self.safe_push_instruction(\n"
        "                        func,\n"
        "                        current_block,\n"
        "                        SemanticInstruction::RegionEnter {\n"
        "                            name: name.clone(),\n"
        "                        },\n"
        "                    );\n"
        "                    let flow =\n"
        "                        self.translate_block(program, func, current_block, body);\n"
        "                    self.pop_scope();\n"
        "                    // If the body terminated (e.g. `return` inside\n"
        "                    // the region), RegionExit is unreachable and\n"
        "                    // shouldn't be emitted — the interpreter's\n"
        "                    // Return handler cleans up any open frames.\n"
        "                    if let FlowResult::Reachable(id) = flow {\n"
        "                        self.safe_push_instruction(\n"
        "                            func,\n"
        "                            id,\n"
        "                            SemanticInstruction::RegionExit {\n"
        "                                name: name.clone(),\n"
        "                            },\n"
        "                        );\n"
        "                    }\n"
        "                    flow\n"
        "                }\n",
        1,
    ),

    # ─── 5a. Interpreter: add RegionFrame type ───
    (
        "src/backends/interpreter/mod.rs",
        "use std::collections::HashMap;\n"
        "\n"
        "mod runtime;\n"
        "mod pattern;\n"
        "mod eval;\n"
        "pub use runtime::RuntimeValue;\n",
        "use std::collections::HashMap;\n"
        "\n"
        "mod runtime;\n"
        "mod pattern;\n"
        "mod eval;\n"
        "pub use runtime::RuntimeValue;\n"
        "\n"
        "/// A single active `region NAME` block. Allocations made inside\n"
        "/// the block are recorded here by `Instruction::Allocate` and\n"
        "/// freed when the region exits (either via `RegionExit` or via\n"
        "/// an early `return` from the enclosing function).\n"
        "#[derive(Debug)]\n"
        "pub(super) struct RegionFrame {\n"
        "    pub name: String,\n"
        "    /// Handles into `Interpreter::heap` for allocations made\n"
        "    /// while this frame was the innermost active region.\n"
        "    pub allocations: Vec<usize>,\n"
        "}\n",
        1,
    ),

    # ─── 5b. Interpreter: struct field + new() ───
    (
        "src/backends/interpreter/mod.rs",
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
        "    /// Simulated heap for `alloc` / `free`. The key is an opaque\n"
        "    /// pointer handle (exposed to the program as\n"
        "    /// `RuntimeValue::Int`); the value is the allocated byte\n"
        "    /// buffer. Real pointers are not meaningful in a tree-walker,\n"
        "    /// so the handle indirection gives the runtime the same\n"
        "    /// observable behavior without FFI.\n"
        "    pub(super) heap: HashMap<usize, Vec<u8>>,\n"
        "    pub(super) next_ptr: usize,\n"
        "    /// Stack of active `region` blocks in the current function.\n"
        "    /// Cleared on function exit; saved and restored across\n"
        "    /// user-function calls so a callee cannot accidentally free\n"
        "    /// its caller's region allocations.\n"
        "    pub(super) region_stack: Vec<RegionFrame>,\n"
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
        "            region_stack: Vec::new(),\n"
        "        }\n"
        "    }\n",
        1,
    ),

    # ─── 5c. Interpreter: Return arm cleans up frames ───
    (
        "src/backends/interpreter/mod.rs",
        "                Some(Terminator::Return { value, .. }) => {\n"
        "                    if let Some(v) = value {\n"
        "                        self.return_value = Some(self.eval_value(v));\n"
        "                    }\n"
        "                    return Ok(());\n"
        "                }\n",
        "                Some(Terminator::Return { value, .. }) => {\n"
        "                    if let Some(v) = value {\n"
        "                        self.return_value = Some(self.eval_value(v));\n"
        "                    }\n"
        "                    // Any `region` blocks still open at return\n"
        "                    // (from early-return paths) get cleaned up\n"
        "                    // here. Region frames are function-local, so\n"
        "                    // the caller's frame stack is untouched.\n"
        "                    while let Some(frame) = self.region_stack.pop() {\n"
        "                        for handle in frame.allocations {\n"
        "                            self.heap.remove(&handle);\n"
        "                        }\n"
        "                    }\n"
        "                    return Ok(());\n"
        "                }\n",
        1,
    ),

    # ─── 5d. Interpreter: Allocate attributes to innermost region ───
    (
        "src/backends/interpreter/mod.rs",
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
        "            }\n",
        "            Instruction::Allocate { target, size, .. } => {\n"
        "                let requested = match self.eval_value(size) {\n"
        "                    RuntimeValue::Int(i) if i > 0 => i as usize,\n"
        "                    RuntimeValue::Float(f) if f > 0.0 => f as usize,\n"
        "                    _ => 0,\n"
        "                };\n"
        "                let handle = self.next_ptr;\n"
        "                self.next_ptr += 1;\n"
        "                self.heap.insert(handle, vec![0u8; requested]);\n"
        "                // Attribute to the innermost active region, if\n"
        "                // any. A region exit will free every handle\n"
        "                // recorded here, so an explicit `free(p)` inside\n"
        "                // a region is safe (removing an already-freed\n"
        "                // handle is a no-op).\n"
        "                if let Some(frame) = self.region_stack.last_mut() {\n"
        "                    frame.allocations.push(handle);\n"
        "                }\n"
        "                self.variables\n"
        "                    .insert(target.clone(), RuntimeValue::Int(handle as i64));\n"
        "            }\n",
        1,
    ),

    # ─── 5e. Interpreter: RegionEnter/RegionExit arms ───
    (
        "src/backends/interpreter/mod.rs",
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
        "            Instruction::Free { ptr } => {\n"
        "                let handle = match self.eval_value(ptr) {\n"
        "                    RuntimeValue::Int(h) if h > 0 => Some(h as usize),\n"
        "                    _ => None,\n"
        "                };\n"
        "                if let Some(h) = handle {\n"
        "                    self.heap.remove(&h);\n"
        "                }\n"
        "            }\n"
        "            Instruction::RegionEnter { name } => {\n"
        "                self.region_stack.push(RegionFrame {\n"
        "                    name: name.clone(),\n"
        "                    allocations: Vec::new(),\n"
        "                });\n"
        "            }\n"
        "            Instruction::RegionExit { .. } => {\n"
        "                if let Some(frame) = self.region_stack.pop() {\n"
        "                    for handle in frame.allocations {\n"
        "                        self.heap.remove(&handle);\n"
        "                    }\n"
        "                }\n"
        "            }\n"
        "            _=> {}\n",
        1,
    ),

    # ─── 5f. Interpreter: save/restore region_stack in eval_call ───
    (
        "src/backends/interpreter/eval.rs",
        "            let saved_vars = std::mem::take(&mut self.variables);\n"
        "            let saved_ret = self.return_value.take();\n",
        "            let saved_vars = std::mem::take(&mut self.variables);\n"
        "            let saved_ret = self.return_value.take();\n"
        "            // Region frames are function-local. A callee must\n"
        "            // start with an empty region stack — otherwise it\n"
        "            // could free the caller's active region allocations\n"
        "            // by accident.\n"
        "            let saved_regions = std::mem::take(&mut self.region_stack);\n",
        1,
    ),

    (
        "src/backends/interpreter/eval.rs",
        "            self.variables = saved_vars;\n"
        "            self.return_value = saved_ret;\n",
        "            self.variables = saved_vars;\n"
        "            self.return_value = saved_ret;\n"
        "            self.region_stack = saved_regions;\n",
        1,
    ),

    # ─── 6. LLVM codegen: no-op arms for region enter/exit ───
    (
        "src/backends/llvm_codegen/instruction.rs",
        "            Instruction::Allocate { .. } => Ok(()),\n"
        "            Instruction::Free { .. } => Ok(()),\n"
        "        }\n",
        "            Instruction::Allocate { .. } => Ok(()),\n"
        "            Instruction::Free { .. } => Ok(()),\n"
        "            // Region enter/exit are no-ops for LLVM: the backend\n"
        "            // has no runtime memory manager, and any region that\n"
        "            // actually allocates is refused by the capability\n"
        "            // check. A region containing only non-memory code\n"
        "            // becomes a lexical hint with zero cost.\n"
        "            Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => Ok(()),\n"
        "        }\n",
        1,
    ),

    # ─── 7. Interpreter backend test ───
    (
        "src/backends/interpreter_backend.rs",
        "    #[test]\n"
        "    fn test_interpreter_captures_output() {\n",
        "    #[test]\n"
        "    fn test_interpreter_region_frees_allocation_on_exit() {\n"
        "        // Step 3 wiring: `region r` opens a frame; `alloc(n)`\n"
        "        // inside records its handle; `RegionExit` frees it.\n"
        "        // The program prints inside the region, then outside,\n"
        "        // and both prints succeed.\n"
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
        "                    Instruction::RegionEnter {\n"
        "                        name: \"r\".to_string(),\n"
        "                    },\n"
        "                    Instruction::Allocate {\n"
        "                        target: \"p\".to_string(),\n"
        "                        size: TypedIRValue::Int(8),\n"
        "                        type_: Type::pointer(Type::Unknown),\n"
        "                    },\n"
        "                    Instruction::Print {\n"
        "                        value: TypedIRValue::String(\"inside\".to_string()),\n"
        "                    },\n"
        "                    Instruction::RegionExit {\n"
        "                        name: \"r\".to_string(),\n"
        "                    },\n"
        "                    Instruction::Print {\n"
        "                        value: TypedIRValue::String(\"outside\".to_string()),\n"
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
        "        assert!(result.is_ok(), \"region test failed: {:?}\", result);\n"
        "        assert_eq!(backend.get_output(), \"inside\\noutside\\n\");\n"
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
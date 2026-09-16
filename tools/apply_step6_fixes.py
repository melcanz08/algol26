#!/usr/bin/env python3
"""
Step 6: LLVM region auto-free.

Closes the divergence where region blocks free allocations on exit
only in the interpreter. The LLVM backend now:

- Tracks a stack of LRegionFrame per function.
- Instruction::RegionEnter pushes a frame.
- Instruction::Allocate inside a region records the variable name.
- Instruction::Free nulls out the variable's alloca after calling
  `free`, so region exit's auto-free is a no-op for explicitly-freed
  pointers (matching the interpreter's idempotent heap.remove).
- Instruction::RegionExit pops the frame and emits a guarded free
  (skip if null) for every recorded variable.
- Terminator::Return cleans up any frames still open when an early
  return happens inside a region.
"""

import sys
from collections import defaultdict
from pathlib import Path

FIXES = [
    # ─── 1. Add LRegionFrame type + field + init ───
    (
        "src/backends/llvm_codegen/mod.rs",
        "    /// ALGOL26 extern name -> C symbol. Populated by\n"
        "    /// `compile()` from the program's FFI metadata. Used in\n"
        "    /// `declare_function` so a call to `print_line` emits\n"
        "    /// `@puts` when the declaration was `as \"puts\"`.\n"
        "    pub(super) ffi_symbols: HashMap<String, String>,\n"
        "}\n",
        "    /// ALGOL26 extern name -> C symbol. Populated by\n"
        "    /// `compile()` from the program's FFI metadata. Used in\n"
        "    /// `declare_function` so a call to `print_line` emits\n"
        "    /// `@puts` when the declaration was `as \"puts\"`.\n"
        "    pub(super) ffi_symbols: HashMap<String, String>,\n"
        "    /// Stack of active `region` frames for the function being\n"
        "    /// compiled. `RegionEnter` pushes, `RegionExit` pops and\n"
        "    /// emits a guarded `free` for each allocation. Early\n"
        "    /// returns clean up every remaining frame. (Step 6.)\n"
        "    pub(super) region_frames: Vec<LRegionFrame>,\n"
        "}\n"
        "\n"
        "#[derive(Debug, Clone)]\n"
        "pub(super) struct LRegionFrame {\n"
        "    pub name: String,\n"
        "    /// Variable names holding region-scoped allocations. On\n"
        "    /// region exit each is loaded; if non-null, `free`d and\n"
        "    /// nulled. Duplicate names are stored once per region —\n"
        "    /// reassigning a `var` inside a region to a new allocation\n"
        "    /// leaks the earlier value (documented divergence from the\n"
        "    /// interpreter, whose heap-remove is idempotent).\n"
        "    pub allocations: Vec<String>,\n"
        "}\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "            iterator_lengths: HashMap::new(),\n"
        "            ffi_symbols: HashMap::new(),\n"
        "        }\n"
        "    }\n",
        "            iterator_lengths: HashMap::new(),\n"
        "            ffi_symbols: HashMap::new(),\n"
        "            region_frames: Vec::new(),\n"
        "        }\n"
        "    }\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/mod.rs",
        "        self.iterator_indices.clear();\n"
        "        self.iterator_lengths.clear();\n",
        "        self.iterator_indices.clear();\n"
        "        self.iterator_lengths.clear();\n"
        "        self.region_frames.clear();\n",
        1,
    ),
    # ─── Helper: emit_free_if_non_null ───
    (
        "src/backends/llvm_codegen/mod.rs",
        "    pub(super) fn create_entry_alloca(&self, name: &str, ty: &Type) -> PointerValue<'ctx> {\n",
        "    /// Emit a guarded `free` on the pointer stored in `alloca`.\n"
        "    ///\n"
        "    /// If the loaded pointer is null, no free is emitted. If it\n"
        "    /// is non-null, `free(ptr)` runs and the alloca is nulled so a\n"
        "    /// second call to `emit_free_if_non_null` on the same alloca\n"
        "    /// is a no-op. This is how region auto-free stays idempotent\n"
        "    /// with respect to explicit `free(p)` calls in the region\n"
        "    /// body.\n"
        "    pub(super) fn emit_free_if_non_null(\n"
        "        &self,\n"
        "        alloca: PointerValue<'ctx>,\n"
        "    ) -> Result<()> {\n"
        "        use inkwell::AddressSpace;\n"
        "        let ptr_ty = self.context.ptr_type(AddressSpace::default());\n"
        "        let loaded = self\n"
        "            .builder\n"
        "            .build_load(ptr_ty, alloca, \"region_free_load\")\n"
        "            .unwrap();\n"
        "        let is_null = self\n"
        "            .builder\n"
        "            .build_is_null(loaded.into_pointer_value(), \"region_free_isnull\")\n"
        "            .unwrap();\n"
        "        let free_fn = self.module.get_function(\"free\").ok_or_else(|| {\n"
        "            CompileError::simple(\n"
        "                \"LLVM codegen: free not registered in stdlib\",\n"
        "                0, 0, \"\", ErrorCode::E0009,\n"
        "            )\n"
        "        })?;\n"
        "        let current_fn = self.current_function.unwrap();\n"
        "        let do_free_bb = self\n"
        "            .context\n"
        "            .append_basic_block(current_fn, \"region_free_do\");\n"
        "        let skip_bb = self\n"
        "            .context\n"
        "            .append_basic_block(current_fn, \"region_free_skip\");\n"
        "        self.builder\n"
        "            .build_conditional_branch(is_null, skip_bb, do_free_bb)\n"
        "            .unwrap();\n"
        "        self.builder.position_at_end(do_free_bb);\n"
        "        self.builder\n"
        "            .build_call(free_fn, &[loaded.into()], \"region_free_call\")\n"
        "            .unwrap();\n"
        "        let null_ptr = ptr_ty.const_null();\n"
        "        self.builder.build_store(alloca, null_ptr).unwrap();\n"
        "        self.builder.build_unconditional_branch(skip_bb).unwrap();\n"
        "        self.builder.position_at_end(skip_bb);\n"
        "        Ok(())\n"
        "    }\n"
        "\n"
        "    pub(super) fn create_entry_alloca(&self, name: &str, ty: &Type) -> PointerValue<'ctx> {\n",
        1,
    ),
    # ─── 2. instruction.rs: track region + Allocate + Free + Enter/Exit ───
    (
        "src/backends/llvm_codegen/instruction.rs",
        "                self.builder.build_store(alloca, ptr_val).unwrap();\n"
        "                Ok(())\n"
        "            }\n",
        "                self.builder.build_store(alloca, ptr_val).unwrap();\n"
        "                // Record the variable in the innermost active\n"
        "                // region so its allocation is freed on exit.\n"
        "                if let Some(frame) = self.region_frames.last_mut() {\n"
        "                    if !frame.allocations.contains(target) {\n"
        "                        frame.allocations.push(target.clone());\n"
        "                    }\n"
        "                }\n"
        "                Ok(())\n"
        "            }\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/instruction.rs",
        "            Instruction::Free { ptr } => {\n"
        "                // `free(p)` lowers to a call to libc `free`.\n"
        "                // `compile_value` on a pointer-typed variable\n"
        "                // loads the pointer; passing the loaded pointer\n"
        "                // to `free` matches the semantic of the\n"
        "                // interpreter's handle-based free.\n"
        "                let ptr_val = self.compile_value(ptr)?;\n"
        "                let free_fn = self.module.get_function(\"free\").ok_or_else(|| {\n"
        "                    CompileError::simple(\n"
        "                        \"LLVM codegen: free not registered in stdlib\",\n"
        "                        0, 0, \"\", ErrorCode::E0009,\n"
        "                    )\n"
        "                })?;\n"
        "                self.builder\n"
        "                    .build_call(free_fn, &[ptr_val.into()], \"free_call\")\n"
        "                    .unwrap();\n"
        "                Ok(())\n"
        "            }\n",
        "            Instruction::Free { ptr } => {\n"
        "                // `free(p)` lowers to a call to libc `free`.\n"
        "                // After the call, if the pointer is held in a\n"
        "                // variable, null its alloca. This makes a later\n"
        "                // region auto-free on the same variable a no-op\n"
        "                // (free(null) is defined as doing nothing), so\n"
        "                // `free(p)` inside a region and its auto-free on\n"
        "                // region exit do not double-free.\n"
        "                let ptr_val = self.compile_value(ptr)?;\n"
        "                let free_fn = self.module.get_function(\"free\").ok_or_else(|| {\n"
        "                    CompileError::simple(\n"
        "                        \"LLVM codegen: free not registered in stdlib\",\n"
        "                        0, 0, \"\", ErrorCode::E0009,\n"
        "                    )\n"
        "                })?;\n"
        "                self.builder\n"
        "                    .build_call(free_fn, &[ptr_val.into()], \"free_call\")\n"
        "                    .unwrap();\n"
        "                if let TypedIRValue::Variable(name, _) = ptr {\n"
        "                    if let Some(alloca) = self.variables.get(name).copied() {\n"
        "                        let ptr_ty = self\n"
        "                            .context\n"
        "                            .ptr_type(inkwell::AddressSpace::default());\n"
        "                        let null_ptr = ptr_ty.const_null();\n"
        "                        self.builder.build_store(alloca, null_ptr).unwrap();\n"
        "                    }\n"
        "                }\n"
        "                Ok(())\n"
        "            }\n",
        1,
    ),
    (
        "src/backends/llvm_codegen/instruction.rs",
        "            // Region enter/exit are no-ops for LLVM: allocations\n"
        "            // are heap-managed by malloc/free, and there is no\n"
        "            // region-scoped auto-free in the LLVM backend. A\n"
        "            // program that relies on `region` auto-free must run\n"
        "            // through the interpreter, or free its allocations\n"
        "            // explicitly. Region without alloc is a pure lexical\n"
        "            // hint, zero cost.\n"
        "            Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => Ok(()),\n"
        "        }\n",
        "            Instruction::RegionEnter { name } => {\n"
        "                self.region_frames.push(LRegionFrame {\n"
        "                    name: name.clone(),\n"
        "                    allocations: Vec::new(),\n"
        "                });\n"
        "                Ok(())\n"
        "            }\n"
        "            Instruction::RegionExit { name } => {\n"
        "                // Pop the top frame and emit a guarded free for\n"
        "                // every allocation recorded in it. Allocation\n"
        "                // names are stored in reverse (LIFO) so cleanup\n"
        "                // order matches the interpreter.\n"
        "                match self.region_frames.pop() {\n"
        "                    Some(frame) if frame.name == *name => {\n"
        "                        let names: Vec<String> =\n"
        "                            frame.allocations.iter().rev().cloned().collect();\n"
        "                        for var_name in names {\n"
        "                            if let Some(alloca) =\n"
        "                                self.variables.get(&var_name).copied()\n"
        "                            {\n"
        "                                self.emit_free_if_non_null(alloca)?;\n"
        "                            }\n"
        "                        }\n"
        "                        Ok(())\n"
        "                    }\n"
        "                    Some(frame) => Err(CompileError::simple(\n"
        "                        &format!(\n"
        "                            \"LLVM codegen: region exit '{}' but top frame is '{}'\",\n"
        "                            name, frame.name\n"
        "                        ),\n"
        "                        0, 0, \"\", ErrorCode::E0009,\n"
        "                    )),\n"
        "                    None => Err(CompileError::simple(\n"
        "                        &format!(\n"
        "                            \"LLVM codegen: region exit '{}' with no matching enter\",\n"
        "                            name\n"
        "                        ),\n"
        "                        0, 0, \"\", ErrorCode::E0009,\n"
        "                    )),\n"
        "                }\n"
        "            }\n"
        "        }\n",
        1,
    ),
    # import LRegionFrame
    (
        "src/backends/llvm_codegen/instruction.rs",
        "use super::IRCodeGen;\n"
        "use super::resolve_math_name;\n",
        "use super::IRCodeGen;\n"
        "use super::LRegionFrame;\n"
        "use super::resolve_math_name;\n",
        1,
    ),
    # ─── 3. terminator.rs: Return cleans up open frames ───
    (
        "src/backends/llvm_codegen/terminator.rs",
        "            Terminator::Return { value, type_ } => {\n"
        "                if let Some(v) = value {\n"
        "                    let compiled = self.compile_value(v)?;\n"
        "                    self.builder.build_return(Some(&compiled)).unwrap();\n"
        "                } else {\n"
        "                    if *ret_type == Type::Void {\n"
        "                        self.builder.build_return(None).unwrap();\n"
        "                    } else {\n"
        "                        let def = self.default_value_for_type(ret_type);\n"
        "                        self.builder.build_return(Some(&def)).unwrap();\n"
        "                    }\n"
        "                }\n"
        "                Ok(())\n"
        "            }\n",
        "            Terminator::Return { value, type_ } => {\n"
        "                // If a `return` happened inside one or more\n"
        "                // `region` blocks, its allocations need cleanup\n"
        "                // before the actual return instruction. The\n"
        "                // guards make this safe to run even if the\n"
        "                // RegionExit instructions are in unreachable\n"
        "                // blocks — the second cleanup sees nulls and\n"
        "                // skips.\n"
        "                let frames: Vec<_> = self.region_frames.iter().rev().cloned().collect();\n"
        "                for frame in &frames {\n"
        "                    let names: Vec<String> =\n"
        "                        frame.allocations.iter().rev().cloned().collect();\n"
        "                    for var_name in names {\n"
        "                        if let Some(alloca) = self.variables.get(&var_name).copied() {\n"
        "                            self.emit_free_if_non_null(alloca)?;\n"
        "                        }\n"
        "                    }\n"
        "                }\n"
        "                if let Some(v) = value {\n"
        "                    let compiled = self.compile_value(v)?;\n"
        "                    self.builder.build_return(Some(&compiled)).unwrap();\n"
        "                } else {\n"
        "                    if *ret_type == Type::Void {\n"
        "                        self.builder.build_return(None).unwrap();\n"
        "                    } else {\n"
        "                        let def = self.default_value_for_type(ret_type);\n"
        "                        self.builder.build_return(Some(&def)).unwrap();\n"
        "                    }\n"
        "                }\n"
        "                Ok(())\n"
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
                print(f"ERROR: fix {fix_i} chaining failed on {rel}", file=sys.stderr)
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
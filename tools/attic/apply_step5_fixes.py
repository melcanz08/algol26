#!/usr/bin/env python3
"""
Step 5: LLVM lowering of alloc/free to malloc/free.

- register_stdlib: add malloc(i64) -> ptr and free(ptr) -> void.
- instruction.rs: replace the no-op Allocate/Free arms with real
  calls.
- capability matrix: LLVM accepts Feature::RawMemory.
- capability test: flip llvm_rejects_raw_memory -> accepts.
- test: end-to-end Allocate/Free through LLVM.

Region exit remains a no-op in LLVM — see the doc note in the
instruction.rs edit. The interpreter auto-frees at region exit;
LLVM relies on explicit `free(p)` (matching C semantics).
"""

import sys
from collections import defaultdict
from pathlib import Path

FIXES = [
    # ─── 1. register_stdlib: add malloc / free ───
    (
        "src/backends/llvm_codegen/builtins.rs",
        "        let strcat_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);\n"
        "        let strcat_fn = self.module.add_function(\"strcat\", strcat_ty, None);\n"
        "        self.functions.insert(\"strcat\".to_string(), strcat_fn);\n"
        "    }\n",
        "        let strcat_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);\n"
        "        let strcat_fn = self.module.add_function(\"strcat\", strcat_ty, None);\n"
        "        self.functions.insert(\"strcat\".to_string(), strcat_fn);\n"
        "\n"
        "        // Raw memory: `alloc(n)` lowers to `malloc(n)`;\n"
        "        // `free(p)` lowers to `free(p)`. (Step 5 wiring.)\n"
        "        let malloc_ty = i8_ptr.fn_type(&[self.context.i64_type().into()], false);\n"
        "        let malloc_fn = self.module.add_function(\"malloc\", malloc_ty, None);\n"
        "        self.functions.insert(\"malloc\".to_string(), malloc_fn);\n"
        "\n"
        "        let free_ty = self\n"
        "            .context\n"
        "            .void_type()\n"
        "            .fn_type(&[i8_ptr.into()], false);\n"
        "        let free_fn = self.module.add_function(\"free\", free_ty, None);\n"
        "        self.functions.insert(\"free\".to_string(), free_fn);\n"
        "    }\n",
        1,
    ),

    # ─── 2. instruction.rs: real Allocate/Free ───
    (
        "src/backends/llvm_codegen/instruction.rs",
        "            Instruction::Allocate { .. } => Ok(()),\n"
        "            Instruction::Free { .. } => Ok(()),\n"
        "            // Region enter/exit are no-ops for LLVM: the backend\n"
        "            // has no runtime memory manager, and any region that\n"
        "            // actually allocates is refused by the capability\n"
        "            // check. A region containing only non-memory code\n"
        "            // becomes a lexical hint with zero cost.\n"
        "            Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => Ok(()),\n"
        "        }\n",
        "            Instruction::Allocate { target, size, type_ } => {\n"
        "                // `alloc(n)` lowers to a call to libc `malloc`.\n"
        "                // The result is stored into a per-variable alloca\n"
        "                // so `p` behaves like any other pointer-typed\n"
        "                // local.\n"
        "                let size_val = self.compile_value(size)?;\n"
        "                let size_i64 = if size_val.is_int_value() {\n"
        "                    let iv = size_val.into_int_value();\n"
        "                    if iv.get_type().get_bit_width() != 64 {\n"
        "                        self.builder\n"
        "                            .build_int_cast(iv, self.context.i64_type(), \"sz64\")\n"
        "                            .unwrap()\n"
        "                    } else {\n"
        "                        iv\n"
        "                    }\n"
        "                } else {\n"
        "                    self.context.i64_type().const_zero()\n"
        "                };\n"
        "                let malloc_fn = self.module.get_function(\"malloc\").ok_or_else(|| {\n"
        "                    CompileError::simple(\n"
        "                        \"LLVM codegen: malloc not registered in stdlib\",\n"
        "                        0, 0, \"\", ErrorCode::E0009,\n"
        "                    )\n"
        "                })?;\n"
        "                let call = self\n"
        "                    .builder\n"
        "                    .build_call(malloc_fn, &[size_i64.into()], \"malloc_call\")\n"
        "                    .unwrap();\n"
        "                let ptr_val = match call.try_as_basic_value() {\n"
        "                    inkwell::values::ValueKind::Basic(v) => v,\n"
        "                    _ => self\n"
        "                        .context\n"
        "                        .ptr_type(inkwell::AddressSpace::default())\n"
        "                        .const_null()\n"
        "                        .into(),\n"
        "                };\n"
        "                // Reuse the alloca if the target already exists\n"
        "                // (e.g. an Allocate inside a loop); otherwise\n"
        "                // create one at function entry.\n"
        "                let alloca = match self.variables.get(target).cloned() {\n"
        "                    Some(p) => p,\n"
        "                    None => {\n"
        "                        let a = self.create_entry_alloca(target, type_);\n"
        "                        self.variables.insert(target.clone(), a);\n"
        "                        self.var_types.insert(target.clone(), type_.clone());\n"
        "                        a\n"
        "                    }\n"
        "                };\n"
        "                self.builder.build_store(alloca, ptr_val).unwrap();\n"
        "                Ok(())\n"
        "            }\n"
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
        "            }\n"
        "            // Region enter/exit are no-ops for LLVM: allocations\n"
        "            // are heap-managed by malloc/free, and there is no\n"
        "            // region-scoped auto-free in the LLVM backend. A\n"
        "            // program that relies on `region` auto-free must run\n"
        "            // through the interpreter, or free its allocations\n"
        "            // explicitly. Region without alloc is a pure lexical\n"
        "            // hint, zero cost.\n"
        "            Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => Ok(()),\n"
        "        }\n",
        1,
    ),

    # ─── 3. capability matrix: LLVM supports RawMemory ───
    (
        "src/backends/capabilities/mod.rs",
        "    pub fn llvm() -> Self {\n"
        "        let mut supported = HashSet::new();\n"
        "        supported.insert(Feature::Ffi);\n"
        "        BackendCapabilities {\n"
        "            name: \"LLVM\",\n"
        "            supported,\n"
        "            has_interpreter_fallback: true,\n"
        "        }\n"
        "    }\n",
        "    pub fn llvm() -> Self {\n"
        "        let mut supported = HashSet::new();\n"
        "        supported.insert(Feature::Ffi);\n"
        "        // Step 5: alloc/free lower to malloc/free; LLVM now\n"
        "        // accepts programs that use them.\n"
        "        supported.insert(Feature::RawMemory);\n"
        "        BackendCapabilities {\n"
        "            name: \"LLVM\",\n"
        "            supported,\n"
        "            has_interpreter_fallback: true,\n"
        "        }\n"
        "    }\n",
        1,
    ),

    # ─── 4. capability test flip ───
    (
        "src/backends/capabilities/tests.rs",
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
        "}\n",
        "#[test]\n"
        "fn llvm_accepts_raw_memory() {\n"
        "    // Step 5 wiring: alloc/free lower to malloc/free.\n"
        "    let program = program_with(\n"
        "        Instruction::Allocate {\n"
        "            target: \"p\".to_string(),\n"
        "            size: TypedIRValue::Int(8),\n"
        "            type_: Type::pointer(Type::Unknown),\n"
        "        },\n"
        "        simple_return(),\n"
        "    );\n"
        "    assert!(check_backend(&program, &BackendCapabilities::llvm()).is_ok());\n"
        "}\n",
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
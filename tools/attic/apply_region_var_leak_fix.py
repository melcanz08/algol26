#!/usr/bin/env python3
"""
Close the region var-reassignment leak in LLVM.

Before: `p := alloc(8); p := alloc(16)` inside a region tracked the
name `p` once. Region exit freed whatever `p` held at the time (the
second allocation). The first leaked.

After: on reassignment, the old value is snapshotted into a fresh
region-local slot. Region exit frees every snapshot plus the current
value, so both allocations are released. Explicit `free(p)` still
works: the variable's alloca is nulled by the Free handler, so
region exit's free of that alloca is a no-op.
"""

import sys
from collections import defaultdict
from pathlib import Path

FIXES = [
    # ─── 1. LRegionFrame struct ───
    (
        "src/backends/llvm_codegen/mod.rs",
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
        "#[derive(Debug, Clone)]\n"
        "pub(super) struct LRegionFrame {\n"
        "    pub name: String,\n"
        "    /// Variable names holding region-scoped allocations. On\n"
        "    /// region exit each is loaded; if non-null, `free`d and\n"
        "    /// nulled. A name is added the first time `alloc` writes\n"
        "    /// to it inside this region.\n"
        "    pub tracked_vars: Vec<String>,\n"
        "    /// Snapshot slots for values overwritten by a subsequent\n"
        "    /// `alloc` to the same variable. Each holds an `i8*` that\n"
        "    /// must be freed at region exit. This is what makes\n"
        "    /// `p := alloc(8); p := alloc(16)` inside a region release\n"
        "    /// both allocations instead of just the second.\n"
        "    pub saved_slots: Vec<inkwell::values::PointerValue<'ctx>>,\n"
        "}\n",
        1,
    ),

    # ─── 2. Allocate: snapshot on reassignment ───
    (
        "src/backends/llvm_codegen/instruction.rs",
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
        "                // Before overwriting, check the region state.\n"
        "                // If this is a *reassignment* of a variable\n"
        "                // already tracked by the innermost region, the\n"
        "                // old value must be snapshotted so region exit\n"
        "                // can free it too. Otherwise the first\n"
        "                // allocation leaks.\n"
        "                let is_reassignment = self\n"
        "                    .region_frames\n"
        "                    .last()\n"
        "                    .is_some_and(|f| f.tracked_vars.iter().any(|v| v == target));\n"
        "                let in_region = !self.region_frames.is_empty();\n"
        "\n"
        "                let snapshot: Option<inkwell::values::PointerValue<'ctx>> =\n"
        "                    if in_region && is_reassignment {\n"
        "                        if let Some(existing_alloca) =\n"
        "                            self.variables.get(target).copied()\n"
        "                        {\n"
        "                            let ptr_ty = self\n"
        "                                .context\n"
        "                                .ptr_type(inkwell::AddressSpace::default());\n"
        "                            let old_val = self\n"
        "                                .builder\n"
        "                                .build_load(ptr_ty, existing_alloca, \"region_saved_load\")\n"
        "                                .unwrap();\n"
        "                            self.iter_counter += 1;\n"
        "                            let slot_name =\n"
        "                                format!(\"__region_saved_{}\", self.iter_counter);\n"
        "                            let slot = self.create_entry_alloca(\n"
        "                                &slot_name,\n"
        "                                &Type::Pointer(Box::new(Type::Unknown)),\n"
        "                            );\n"
        "                            self.builder.build_store(slot, old_val).unwrap();\n"
        "                            Some(slot)\n"
        "                        } else {\n"
        "                            None\n"
        "                        }\n"
        "                    } else {\n"
        "                        None\n"
        "                    };\n"
        "\n"
        "                self.builder.build_store(alloca, ptr_val).unwrap();\n"
        "\n"
        "                // Update the region frame with the new tracking\n"
        "                // state.\n"
        "                if in_region {\n"
        "                    if let Some(slot) = snapshot {\n"
        "                        if let Some(frame) = self.region_frames.last_mut() {\n"
        "                            frame.saved_slots.push(slot);\n"
        "                        }\n"
        "                    } else if !is_reassignment {\n"
        "                        if let Some(frame) = self.region_frames.last_mut() {\n"
        "                            frame.tracked_vars.push(target.clone());\n"
        "                        }\n"
        "                    }\n"
        "                }\n"
        "                Ok(())\n"
        "            }\n",
        1,
    ),

    # ─── 3. RegionEnter: new fields ───
    (
        "src/backends/llvm_codegen/instruction.rs",
        "            Instruction::RegionEnter { name } => {\n"
        "                self.region_frames.push(LRegionFrame {\n"
        "                    name: name.clone(),\n"
        "                    allocations: Vec::new(),\n"
        "                });\n"
        "                Ok(())\n"
        "            }\n",
        "            Instruction::RegionEnter { name } => {\n"
        "                self.region_frames.push(LRegionFrame {\n"
        "                    name: name.clone(),\n"
        "                    tracked_vars: Vec::new(),\n"
        "                    saved_slots: Vec::new(),\n"
        "                });\n"
        "                Ok(())\n"
        "            }\n",
        1,
    ),

    # ─── 4. RegionExit: free snapshots + tracked vars ───
    (
        "src/backends/llvm_codegen/instruction.rs",
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
        "                    }\n",
        "                match self.region_frames.pop() {\n"
        "                    Some(frame) if frame.name == *name => {\n"
        "                        // Collect all pointers to free, then emit\n"
        "                        // the frees. Order: snapshots first\n"
        "                        // (LIFO), then currently-tracked vars\n"
        "                        // (LIFO). The `frame` is owned (from\n"
        "                        // pop()), so no borrow conflict.\n"
        "                        let mut cleanups: Vec<\n"
        "                            inkwell::values::PointerValue<'ctx>,\n"
        "                        > = Vec::new();\n"
        "                        for slot in frame.saved_slots.iter().rev() {\n"
        "                            cleanups.push(*slot);\n"
        "                        }\n"
        "                        for var_name in frame.tracked_vars.iter().rev() {\n"
        "                            if let Some(alloca) =\n"
        "                                self.variables.get(var_name).copied()\n"
        "                            {\n"
        "                                cleanups.push(alloca);\n"
        "                            }\n"
        "                        }\n"
        "                        for alloca in cleanups {\n"
        "                            self.emit_free_if_non_null(alloca)?;\n"
        "                        }\n"
        "                        Ok(())\n"
        "                    }\n",
        1,
    ),

    # ─── 5. Return: same cleanup shape ───
    (
        "src/backends/llvm_codegen/terminator.rs",
        "                let frames: Vec<_> = self.region_frames.iter().rev().cloned().collect();\n"
        "                for frame in &frames {\n"
        "                    let names: Vec<String> =\n"
        "                        frame.allocations.iter().rev().cloned().collect();\n"
        "                    for var_name in names {\n"
        "                        if let Some(alloca) = self.variables.get(&var_name).copied() {\n"
        "                            self.emit_free_if_non_null(alloca)?;\n"
        "                        }\n"
        "                    }\n"
        "                }\n",
        "                // Collect everything to free first, then emit the\n"
        "                // frees. Order: for each frame (innermost first),\n"
        "                // snapshots then tracked vars, both LIFO.\n"
        "                let mut cleanups: Vec<\n"
        "                    inkwell::values::PointerValue<'ctx>,\n"
        "                > = Vec::new();\n"
        "                for frame in self.region_frames.iter().rev() {\n"
        "                    for slot in frame.saved_slots.iter().rev() {\n"
        "                        cleanups.push(*slot);\n"
        "                    }\n"
        "                    for var_name in frame.tracked_vars.iter().rev() {\n"
        "                        if let Some(alloca) = self.variables.get(var_name).copied() {\n"
        "                            cleanups.push(alloca);\n"
        "                        }\n"
        "                    }\n"
        "                }\n"
        "                for alloca in cleanups {\n"
        "                    self.emit_free_if_non_null(alloca)?;\n"
        "                }\n",
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
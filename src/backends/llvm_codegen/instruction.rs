// src/backends/llvm_codegen/instruction.rs

use super::resolve_math_name;
use super::IRCodeGen;
use super::LRegionFrame;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{Instruction, TypedIRValue};
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, PointerValue};
use inkwell::AddressSpace;

impl<'ctx> IRCodeGen<'ctx> {
    /// ADR 0042 phase 4. Lower `xs.append(v)` to a grow-and-write
    /// sequence.
    ///
    /// The receiver is `args[0]`, always a `Variable(name)` — the
    /// IR builder emits it that way (see `builder/expr.rs`), and
    /// the interpreter refuses any other shape. That gives us the
    /// descriptor alloca from `list_structs[name]`, which every
    /// subsequent access reads through, so the mutation reaches
    /// the caller automatically.
    ///
    /// Three paths, one for each `(capacity, length)` shape:
    ///
    ///   - `capacity == 0`  — stack-backed. `malloc(max(2*len,
    ///     len+1) * elem_size)`, `memcpy` the old elements,
    ///     continue to the common write with the new buffer and
    ///     capacity.
    ///   - `length < capacity` — heap-backed, spare slots. Write
    ///     in place; capacity unchanged.
    ///   - `length == capacity` — heap-backed, full. `realloc` to
    ///     `2 * capacity`, continue to the common write.
    ///
    /// All three converge on `write_bb`, which stores the element
    /// at `buffer[length]`, increments `length`, and writes all
    /// three descriptor fields back. Buffer and capacity for the
    /// merge come from phi nodes.
    pub(super) fn emit_list_append(&self, args: &[TypedIRValue]) -> Result<()> {
        if args.len() != 2 {
            return Err(CompileError::simple(
                &format!(
                    "List.append expects 2 arguments (receiver, value), got {}",
                    args.len()
                ),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }
        // ADR 0052 phase 4. The receiver is either a variable
        // (`xs.append(v)`) or a field access (`b.items.append(v)`).
        // The variable path looks up the descriptor alloca in
        // `list_structs`; the field path GEPs into the containing
        // record's storage — same shape as `Instruction::FieldAssign`.
        let (struct_alloca, list_ty) = match &args[0] {
            TypedIRValue::Variable(n, _) => {
                let ty = self.var_types.get(n).cloned().ok_or_else(|| {
                    CompileError::simple(
                        &format!(
                            "LLVM codegen: List.append on `{}` which has no recorded type",
                            n
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let alloca = self.list_structs.get(n).copied().ok_or_else(|| {
                    CompileError::simple(
                        &format!(
                            "LLVM codegen: List.append on `{}` has no descriptor struct \
                             — a producer registered the variable without populating \
                             `list_structs`",
                            n
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                (alloca, ty)
            }
            TypedIRValue::FieldAccess {
                object,
                field,
                field_type,
            } => {
                // Auto-deref through references on the containing
                // object — `self.items.append(x)` where `self:
                // &Bag` has object type `Borrow<Bag>`.
                let raw_obj_ty = object.type_of();
                let obj_ty = match &raw_obj_ty {
                    Type::Borrow(inner) | Type::MutBorrow(inner) => (**inner).clone(),
                    other => other.clone(),
                };
                let record_name = match &obj_ty {
                    Type::Record(name, _) => name.clone(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("List.append on field of non-record type {}", raw_obj_ty),
                            "llvm",
                        ));
                    }
                };
                let rec_decl = self
                    .record_decls
                    .get(&record_name)
                    .cloned()
                    .ok_or_else(|| {
                        CompileError::simple(
                            &format!("LLVM codegen: unknown record '{}'", record_name),
                            0,
                            0,
                            "",
                            ErrorCode::E0003,
                        )
                    })?;
                let field_idx = rec_decl
                    .fields
                    .iter()
                    .position(|(n, _, _)| n == field)
                    .ok_or_else(|| {
                        CompileError::simple(
                            &format!(
                                "LLVM codegen: record '{}' has no field '{}'",
                                record_name, field
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        )
                    })?;
                let record_struct_ty = match self.map_type(&obj_ty) {
                    BasicTypeEnum::StructType(s) => s,
                    _ => unreachable!(
                        "map_type(Type::Record) returned a non-struct type for '{}'",
                        record_name
                    ),
                };
                let record_ptr = match object.as_ref() {
                    TypedIRValue::Variable(name, _) => {
                        self.variables.get(name).copied().ok_or_else(|| {
                            CompileError::simple(
                                &format!("LLVM codegen: unknown variable '{}'", name),
                                0,
                                0,
                                "",
                                ErrorCode::E0003,
                            )
                        })?
                    }
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "List.append on deeply nested field receiver \
                             (only variable.field is supported)",
                            "llvm",
                        ));
                    }
                };
                let field_ptr = unsafe {
                    self.builder
                        .build_gep(
                            record_struct_ty,
                            record_ptr,
                            &[
                                self.context.i32_type().const_zero(),
                                self.context.i32_type().const_int(field_idx as u64, false),
                            ],
                            &format!("{}_{}_append_ptr", record_name, field),
                        )
                        .unwrap()
                };
                (field_ptr, field_type.clone())
            }
            _ => {
                return Err(CompileError::unsupported_operation(
                    "List.append requires a variable or field receiver",
                    "llvm",
                ));
            }
        };

        let elem_ty = match &list_ty {
            Type::List(inner) => (**inner).clone(),
            _ => {
                return Err(CompileError::simple(
                    &format!(
                        "LLVM codegen: List.append receiver is not a list (type {})",
                        list_ty
                    ),
                    0,
                    0,
                    "",
                    ErrorCode::E0002,
                ));
            }
        };

        let struct_ty = match self.map_type(&list_ty) {
            BasicTypeEnum::StructType(st) => st,
            _ => unreachable!("map_type(Type::List) returned non-struct"),
        };
        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let i64_ty = self.context.i64_type();

        let buf_slot = self
            .builder
            .build_struct_gep(struct_ty, struct_alloca, 0, "la_buf_slot")
            .unwrap();
        let len_slot = self
            .builder
            .build_struct_gep(struct_ty, struct_alloca, 1, "la_len_slot")
            .unwrap();
        let cap_slot = self
            .builder
            .build_struct_gep(struct_ty, struct_alloca, 2, "la_cap_slot")
            .unwrap();
        let buf = self
            .builder
            .build_load(ptr_ty, buf_slot, "la_buf")
            .unwrap()
            .into_pointer_value();
        let len = self
            .builder
            .build_load(i64_ty, len_slot, "la_len")
            .unwrap()
            .into_int_value();
        let cap = self
            .builder
            .build_load(i64_ty, cap_slot, "la_cap")
            .unwrap()
            .into_int_value();

        // Compile the appended value first — it may itself do work
        // (allocate, call a user function), and doing so before the
        // branch tree keeps the control flow simple.
        let raw_value = self.compile_value(&args[1])?;

        let elem_llvm_ty = self.map_type(&elem_ty);
        let elem_size = elem_llvm_ty.size_of().ok_or_else(|| {
            CompileError::unsupported_operation(
                "list element type has no LLVM size (variable-length element?)",
                "llvm",
            )
        })?;
        let elem_size_i64 = self
            .builder
            .build_int_z_extend(elem_size, i64_ty, "la_elem_size")
            .unwrap();

        // Match the record-literal convention: a struct-typed slot
        // stores the struct value, not a pointer to it. Load through
        // the pointer if the value came in as one.
        let value = match (raw_value, &elem_llvm_ty) {
            (BasicValueEnum::PointerValue(p), BasicTypeEnum::StructType(st)) => {
                self.builder.build_load(*st, p, "la_elem_load").unwrap()
            }
            (v, _) => v,
        };

        let current_fn = self.current_function.ok_or_else(|| {
            CompileError::simple(
                "LLVM codegen: emit_list_append with no current function",
                0,
                0,
                "",
                ErrorCode::E0009,
            )
        })?;
        let stack_bb = self.context.append_basic_block(current_fn, "la_stack");
        let spare_check_bb = self
            .context
            .append_basic_block(current_fn, "la_spare_check");
        let grow_bb = self.context.append_basic_block(current_fn, "la_grow");
        let write_bb = self.context.append_basic_block(current_fn, "la_write");
        let done_bb = self.context.append_basic_block(current_fn, "la_done");

        // cap == 0 ? -> stack path : heap path
        let is_stack = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                cap,
                i64_ty.const_zero(),
                "la_is_stack",
            )
            .unwrap();
        self.builder
            .build_conditional_branch(is_stack, stack_bb, spare_check_bb)
            .unwrap();

        // Stack path: malloc(max(2*len, len+1) * elem_size), memcpy.
        self.builder.position_at_end(stack_bb);
        let two_len = self
            .builder
            .build_int_mul(i64_ty.const_int(2, false), len, "la_2len")
            .unwrap();
        let len_plus_one = self
            .builder
            .build_int_add(len, i64_ty.const_int(1, false), "la_len1")
            .unwrap();
        let two_len_bigger = self
            .builder
            .build_int_compare(
                inkwell::IntPredicate::UGT,
                two_len,
                len_plus_one,
                "la_2len_gt",
            )
            .unwrap();
        let new_cap_stack = self
            .builder
            .build_select(two_len_bigger, two_len, len_plus_one, "la_new_cap_stack")
            .unwrap()
            .into_int_value();
        let stack_bytes = self
            .builder
            .build_int_mul(new_cap_stack, elem_size_i64, "la_stack_bytes")
            .unwrap();
        let malloc_fn = self.module.get_function("malloc").ok_or_else(|| {
            CompileError::simple(
                "LLVM codegen: malloc not registered in stdlib",
                0,
                0,
                "",
                ErrorCode::E0009,
            )
        })?;
        let new_buf_stack = match self
            .builder
            .build_call(malloc_fn, &[stack_bytes.into()], "la_malloc")
            .unwrap()
            .try_as_basic_value()
        {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(CompileError::unsupported_operation(
                    "malloc returned no value (unexpected)",
                    "llvm",
                ));
            }
        };
        let copy_bytes = self
            .builder
            .build_int_mul(len, elem_size_i64, "la_copy_bytes")
            .unwrap();
        self.builder
            .build_memcpy(new_buf_stack, 4, buf, 4, copy_bytes)
            .unwrap();
        self.builder.build_unconditional_branch(write_bb).unwrap();

        // Heap spare check: len < cap ?
        self.builder.position_at_end(spare_check_bb);
        let has_room = self
            .builder
            .build_int_compare(inkwell::IntPredicate::ULT, len, cap, "la_has_room")
            .unwrap();
        self.builder
            .build_conditional_branch(has_room, write_bb, grow_bb)
            .unwrap();

        // Grow path: realloc to 2 * cap.
        self.builder.position_at_end(grow_bb);
        let new_cap_grow = self
            .builder
            .build_int_mul(i64_ty.const_int(2, false), cap, "la_2cap")
            .unwrap();
        let grow_bytes = self
            .builder
            .build_int_mul(new_cap_grow, elem_size_i64, "la_grow_bytes")
            .unwrap();
        let realloc_fn = self.module.get_function("realloc").ok_or_else(|| {
            CompileError::simple(
                "LLVM codegen: realloc not registered in stdlib",
                0,
                0,
                "",
                ErrorCode::E0009,
            )
        })?;
        let new_buf_grow = match self
            .builder
            .build_call(realloc_fn, &[buf.into(), grow_bytes.into()], "la_realloc")
            .unwrap()
            .try_as_basic_value()
        {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            inkwell::values::ValueKind::Instruction(_) => {
                return Err(CompileError::unsupported_operation(
                    "realloc returned no value (unexpected)",
                    "llvm",
                ));
            }
        };
        self.builder.build_unconditional_branch(write_bb).unwrap();

        // Write block: phi the buffer/capacity from the three
        // predecessors, write the element at index `len`, store
        // the updated descriptor fields.
        self.builder.position_at_end(write_bb);
        let phi_buf = self.builder.build_phi(ptr_ty, "la_final_buf").unwrap();
        phi_buf.add_incoming(&[
            (&new_buf_stack, stack_bb),
            (&buf, spare_check_bb),
            (&new_buf_grow, grow_bb),
        ]);
        let final_buf = phi_buf.as_basic_value().into_pointer_value();

        let phi_cap = self.builder.build_phi(i64_ty, "la_final_cap").unwrap();
        phi_cap.add_incoming(&[
            (&new_cap_stack, stack_bb),
            (&cap, spare_check_bb),
            (&new_cap_grow, grow_bb),
        ]);
        let final_cap = phi_cap.as_basic_value().into_int_value();

        let elem_ptr = unsafe {
            self.builder
                .build_gep(elem_llvm_ty, final_buf, &[len], "la_elem_ptr")
                .unwrap()
        };
        self.builder.build_store(elem_ptr, value).unwrap();

        let new_len = self
            .builder
            .build_int_add(len, i64_ty.const_int(1, false), "la_new_len")
            .unwrap();
        self.builder.build_store(buf_slot, final_buf).unwrap();
        self.builder.build_store(len_slot, new_len).unwrap();
        self.builder.build_store(cap_slot, final_cap).unwrap();
        self.builder.build_unconditional_branch(done_bb).unwrap();

        self.builder.position_at_end(done_bb);
        Ok(())
    }

    /// ADR 0042 phase 1a. Allocate a `{ptr, i64, i64}` descriptor
    /// struct for a list, store `{buffer, length, 0}` into it, and
    /// return the struct alloca. `capacity == 0` means the buffer
    /// is a stack alloca owned by the enclosing frame.
    ///
    /// Write-only in phase 1a. Phase 2 (runtime length) reads
    /// field 1; phase 4 (`List.append`) writes fields 0 and 2.
    fn emit_list_struct(
        &self,
        name: &str,
        buffer: PointerValue<'ctx>,
        len: usize,
    ) -> PointerValue<'ctx> {
        let ptr_ty = self.context.ptr_type(AddressSpace::default());
        let i64_ty = self.context.i64_type();
        let struct_ty = self
            .context
            .struct_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
        let alloca =
            self.create_entry_alloca_llvm(&format!("{}_list_struct", name), struct_ty.into());
        let buf_slot = self
            .builder
            .build_struct_gep(struct_ty, alloca, 0, &format!("{}_buf_slot", name))
            .unwrap();
        let len_slot = self
            .builder
            .build_struct_gep(struct_ty, alloca, 1, &format!("{}_len_slot", name))
            .unwrap();
        let cap_slot = self
            .builder
            .build_struct_gep(struct_ty, alloca, 2, &format!("{}_cap_slot", name))
            .unwrap();
        self.builder.build_store(buf_slot, buffer).unwrap();
        self.builder
            .build_store(len_slot, i64_ty.const_int(len as u64, false))
            .unwrap();
        self.builder
            .build_store(cap_slot, i64_ty.const_zero())
            .unwrap();
        alloca
    }

    pub(super) fn compile_instruction(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
            Instruction::Nop => Ok(()),
            Instruction::Declare {
                name,
                type_,
                value,
                mutable: _,
            } => {
                if let TypedIRValue::List(elems, elem_ty) = value {
                    let len = elems.len();
                    let elem_llvm_ty = self.map_type(elem_ty);
                    let array_ty = elem_llvm_ty.array_type(len as u32);

                    let arr_alloca = self.create_entry_alloca(
                        &format!("{}_data", name),
                        &Type::Array(Box::new(elem_ty.clone()), len),
                    );

                    for (i, elem) in elems.iter().enumerate() {
                        let ev = self.compile_value(elem)?;
                        // A `Record` literal compiles to a pointer to
                        // its alloca, but a `List<Record>` slot holds
                        // the struct value. Load through the pointer
                        // when the slot's element type is a non-pointer
                        // type. Without this, the store writes the
                        // address of the temp alloca into the struct
                        // slot, and any later read of the element
                        // dereferences garbage.
                        let ev = match (ev, &elem_llvm_ty) {
                            (BasicValueEnum::PointerValue(p), BasicTypeEnum::StructType(st)) => {
                                self.builder
                                    .build_load(*st, p, &format!("{}_elem_load_{}", name, i))
                                    .unwrap()
                            }
                            (v, _) => v,
                        };
                        let idx = self.context.i32_type().const_int(i as u64, false);
                        let ptr = unsafe {
                            self.builder
                                .build_gep(
                                    array_ty,
                                    arr_alloca,
                                    &[self.context.i32_type().const_zero(), idx],
                                    &format!("{}_gep_{}", name, i),
                                )
                                .unwrap()
                        };
                        self.builder.build_store(ptr, ev).unwrap();
                    }

                    // For lists, the variable's "value" is the array pointer itself.
                    // Register both the array bookkeeping and the variable name.
                    self.list_arrays.insert(name.clone(), arr_alloca);
                    // ADR 0021. Register the array's LLVM type
                    // alongside its pointer. Without this, a later
                    // IteratorInit over this variable misses the
                    // type and falls back to a wrong default.
                    self.list_array_types.insert(name.clone(), array_ty.into());
                    self.list_lengths.insert(name.clone(), len);
                    self.variables.insert(name.clone(), arr_alloca);
                    self.var_types.insert(name.clone(), type_.clone());
                    // ADR 0042 phase 1a: populate the descriptor
                    // struct. Write-only for now.
                    let list_struct = self.emit_list_struct(name, arr_alloca, len);
                    self.list_structs.insert(name.clone(), list_struct);
                    // ADR 0050.
                    let st = match self.map_type(type_) {
                        BasicTypeEnum::StructType(s) => s,
                        _ => unreachable!("map_type(Type::List) returned non-struct"),
                    };
                    self.register_region_list(name, st, list_struct);
                    return Ok(());
                }

                if let TypedIRValue::Record { .. } = value {
                    // ADR 0036 L4. A record's storage IS the alloca
                    // that `compile_value(Record{...})` creates (L2).
                    // Storing that pointer in a second alloca would
                    // try to store a `%Point*` into a `%Point` slot;
                    // instead, `variables[name]` holds the struct
                    // pointer directly. Consumers that want the
                    // pointer (FieldAccess, FieldAssign) look it up
                    // via `self.variables` without a load.
                    let val = self.compile_value(value)?;
                    if !val.is_pointer_value() {
                        return Err(CompileError::simple(
                            "LLVM codegen: record literal did not produce a pointer",
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                    self.variables
                        .insert(name.clone(), val.into_pointer_value());
                    self.var_types.insert(name.clone(), type_.clone());
                    return Ok(());
                }

                // Call-result handoff. A struct-returning call —
                // `val a := find(5)` where `find() -> Option<Int>`
                // — is emitted by the IR builder as
                //     Call { result: Some("__t") }
                //     Declare { a, value: Variable("__t") }
                // The Call handler stores the struct into a fresh
                // alloca but registers `var_types["__t"] = Float` —
                // a placeholder from before struct returns existed,
                // because the Call instruction carries no ALGOL26
                // type. If the Declare's annotation is *not* Float
                // but the source's recorded type *is*, the source
                // is that placeholder. Alias the two names to the
                // same alloca and give the new name the correct
                // type: no load, no store, no reinterpretation.
                //
                // The proper fix is for `Instruction::Call` to carry
                // its result's ALGOL26 type; until then, this detects
                // the placeholder by the mismatch.
                if let TypedIRValue::Variable(src, _) = value {
                    if matches!(self.var_types.get(src), Some(Type::Float))
                        && !matches!(type_, Type::Float)
                    {
                        if let Some(src_alloca) = self.variables.get(src).copied() {
                            self.variables.insert(name.clone(), src_alloca);
                            self.var_types.insert(name.clone(), type_.clone());
                            return Ok(());
                        }
                    }
                }

                // List move: `var b := a` where `a` is a list
                // variable. `List<T>` is non-Copy and the analyzer
                // enforces move semantics (`E0007` on any later use
                // of `a`), so this is safe to lower as an alias: copy
                // `a`'s pointer and the three bookkeeping entries
                // under `b`. Any subsequent `b[i]` or `for x in b`
                // then finds them in `list_arrays` / `list_array_types`
                // / `list_lengths`, which the scalar path below would
                // have left unpopulated (leading to E0004 "unknown
                // list" at the first use of `b`).
                if let TypedIRValue::Variable(src, _) = value {
                    // Only take the alias shortcut when the source
                    // already has a registered array type. A Call
                    // result registered by `Instruction::Call` sets
                    // `list_structs` and `list_arrays` but not
                    // `list_array_types` (the Call has no element
                    // type). Falling through to the phase-4-ext
                    // path below lets that path read the concrete
                    // `type_` and register the array type correctly.
                    if matches!(self.var_types.get(src), Some(Type::List(_)))
                        && self.list_array_types.contains_key(src.as_str())
                    {
                        let src_ptr = self.variables.get(src).copied().ok_or_else(|| {
                            CompileError::simple(
                                &format!(
                                    "LLVM codegen: list move from undefined variable `{}`",
                                    src
                                ),
                                0,
                                0,
                                "",
                                ErrorCode::E0004,
                            )
                        })?;
                        let src_ty = self.var_types.get(src).cloned().unwrap();
                        let src_ty_clone = src_ty.clone();
                        let src_arr_ty = self.list_array_types.get(src).cloned();
                        let src_len = self.list_lengths.get(src).copied();

                        self.variables.insert(name.clone(), src_ptr);
                        self.var_types.insert(name.clone(), src_ty);
                        if let Some(arr_ty) = src_arr_ty {
                            self.list_arrays.insert(name.clone(), src_ptr);
                            self.list_array_types.insert(name.clone(), arr_ty);
                        }
                        if let Some(len) = src_len {
                            self.list_lengths.insert(name.clone(), len);
                        }
                        // ADR 0042 phase 1a: share the descriptor
                        // struct with the source. Safe because the
                        // analyzer guarantees `src` is dead after
                        // the move.
                        if let Some(struct_alloca) = self.list_structs.get(src).copied() {
                            self.list_structs.insert(name.clone(), struct_alloca);
                            // ADR 0050.
                            let st = match self.map_type(&src_ty_clone) {
                                BasicTypeEnum::StructType(s) => s,
                                _ => unreachable!("map_type(Type::List) returned non-struct"),
                            };
                            self.register_region_list(name, st, struct_alloca);
                        }
                        return Ok(());
                    }
                }
                // ADR 0042 phase 1b (ext). A list-typed declaration
                // whose value is neither a literal nor a variable
                // move — e.g. the result of a function call returning
                // List<T>. The value compiles to the
                // {buffer, length, capacity} struct. Store it into a
                // fresh descriptor alloca and extract the buffer
                // field so downstream indexing can find it.
                if matches!(type_, Type::List(_)) {
                    // ADR 0042 phase 1b (ext). If the value is a
                    // variable whose alloca already holds a
                    // {ptr, i64, i64} struct (produced by a preceding
                    // Instruction::Call that returned a list), adopt
                    // that alloca. This is the `val c := make()` case
                    // where `make() -> List<T>`.
                    if let TypedIRValue::Variable(src, _) = value {
                        if self.list_structs.contains_key(src.as_str()) {
                            if let Some(src_alloca) = self.list_structs.get(src).copied() {
                                let st = match self.map_type(type_) {
                                    BasicTypeEnum::StructType(s) => s,
                                    _ => unreachable!("map_type(Type::List) returned non-struct"),
                                };
                                if st.count_fields() == 3 {
                                    self.list_structs.insert(name.clone(), src_alloca);
                                    self.variables.insert(name.clone(), src_alloca);
                                    self.var_types.insert(name.clone(), type_.clone());
                                    let ptr_ty = self.context.ptr_type(AddressSpace::default());
                                    let buf_slot = self
                                        .builder
                                        .build_struct_gep(
                                            st,
                                            src_alloca,
                                            0,
                                            &format!("{}_buf_slot", name),
                                        )
                                        .unwrap();
                                    let buf = self
                                        .builder
                                        .build_load(ptr_ty, buf_slot, &format!("{}_buf", name))
                                        .unwrap()
                                        .into_pointer_value();
                                    self.list_arrays.insert(name.clone(), buf);
                                    if let Type::List(inner) = type_ {
                                        let elem_llvm = self.map_type(inner);
                                        let arr_ty: BasicTypeEnum<'ctx> = match elem_llvm {
                                            BasicTypeEnum::IntType(t) => t.array_type(0).into(),
                                            BasicTypeEnum::FloatType(t) => t.array_type(0).into(),
                                            BasicTypeEnum::PointerType(t) => t.array_type(0).into(),
                                            BasicTypeEnum::StructType(t) => t.array_type(0).into(),
                                            other => {
                                                return Err(CompileError::unsupported_operation(
                                                        &format!(
                                                            "LLVM codegen: list `{}` has element type {:?} with no LLVM array lowering",
                                                            name, other
                                                        ),
                                                        "llvm",
                                                    ));
                                            }
                                        };
                                        self.list_array_types.insert(name.clone(), arr_ty);
                                    }
                                    // ADR 0050.
                                    self.register_region_list(name, st, src_alloca);
                                    return Ok(());
                                }
                            }
                        }
                    }
                    let val = self.compile_value(value)?;
                    if !val.is_struct_value() {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: list declaration `{}` expected a \
                                 struct value, got LLVM kind {:?}",
                                name, val
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                    let st = val.into_struct_value();
                    let struct_ty = match self.map_type(type_) {
                        BasicTypeEnum::StructType(s) => s,
                        _ => unreachable!("map_type(Type::List) returned non-struct"),
                    };
                    let alloca = self.create_entry_alloca(name, type_);
                    self.builder.build_store(alloca, st).unwrap();
                    self.variables.insert(name.clone(), alloca);
                    self.var_types.insert(name.clone(), type_.clone());
                    self.list_structs.insert(name.clone(), alloca);

                    let ptr_ty = self.context.ptr_type(AddressSpace::default());
                    let buf_slot = self
                        .builder
                        .build_struct_gep(struct_ty, alloca, 0, &format!("{}_buf_slot", name))
                        .unwrap();
                    let buf = self
                        .builder
                        .build_load(ptr_ty, buf_slot, &format!("{}_buf", name))
                        .unwrap()
                        .into_pointer_value();
                    self.list_arrays.insert(name.clone(), buf);

                    if let Type::List(inner) = type_ {
                        let elem_llvm = self.map_type(inner);
                        let arr_ty: BasicTypeEnum<'ctx> = match elem_llvm {
                            BasicTypeEnum::IntType(t) => t.array_type(0).into(),
                            BasicTypeEnum::FloatType(t) => t.array_type(0).into(),
                            BasicTypeEnum::PointerType(t) => t.array_type(0).into(),
                            BasicTypeEnum::StructType(t) => t.array_type(0).into(),
                            other => {
                                return Err(CompileError::unsupported_operation(
                                    &format!(
                                        "LLVM codegen: list `{}` has element type \
                                         {:?} with no LLVM array lowering",
                                        name, other
                                    ),
                                    "llvm",
                                ));
                            }
                        };
                        self.list_array_types.insert(name.clone(), arr_ty);
                    }
                    return Ok(());
                }
                // Non-list: single alloca, straightforward store.
                let alloca = self.create_entry_alloca(name, type_);
                let val = self.compile_value(value)?;
                self.builder.build_store(alloca, val).unwrap();
                self.variables.insert(name.clone(), alloca);
                self.var_types.insert(name.clone(), type_.clone());
                Ok(())
            }
            Instruction::Assign { target, value } => {
                // A list assignment must mirror `Declare`'s list
                // path: allocate a fresh array, populate it, and
                // update `list_arrays`, `list_array_types`, and
                // `list_lengths` together. Updating only
                // `list_lengths` left `list_arrays[target]`
                // pointing at the *old* array — subsequent
                // indexing read from stale memory.
                if let TypedIRValue::List(elems, elem_ty) = value {
                    let len = elems.len();
                    let elem_llvm_ty = self.map_type(elem_ty);
                    let array_ty = elem_llvm_ty.array_type(len as u32);
                    let arr_alloca = self.create_entry_alloca(
                        &format!("{}_data", target),
                        &Type::Array(Box::new(elem_ty.clone()), len),
                    );
                    for (i, elem) in elems.iter().enumerate() {
                        let ev = self.compile_value(elem)?;
                        // A record literal compiles to a pointer to
                        // its alloca, but a struct-typed slot holds
                        // the struct value. Load through the pointer
                        // when the slot expects a struct, matching
                        // the fix in `Declare`'s list path.
                        let ev = match (ev, &elem_llvm_ty) {
                            (BasicValueEnum::PointerValue(p), BasicTypeEnum::StructType(st)) => {
                                self.builder
                                    .build_load(*st, p, &format!("{}_assign_load_{}", target, i))
                                    .unwrap()
                            }
                            (v, _) => v,
                        };
                        let idx = self.context.i32_type().const_int(i as u64, false);
                        let ptr = unsafe {
                            self.builder
                                .build_gep(
                                    array_ty,
                                    arr_alloca,
                                    &[self.context.i32_type().const_zero(), idx],
                                    &format!("{}_assign_gep_{}", target, i),
                                )
                                .unwrap()
                        };
                        self.builder.build_store(ptr, ev).unwrap();
                    }
                    self.list_arrays.insert(target.clone(), arr_alloca);
                    self.list_array_types
                        .insert(target.clone(), array_ty.into());
                    self.list_lengths.insert(target.clone(), len);
                    self.variables.insert(target.clone(), arr_alloca);
                    // ADR 0042 phase 1a: populate the descriptor
                    // struct.
                    let list_struct = self.emit_list_struct(target, arr_alloca, len);
                    self.list_structs.insert(target.clone(), list_struct);
                    // ADR 0050.
                    let target_ty_for_reg = self
                        .var_types
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| Type::list(Type::Unknown));
                    let st = match self.map_type(&target_ty_for_reg) {
                        BasicTypeEnum::StructType(s) => s,
                        _ => unreachable!("map_type(Type::List) returned non-struct"),
                    };
                    self.register_region_list(target, st, list_struct);
                    return Ok(());
                }
                if let TypedIRValue::Record { .. } = value {
                    let val = self.compile_value(value)?;
                    if !val.is_pointer_value() {
                        return Err(CompileError::simple(
                            "LLVM codegen: record reassignment did not produce a pointer",
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                    self.variables
                        .insert(target.clone(), val.into_pointer_value());
                    return Ok(());
                }
                // List move: `b := a` where both are list variables.
                // Same reasoning as Declare's list-variable arm above:
                // alias the source's pointer and bookkeeping under the
                // target name. The analyzer's move rules guarantee
                // `a` is dead after this instruction, so aliasing is
                // safe; without it, `list_arrays[target]` is never
                // populated and the next `target[i]` fails with E0004.
                if let TypedIRValue::Variable(src, _) = value {
                    // Only take the alias shortcut when the source
                    // already has a registered array type. A Call
                    // result registered by `Instruction::Call` sets
                    // `list_structs` and `list_arrays` but not
                    // `list_array_types` (the Call has no element
                    // type). Falling through to the phase-4-ext
                    // path below lets that path read the concrete
                    // `type_` and register the array type correctly.
                    if matches!(self.var_types.get(src), Some(Type::List(_)))
                        && self.list_array_types.contains_key(src.as_str())
                    {
                        let src_ptr = self.variables.get(src).copied().ok_or_else(|| {
                            CompileError::simple(
                                &format!(
                                    "LLVM codegen: list move from undefined variable `{}`",
                                    src
                                ),
                                0,
                                0,
                                "",
                                ErrorCode::E0004,
                            )
                        })?;
                        let src_ty = self.var_types.get(src).cloned().unwrap();
                        let src_ty_clone = src_ty.clone();
                        let src_arr_ty = self.list_array_types.get(src).cloned();
                        let src_len = self.list_lengths.get(src).copied();

                        self.variables.insert(target.clone(), src_ptr);
                        self.var_types.insert(target.clone(), src_ty);
                        if let Some(arr_ty) = src_arr_ty {
                            self.list_arrays.insert(target.clone(), src_ptr);
                            self.list_array_types.insert(target.clone(), arr_ty);
                        }
                        if let Some(len) = src_len {
                            self.list_lengths.insert(target.clone(), len);
                        }
                        // ADR 0042 phase 1a: share the descriptor
                        // struct with the source.
                        if let Some(struct_alloca) = self.list_structs.get(src).copied() {
                            self.list_structs.insert(target.clone(), struct_alloca);
                            // ADR 0050.
                            let st = match self.map_type(&src_ty_clone) {
                                BasicTypeEnum::StructType(s) => s,
                                _ => unreachable!("map_type(Type::List) returned non-struct"),
                            };
                            self.register_region_list(target, st, struct_alloca);
                        }
                        return Ok(());
                    }
                }
                // ADR 0042 phase 1b (ext). A list-typed assignment
                // whose value is neither a literal nor a variable
                // move — e.g. a call result. Reuse the target's
                // existing descriptor alloca if it has one, else
                // create a fresh one; store the incoming struct and
                // refresh the buffer mapping.
                if matches!(self.var_types.get(target), Some(Type::List(_))) {
                    let val = self.compile_value(value)?;
                    if !val.is_struct_value() {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: list assignment to `{}` expected \
                                 a struct value, got LLVM kind {:?}",
                                target, val
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                    let st = val.into_struct_value();
                    let target_ty = self.var_types.get(target).cloned().unwrap();
                    let struct_ty = match self.map_type(&target_ty) {
                        BasicTypeEnum::StructType(s) => s,
                        _ => unreachable!("map_type(Type::List) returned non-struct"),
                    };
                    let alloca = match self.list_structs.get(target).copied() {
                        Some(a) => a,
                        None => {
                            let a = self.create_entry_alloca(target, &target_ty);
                            self.list_structs.insert(target.clone(), a);
                            self.variables.insert(target.clone(), a);
                            a
                        }
                    };
                    self.builder.build_store(alloca, st).unwrap();

                    let ptr_ty = self.context.ptr_type(AddressSpace::default());
                    let buf_slot = self
                        .builder
                        .build_struct_gep(struct_ty, alloca, 0, &format!("{}_buf_slot", target))
                        .unwrap();
                    let buf = self
                        .builder
                        .build_load(ptr_ty, buf_slot, &format!("{}_buf", target))
                        .unwrap()
                        .into_pointer_value();
                    self.list_arrays.insert(target.clone(), buf);

                    if let Type::List(inner) = &target_ty {
                        let elem_llvm = self.map_type(inner);
                        let arr_ty: BasicTypeEnum<'ctx> = match elem_llvm {
                            BasicTypeEnum::IntType(t) => t.array_type(0).into(),
                            BasicTypeEnum::FloatType(t) => t.array_type(0).into(),
                            BasicTypeEnum::PointerType(t) => t.array_type(0).into(),
                            BasicTypeEnum::StructType(t) => t.array_type(0).into(),
                            other => {
                                return Err(CompileError::unsupported_operation(
                                    &format!(
                                        "LLVM codegen: list `{}` has element type \
                                         {:?} with no LLVM array lowering",
                                        target, other
                                    ),
                                    "llvm",
                                ));
                            }
                        };
                        self.list_array_types.insert(target.clone(), arr_ty);
                    }
                    return Ok(());
                }
                let ptr = self.variables.get(target).cloned().ok_or_else(|| {
                    CompileError::simple(
                        &format!("var {} not found", target),
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    )
                })?;
                let val = self.compile_value(value)?;
                self.builder.build_store(ptr, val).unwrap();
                Ok(())
            }
            Instruction::WriteReference { reference, value } => {
                // Load the pointer held by the reference variable,
                // then store `value` through it.
                let ptr_val = self.compile_value(reference)?;
                if !ptr_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        &format!(
                            "WriteReference reference did not lower to a pointer \
                             (kind: {:?})",
                            ptr_val
                        ),
                        "llvm",
                    ));
                }
                let target_ptr = ptr_val.into_pointer_value();
                let val = self.compile_value(value)?;
                self.builder.build_store(target_ptr, val).unwrap();
                Ok(())
            }
            Instruction::ArrayAssign {
                array,
                index,
                value,
            } => {
                let arr_name = match array.as_ref() {
                    TypedIRValue::Variable(n, _) => n.clone(),
                    other => {
                        // A non-variable array in ArrayAssign means the
                        // IR builder emitted an array write to something
                        // that is not a place. Silently dropping the
                        // write would produce wrong code; error instead.
                        return Err(CompileError::unsupported_operation(
                            &format!("ArrayAssign to non-variable array expression: {:?}", other),
                            "llvm",
                        ));
                    }
                };
                let idx_val = self.compile_value(index)?;
                // ADR 0042 phase 2. Same as the sibling site in
                // `value.rs`: bounds checking now works for every
                // list, including parameters, using the descriptor
                // struct as the length source when no compile-time
                // hint exists.
                if idx_val.is_int_value() {
                    let idx_int = idx_val.into_int_value();
                    let len_val = self.list_length_value(&arr_name).ok_or_else(|| {
                        CompileError::simple(
                            &format!(
                                "LLVM codegen: bounds check on `{}` has no \
                                 length source (no `list_lengths` entry and \
                                 no `list_structs` descriptor)",
                                arr_name
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                    let zero = self.context.i64_type().const_int(0, false);

                    let is_negative = self
                        .builder
                        .build_int_compare(
                            inkwell::IntPredicate::SLT,
                            idx_int,
                            zero,
                            "bounds_check_neg",
                        )
                        .unwrap();
                    let is_too_big = self
                        .builder
                        .build_int_compare(
                            inkwell::IntPredicate::SGE,
                            idx_int,
                            len_val,
                            "bounds_check_big",
                        )
                        .unwrap();
                    let out_of_bounds = self
                        .builder
                        .build_or(is_negative, is_too_big, "oob")
                        .unwrap();

                    let error_bb = self
                        .context
                        .append_basic_block(self.current_function.unwrap(), "bounds_error_write");
                    let continue_bb = self
                        .context
                        .append_basic_block(self.current_function.unwrap(), "bounds_ok_write");

                    self.builder
                        .build_conditional_branch(out_of_bounds, error_bb, continue_bb)
                        .unwrap();

                    self.builder.position_at_end(error_bb);
                    let error_msg = self
                        .builder
                        .build_global_string_ptr(
                            "Error: Array index out of bounds\n",
                            "bounds_err_msg_write",
                        )
                        .unwrap();
                    let printf_fn = self.module.get_function("printf").unwrap();
                    self.builder
                        .build_call(
                            printf_fn,
                            &[error_msg.as_pointer_value().into()],
                            "print_bounds_error",
                        )
                        .unwrap();

                    // ADR 0042 phase 2. Same change as the sibling
                    // site in `value.rs`: the OOB error block calls
                    // `exit(1)` instead of emitting a `return`.
                    let exit_fn = self.module.get_function("exit").ok_or_else(|| {
                        CompileError::simple(
                            "LLVM codegen: exit not registered in stdlib",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                    let status = self.context.i32_type().const_int(1, false);
                    self.builder
                        .build_call(exit_fn, &[status.into()], "oob_write_exit")
                        .unwrap();
                    self.builder.build_unreachable().unwrap();

                    self.builder.position_at_end(continue_bb);
                }
                let val = self.compile_value(value)?;
                // ADR 0042 phase 4. Runtime buffer load when a
                // descriptor exists.
                let arr_ptr = self.list_buffer_value(&arr_name).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "ArrayAssign on `{}` which is not a tracked list \
                             (known lists: {:?})",
                            arr_name,
                            self.list_arrays.keys().collect::<Vec<_>>()
                        ),
                        "llvm",
                    )
                })?;
                let array_ty = self
                    .list_array_types
                    .get(&arr_name)
                    .cloned()
                    .unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                let idx_i32 = if idx_val.is_int_value() {
                    let iv = idx_val.into_int_value();
                    if iv.get_type().get_bit_width() != 32 {
                        self.builder
                            .build_int_cast(iv, self.context.i32_type(), "idx32")
                            .unwrap()
                    } else {
                        iv
                    }
                } else {
                    self.context.i32_type().const_zero()
                };
                let gep = unsafe {
                    self.builder
                        .build_gep(
                            array_ty,
                            arr_ptr,
                            &[self.context.i32_type().const_zero(), idx_i32],
                            "arr_gep",
                        )
                        .unwrap()
                };
                // A record literal compiles to a pointer to its
                // alloca, but a struct-typed slot holds the struct
                // value. Load through the pointer when the element
                // type is a struct. Same fix as the list-literal
                // and iterator-init paths.
                let val = {
                    let elem_ty = match array_ty {
                        BasicTypeEnum::ArrayType(at) => at.get_element_type(),
                        _ => array_ty,
                    };
                    match (val, &elem_ty) {
                        (BasicValueEnum::PointerValue(p), BasicTypeEnum::StructType(st)) => self
                            .builder
                            .build_load(*st, p, "arr_assign_elem_load")
                            .unwrap(),
                        (v, _) => v,
                    }
                };
                self.builder.build_store(gep, val).unwrap();
                Ok(())
            }
            Instruction::Print { value } => {
                let v = self.compile_value(value)?;
                self.emit_print(v, value.type_of())?;
                Ok(())
            }
            Instruction::Call { func, args, result } => {
                // Propagate errors from argument compilation — was
                // previously `.unwrap()`, which panicked on any
                // nested codegen failure instead of surfacing it.
                let arg_vals: Vec<BasicValueEnum> = args
                    .iter()
                    .map(|a| self.compile_value(a))
                    .collect::<Result<Vec<_>>>()?;
                let callee_name = func.trim_end_matches("()").to_string();
                let llvm_name = resolve_math_name(&callee_name)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| callee_name.clone());
                if let Some(callee) = self
                    .functions
                    .get(&llvm_name)
                    .cloned()
                    .or_else(|| self.module.get_function(&llvm_name))
                {
                    // Coerce args to the callee's LLVM parameter types.
                    // The most common case: a method's `&self` parameter
                    // is `ptr` in LLVM, but `compile_value` may have
                    // produced a struct value for a field-access receiver.
                    let coerced: Vec<BasicValueEnum> = arg_vals
                        .iter()
                        .enumerate()
                        .map(|(i, v)| self.coerce_arg_to_param(callee, i, *v))
                        .collect::<Result<Vec<_>>>()?;
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                        coerced.iter().map(|v| (*v).into()).collect();
                    let call_site = self
                        .builder
                        .build_call(callee, &call_args, "calltmp")
                        .unwrap();
                    if let Some(res_name) = result {
                        match call_site.try_as_basic_value() {
                            inkwell::values::ValueKind::Basic(ret) => {
                                if let Some(ptr) = self.variables.get(res_name).cloned() {
                                    self.builder.build_store(ptr, ret).unwrap();
                                } else {
                                    // Struct-returning calls (records,
                                    // Option, Result) need an alloca of
                                    // the actual struct type — a `f64`
                                    // scratch slot is only 8 bytes and
                                    // overflows on multi-field records.
                                    // The subsequent `Declare` copies
                                    // through a properly-typed alloca;
                                    // this one just has to be big enough.
                                    let alloca = if ret.is_struct_value() {
                                        self.create_entry_alloca_llvm(res_name, ret.get_type())
                                    } else {
                                        self.create_entry_alloca(res_name, &Type::Float)
                                    };
                                    self.builder.build_store(alloca, ret).unwrap();
                                    self.variables.insert(res_name.clone(), alloca);
                                    // ADR 0042 phase 1b (ext). A
                                    // list-returning call yields a
                                    // {ptr, i64, i64} struct. Detect
                                    // the shape and register the
                                    // descriptor so the subsequent
                                    // Declare can adopt it. The
                                    // element type is unknown here
                                    // (the IR instruction carries no
                                    // return type), so var_types is
                                    // set to List<Unknown> — the
                                    // Declare updates it with the
                                    // concrete element type.
                                    let mut is_list_shape = false;
                                    if let BasicTypeEnum::StructType(st) = ret.get_type() {
                                        if st.count_fields() == 3 {
                                            is_list_shape = true;
                                            self.list_structs.insert(res_name.clone(), alloca);
                                            let ptr_ty =
                                                self.context.ptr_type(AddressSpace::default());
                                            let buf_slot = self
                                                .builder
                                                .build_struct_gep(
                                                    st,
                                                    alloca,
                                                    0,
                                                    &format!("{}_buf_slot", res_name),
                                                )
                                                .unwrap();
                                            let buf = self
                                                .builder
                                                .build_load(
                                                    ptr_ty,
                                                    buf_slot,
                                                    &format!("{}_buf", res_name),
                                                )
                                                .unwrap()
                                                .into_pointer_value();
                                            self.list_arrays.insert(res_name.clone(), buf);
                                        }
                                    }
                                    if is_list_shape {
                                        self.var_types
                                            .insert(res_name.clone(), Type::list(Type::Unknown));
                                    } else {
                                        self.var_types.insert(res_name.clone(), Type::Float);
                                    }
                                }
                            }
                            inkwell::values::ValueKind::Instruction(_) => {
                                // The IR says this call produces a
                                // value bound to `res_name`, but LLVM
                                // says the callee returns void. A
                                // mismatch; fail closed.
                                return Err(CompileError::unsupported_operation(
                                    &format!(
                                        "call to `{}` bound to result `{}` but \
                                         LLVM reports the call produces no value",
                                        callee_name, res_name
                                    ),
                                    "llvm",
                                ));
                            }
                        }
                    }
                } else if callee_name == "List.append" {
                    // ADR 0042 phase 4.
                    self.emit_list_append(args)?;
                } else if callee_name == "print" || callee_name == "println" {
                    if let Some(first) = arg_vals.first() {
                        // `arg_vals` is `args.map(compile_value)`, so if
                        // `arg_vals.first()` is Some, `args.first()` is
                        // Some too. The old `unwrap_or(Type::Float)` was
                        // a fail-open: if the two ever diverged it would
                        // print the first argument as if it were a Float.
                        let ty = args
                            .first()
                            .expect("arg_vals has an element but args does not")
                            .type_of();
                        self.emit_print(*first, ty)?;
                    }
                } else {
                    self.compile_builtin_call(&callee_name, args, result)?;
                }
                Ok(())
            }
            Instruction::VirtualCall {
                receiver,
                method_name,
                slot,
                args,
                result,
                return_type,
            } => {
                // ADR 0038 D4b. Load the fat pointer's two halves,
                // GEP to slot `slot` of the vtable, load the method
                // pointer, then build an indirect call with `data`
                // as the receiver argument.
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let fat_ty = self
                    .context
                    .struct_type(&[ptr_ty.into(), ptr_ty.into()], false);

                // Load the receiver's slot: `s` holds a `ptr` to
                // the fat pointer struct (because `compile_value`
                // for `DynTrait` returns the alloca address of that
                // struct). Loading yields that pointer; the GEPs
                // below index into the struct it points to.
                //
                // Previously this used `compile_reference`, which
                // returned `&s` — the address of the 8-byte slot.
                // GEP at slot 1 then read 8 bytes past the slot,
                // and the indirect call crashed (exit 96).
                let fat_val = self.compile_value(receiver)?;
                let fat_ptr = fat_val.into_pointer_value();
                let data_slot = self
                    .builder
                    .build_struct_gep(fat_ty, fat_ptr, 0, "dyn_data_slot")
                    .unwrap();
                let vtable_slot = self
                    .builder
                    .build_struct_gep(fat_ty, fat_ptr, 1, "dyn_vtable_slot")
                    .unwrap();
                let data_ptr = self
                    .builder
                    .build_load(ptr_ty, data_slot, "dyn_data")
                    .unwrap()
                    .into_pointer_value();
                let vtable_ptr = self
                    .builder
                    .build_load(ptr_ty, vtable_slot, "dyn_vtable")
                    .unwrap()
                    .into_pointer_value();

                let slot_const = self.context.i32_type().const_int(*slot as u64, false);
                let method_slot_ptr = unsafe {
                    self.builder
                        .build_gep(ptr_ty, vtable_ptr, &[slot_const], "method_slot")
                        .unwrap()
                };
                let method_fn = self
                    .builder
                    .build_load(ptr_ty, method_slot_ptr, "method_fn")
                    .unwrap()
                    .into_pointer_value();

                // Build the indirect-call's function type: `(ptr,
                // arg_ts...) -> ret`. `ret` comes from
                // `return_type`; a `Void` return means no value.
                let ret_ty = match return_type {
                    Type::Void => None,
                    other => Some(self.map_type(other)),
                };
                let mut param_types: Vec<inkwell::types::BasicMetadataTypeEnum> =
                    vec![ptr_ty.into()];
                let mut arg_vals: Vec<BasicValueEnum> = Vec::with_capacity(args.len() + 1);
                arg_vals.push(data_ptr.into());
                for a in args {
                    let v = self.compile_value(a)?;
                    param_types.push(v.get_type().into());
                    arg_vals.push(v);
                }
                let fn_ty = match ret_ty {
                    Some(rt) => rt.fn_type(&param_types, false),
                    None => self.context.void_type().fn_type(&param_types, false),
                };

                let call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                    arg_vals.iter().map(|v| (*v).into()).collect();
                let call_site = self
                    .builder
                    .build_indirect_call(fn_ty, method_fn, &call_args, "vcall")
                    .unwrap();

                if let Some(res_name) = result {
                    match call_site.try_as_basic_value() {
                        inkwell::values::ValueKind::Basic(ret) => {
                            if let Some(ptr) = self.variables.get(res_name).cloned() {
                                self.builder.build_store(ptr, ret).unwrap();
                            } else {
                                let alloca = if ret.is_struct_value() {
                                    self.create_entry_alloca_llvm(res_name, ret.get_type())
                                } else {
                                    self.create_entry_alloca(res_name, &Type::Float)
                                };
                                self.builder.build_store(alloca, ret).unwrap();
                                self.variables.insert(res_name.clone(), alloca);
                                self.var_types.insert(res_name.clone(), return_type.clone());
                            }
                        }
                        inkwell::values::ValueKind::Instruction(_) => {
                            // IR says there's a result but the method
                            // returns void. The analyzer's return
                            // type disagrees with the impl; that is a
                            // compiler-internal inconsistency.
                            return Err(CompileError::unsupported_operation(
                                &format!(
                                    "vcall `{}` bound to result `{}` but \
                                     method returns void",
                                    method_name, res_name
                                ),
                                "llvm",
                            ));
                        }
                    }
                }
                Ok(())
            }
            Instruction::IteratorInit { iterator, iterable } => {
                let arr_name_opt = match iterable {
                    TypedIRValue::Variable(n, _) => Some(n.clone()),
                    _ => None,
                };
                if let Some(arr_name) = arr_name_opt {
                    // ADR 0042 follow-up. Read the buffer from the
                    // descriptor, not `list_arrays`. The table is
                    // populated at `Declare` time with the original
                    // stack alloca; a subsequent `.append` grows to
                    // a heap buffer and updates the descriptor but
                    // not the table. Using the stale pointer reads
                    // garbage past the original alloca. Same fix as
                    // `ArrayAccess`, `ArrayAssign`, and
                    // `compile_reference` already carry.
                    let arr_ptr = self.list_buffer_value(&arr_name).ok_or_else(|| {
                        CompileError::unsupported_operation(
                            &format!(
                                "IteratorInit on `{}` which is not a tracked list \
                                 (known lists: {:?})",
                                arr_name,
                                self.list_arrays.keys().collect::<Vec<_>>()
                            ),
                            "llvm",
                        )
                    })?;
                    // ADR 0021. The array's LLVM type must have
                    // been registered when the array was created
                    // (Declare, Assign, or a prior IteratorInit).
                    // A default here silently treats the array as
                    // length-0 f64, which produces wrong IR and
                    // can fail LLVM verification with a confusing
                    // type-mismatch. Fail closed with an internal
                    // invariant error instead.
                    let arr_ty =
                        self.list_array_types
                            .get(&arr_name)
                            .cloned()
                            .ok_or_else(|| {
                                CompileError::simple(
                                    &format!(
                                        "LLVM codegen: IteratorInit over `{}` but its \
                                     LLVM array type was never registered. The \
                                     producer that created `{}` did not record \
                                     its array type in `list_array_types`.",
                                        arr_name, arr_name
                                    ),
                                    0,
                                    0,
                                    "",
                                    ErrorCode::E0009,
                                )
                            })?;
                    self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                    self.iterator_array_types.insert(iterator.clone(), arr_ty);
                    // Record the ALGOL26 element type so `IteratorNext`
                    // can bind the loop variable without reverse-
                    // mapping an LLVM struct type back to ALGOL26.
                    //
                    // The `list_arrays` / `list_array_types` lookup
                    // above proved `arr_name` is a tracked list, so
                    // `var_types[arr_name]` must be `List<_>` — the
                    // codegen's invariant is that every name in
                    // `list_arrays` has a matching `List<_>` entry in
                    // `var_types`. A miss here is a producer bug;
                    // silently binding the loop variable to `Unknown`
                    // would let the wrong type through undetected.
                    let arr_elem_ty = self
                        .var_types
                        .get(&arr_name)
                        .and_then(|t| match t {
                            Type::List(inner) => Some((**inner).clone()),
                            _ => None,
                        })
                        .ok_or_else(|| {
                            CompileError::simple(
                                &format!(
                                    "LLVM codegen: iterator over `{}` but its \
                                     ALGOL26 element type is not recorded in \
                                     `var_types` as a `List<_>`. A producer \
                                     registered `{}` in `list_arrays` without \
                                     setting `var_types[{}]` to a list type; \
                                     this is a compiler bug.",
                                    arr_name, arr_name, arr_name
                                ),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                    self.iterator_elem_types
                        .insert(iterator.clone(), arr_elem_ty);
                    // ADR 0042 phase 2. Iteration bound is either
                    // a compile-time constant (a stack-backed list
                    // literal, tracked in `list_lengths`) or a
                    // runtime load from the descriptor's length
                    // field (a list variable or parameter — phase
                    // 1a/1b put the descriptor in `list_structs`
                    // but only a literal gets a `list_lengths`
                    // entry). The old "cannot iterate list
                    // parameter" refusal is gone.
                    // ADR 0042 phase 4. Descriptor first: it holds
                    // the runtime length and is updated by
                    // `.append`. `list_lengths` is a construction-
                    // time hint, stale after any append.
                    let len_val: inkwell::values::IntValue<'ctx> = if let Some(struct_alloca) =
                        self.list_structs.get(&arr_name).copied()
                    {
                        let list_ty = self.var_types.get(&arr_name).cloned().ok_or_else(|| {
                            CompileError::simple(
                                &format!(
                                    "iterator over `{}` but `var_types` \
                                             has no entry for it",
                                    arr_name
                                ),
                                0,
                                0,
                                "",
                                ErrorCode::E0009,
                            )
                        })?;
                        let struct_ty = match self.map_type(&list_ty) {
                            BasicTypeEnum::StructType(st) => st,
                            _ => unreachable!("map_type(Type::List) returned non-struct"),
                        };
                        let len_slot = self
                            .builder
                            .build_struct_gep(
                                struct_ty,
                                struct_alloca,
                                1,
                                &format!("{}_iter_len_slot", arr_name),
                            )
                            .unwrap();
                        self.builder
                            .build_load(
                                self.context.i64_type(),
                                len_slot,
                                &format!("{}_iter_len", arr_name),
                            )
                            .unwrap()
                            .into_int_value()
                    } else if let Some(static_len) = self.list_lengths.get(&arr_name).copied() {
                        self.context.i64_type().const_int(static_len as u64, false)
                    } else {
                        return Err(CompileError::unsupported_operation(
                            &format!(
                                "cannot determine iteration length of `{}`: \
                                     no compile-time length and no descriptor \
                                     struct registered",
                                arr_name
                            ),
                            "llvm",
                        ));
                    };
                    self.iterator_lengths.insert(iterator.clone(), len_val);
                    let idx_alloca =
                        self.create_entry_alloca(&format!("{}_idx", iterator), &Type::Int);
                    self.builder
                        .build_store(idx_alloca, self.context.i64_type().const_zero())
                        .unwrap();
                    self.iterator_indices.insert(iterator.clone(), idx_alloca);
                    let it_alloca =
                        self.create_entry_alloca(iterator, &Type::List(Box::new(Type::Float)));
                    self.builder.build_store(it_alloca, arr_ptr).unwrap();
                    self.variables.insert(iterator.clone(), it_alloca);
                    self.var_types
                        .insert(iterator.clone(), Type::List(Box::new(Type::Float)));
                    self.list_arrays.insert(iterator.clone(), arr_ptr);
                    self.list_array_types.insert(iterator.clone(), arr_ty);
                } else if let TypedIRValue::List(elems, elem_ty) = iterable {
                    let len = elems.len();
                    let elem_llvm_ty = self.map_type(elem_ty);
                    // ADR 0021. Every list-literal element type
                    // must have an array lowering. The previous
                    // `_ =>` arm silently lowered anything not
                    // Int/Float/Pointer lower directly; Struct
                    // (records) also lowers. The `other` arm is a
                    // fail-closed guard for future element types
                    // that need their own handling.
                    let array_ty = match elem_llvm_ty {
                        BasicTypeEnum::FloatType(t) => t.array_type(len as u32).into(),
                        BasicTypeEnum::IntType(t) => t.array_type(len as u32).into(),
                        BasicTypeEnum::PointerType(t) => t.array_type(len as u32).into(),
                        BasicTypeEnum::StructType(t) => t.array_type(len as u32).into(),
                        other => {
                            return Err(CompileError::unsupported_operation(
                                &format!(
                                    "iterator over list literal with element type \
                                     {:?}: no LLVM array lowering for this element \
                                     type",
                                    other
                                ),
                                "llvm",
                            ));
                        }
                    };
                    let func = self.current_function.unwrap();
                    let entry = func.get_first_basic_block().unwrap();
                    let b = self.context.create_builder();
                    if let Some(first) = entry.get_first_instruction() {
                        b.position_before(&first);
                    } else {
                        b.position_at_end(entry);
                    }
                    let arr_alloca = b
                        .build_alloca(array_ty, &format!("{}_arr_lit", iterator))
                        .unwrap();
                    for (i, elem) in elems.iter().enumerate() {
                        let ev = self.compile_value(elem)?;
                        // Load through a pointer when the slot
                        // expects a struct. See the ArrayAssign
                        // loop above for the same pattern.
                        let ev = match (ev, &elem_llvm_ty) {
                            (BasicValueEnum::PointerValue(p), BasicTypeEnum::StructType(st)) => b
                                .build_load(*st, p, &format!("lit_elem_load_{}", i))
                                .unwrap(),
                            (v, _) => v,
                        };
                        let idx = self.context.i32_type().const_int(i as u64, false);
                        let ptr = unsafe {
                            b.build_gep(
                                array_ty,
                                arr_alloca,
                                &[self.context.i32_type().const_zero(), idx],
                                &format!("lit_gep_{}", i),
                            )
                            .unwrap()
                        };
                        self.builder.build_store(ptr, ev).unwrap();
                    }
                    self.iterator_arrays.insert(iterator.clone(), arr_alloca);
                    self.iterator_array_types.insert(iterator.clone(), array_ty);
                    // The destructured `elem_ty` is already the
                    // ALGOL26 element type. Stash it for
                    // `IteratorNext`.
                    self.iterator_elem_types
                        .insert(iterator.clone(), elem_ty.clone());
                    // ADR 0042 phase 2: length is now a runtime
                    // IntValue. A literal has a compile-time
                    // constant.
                    self.iterator_lengths.insert(
                        iterator.clone(),
                        self.context.i64_type().const_int(len as u64, false),
                    );
                    let idx_alloca =
                        self.create_entry_alloca(&format!("{}_idx", iterator), &Type::Int);
                    self.builder
                        .build_store(idx_alloca, self.context.i64_type().const_zero())
                        .unwrap();
                    self.iterator_indices.insert(iterator.clone(), idx_alloca);
                    let it_alloca =
                        self.create_entry_alloca(iterator, &Type::List(Box::new(Type::Float)));
                    self.builder.build_store(it_alloca, arr_alloca).unwrap();
                    self.variables.insert(iterator.clone(), it_alloca);
                    self.var_types
                        .insert(iterator.clone(), Type::List(Box::new(Type::Float)));
                    self.list_arrays.insert(iterator.clone(), arr_alloca);
                    self.list_array_types.insert(iterator.clone(), array_ty);
                    self.list_lengths.insert(iterator.clone(), len);
                } else {
                    // ADR 0042 follow-up. Any other list-typed value —
                    // a `FieldAccess` like `self.scores`, an
                    // `ArrayAccess` on a `List<List<T>>`, a call
                    // result. Compile it to the `{ptr, i64, i64}`
                    // descriptor struct, extract the buffer and
                    // length, and register the iterator. Same shape
                    // as the variable path, but without the side
                    // tables to consult — the descriptor is the
                    // source of truth.
                    let list_ty = iterable.type_of();
                    let elem_ir_ty = match &list_ty {
                        Type::List(inner) => (**inner).clone(),
                        _ => {
                            return Err(CompileError::unsupported_operation(
                                &format!(
                                    "IteratorInit over non-list-typed value: {:?} (type {})",
                                    iterable, list_ty
                                ),
                                "llvm",
                            ));
                        }
                    };
                    let val = self.compile_value(iterable)?;
                    let sv = match val {
                        BasicValueEnum::StructValue(s) => s,
                        _ => {
                            return Err(CompileError::unsupported_operation(
                                &format!(
                                    "list-typed iterable did not lower to a struct \
                                     (kind {:?})",
                                    val
                                ),
                                "llvm",
                            ));
                        }
                    };
                    let arr_ptr = self
                        .builder
                        .build_extract_value(sv, 0, &format!("{}_buf", iterator))
                        .unwrap()
                        .into_pointer_value();
                    let len_val = self
                        .builder
                        .build_extract_value(sv, 1, &format!("{}_len", iterator))
                        .unwrap()
                        .into_int_value();

                    let elem_llvm = self.map_type(&elem_ir_ty);
                    let arr_ty: BasicTypeEnum<'ctx> = match elem_llvm {
                        BasicTypeEnum::IntType(t) => t.array_type(0).into(),
                        BasicTypeEnum::FloatType(t) => t.array_type(0).into(),
                        BasicTypeEnum::PointerType(t) => t.array_type(0).into(),
                        BasicTypeEnum::StructType(t) => t.array_type(0).into(),
                        other => {
                            return Err(CompileError::unsupported_operation(
                                &format!(
                                    "iterator element type has no LLVM array \
                                     lowering: {:?}",
                                    other
                                ),
                                "llvm",
                            ));
                        }
                    };

                    self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                    self.iterator_array_types.insert(iterator.clone(), arr_ty);
                    self.iterator_elem_types
                        .insert(iterator.clone(), elem_ir_ty);
                    self.iterator_lengths.insert(iterator.clone(), len_val);

                    let idx_alloca =
                        self.create_entry_alloca(&format!("{}_idx", iterator), &Type::Int);
                    self.builder
                        .build_store(idx_alloca, self.context.i64_type().const_zero())
                        .unwrap();
                    self.iterator_indices.insert(iterator.clone(), idx_alloca);
                }
                Ok(())
            }
            // Channels have no LLVM lowering. The capability check
            // should refuse any program that reaches these arms, so
            // this code is defense in depth: if the check ever
            // regresses, codegen errors instead of emitting wrong
            // code. A silent no-op would leave a program that sends
            // to a channel appearing to work but doing nothing.
            Instruction::ChannelDecl { .. } => Err(CompileError::unsupported_operation(
                "channel declaration (channels have no LLVM lowering)",
                "llvm",
            )),
            Instruction::SendChannel { .. } => Err(CompileError::unsupported_operation(
                "channel send (channels have no LLVM lowering)",
                "llvm",
            )),
            Instruction::ReceiveChannel { .. } => Err(CompileError::unsupported_operation(
                "channel receive (channels have no LLVM lowering)",
                "llvm",
            )),
            Instruction::Allocate {
                target,
                size,
                type_,
            } => {
                // `alloc(n)` lowers to a call to libc `malloc`.
                // The result is stored into a per-variable alloca
                // so `p` behaves like any other pointer-typed
                // local.
                let size_val = self.compile_value(size)?;
                let size_i64 = if size_val.is_int_value() {
                    let iv = size_val.into_int_value();
                    if iv.get_type().get_bit_width() != 64 {
                        self.builder
                            .build_int_cast(iv, self.context.i64_type(), "sz64")
                            .unwrap()
                    } else {
                        iv
                    }
                } else {
                    return Err(CompileError::unsupported_operation(
                        &format!("alloc size must be an integer, got {:?}", size_val),
                        "llvm",
                    ));
                };
                let malloc_fn = self.module.get_function("malloc").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: malloc not registered in stdlib",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let call = self
                    .builder
                    .build_call(malloc_fn, &[size_i64.into()], "malloc_call")
                    .unwrap();
                let ptr_val = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v,
                    inkwell::values::ValueKind::Instruction(_) => {
                        // malloc has a return type in C, so this
                        // branch is unreachable in practice. Failing
                        // closed rather than inserting a null.
                        return Err(CompileError::unsupported_operation(
                            "malloc returned no value (unexpected)",
                            "llvm",
                        ));
                    }
                };
                // Reuse the alloca if the target already exists
                // (e.g. an Allocate inside a loop); otherwise
                // create one at function entry.
                let alloca = match self.variables.get(target).cloned() {
                    Some(p) => p,
                    None => {
                        let a = self.create_entry_alloca(target, type_);
                        self.variables.insert(target.clone(), a);
                        self.var_types.insert(target.clone(), type_.clone());
                        a
                    }
                };
                // Before overwriting, check the region state.
                // If this is a *reassignment* of a variable
                // already tracked by the innermost region, the
                // old value must be snapshotted so region exit
                // can free it too. Otherwise the first
                // allocation leaks.
                let is_reassignment = self
                    .region_frames
                    .last()
                    .is_some_and(|f| f.tracked_vars.iter().any(|v| v == target));
                let in_region = !self.region_frames.is_empty();

                let snapshot: Option<inkwell::values::PointerValue<'ctx>> =
                    if in_region && is_reassignment {
                        if let Some(existing_alloca) = self.variables.get(target).copied() {
                            let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
                            let old_val = self
                                .builder
                                .build_load(ptr_ty, existing_alloca, "region_saved_load")
                                .unwrap();
                            self.iter_counter += 1;
                            let slot_name = format!("__region_saved_{}", self.iter_counter);
                            let slot = self.create_entry_alloca(
                                &slot_name,
                                &Type::Pointer(Box::new(Type::Unknown)),
                            );
                            self.builder.build_store(slot, old_val).unwrap();
                            Some(slot)
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                self.builder.build_store(alloca, ptr_val).unwrap();

                // Update the region frame with the new tracking
                // state.
                if in_region {
                    if let Some(slot) = snapshot {
                        if let Some(frame) = self.region_frames.last_mut() {
                            frame.saved_slots.push(slot);
                        }
                    } else if !is_reassignment {
                        if let Some(frame) = self.region_frames.last_mut() {
                            frame.tracked_vars.push(target.clone());
                        }
                    }
                }
                Ok(())
            }
            Instruction::Free { ptr } => {
                // `free(p)` lowers to a call to libc `free`.
                // After the call, if the pointer is held in a
                // variable, null its alloca. This makes a later
                // region auto-free on the same variable a no-op
                // (free(null) is defined as doing nothing), so
                // `free(p)` inside a region and its auto-free on
                // region exit do not double-free.
                let ptr_val = self.compile_value(ptr)?;
                let free_fn = self.module.get_function("free").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: free not registered in stdlib",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                self.builder
                    .build_call(free_fn, &[ptr_val.into()], "free_call")
                    .unwrap();
                if let TypedIRValue::Variable(name, _) = ptr {
                    if let Some(alloca) = self.variables.get(name).copied() {
                        let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
                        let null_ptr = ptr_ty.const_null();
                        self.builder.build_store(alloca, null_ptr).unwrap();
                    }
                }
                Ok(())
            }
            Instruction::BoundsCheck {
                value,
                low,
                high,
                message,
            } => {
                // ADR 0031: runtime subrange bounds check. The value
                // is an Int or an enum ordinal — both are i64 at
                // runtime. Same shape as the array OOB check in the
                // ArrayAssign arm: compare, branch to an error block
                // that prints and returns, else fall through.
                let v = self.compile_value(value)?;
                if !v.is_int_value() {
                    return Err(CompileError::unsupported_operation(
                        &format!(
                            "BoundsCheck value did not lower to an integer \
                             (kind: {:?})",
                            v
                        ),
                        "llvm",
                    ));
                }
                let iv = v.into_int_value();
                let iv_i64 = if iv.get_type().get_bit_width() != 64 {
                    self.builder
                        .build_int_cast(iv, self.context.i64_type(), "bc_i64")
                        .unwrap()
                } else {
                    iv
                };
                let low_val = self.context.i64_type().const_int(*low as u64, true);
                let high_val = self.context.i64_type().const_int(*high as u64, true);
                let below = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::SLT, iv_i64, low_val, "bc_below")
                    .unwrap();
                let above = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::SGT, iv_i64, high_val, "bc_above")
                    .unwrap();
                let out_of_range = self.builder.build_or(below, above, "bc_oob").unwrap();

                let error_bb = self
                    .context
                    .append_basic_block(self.current_function.unwrap(), "bounds_check_error");
                let continue_bb = self
                    .context
                    .append_basic_block(self.current_function.unwrap(), "bounds_check_ok");

                self.builder
                    .build_conditional_branch(out_of_range, error_bb, continue_bb)
                    .unwrap();

                self.builder.position_at_end(error_bb);
                let msg_global = self
                    .builder
                    .build_global_string_ptr(&format!("{}\n", message), "bc_msg")
                    .unwrap();
                let printf_fn = self.module.get_function("printf").unwrap();
                self.builder
                    .build_call(
                        printf_fn,
                        &[msg_global.as_pointer_value().into()],
                        "print_bounds_err",
                    )
                    .unwrap();
                // ADR 0042 phase 2. Same change as the OOB sites:
                // the bounds-check error block calls `exit(1)`
                // instead of `return`.
                let exit_fn = self.module.get_function("exit").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: exit not registered in stdlib",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let status = self.context.i32_type().const_int(1, false);
                self.builder
                    .build_call(exit_fn, &[status.into()], "bc_exit")
                    .unwrap();
                self.builder.build_unreachable().unwrap();

                self.builder.position_at_end(continue_bb);
                Ok(())
            }
            Instruction::RegionEnter { name } => {
                self.region_frames.push(LRegionFrame {
                    name: name.clone(),
                    tracked_vars: Vec::new(),
                    saved_slots: Vec::new(),
                    tracked_lists: Vec::new(),
                });
                Ok(())
            }
            Instruction::RegionExit { name } => {
                // Pop the top frame and emit a guarded free for
                // every allocation recorded in it. Allocation
                // names are stored in reverse (LIFO) so cleanup
                // order matches the interpreter.
                match self.region_frames.pop() {
                    Some(frame) if frame.name == *name => {
                        // Collect all pointers to free, then emit
                        // the frees. Order: snapshots first
                        // (LIFO), then currently-tracked vars
                        // (LIFO). The `frame` is owned (from
                        // pop()), so no borrow conflict.
                        let mut cleanups: Vec<inkwell::values::PointerValue<'ctx>> = Vec::new();
                        for slot in frame.saved_slots.iter().rev() {
                            cleanups.push(*slot);
                        }
                        for var_name in frame.tracked_vars.iter().rev() {
                            if let Some(alloca) = self.variables.get(var_name).copied() {
                                cleanups.push(alloca);
                            }
                        }
                        for alloca in cleanups {
                            self.emit_free_if_non_null(alloca)?;
                        }
                        // ADR 0050. Free each tracked list whose
                        // capacity is > 0, in LIFO order.
                        for (name, st, alloca) in frame.tracked_lists.iter().rev() {
                            self.emit_free_list_if_heap(*alloca, *st, name)?;
                        }
                        Ok(())
                    }
                    Some(frame) => Err(CompileError::simple(
                        &format!(
                            "LLVM codegen: region exit '{}' but top frame is '{}'",
                            name, frame.name
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )),
                    None => Err(CompileError::simple(
                        &format!(
                            "LLVM codegen: region exit '{}' with no matching enter",
                            name
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )),
                }
            }
            Instruction::FieldAssign {
                target,
                field,
                value,
            } => {
                // ADR 0036 L4. `p.x := v` compiles to GEP + store.
                // The target is a variable name (the analyzer rejects
                // field assignment on non-variable receivers), so the
                // record's alloca is found via the variables map.
                let raw_var_ty = match self.var_types.get(target).cloned() {
                    Some(t) => t,
                    None => {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: field assign to unknown variable '{}'",
                                target
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        ));
                    }
                };

                // Auto-deref through a reference. Inside a method or a
                // free function taking `u: &mut User`, the target
                // variable's type is `MutBorrow(Record)`. The pointer
                // in `variables[target]` already points at the record's
                // alloca, so the GEP+store work the same way once the
                // borrow wrapper is stripped.
                let var_ty = match &raw_var_ty {
                    Type::Borrow(inner) | Type::MutBorrow(inner) => (**inner).clone(),
                    other => other.clone(),
                };

                let record_name = match &var_ty {
                    Type::Record(name, _) => name.clone(),
                    _ => {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: field assign on non-record type {:?}",
                                raw_var_ty
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0002,
                        ));
                    }
                };

                let rec_decl = match self.record_decls.get(&record_name).cloned() {
                    Some(r) => r,
                    None => {
                        return Err(CompileError::simple(
                            &format!("LLVM codegen: unknown record '{}'", record_name),
                            0,
                            0,
                            "",
                            ErrorCode::E0003,
                        ));
                    }
                };

                let field_idx = match rec_decl.fields.iter().position(|(n, _, _)| n == field) {
                    Some(i) => i,
                    None => {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: record '{}' has no field '{}'",
                                record_name, field
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        ));
                    }
                };

                let struct_ty = match self.map_type(&var_ty) {
                    BasicTypeEnum::StructType(s) => s,
                    _ => unreachable!(
                        "map_type(Type::Record) returned a non-struct type for '{}'",
                        record_name
                    ),
                };

                let record_ptr = match self.variables.get(target).copied() {
                    Some(p) => p,
                    None => {
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: no alloca for field-assign target '{}'",
                                target
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        ));
                    }
                };

                let field_ptr = unsafe {
                    self.builder
                        .build_gep(
                            struct_ty,
                            record_ptr,
                            &[
                                self.context.i32_type().const_zero(),
                                self.context.i32_type().const_int(field_idx as u64, false),
                            ],
                            &format!("{}_{}_ptr", record_name, field),
                        )
                        .unwrap()
                };

                let val = self.compile_value(value)?;
                self.builder.build_store(field_ptr, val).unwrap();
                Ok(())
            }
        }
    }
}

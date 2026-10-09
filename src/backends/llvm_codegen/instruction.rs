// src/backends/llvm_codegen/instruction.rs

use super::resolve_math_name;
use super::IRCodeGen;
use super::LRegionFrame;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{Instruction, TypedIRValue};
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;

impl<'ctx> IRCodeGen<'ctx> {
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
                    if matches!(self.var_types.get(src), Some(Type::List(_))) {
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
                        return Ok(());
                    }
                }
                // Fail closed: a list-typed target must have been
                // handled by one of the arms above (list literal,
                // record literal, or list-variable move). Any other
                // value shape reaching this point would produce a
                // list whose bookkeeping is unpopulated, which
                // silently fails later at the first `b[i]`.
                if matches!(type_, Type::List(_)) {
                    return Err(CompileError::unsupported_operation(
                        &format!(
                            "LLVM codegen: cannot lower list declaration `{}` \
                             — only list literals and list-variable moves are \
                             currently supported",
                            name
                        ),
                        "llvm",
                    ));
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
                    if matches!(self.var_types.get(src), Some(Type::List(_))) {
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
                        return Ok(());
                    }
                }
                // Fail closed: list-typed target must have been
                // handled above (list literal or list-variable move).
                if matches!(self.var_types.get(target), Some(Type::List(_))) {
                    return Err(CompileError::unsupported_operation(
                        &format!(
                            "LLVM codegen: cannot lower list assignment to `{}` \
                             from this value shape — only list literals and \
                             list-variable moves are currently supported",
                            target
                        ),
                        "llvm",
                    ));
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
                // Same guard as the sibling site in `value.rs`: only
                // emit the bounds check when the codegen has a
                // compile-time length. List parameters have no
                // `list_lengths` entry.
                if self.list_lengths.contains_key(&arr_name) && idx_val.is_int_value() {
                    let idx_int = idx_val.into_int_value();
                    // Same reasoning as the sibling site in `value.rs`:
                    // the guard proves the entry exists. Fail closed
                    // rather than silently defaulting to 0.
                    let len = self
                        .list_lengths
                        .get(&arr_name)
                        .cloned()
                        .expect("list_lengths entry disappeared between contains_key and get")
                        as u64;
                    let len_val = self.context.i64_type().const_int(len, false);
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

                    // Return void if the enclosing function is void; otherwise return 1.
                    let current_fn = self.current_function.unwrap();
                    match current_fn.get_type().get_return_type() {
                        Some(_) => {
                            self.builder
                                .build_return(Some(&self.context.i32_type().const_int(1, false)))
                                .unwrap();
                        }
                        None => {
                            self.builder.build_return(None).unwrap();
                        }
                    }

                    self.builder.position_at_end(continue_bb);
                }
                let val = self.compile_value(value)?;
                let arr_ptr = self.list_arrays.get(&arr_name).cloned().ok_or_else(|| {
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
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                        arg_vals.iter().map(|v| (*v).into()).collect();
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
                                    self.var_types.insert(res_name.clone(), Type::Float);
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
                    let arr_ptr = self.list_arrays.get(&arr_name).cloned().ok_or_else(|| {
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
                    // Fail closed when the iterable's length is not
                    // known. This happens for list parameters: the
                    // callee has the array pointer (registered in
                    // `list_arrays` by `compile_function`) but not
                    // the caller's length. A rolled loop needs a
                    // runtime count and an unrolled one needs a
                    // compile-time count; neither is available.
                    // Refuse rather than emit a wrong-length loop.
                    let len = self.list_lengths.get(&arr_name).copied().ok_or_else(|| {
                        CompileError::unsupported_operation(
                            &format!(
                                "cannot iterate list parameter `{}` \
                                 — its length is not tracked in the callee. \
                                 Index it (`{}[i]`) or copy it into a local list \
                                 first.",
                                arr_name, arr_name
                            ),
                            "llvm",
                        )
                    })?;
                    self.iterator_lengths.insert(iterator.clone(), len);
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
                    self.iterator_lengths.insert(iterator.clone(), len);
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
                    return Err(CompileError::unsupported_operation(
                        &format!(
                            "IteratorInit over iterable that is neither a variable \
                             nor a list literal: {:?}",
                            iterable
                        ),
                        "llvm",
                    ));
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
                let current_fn = self.current_function.unwrap();
                match current_fn.get_type().get_return_type() {
                    Some(_) => {
                        self.builder
                            .build_return(Some(&self.context.i32_type().const_int(1, false)))
                            .unwrap();
                    }
                    None => {
                        self.builder.build_return(None).unwrap();
                    }
                }

                self.builder.position_at_end(continue_bb);
                Ok(())
            }
            Instruction::RegionEnter { name } => {
                self.region_frames.push(LRegionFrame {
                    name: name.clone(),
                    tracked_vars: Vec::new(),
                    saved_slots: Vec::new(),
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

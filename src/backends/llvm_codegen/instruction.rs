// src/backends/llvm_codegen/instruction.rs

use super::resolve_math_name;
use super::IRCodeGen;
use super::LRegionFrame;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{Instruction, TypedIRValue};
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValueEnum;

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
                    self.list_lengths.insert(name.clone(), len);
                    self.variables.insert(name.clone(), arr_alloca);
                    self.var_types.insert(name.clone(), type_.clone());
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
                if idx_val.is_int_value() {
                    let idx_int = idx_val.into_int_value();
                    let len = self.list_lengths.get(&arr_name).cloned().unwrap_or(0) as u64;
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
                                    let alloca = self.create_entry_alloca(res_name, &Type::Float);
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
                        let ty = args.first().map(|a| a.type_of()).unwrap_or(Type::Float);
                        self.emit_print(*first, ty)?;
                    }
                } else {
                    self.compile_builtin_call(&callee_name, args, result)?;
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
                    let arr_ty = self
                        .list_array_types
                        .get(&arr_name)
                        .cloned()
                        .unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                    self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                    self.iterator_array_types.insert(iterator.clone(), arr_ty);
                    if let Some(len) = self.list_lengths.get(&arr_name) {
                        self.iterator_lengths.insert(iterator.clone(), *len);
                    }
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
                    let array_ty = match elem_llvm_ty {
                        BasicTypeEnum::FloatType(t) => t.array_type(len as u32).into(),
                        BasicTypeEnum::IntType(t) => t.array_type(len as u32).into(),
                        _ => self.context.f64_type().array_type(len as u32).into(),
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
        }
    }
}

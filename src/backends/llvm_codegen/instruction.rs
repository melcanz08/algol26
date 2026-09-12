// src/backends/llvm_codegen/instruction.rs

use super::IRCodeGen;
use super::resolve_math_name;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{Instruction, TypedIRValue};
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::BasicValueEnum;

impl<'ctx> IRCodeGen<'ctx> {
    pub(super) fn compile_instruction(&mut self, instr: &Instruction) -> Result<()> {
        match instr {
            Instruction::Nop => Ok(()),
            Instruction::Declare { name, type_, value, mutable: _ } => {
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
                            self.builder.build_gep(
                                array_ty,
                                arr_alloca,
                                &[self.context.i32_type().const_zero(), idx],
                                &format!("{}_gep_{}", name, i),
                            ).unwrap()
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
                if let TypedIRValue::List(elems, _) = value {
                    self.list_lengths.insert(target.clone(), elems.len());
                }
                Ok(())
            }
            Instruction::ArrayAssign {
                array,
                index,
                value,
            } => {
                let arr_name = match array.as_ref() {
                    TypedIRValue::Variable(n, _) => n.clone(),
                    _ => {
                        return Ok(());
                    }
                };
                let idx_val = self.compile_value(index)?;
                if idx_val.is_int_value() {
                    let idx_int = idx_val.into_int_value();
                    let len = self.list_lengths.get(&arr_name).cloned().unwrap_or(0) as u64;
                    let len_val = self.context.i64_type().const_int(len, false);
                    let zero = self.context.i64_type().const_int(0, false);

                    let is_negative = self.builder.build_int_compare(
                        inkwell::IntPredicate::SLT, idx_int, zero, "bounds_check_neg"
                    ).unwrap();
                    let is_too_big = self.builder.build_int_compare(
                        inkwell::IntPredicate::SGE, idx_int, len_val, "bounds_check_big"
                    ).unwrap();
                    let out_of_bounds = self.builder.build_or(is_negative, is_too_big, "oob").unwrap();

                    let error_bb = self.context.append_basic_block(
                        self.current_function.unwrap(), "bounds_error_write"
                    );
                    let continue_bb = self.context.append_basic_block(
                        self.current_function.unwrap(), "bounds_ok_write"
                    );

                    self.builder.build_conditional_branch(out_of_bounds, error_bb, continue_bb).unwrap();

                    self.builder.position_at_end(error_bb);
                    let error_msg = self.builder.build_global_string_ptr(
                        "Error: Array index out of bounds\n", "bounds_err_msg_write"
                    ).unwrap();
                    let printf_fn = self.module.get_function("printf").unwrap();
                    self.builder.build_call(
                        printf_fn, &[error_msg.as_pointer_value().into()], "print_bounds_error"
                    ).unwrap();

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
                if let Some(arr_ptr) = self.list_arrays.get(&arr_name).cloned() {
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
                }
                Ok(())
            }
            Instruction::Print { value } => {
                let v = self.compile_value(value)?;
                self.emit_print(v, value.type_of())?;
                Ok(())
            }
            Instruction::Call { func, args, result } => {
                let arg_vals: Vec<BasicValueEnum> = args
                    .iter()
                    .map(|a| self.compile_value(a).unwrap())
                    .collect();
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
                        let __ret_opt = match call_site.try_as_basic_value() {
                            inkwell::values::ValueKind::Basic(v) => Some(v),
                            _ => None,
                        };
                        if let Some(ret) = __ret_opt {
                            if let Some(ptr) = self.variables.get(res_name).cloned() {
                                self.builder.build_store(ptr, ret).unwrap();
                            } else {
                                let alloca = self.create_entry_alloca(res_name, &Type::Float);
                                self.builder.build_store(alloca, ret).unwrap();
                                self.variables.insert(res_name.clone(), alloca);
                                self.var_types.insert(res_name.clone(), Type::Float);
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
                    if let Some(arr_ptr) = self.list_arrays.get(&arr_name).cloned() {
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
                    }
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
                        let ev = self.compile_value(elem).unwrap();
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
                }
                Ok(())
            }
            Instruction::ChannelDecl { name, type_ } => {
                let alloca = self.create_entry_alloca(name, type_);
                self.variables.insert(name.clone(), alloca);
                self.var_types.insert(name.clone(), type_.clone());
                Ok(())
            }
            Instruction::Send { .. } => Ok(()),
            Instruction::Receive { .. } => Ok(()),
            Instruction::ChannelSend { .. } => Ok(()),
            Instruction::ChannelReceive { .. } => Ok(()),
            Instruction::Allocate { .. } => Ok(()),
            Instruction::Free { .. } => Ok(()),
        }
    }
}
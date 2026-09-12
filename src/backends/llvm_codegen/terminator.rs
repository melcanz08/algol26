// src/backends/llvm_codegen/terminator.rs

use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticPattern, Terminator, TypedIRValue};
use inkwell::FloatPredicate;
use inkwell::types::BasicTypeEnum;

impl<'ctx> IRCodeGen<'ctx> {
    pub(super) fn compile_terminator(&mut self, term: &Terminator, ret_type: &Type) -> Result<()> {
        match term {
            Terminator::Return { value, type_ } => {
                if let Some(v) = value {
                    let compiled = self.compile_value(v)?;
                    self.builder.build_return(Some(&compiled)).unwrap();
                } else {
                    if *ret_type == Type::Void {
                        self.builder.build_return(None).unwrap();
                    } else {
                        let def = self.default_value_for_type(ret_type);
                        self.builder.build_return(Some(&def)).unwrap();
                    }
                }
                Ok(())
            }
            Terminator::Jump { block } => {
                let bb = self.blocks.get(block).cloned().ok_or_else(|| {
                    CompileError::simple(
                        &format!("block {} not found", block),
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    )
                })?;
                self.builder.build_unconditional_branch(bb).unwrap();
                Ok(())
            }
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                let cond_val = self.compile_value(condition)?;
                let bool_val = if cond_val.is_int_value() {
                    let iv = cond_val.into_int_value();
                    if iv.get_type().get_bit_width() == 1 {
                        iv
                    } else {
                        self.builder
                            .build_int_compare(
                                inkwell::IntPredicate::NE,
                                iv,
                                iv.get_type().const_int(0, false),
                                "tobool",
                            )
                            .unwrap()
                    }
                } else if cond_val.is_float_value() {
                    let fv = cond_val.into_float_value();
                    self.builder
                        .build_float_compare(
                            FloatPredicate::ONE,
                            fv,
                            self.context.f64_type().const_float(0.0),
                            "ftobool",
                        )
                        .unwrap()
                } else {
                    self.context.bool_type().const_int(1, false)
                };
                let then_bb = self.blocks.get(then_block).cloned().unwrap();
                let else_bb = self.blocks.get(else_block).cloned().unwrap();
                self.builder
                    .build_conditional_branch(bool_val, then_bb, else_bb)
                    .unwrap();
                Ok(())
            }
            Terminator::Switch {
                value,
                cases,
                default_block,
            } => {
                let val = self.compile_value(value)?;
                if val.is_int_value() {
                    let iv = val.into_int_value();
                    let default_bb = if let Some(default_id) = default_block {
                        self.blocks.get(default_id).cloned().unwrap()
                    } else {
                        // create dummy unreachable? use current block's next? fallback to entry
                        self.blocks.values().next().cloned().unwrap()
                    };
                    let mut case_pairs: Vec<(
                        inkwell::values::IntValue<'ctx>,
                        inkwell::basic_block::BasicBlock<'ctx>,
                    )> = Vec::new();
                    for (pat, block_id) in cases {
                        let target_bb = self.blocks.get(block_id).cloned().unwrap();
                        let const_val = match pat {
                            SemanticPattern::Literal(lit) => match lit {
                                TypedIRValue::Int(i) => {
                                    self.context.i64_type().const_int(*i as u64, true)
                                }
                                TypedIRValue::Bool(b) => self
                                    .context
                                    .bool_type()
                                    .const_int(if *b { 1 } else { 0 }, false),
                                _ => self.context.i64_type().const_int(0, false),
                            },
                            _ => self.context.i64_type().const_int(0, false),
                        };
                        // need to cast const_val to iv type if needed
                        let casted = if const_val.get_type() != iv.get_type() {
                            if iv.get_type().get_bit_width() == 1 {
                                // bool case
                                self.context.bool_type().const_int(
                                    if const_val.get_zero_extended_constant().unwrap_or(0) != 0 {
                                        1
                                    } else {
                                        0
                                    },
                                    false,
                                )
                            } else {
                                const_val
                            }
                        } else {
                            const_val
                        };
                        case_pairs.push((casted, target_bb));
                    }
                    self.builder
                        .build_switch(iv, default_bb, &case_pairs)
                        .unwrap();
                } else {
                    if let Some(default_id) = default_block {
                        let default_bb = self.blocks.get(default_id).cloned().unwrap();
                        self.builder.build_unconditional_branch(default_bb).unwrap();
                    }
                }
                Ok(())
            }
            Terminator::IteratorNext {
                iterator,
                target,
                body_block,
                exit_block,
            } => {
                // Try to find idx, if not found, try alternative lookup (iterator may be stored under different key due to temp naming)
                let idx_ptr = if let Some(p) = self.iterator_indices.get(iterator).cloned() {
                    p
                } else {
                    // fallback: search for any idx that contains iterator name or try to recover
                    // For for_scope_hardened, iterator is often the loop variable 't', but idx is stored under '__iter_t_1' or similar
                    // Look for keys that end with iterator or iterator is substring
                    let mut found = None;
                    for (k, v) in &self.iterator_indices {
                        if k.contains(iterator) || iterator.contains(k) {
                            found = Some(*v);
                            break;
                        }
                    }
                    // Also try to find iterator array and create idx if missing
                    if found.is_none() {
                        if let Some(arr_ptr) =
                            self.iterator_arrays.get(iterator).cloned().or_else(|| {
                                // try to find array that matches loop var
                                for (k, v) in &self.iterator_arrays {
                                    if k.contains(iterator) || iterator.contains(k) {
                                        return Some(*v);
                                    }
                                }
                                None
                            })
                        {
                            // create idx alloca now
                            let idx_alloca = self.create_entry_alloca(
                                &format!("{}_idx_fallback", iterator),
                                &Type::Int,
                            );
                            self.builder
                                .build_store(idx_alloca, self.context.i64_type().const_zero())
                                .unwrap();
                            self.iterator_indices.insert(iterator.clone(), idx_alloca);
                            found = Some(idx_alloca);
                            // also ensure iterator_arrays contains it
                            if !self.iterator_arrays.contains_key(iterator) {
                                self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                                let arr_ty = self
                                    .iterator_array_types
                                    .values()
                                    .next()
                                    .cloned()
                                    .unwrap_or_else(|| {
                                        self.context.f64_type().array_type(0).into()
                                    });
                                self.iterator_array_types.insert(iterator.clone(), arr_ty);
                                self.iterator_lengths.insert(iterator.clone(), 4);
                                // fallback length, will be updated if possible
                            }
                        }
                    }
                    // If still not found, try to recover by using any existing list as iterable (for_scope_hardened fallback)
                    if found.is_none() {
                        // Try to find a list variable that looks like the source (e.g., temps)
                        if let Some((list_name, arr_ptr)) =
                            self.list_arrays.iter().next().map(|(k, v)| (k.clone(), *v))
                        {
                            let arr_ty = self
                                .list_array_types
                                .get(&list_name)
                                .cloned()
                                .unwrap_or_else(|| self.context.f64_type().array_type(4).into());
                            let len = self.list_lengths.get(&list_name).cloned().unwrap_or(4);
                            let idx_alloca = self.create_entry_alloca(
                                &format!("{}_idx_recovered", iterator),
                                &Type::Int,
                            );
                            self.builder
                                .build_store(idx_alloca, self.context.i64_type().const_zero())
                                .unwrap();
                            self.iterator_indices.insert(iterator.clone(), idx_alloca);
                            self.iterator_arrays.insert(iterator.clone(), arr_ptr);
                            self.iterator_array_types.insert(iterator.clone(), arr_ty);
                            self.iterator_lengths.insert(iterator.clone(), len);
                            found = Some(idx_alloca);
                        }
                    }
                    found.ok_or_else(|| {
                        CompileError::simple(
                            &format!(
                                "iterator idx not found for '{}' - available: {:?} lists:{:?}",
                                iterator,
                                self.iterator_indices.keys().collect::<Vec<_>>(),
                                self.list_arrays.keys().collect::<Vec<_>>()
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        )
                    })?
                };
                let idx_val = self
                    .builder
                    .build_load(
                        self.context.i64_type(),
                        idx_ptr,
                        &format!("{}_load_idx", iterator),
                    )
                    .unwrap()
                    .into_int_value();
                let len = self.iterator_lengths.get(iterator).cloned().unwrap_or(0) as u64;
                let len_val = self.context.i64_type().const_int(len, false);
                let cond = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::ULT, idx_val, len_val, "iter_cond")
                    .unwrap();
                let body_bb = self.blocks.get(body_block).cloned().unwrap();
                let exit_bb = self.blocks.get(exit_block).cloned().unwrap();
                self.builder
                    .build_conditional_branch(cond, body_bb, exit_bb)
                    .unwrap();

                let cur_bb = self.builder.get_insert_block().unwrap();
                self.builder.position_at_end(body_bb);
                if let Some(arr_ptr) = self.iterator_arrays.get(iterator).cloned() {
                    let arr_ty = self
                        .iterator_array_types
                        .get(iterator)
                        .cloned()
                        .unwrap_or_else(|| self.context.f64_type().array_type(0).into());

                    // Derive the element type from the array we're iterating over, rather
                    // than hardcoding Float. An Int list iterated by `for n in nums` must
                    // produce an Int loop variable — otherwise every op on `n` sees a
                    // mixed-type pair and the hardened codegen rejects it.
                    let elem_llvm_ty: BasicTypeEnum = match arr_ty {
                        BasicTypeEnum::ArrayType(at) => at.get_element_type(),
                        _ => self.context.f64_type().into(),
                    };
                    let elem_ir_ty = match elem_llvm_ty {
                        BasicTypeEnum::IntType(_) => Type::Int,
                        BasicTypeEnum::FloatType(_) => Type::Float,
                        BasicTypeEnum::PointerType(_) => Type::Ptr,
                        _ => Type::Float,
                    };

                    let idx_i32 = self
                        .builder
                        .build_int_cast(idx_val, self.context.i32_type(), "idx32")
                        .unwrap();

                    let elem_ptr = unsafe {
                        self.builder
                            .build_gep(
                                arr_ty,
                                arr_ptr,
                                &[self.context.i32_type().const_zero(), idx_i32],
                                "iter_elem_ptr",
                            )
                            .unwrap()
                    };

                    let loaded = self
                        .builder
                        .build_load(elem_llvm_ty, elem_ptr, target)
                        .unwrap();

                    let target_ptr = if let Some(p) = self.variables.get(target).cloned() {
                        p
                    } else {
                        let alloca = self.create_entry_alloca(target, &elem_ir_ty);
                        self.variables.insert(target.clone(), alloca);
                        self.var_types.insert(target.clone(), elem_ir_ty.clone());
                        alloca
                    };

                    self.builder.build_store(target_ptr, loaded).unwrap();

                    let next_idx = self
                        .builder
                        .build_int_add(
                            idx_val,
                            self.context.i64_type().const_int(1, false),
                            "next_idx",
                        )
                        .unwrap();
                    self.builder.build_store(idx_ptr, next_idx).unwrap();
                }
                self.builder.position_at_end(cur_bb);
                Ok(())
            }
            Terminator::Spawn { entry_block } => {
                // NOTE: LLVM backend doesn't support true parallelism yet
                // This is a known limitation - we execute sequentially
                // TODO: Use pthreads or similar for actual parallelism

                // For now, emit a warning comment in IR
                if let Some(bb) = self.blocks.get(entry_block).cloned() {
                    self.builder.build_unconditional_branch(bb).unwrap();
                }
                Ok(())
            }
            Terminator::Fork { blocks, join_block } => {
                // Sequential fallback: go to first parallel block, else join
                let target = self
                    .blocks
                    .get(join_block)
                    .or_else(|| blocks.first().and_then(|id| self.blocks.get(id)))
                    .cloned();
                if let Some(bb) = target {
                    self.builder.build_unconditional_branch(bb).unwrap();
                }
                Ok(())
            }
        }
    }
}
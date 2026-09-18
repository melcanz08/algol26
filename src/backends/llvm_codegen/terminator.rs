// src/backends/llvm_codegen/terminator.rs

use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticPattern, Terminator, TypedIRValue};
use inkwell::types::BasicTypeEnum;
use inkwell::FloatPredicate;

impl<'ctx> IRCodeGen<'ctx> {
    pub(super) fn compile_terminator(&mut self, term: &Terminator, ret_type: &Type) -> Result<()> {
        match term {
            Terminator::Return { value, type_ } => {
                // If a `return` happened inside one or more
                // `region` blocks, its allocations need cleanup
                // before the actual return instruction. The
                // guards make this safe to run even if the
                // RegionExit instructions are in unreachable
                // blocks — the second cleanup sees nulls and
                // skips.
                // Collect everything to free first, then emit the
                // frees. Order: for each frame (innermost first),
                // snapshots then tracked vars, both LIFO.
                let mut cleanups: Vec<inkwell::values::PointerValue<'ctx>> = Vec::new();
                for frame in self.region_frames.iter().rev() {
                    for slot in frame.saved_slots.iter().rev() {
                        cleanups.push(*slot);
                    }
                    for var_name in frame.tracked_vars.iter().rev() {
                        if let Some(alloca) = self.variables.get(var_name).copied() {
                            cleanups.push(alloca);
                        }
                    }
                }
                for alloca in cleanups {
                    self.emit_free_if_non_null(alloca)?;
                }
                if let Some(v) = value {
                    let compiled = self.compile_value(v)?;
                    self.builder.build_return(Some(&compiled)).unwrap();
                } else {
                    if *ret_type == Type::Void {
                        self.builder.build_return(None).unwrap();
                    } else {
                        // A non-Void function returning without a
                        // value is a verifier failure. Failing closed
                        // rather than synthesizing a default.
                        return Err(CompileError::unsupported_operation(
                            &format!(
                                "return with no value in a function returning `{}`",
                                ret_type
                            ),
                            "llvm",
                        ));
                    }
                }
                Ok(())
            }
            Terminator::Jump { block } => {
                let bb = self.blocks.get(block).cloned().ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!("jump to unknown block {}", block),
                        "llvm",
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
                    // The branch condition must be a value the LLVM
                    // backend can lower to i1. The type checker should
                    // have rejected non-bool conditions; failing
                    // closed rather than silently treating as `true`.
                    return Err(CompileError::unsupported_operation(
                        &format!(
                            "branch condition lowered to non-scalar value {:?}",
                            cond_val
                        ),
                        "llvm",
                    ));
                };
                let then_bb = self.blocks.get(then_block).cloned().ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!("branch to unknown then-block {}", then_block),
                        "llvm",
                    )
                })?;
                let else_bb = self.blocks.get(else_block).cloned().ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!("branch to unknown else-block {}", else_block),
                        "llvm",
                    )
                })?;
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
                // Only Literal patterns are supported by the LLVM backend today.
                // Some/None/Ok/Error/Wildcard and pattern bindings require runtime
                // tag decoding and payload extraction that the LLVM codegen doesn't
                // implement yet. The interpreter handles them all — refuse cleanly
                // rather than emitting a switch that silently matches the wrong case.
                let has_non_literal = cases
                    .iter()
                    .any(|(pat, _)| !matches!(pat, SemanticPattern::Literal(_)));

                if has_non_literal {
                    return Err(CompileError::simple(
                        "The LLVM backend does not support match with pattern bindings. \
                         Run through the interpreter instead: \
                         `algol26 run --interpreter <file.gol>`",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                if val.is_int_value() {
                    let iv = val.into_int_value();
                    let default_bb = if let Some(default_id) = default_block {
                        self.blocks.get(default_id).cloned().ok_or_else(|| {
                            CompileError::unsupported_operation(
                                &format!("switch to unknown default block {}", default_id),
                                "llvm",
                            )
                        })?
                    } else {
                        // No default. Emit an unreachable block for
                        // unmatched values instead of jumping to an
                        // arbitrary block. The analyzer's match
                        // exhaustiveness check should have caught
                        // this at compile time.
                        let saved_bb = self.builder.get_insert_block().unwrap();
                        let un_bb = self
                            .context
                            .append_basic_block(self.current_function.unwrap(), "switch_unmatched");
                        self.builder.position_at_end(un_bb);
                        self.builder.build_unreachable().unwrap();
                        self.builder.position_at_end(saved_bb);
                        un_bb
                    };
                    let mut case_pairs: Vec<(
                        inkwell::values::IntValue<'ctx>,
                        inkwell::basic_block::BasicBlock<'ctx>,
                    )> = Vec::new();
                    for (pat, block_id) in cases {
                        let target_bb = self.blocks.get(block_id).cloned().ok_or_else(|| {
                            CompileError::unsupported_operation(
                                &format!("switch case to unknown block {}", block_id),
                                "llvm",
                            )
                        })?;
                        let const_val = match pat {
                            SemanticPattern::Literal(lit) => match lit {
                                TypedIRValue::Int(i) => {
                                    self.context.i64_type().const_int(*i as u64, true)
                                }
                                TypedIRValue::Bool(b) => self
                                    .context
                                    .bool_type()
                                    .const_int(if *b { 1 } else { 0 }, false),
                                other => {
                                    // Only Int and Bool literals lower to
                                    // integer switch cases. A Float or
                                    // String literal here would silently
                                    // match case 0. Fail closed instead.
                                    return Err(CompileError::unsupported_operation(
                                        &format!(
                                            "switch literal pattern of unsupported kind: {:?}",
                                            other
                                        ),
                                        "llvm",
                                    ));
                                }
                            },
                            // has_non_literal was already checked above,
                            // so this arm is unreachable.
                            _ => {
                                return Err(CompileError::unsupported_operation(
                                    "non-literal switch pattern (internal invariant violated)",
                                    "llvm",
                                ));
                            }
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
                } else if let Some(default_id) = default_block {
                    // Non-integer switch value (e.g. String): we cannot
                    // build an LLVM switch on it, so branch to the
                    // default. This is valid only if the analyzer's
                    // exhaustiveness check has ensured that every case
                    // is handled by the default.
                    let default_bb = self.blocks.get(default_id).cloned().ok_or_else(|| {
                        CompileError::unsupported_operation(
                            &format!("switch to unknown default block {}", default_id),
                            "llvm",
                        )
                    })?;
                    self.builder.build_unconditional_branch(default_bb).unwrap();
                } else {
                    // Non-integer switch value with no default. The
                    // analyzer should have rejected this. Failing
                    // closed rather than silently dropping the switch.
                    return Err(CompileError::unsupported_operation(
                        "switch on non-integer value with no default block",
                        "llvm",
                    ));
                }
                Ok(())
            }
            Terminator::IteratorNext {
                iterator,
                target,
                body_block,
                exit_block,
            } => {
                // The IR builder should have registered this iterator
                // during IteratorInit. If it is not present, the IR is
                // inconsistent — this is a compiler bug, not a user
                // error, and the backend cannot guess which array was
                // meant. Fail closed.
                let idx_ptr = self
                    .iterator_indices
                    .get(iterator)
                    .cloned()
                    .ok_or_else(|| {
                        CompileError::unsupported_operation(
                            &format!(
                                "iterator `{}` has no index slot (registered iterators: {:?})",
                                iterator,
                                self.iterator_indices.keys().collect::<Vec<_>>()
                            ),
                            "llvm",
                        )
                    })?;
                let arr_ptr = self.iterator_arrays.get(iterator).cloned().ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "iterator `{}` has no backing array (registered iterators: {:?})",
                            iterator,
                            self.iterator_arrays.keys().collect::<Vec<_>>()
                        ),
                        "llvm",
                    )
                })?;
                let arr_ty = self
                    .iterator_array_types
                    .get(iterator)
                    .cloned()
                    .ok_or_else(|| {
                        CompileError::unsupported_operation(
                            &format!("iterator `{}` has no array type", iterator),
                            "llvm",
                        )
                    })?;

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
                let body_bb = self.blocks.get(body_block).cloned().ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!("iterator body block {} not found", body_block),
                        "llvm",
                    )
                })?;
                let exit_bb = self.blocks.get(exit_block).cloned().ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!("iterator exit block {} not found", exit_block),
                        "llvm",
                    )
                })?;
                self.builder
                    .build_conditional_branch(cond, body_bb, exit_bb)
                    .unwrap();

                let cur_bb = self.builder.get_insert_block().unwrap();
                self.builder.position_at_end(body_bb);

                // Derive the element type from the array we're iterating over, rather
                // than hardcoding Float. An Int list iterated by `for n in nums` must
                // produce an Int loop variable — otherwise every op on `n` sees a
                // mixed-type pair and the hardened codegen rejects it.
                let elem_llvm_ty: BasicTypeEnum = match arr_ty {
                    BasicTypeEnum::ArrayType(at) => at.get_element_type(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("iterator `{}` backing type is not an array", iterator),
                            "llvm",
                        ));
                    }
                };
                let elem_ir_ty = match elem_llvm_ty {
                    BasicTypeEnum::IntType(_) => Type::Int,
                    BasicTypeEnum::FloatType(_) => Type::Float,
                    BasicTypeEnum::PointerType(_) => Type::Ptr,
                    other => {
                        return Err(CompileError::unsupported_operation(
                            &format!(
                                "iterator `{}` element type {:?} has no ALGOL26 equivalent",
                                iterator, other
                            ),
                            "llvm",
                        ));
                    }
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

                self.builder.position_at_end(cur_bb);
                Ok(())
            }
            // The capability check refuses programs that use spawn or
            // parallel on the LLVM backend. Reaching these arms means
            // the capability check was bypassed or the IR is
            // inconsistent. Failing closed rather than emitting
            // sequential code that silently differs from the
            // concurrent semantics the program requested.
            Terminator::Spawn { .. } => Err(CompileError::unsupported_operation(
                "spawn (LLVM backend has no threading model)",
                "llvm",
            )),
            Terminator::Fork { .. } => Err(CompileError::unsupported_operation(
                "parallel (LLVM backend has no threading model)",
                "llvm",
            )),
        }
    }
}

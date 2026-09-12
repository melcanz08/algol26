// src/backends/llvm_codegen/value.rs

use super::IRCodeGen;
use super::resolve_math_name;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::TypedIRValue;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;


impl<'ctx> IRCodeGen<'ctx> {
    /// Compile an expression as a *reference* — i.e., produce the
    /// address of its inner value rather than loading from it. Used by
    /// `Borrow` and `MutBorrow` (and `AddrOf` could delegate here too).
    pub(super) fn compile_reference(&self, expr: &TypedIRValue) -> Result<BasicValueEnum<'ctx>> {
        match expr {
            TypedIRValue::Variable(name, _) => {
                if let Some(ptr) = self.variables.get(name) {
                    Ok((*ptr).into())
                } else {
                    Ok(self
                        .context
                        .ptr_type(AddressSpace::default())
                        .const_null()
                        .into())
                }
            }
            // A reference-to-reference yields the inner reference.
            TypedIRValue::Borrow { expr, .. } | TypedIRValue::MutBorrow { expr, .. } => {
                self.compile_value(expr)
            }
            // Anything else falls back to producing a value; the caller
            // (or verifier) is responsible for ensuring it's used as a
            // reference only when the inner form is addressable.
            other => self.compile_value(other),
        }
    }

    pub(super) fn compile_value(&self, val: &TypedIRValue) -> Result<BasicValueEnum<'ctx>> {
        Ok(match val {
            TypedIRValue::Int(i) => self.context.i64_type().const_int(*i as u64, true).into(),
            TypedIRValue::Float(f) => self.context.f64_type().const_float(*f).into(),
            TypedIRValue::Bool(b) => self
                .context
                .bool_type()
                .const_int(if *b { 1 } else { 0 }, false)
                .into(),
            TypedIRValue::String(s) => {
                let global = self.builder.build_global_string_ptr(s, "strtmp").unwrap();
                global.as_pointer_value().into()
            }
            TypedIRValue::Void => self.context.f64_type().const_float(0.0).into(),
            TypedIRValue::NullPtr => self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
            TypedIRValue::PtrLiteral(p) => {
                self.context.i64_type().const_int(*p as u64, false).into()
            }
            TypedIRValue::List(_, _) => self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
            TypedIRValue::Variable(name, _) => {
                // ALGOL26: UNDEFINED VARIABLE IS AN ERROR, not 0.0!
                let ptr = self.variables.get(name).ok_or_else(|| {
                    CompileError::simple(
                        &format!("Undefined variable '{}' during LLVM codegen", name),
                        0,
                        0,
                        "",
                        ErrorCode::E0003,
                    )
                })?;

                let ty = self.var_types.get(name).cloned().unwrap_or(Type::Float);
                let llvm_ty = self.map_type(&ty);
                self.builder.build_load(llvm_ty, *ptr, name).unwrap()
            }
            TypedIRValue::BinaryOp {
                op,
                left,
                right,
                result_type,
            } => {
                let l = self.compile_value(left)?;
                let r = self.compile_value(right)?;
                self.compile_binop(op, l, r)?
            }
            TypedIRValue::Call {
                function,
                args,
                return_type,
            } => {
                let arg_vals: Vec<BasicValueEnum> = args
                    .iter()
                    .map(|a| self.compile_value(a).unwrap())
                    .collect();
                let callee_name = function.trim_end_matches("()").to_string();
                // `Math.*` names are registered in the LLVM module
                // under their unmangled C names (sqrt, pow, fabs, ...).
                // Translate before lookup so the call finds them.
                let llvm_name = resolve_math_name(&callee_name)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| callee_name.clone());

                if let Some(callee) = self
                    .module
                    .get_function(&llvm_name)
                    .or_else(|| self.functions.get(&llvm_name).cloned())
                {
                    let call_args: Vec<inkwell::values::BasicMetadataValueEnum> =
                        arg_vals.iter().map(|v| (*v).into()).collect();
                    let call_site = self
                        .builder
                        .build_call(callee, &call_args, "calltmp")
                        .unwrap();
                    let __ret_opt = match call_site.try_as_basic_value() {
                        inkwell::values::ValueKind::Basic(v) => Some(v),
                        _ => None,
                    };
                    if let Some(ret) = __ret_opt {
                        ret
                    } else {
                        self.context.f64_type().const_float(0.0).into()
                    }
                } else {
                    self.compile_builtin_value(&callee_name, args)?
                }
            }
            TypedIRValue::ArrayAccess {
                array,
                index,
                element_type,
            } => {
                let arr_name = match array.as_ref() {
                    TypedIRValue::Variable(n, _) => Some(n.clone()),
                    _ => None,
                };

                if let Some(arr_name) = arr_name {
                    if let Some(arr_ptr) = self.list_arrays.get(&arr_name).cloned() {
                        let arr_ty = self
                            .list_array_types
                            .get(&arr_name)
                            .cloned()
                            .unwrap_or_else(|| self.context.f64_type().array_type(0).into());
                        let idx_val = self.compile_value(index)?;

                        // Bounds checking
                        if idx_val.is_int_value() {
                            let idx_int = idx_val.into_int_value();
                            let len = self.list_lengths.get(&arr_name).cloned().unwrap_or(0) as u64;
                            let len_val = self.context.i64_type().const_int(len, false);

                            // Check idx >= 0
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

                            // Check idx < len
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
                                .build_or(is_negative, is_too_big, "out_of_bounds")
                                .unwrap();

                            // Create error block
                            let current_bb = self.builder.get_insert_block().unwrap();
                            let error_bb = self
                                .context
                                .append_basic_block(self.current_function.unwrap(), "bounds_error");
                            let continue_bb = self
                                .context
                                .append_basic_block(self.current_function.unwrap(), "bounds_ok");

                            self.builder
                                .build_conditional_branch(out_of_bounds, error_bb, continue_bb)
                                .unwrap();

                            // Error block: print error and exit
                            self.builder.position_at_end(error_bb);
                            let error_msg = self
                                .builder
                                .build_global_string_ptr(
                                    "Error: Array index out of bounds\n",
                                    "bounds_error_msg",
                                )
                                .unwrap();
                            let printf_fn = self.module.get_function("printf").unwrap();
                            self.builder
                                .build_call(
                                    printf_fn,
                                    &[error_msg.as_pointer_value().into()],
                                    "print_error",
                                )
                                .unwrap();
                            // The error block terminates the function. Use the
                            // function's actual return type — otherwise LLVM
                            // rejects `ret i32 1` inside a void function.
                            let current_fn = self.current_function.unwrap();
                            let ret_type = current_fn.get_type().get_return_type();
                            match ret_type {
                                Some(_) => {
                                    self.builder
                                        .build_return(Some(&self.context.i32_type().const_int(1, false)))
                                        .unwrap();
                                }
                                None => {
                                    self.builder.build_return(None).unwrap();
                                }
                            }

                            // Continue block
                            self.builder.position_at_end(continue_bb);
                        }

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

                        let elem_ptr = unsafe {
                            self.builder
                                .build_gep(
                                    arr_ty,
                                    arr_ptr,
                                    &[self.context.i32_type().const_zero(), idx_i32],
                                    "arr_access",
                                )
                                .unwrap()
                        };

                        let elem_llvm_ty = self.map_type(element_type);
                        self.builder
                            .build_load(elem_llvm_ty, elem_ptr, "elem_load")
                            .unwrap()
                    } else {
                        return Err(CompileError::simple(
                            &format!(
                                "codegen: ArrayAccess on unknown list '{}' (known lists: {:?})",
                                arr_name,
                                self.list_arrays.keys().collect::<Vec<_>>()
                            ),
                            0, 0, "", ErrorCode::E0004,
                        ));
                    }
                } else {
                    return Err(CompileError::simple(
                        "codegen: ArrayAccess with non-variable array expression",
                        0, 0, "", ErrorCode::E0004,
                    ));
                }
            }
            TypedIRValue::Cast { value, target_type } => {
                let v = self.compile_value(value)?;
                // ALGOL26: Check ACTUAL LLVM type, not IR type.
                match (&v, target_type) {
                    (BasicValueEnum::IntValue(iv), Type::Float) => self
                        .builder
                        .build_signed_int_to_float(*iv, self.context.f64_type(), "i2f")
                        .unwrap()
                        .into(),
                    (BasicValueEnum::FloatValue(fv), Type::Int) => self
                        .builder
                        .build_float_to_signed_int(*fv, self.context.i64_type(), "f2i")
                        .unwrap()
                        .into(),
                    (BasicValueEnum::FloatValue(_), Type::Float) => {
                        // Already a float - no cast needed
                        v
                    }
                    (BasicValueEnum::IntValue(_), Type::Int) => {
                        // Already an int - no cast needed
                        v
                    }
                    (source_llvm, target) => {
                        // Reaching this arm means the analyzer permitted
                        // a cast the LLVM backend cannot express. The
                        // verifier's cast-legality check should have
                        // rejected it before codegen.
                        return Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: cannot lower cast from LLVM value \
                                 of kind {:?} to {:?}",
                                source_llvm, target
                            ),
                            0, 0, "", ErrorCode::E0002,
                        ));
                    }
                }
            }
            TypedIRValue::Borrow { expr, .. } => {
                // A borrow is a reference — it represents the *address*
                // of the inner value, not a copy of the value itself.
                // Returning the loaded value here would pass a double
                // where a pointer is expected, and deref of that
                // would segfault.
                self.compile_reference(expr)?
            }
            TypedIRValue::MutBorrow { expr, .. } => {
                self.compile_reference(expr)?
            }
            TypedIRValue::Deref { expr, target_type } => {
                let ptr = self.compile_value(expr)?;
                if ptr.is_pointer_value() {
                    let llvm_ty = self.map_type(target_type);
                    self.builder
                        .build_load(llvm_ty, ptr.into_pointer_value(), "deref_load")
                        .unwrap()
                } else {
                    // Not a pointer — the IR is malformed but we don't
                    // panic; returning the value as-is keeps codegen
                    // running so the verifier can report the real issue.
                    ptr
                }
            }
            TypedIRValue::AddrOf { expr, .. } => {
                if let TypedIRValue::Variable(name, _) = expr.as_ref() {
                    if let Some(ptr) = self.variables.get(name) {
                        (*ptr).into()
                    } else {
                        self.context
                            .ptr_type(AddressSpace::default())
                            .const_null()
                            .into()
                    }
                } else {
                    self.compile_value(expr)?
                }
            }
            TypedIRValue::Some(v) => self.compile_value(v)?,
            TypedIRValue::None { .. } => self
                .context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
            TypedIRValue::Ok { value, .. } => self.compile_value(value)?,
            TypedIRValue::Error { value, .. } => self.compile_value(value)?,

            // These variants have no LLVM lowering. The capability
            // matrix refuses programs that would produce them, so
            // reaching this point means the IR builder emitted
            // something the backend cannot handle — a compiler bug,
            // not a program the user should have written.
            TypedIRValue::Array(_, _, _) => {
                return Err(CompileError::simple(
                    "LLVM codegen: array literal value has no LLVM lowering; \
                     the capability matrix should have refused this program",
                    0, 0, "", ErrorCode::E0002,
                ));
            }
            TypedIRValue::Range(_, _) => {
                return Err(CompileError::simple(
                    "LLVM codegen: range value has no LLVM lowering; \
                     the capability matrix should have refused this program",
                    0, 0, "", ErrorCode::E0002,
                ));
            }
            TypedIRValue::FieldAccess { .. } => {
                return Err(CompileError::simple(
                    "LLVM codegen: field access has no LLVM lowering \
                     (no struct support); the capability matrix should \
                     have refused this program",
                    0, 0, "", ErrorCode::E0002,
                ));
            }
        })
    }
}
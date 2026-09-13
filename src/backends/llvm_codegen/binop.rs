// src/backends/llvm_codegen/binop.rs

use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::semantic_ir::SemanticBinOp;
use inkwell::values::BasicValueEnum;
use inkwell::FloatPredicate;

impl<'ctx> IRCodeGen<'ctx> {
    pub(super) fn compile_binop(
        &self,
        op: &SemanticBinOp,
        left: BasicValueEnum<'ctx>,
        right: BasicValueEnum<'ctx>,
    ) -> Result<BasicValueEnum<'ctx>> {
        // Describe operand kinds for error messages. Only evaluated on
        // the error path, but cheap enough to always compute.
        let describe = |v: &BasicValueEnum<'ctx>| -> &'static str {
            if v.is_int_value() {
                "int"
            } else if v.is_float_value() {
                "float"
            } else if v.is_pointer_value() {
                "ptr"
            } else if v.is_struct_value() {
                "struct"
            } else if v.is_array_value() {
                "array"
            } else {
                "other"
            }
        };

        let result = match op {
            SemanticBinOp::Add => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_add(left.into_int_value(), right.into_int_value(), "add")
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_add(left.into_float_value(), right.into_float_value(), "fadd")
                        .unwrap()
                        .into()
                } else if left.is_int_value() && right.is_float_value() {
                    let l = self
                        .builder
                        .build_signed_int_to_float(
                            left.into_int_value(),
                            self.context.f64_type(),
                            "i2f",
                        )
                        .unwrap();
                    self.builder
                        .build_float_add(l, right.into_float_value(), "fadd")
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_int_value() {
                    let r = self
                        .builder
                        .build_signed_int_to_float(
                            right.into_int_value(),
                            self.context.f64_type(),
                            "i2f",
                        )
                        .unwrap();
                    self.builder
                        .build_float_add(left.into_float_value(), r, "fadd")
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: Add received non-numeric operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::Subtract => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_sub(left.into_int_value(), right.into_int_value(), "sub")
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_sub(left.into_float_value(), right.into_float_value(), "fsub")
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: Subtract received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::Multiply => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_mul(left.into_int_value(), right.into_int_value(), "mul")
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_mul(left.into_float_value(), right.into_float_value(), "fmul")
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: Multiply received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::Divide => {
                if left.is_int_value() && right.is_int_value() {
                    let l = left.into_int_value();
                    let r = right.into_int_value();

                    // Runtime check: divisor == 0 → print diagnostic, exit(1).
                    let zero = self.context.i64_type().const_zero();
                    let is_zero = self
                        .builder
                        .build_int_compare(inkwell::IntPredicate::EQ, r, zero, "div_zero_check")
                        .unwrap();

                    let current_fn = self.current_function.unwrap();
                    let err_bb = self
                        .context
                        .append_basic_block(current_fn, "div_zero_err");
                    let ok_bb = self
                        .context
                        .append_basic_block(current_fn, "div_ok");

                    self.builder
                        .build_conditional_branch(is_zero, err_bb, ok_bb)
                        .unwrap();

                    // Error path: print + exit(1) + unreachable.
                    self.builder.position_at_end(err_bb);
                    let msg = self
                        .builder
                        .build_global_string_ptr(
                            "Error: integer division by zero\n",
                            "div_zero_msg",
                        )
                        .unwrap();
                    let printf_fn = self.module.get_function("printf").unwrap();
                    self.builder
                        .build_call(
                            printf_fn,
                            &[msg.as_pointer_value().into()],
                            "print_div_err",
                        )
                        .unwrap();
                    let exit_fn = self.module.get_function("exit").unwrap();
                    self.builder
                        .build_call(
                            exit_fn,
                            &[self.context.i32_type().const_int(1, false).into()],
                            "do_exit",
                        )
                        .unwrap();
                    self.builder.build_unreachable().unwrap();

                    // Continuation: real sdiv.
                    self.builder.position_at_end(ok_bb);
                    self.builder.build_int_signed_div(l, r, "sdiv").unwrap().into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_div(left.into_float_value(), right.into_float_value(), "fdiv")
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: Divide received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::Greater => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SGT,
                            left.into_int_value(),
                            right.into_int_value(),
                            "gt",
                        )
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_compare(
                            FloatPredicate::OGT,
                            left.into_float_value(),
                            right.into_float_value(),
                            "fgt",
                        )
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: Greater received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::Less => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SLT,
                            left.into_int_value(),
                            right.into_int_value(),
                            "lt",
                        )
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_compare(
                            FloatPredicate::OLT,
                            left.into_float_value(),
                            right.into_float_value(),
                            "flt",
                        )
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: Less received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::GreaterEqual => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SGE,
                            left.into_int_value(),
                            right.into_int_value(),
                            "ge",
                        )
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_compare(
                            FloatPredicate::OGE,
                            left.into_float_value(),
                            right.into_float_value(),
                            "fge",
                        )
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: GreaterEqual received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::LessEqual => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::SLE,
                            left.into_int_value(),
                            right.into_int_value(),
                            "le",
                        )
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_compare(
                            FloatPredicate::OLE,
                            left.into_float_value(),
                            right.into_float_value(),
                            "fle",
                        )
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: LessEqual received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::Equal => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::EQ,
                            left.into_int_value(),
                            right.into_int_value(),
                            "eq",
                        )
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_compare(
                            FloatPredicate::OEQ,
                            left.into_float_value(),
                            right.into_float_value(),
                            "feq",
                        )
                        .unwrap()
                        .into()
                } else if left.is_pointer_value() && right.is_pointer_value() {
                    let l_int = self
                        .builder
                        .build_ptr_to_int(left.into_pointer_value(), self.context.i64_type(), "eq_l_ptr")
                        .unwrap();
                    let r_int = self
                        .builder
                        .build_ptr_to_int(right.into_pointer_value(), self.context.i64_type(), "eq_r_ptr")
                        .unwrap();
                    self.builder
                        .build_int_compare(inkwell::IntPredicate::EQ, l_int, r_int, "ptr_eq")
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: Equal received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
            SemanticBinOp::NotEqual => {
                if left.is_int_value() && right.is_int_value() {
                    self.builder
                        .build_int_compare(
                            inkwell::IntPredicate::NE,
                            left.into_int_value(),
                            right.into_int_value(),
                            "ne",
                        )
                        .unwrap()
                        .into()
                } else if left.is_float_value() && right.is_float_value() {
                    self.builder
                        .build_float_compare(
                            FloatPredicate::ONE,
                            left.into_float_value(),
                            right.into_float_value(),
                            "fne",
                        )
                        .unwrap()
                        .into()
                } else if left.is_pointer_value() && right.is_pointer_value() {
                    let l_int = self
                        .builder
                        .build_ptr_to_int(left.into_pointer_value(), self.context.i64_type(), "eq_l_ptr")
                        .unwrap();
                    let r_int = self
                        .builder
                        .build_ptr_to_int(right.into_pointer_value(), self.context.i64_type(), "eq_r_ptr")
                        .unwrap();
                    self.builder
                        .build_int_compare(inkwell::IntPredicate::EQ, l_int, r_int, "ptr_eq")
                        .unwrap()
                        .into()
                } else {
                    return Err(CompileError::simple(
                        &format!(
                            "codegen: NotEqual received mixed operands (left={}, right={}) — \
                             builder should have coerced",
                            describe(&left),
                            describe(&right)
                        ),
                        0, 0, "", ErrorCode::E0009,
                    ));
                }
            }
        };
        Ok(result)
    }
}
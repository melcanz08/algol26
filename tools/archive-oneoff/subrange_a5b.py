#!/usr/bin/env python3
"""
A5b: BoundsCheck arms for the capability scanner, interpreter, and
LLVM codegen.
"""

from pathlib import Path

SCAN = Path("src/backends/capabilities/scan.rs")
INTERP = Path("src/backends/interpreter/mod.rs")
LLVM = Path("src/backends/llvm_codegen/instruction.rs")


def patch(path, edits):
    src = path.read_text()
    for old, new, label in edits:
        n = src.count(old)
        if n != 1:
            print(f"FAIL: {path} — {label} matched {n} times")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            path.write_text(src)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
        path.write_text(src)
        print(f"OK: {path} — {label}")


# 1. Capability scanner — BoundsCheck has no feature implications of
#    its own; scan the value for nested features.
patch(SCAN, [
    (
        """        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => {}
        Instruction::Nop => {}""",
        """        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => {}
        // ADR 0031: BoundsCheck is transparent to the capability
        // matrix. The value's own features (if any — subrange
        // construction only accepts Int or enum arguments) are
        // scanned; the check itself introduces no feature.
        Instruction::BoundsCheck { value, .. } => scan_value(value, extern_fns, used),
        Instruction::Nop => {}""",
        "scanner arm",
    ),
])

# 2. Interpreter — evaluate the value, compare, return EvalError::Runtime
#    on out of range.
patch(INTERP, [
    (
        """            Instruction::Print { value } => {
                let val = self.eval_value(value)?;
                self.output.push(val.display());
            }""",
        """            Instruction::Print { value } => {
                let val = self.eval_value(value)?;
                self.output.push(val.display());
            }
            Instruction::BoundsCheck {
                value,
                low,
                high,
                message,
            } => {
                let v = self.eval_value(value)?;
                let n = match v {
                    RuntimeValue::Int(i) => i,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "BoundsCheck",
                            left: runtime_kind(&other),
                            right: "Int",
                        });
                    }
                };
                if n < *low || n > *high {
                    return Err(EvalError::Runtime(message.clone()));
                }
            }""",
        "interpreter arm",
    ),
])

# 3. LLVM — mirror the ArrayAssign OOB pattern.
patch(LLVM, [
    (
        """            Instruction::RegionEnter { name } => {""",
        """            Instruction::BoundsCheck {
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
                            "BoundsCheck value did not lower to an integer \\
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
                    .build_int_compare(
                        inkwell::IntPredicate::SLT,
                        iv_i64,
                        low_val,
                        "bc_below",
                    )
                    .unwrap();
                let above = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::SGT,
                        iv_i64,
                        high_val,
                        "bc_above",
                    )
                    .unwrap();
                let out_of_range = self.builder.build_or(below, above, "bc_oob").unwrap();

                let error_bb = self.context.append_basic_block(
                    self.current_function.unwrap(),
                    "bounds_check_error",
                );
                let continue_bb = self.context.append_basic_block(
                    self.current_function.unwrap(),
                    "bounds_check_ok",
                );

                self.builder
                    .build_conditional_branch(out_of_range, error_bb, continue_bb)
                    .unwrap();

                self.builder.position_at_end(error_bb);
                let msg_global = self
                    .builder
                    .build_global_string_ptr(&format!("{}\\n", message), "bc_msg")
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
            Instruction::RegionEnter { name } => {""",
        "llvm arm",
    ),
])

// src/backends/llvm_codegen/effects.rs

use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use inkwell::values::BasicValueEnum;

impl<'ctx> IRCodeGen<'ctx> {
    /// Print a `Bool` value as `true` or `false`, matching the
    /// interpreter's `RuntimeValue::display()`.
    ///
    /// `printf("%d\n", i1)` would print 0/1, diverging from the
    /// interpreter. Instead, branch on the value and call `printf`
    /// with a constant "true\n" or "false\n" string.
    pub (super) fn emit_print_bool(
        &self,
        printf_fn: inkwell::values::FunctionValue<'ctx>,
        val: BasicValueEnum<'ctx>,
    ) -> Result<()> {
        let bool_val = val.into_int_value();
        let bool_i1 = if bool_val.get_type().get_bit_width() == 1 {
            bool_val
        } else {
            self.builder
                .build_int_compare(
                    inkwell::IntPredicate::NE,
                    bool_val,
                    bool_val.get_type().const_zero(),
                    "bool_normalize",
                )
                .unwrap()
        };

        let true_str = self
            .builder
            .build_global_string_ptr("true\n", "bool_true_str")
            .unwrap();
        let false_str = self
            .builder
            .build_global_string_ptr("false\n", "bool_false_str")
            .unwrap();

        let current_fn = self.current_function.unwrap();
        let true_bb = self
            .context
            .append_basic_block(current_fn, "print_bool_true");
        let false_bb = self
            .context
            .append_basic_block(current_fn, "print_bool_false");
        let after_bb = self
            .context
            .append_basic_block(current_fn, "print_bool_after");

        self.builder
            .build_conditional_branch(bool_i1, true_bb, false_bb)
            .unwrap();

        self.builder.position_at_end(true_bb);
        self.builder
            .build_call(
                printf_fn,
                &[true_str.as_pointer_value().into()],
                "print_true_call",
            )
            .unwrap();
        self.builder.build_unconditional_branch(after_bb).unwrap();

        self.builder.position_at_end(false_bb);
        self.builder
            .build_call(
                printf_fn,
                &[false_str.as_pointer_value().into()],
                "print_false_call",
            )
            .unwrap();
        self.builder.build_unconditional_branch(after_bb).unwrap();

        self.builder.position_at_end(after_bb);
        Ok(())
    }

    pub(super) fn emit_print(&self, val: BasicValueEnum<'ctx>, ty: Type) -> Result<()> {
        // Peel reference and pointer wrappers, loading through the
        // pointer to reach the printable value. `print(&x)` prints the
        // value of `x`, matching the interpreter's eventual
        // implicit-deref behavior for references.
        let (val, ty) = match ty {
            Type::Borrow(inner) | Type::MutBorrow(inner) | Type::Pointer(inner) => {
                if val.is_pointer_value() {
                    let inner_ty = (*inner).clone();
                    let llvm_ty = self.map_type(&inner_ty);
                    let loaded = self
                        .builder
                        .build_load(llvm_ty, val.into_pointer_value(), "print_deref")
                        .unwrap();
                    (loaded, inner_ty)
                } else {
                    // The value isn't a pointer even though the type
                    // says it should be. Fall back to the inner type
                    // without loading — the codegen contract is
                    // violated, but producing *some* value beats
                    // emitting a diagnostic mid-print.
                    (val, (*inner).clone())
                }
            }
            other => (val, other),
        };

        let printf = self.module.get_function("printf");
        if let Some(printf_fn) = printf {
            // Bool is special: the interpreter prints "true" / "false",
            // and `printf("%d\n", i1)` would print 0/1. Route through
            // the specialized helper so the two backends agree.
            if ty == Type::Bool {
                return self.emit_print_bool(printf_fn, val);
            }

            let format_str = match crate::common::types::print::llvm_format(&ty) {
                Some(s) => s,
                None => {
                    return Err(CompileError::simple(
                        &format!(
                            "LLVM codegen: cannot print value of type {:?} \
                             (no LLVM format mapping; see common::types::print)",
                            ty
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ));
                }
            };
            let fmt_ptr = self
                .builder
                .build_global_string_ptr(format_str, "fmt")
                .unwrap();
            let fmt_arg: BasicValueEnum = fmt_ptr.as_pointer_value().into();
            let args: Vec<inkwell::values::BasicMetadataValueEnum> =
                vec![fmt_arg.into(), val.into()];
            self.builder
                .build_call(printf_fn, &args, "printcall")
                .unwrap();
        }
        Ok(())
    }

}
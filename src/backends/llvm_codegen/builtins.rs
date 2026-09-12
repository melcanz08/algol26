// src/backends/llvm_codegen/builtins.rs

use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::TypedIRValue;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;


impl<'ctx> IRCodeGen<'ctx> {
    pub(super) fn register_stdlib(&mut self) {
        // Print functions
        let i8_ptr = self.context.ptr_type(AddressSpace::default());
        let printf_ty = self.context.i32_type().fn_type(&[i8_ptr.into()], true);
        let printf_fn = self.module.add_function("printf", printf_ty, None);
        self.functions.insert("printf".to_string(), printf_fn);

        // exit(i32) -> void
        let exit_ty = self
            .context
            .void_type()
            .fn_type(&[self.context.i32_type().into()], false);
        let exit_fn = self.module.add_function("exit", exit_ty, None);
        self.functions.insert("exit".to_string(), exit_fn);

        // Math functions
        let f64_ty = self.context.f64_type();
        let f64_param = f64_ty.into();

        let math_functions = [
            ("sqrt", vec![f64_param]),
            ("sin", vec![f64_param]),
            ("cos", vec![f64_param]),
            ("tan", vec![f64_param]),
            ("exp", vec![f64_param]),
            ("log", vec![f64_param]),
            ("floor", vec![f64_param]),
            ("ceil", vec![f64_param]),
            ("fabs", vec![f64_param]), // abs for floats
        ];

        for (name, params) in math_functions {
            let fn_ty = f64_ty.fn_type(&params, false);
            let fn_val = self.module.add_function(name, fn_ty, None);
            self.functions.insert(name.to_string(), fn_val);
        }

        // pow takes two f64 params
        let pow_ty = f64_ty.fn_type(&[f64_param, f64_param], false);
        let pow_fn = self.module.add_function("pow", pow_ty, None);
        self.functions.insert("pow".to_string(), pow_fn);

        // String functions
        let strlen_ty = self.context.i64_type().fn_type(&[i8_ptr.into()], false);
        let strlen_fn = self.module.add_function("strlen", strlen_ty, None);
        self.functions.insert("strlen".to_string(), strlen_fn);

        let strcmp_ty = self
            .context
            .i32_type()
            .fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let strcmp_fn = self.module.add_function("strcmp", strcmp_ty, None);
        self.functions.insert("strcmp".to_string(), strcmp_fn);

        let strcat_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let strcat_fn = self.module.add_function("strcat", strcat_ty, None);
        self.functions.insert("strcat".to_string(), strcat_fn);
    }

    pub(super) fn compile_builtin_value(
        &self,
        name: &str,
        args: &[TypedIRValue],
    ) -> Result<BasicValueEnum<'ctx>> {
        match name {
            "List.length" | "len" | "length" => {
                match args.first() {
                    Some(TypedIRValue::Variable(var_name, _)) => {
                        match self.list_lengths.get(var_name) {
                            Some(len) => Ok(self
                                .context
                                .i64_type()
                                .const_int(*len as u64, false)
                                .into()),
                            None => Err(CompileError::simple(
                                &format!(
                                    "LLVM codegen: List.length called on unknown list '{}' \
                                     (known lists: {:?})",
                                    var_name,
                                    self.list_lengths.keys().collect::<Vec<_>>()
                                ),
                                0, 0, "", ErrorCode::E0004,
                            )),
                        }
                    }
                    _ => Err(CompileError::simple(
                        "LLVM codegen: List.length requires a variable argument",
                        0, 0, "", ErrorCode::E0004,
                    )),
                }
            }
            "String.length" | "String.len" => {
                let arg = args.first().ok_or_else(|| CompileError::simple(
                    "LLVM codegen: String.length requires exactly one argument",
                    0, 0, "", ErrorCode::E0004,
                ))?;
                let s_val = self.compile_value(arg)?;
                if !s_val.is_pointer_value() {
                    return Err(CompileError::simple(
                        &format!(
                            "LLVM codegen: String.length expected a pointer argument, got {:?}",
                            s_val
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ));
                }
                let strlen_fn = self.module.get_function("strlen").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: strlen not registered in stdlib",
                        0, 0, "", ErrorCode::E0009,
                    )
                })?;
                let call = self
                    .builder
                    .build_call(strlen_fn, &[s_val.into()], "strlen_call")
                    .unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok(v),
                    _ => Ok(self.context.i64_type().const_zero().into()),
                }
            }
            other => Err(CompileError::simple(
                &format!(
                    "LLVM codegen: unhandled builtin '{}' in compile_builtin_value \
                     (this builtin has no LLVM lowering)",
                    other
                ),
                0, 0, "", ErrorCode::E0004,
            )),
        }
    }

    pub(super) fn compile_builtin_call(
        &mut self,
        name: &str,
        args: &[TypedIRValue],
        result: &Option<String>,
    ) -> Result<()> {
        let val = self.compile_builtin_value(name, args)?;
        if let Some(res_name) = result {
            if let Some(ptr) = self.variables.get(res_name).cloned() {
                self.builder.build_store(ptr, val).unwrap();
            } else {
                let alloca = self.create_entry_alloca(res_name, &Type::Float);
                self.builder.build_store(alloca, val).unwrap();
                self.variables.insert(res_name.clone(), alloca);
                self.var_types.insert(res_name.clone(), Type::Float);
            }
        }
        Ok(())
    }
}
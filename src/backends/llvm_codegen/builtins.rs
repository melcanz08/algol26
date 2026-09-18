// src/backends/llvm_codegen/builtins.rs

use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::TypedIRValue;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;
use inkwell::IntPredicate;

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

        // UTF-8 codepoint-counting helper (Tier 0.2b).
        //
        //   i64 @algol26_strlen_utf8(i8* %s)
        //
        // Walks a null-terminated byte stream and counts every byte
        // whose top two bits are *not* `10`. That counts ASCII bytes
        // and lead bytes of multi-byte sequences, while skipping
        // continuation bytes. Result is the Unicode codepoint count,
        // matching the interpreter's `str::chars().count()`.
        //
        // `String.length` in `compile_builtin_value` calls this
        // instead of C `strlen` (which counts bytes).
        let utf8_len_ty = self.context.i64_type().fn_type(&[i8_ptr.into()], false);
        let utf8_len_fn = self
            .module
            .add_function("algol26_strlen_utf8", utf8_len_ty, None);
        self.functions
            .insert("algol26_strlen_utf8".to_string(), utf8_len_fn);

        let i64_ty = self.context.i64_type();
        let i8_ty = self.context.i8_type();

        let saved_bb = self.builder.get_insert_block();

        let entry_bb = self.context.append_basic_block(utf8_len_fn, "entry");
        let loop_bb = self.context.append_basic_block(utf8_len_fn, "loop");
        let body_bb = self.context.append_basic_block(utf8_len_fn, "body");
        let done_bb = self.context.append_basic_block(utf8_len_fn, "done");

        // entry: alloca idx and count, init to 0, jump to loop.
        self.builder.position_at_end(entry_bb);
        let idx_ptr = self.builder.build_alloca(i64_ty, "idx_ptr").unwrap();
        let count_ptr = self.builder.build_alloca(i64_ty, "count_ptr").unwrap();
        self.builder
            .build_store(idx_ptr, i64_ty.const_zero())
            .unwrap();
        self.builder
            .build_store(count_ptr, i64_ty.const_zero())
            .unwrap();
        self.builder.build_unconditional_branch(loop_bb).unwrap();

        // loop: load idx, compute ptr, load byte, check for NUL.
        self.builder.position_at_end(loop_bb);
        let s_ptr = utf8_len_fn
            .get_nth_param(0)
            .unwrap()
            .into_pointer_value();
        let idx = self
            .builder
            .build_load(i64_ty, idx_ptr, "idx")
            .unwrap()
            .into_int_value();
        // SAFETY: `i8_ty` is the pointee type of `s_ptr` (a `i8*`),
        // and `idx` is the byte offset into that array. The pointer
        // is a valid null-terminated C string supplied by the
        // caller of `algol26_strlen_utf8`. GEP cannot produce UB
        // here; the unsafe marker is inkwell's conservative
        // requirement.
        let ptr = unsafe {
            self.builder
                .build_gep(i8_ty, s_ptr, &[idx], "ptr")
                .unwrap()
        };
        let byte = self
            .builder
            .build_load(i8_ty, ptr, "byte")
            .unwrap()
            .into_int_value();
        let is_end = self
            .builder
            .build_int_compare(IntPredicate::EQ, byte, i8_ty.const_zero(), "is_end")
            .unwrap();
        self.builder
            .build_conditional_branch(is_end, done_bb, body_bb)
            .unwrap();

        // body: increment count if byte is not a UTF-8 continuation
        // byte (`10xxxxxx`, i.e. `byte & 0xC0 == 0x80`), increment idx,
        // loop.
        self.builder.position_at_end(body_bb);
        let count = self
            .builder
            .build_load(i64_ty, count_ptr, "count")
            .unwrap()
            .into_int_value();
        let masked = self
            .builder
            .build_and(byte, i8_ty.const_int(0xC0, false), "masked")
            .unwrap();
        let is_cont = self
            .builder
            .build_int_compare(
                IntPredicate::EQ,
                masked,
                i8_ty.const_int(0x80, false),
                "is_cont",
            )
            .unwrap();
        let inc = self
            .builder
            .build_select(
                is_cont,
                i64_ty.const_zero(),
                i64_ty.const_int(1, false),
                "inc",
            )
            .unwrap()
            .into_int_value();
        let count_new = self
            .builder
            .build_int_add(count, inc, "count_new")
            .unwrap();
        self.builder.build_store(count_ptr, count_new).unwrap();
        let idx_new = self
            .builder
            .build_int_add(idx, i64_ty.const_int(1, false), "idx_new")
            .unwrap();
        self.builder.build_store(idx_ptr, idx_new).unwrap();
        self.builder.build_unconditional_branch(loop_bb).unwrap();

        // done: load final count and return.
        self.builder.position_at_end(done_bb);
        let result = self
            .builder
            .build_load(i64_ty, count_ptr, "result")
            .unwrap();
        self.builder.build_return(Some(&result)).unwrap();

        // Restore the builder's insert point if the caller had one.
        if let Some(bb) = saved_bb {
            self.builder.position_at_end(bb);
        }

        let strcmp_ty = self
            .context
            .i32_type()
            .fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let strcmp_fn = self.module.add_function("strcmp", strcmp_ty, None);
        self.functions.insert("strcmp".to_string(), strcmp_fn);

        let strcat_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let strcat_fn = self.module.add_function("strcat", strcat_ty, None);
        self.functions.insert("strcat".to_string(), strcat_fn);

        // Raw memory: `alloc(n)` lowers to `malloc(n)`;
        // `free(p)` lowers to `free(p)`. (Step 5 wiring.)
        let malloc_ty = i8_ptr.fn_type(&[self.context.i64_type().into()], false);
        let malloc_fn = self.module.add_function("malloc", malloc_ty, None);
        self.functions.insert("malloc".to_string(), malloc_fn);

        let free_ty = self.context.void_type().fn_type(&[i8_ptr.into()], false);
        let free_fn = self.module.add_function("free", free_ty, None);
        self.functions.insert("free".to_string(), free_fn);
    }

    pub(super) fn compile_builtin_value(
        &self,
        name: &str,
        args: &[TypedIRValue],
    ) -> Result<BasicValueEnum<'ctx>> {
        match name {
            "List.length" | "len" | "length" => match args.first() {
                Some(TypedIRValue::Variable(var_name, _)) => {
                    match self.list_lengths.get(var_name) {
                        Some(len) => {
                            Ok(self.context.i64_type().const_int(*len as u64, false).into())
                        }
                        None => Err(CompileError::simple(
                            &format!(
                                "LLVM codegen: List.length called on unknown list '{}' \
                                     (known lists: {:?})",
                                var_name,
                                self.list_lengths.keys().collect::<Vec<_>>()
                            ),
                            0,
                            0,
                            "",
                            ErrorCode::E0004,
                        )),
                    }
                }
                _ => Err(CompileError::simple(
                    "LLVM codegen: List.length requires a variable argument",
                    0,
                    0,
                    "",
                    ErrorCode::E0004,
                )),
            },
            // Lowered to `algol26_strlen_utf8`, a UTF-8 codepoint
            // counter emitted by `register_stdlib`. Matches the
            // interpreter's `str::chars().count()` (Tier 0.2b).
            "String.length" | "String.len" => {
                let arg = args.first().ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: String.length requires exactly one argument",
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    )
                })?;
                let s_val = self.compile_value(arg)?;
                if !s_val.is_pointer_value() {
                    return Err(CompileError::simple(
                        &format!(
                            "LLVM codegen: String.length expected a pointer argument, got {:?}",
                            s_val
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let utf8_len_fn = self
                    .module
                    .get_function("algol26_strlen_utf8")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "LLVM codegen: algol26_strlen_utf8 not registered in stdlib",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(utf8_len_fn, &[s_val.into()], "utf8_strlen_call")
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
                0,
                0,
                "",
                ErrorCode::E0004,
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
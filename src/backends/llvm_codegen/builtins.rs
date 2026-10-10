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

        // `sprintf` writes a formatted string to a caller-supplied
        // buffer. Used by `Int.to_string` and (eventually) other
        // number→string builtins. Variadic, like printf.
        let sprintf_ty = self
            .context
            .i32_type()
            .fn_type(&[i8_ptr.into(), i8_ptr.into()], true);
        self.module.add_function("sprintf", sprintf_ty, None);

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

        // ── algol26_string_concat ──
        // Allocates strlen(a)+strlen(b)+1 bytes, copies both strings
        // into the buffer, NUL-terminates. Used by `String.concat`.
        // ADR 00XX (string builtins on LLVM/WASM).
        let concat_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let concat_fn = self
            .module
            .add_function("algol26_string_concat", concat_ty, None);
        self.functions
            .insert("algol26_string_concat".to_string(), concat_fn);
        {
            let saved = self.builder.get_insert_block();
            let entry = self.context.append_basic_block(concat_fn, "entry");
            self.builder.position_at_end(entry);

            let a = concat_fn.get_nth_param(0).unwrap().into_pointer_value();
            let b = concat_fn.get_nth_param(1).unwrap().into_pointer_value();
            let i64_loc = self.context.i64_type();

            let strlen_ty = i64_loc.fn_type(&[i8_ptr.into()], false);
            let strlen_fn = self
                .module
                .get_function("strlen")
                .unwrap_or_else(|| self.module.add_function("strlen", strlen_ty, None));
            let la_call = self
                .builder
                .build_call(strlen_fn, &[a.into()], "la")
                .unwrap();
            let la = match la_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                inkwell::values::ValueKind::Instruction(_) => {
                    unreachable!("algol26_string_concat: strlen(a) returned no value")
                }
            };
            let lb_call = self
                .builder
                .build_call(strlen_fn, &[b.into()], "lb")
                .unwrap();
            let lb = match lb_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                inkwell::values::ValueKind::Instruction(_) => {
                    unreachable!("algol26_string_concat: strlen(b) returned no value")
                }
            };
            let total = self.builder.build_int_add(la, lb, "total").unwrap();
            let size = self
                .builder
                .build_int_add(total, i64_loc.const_int(1, false), "size")
                .unwrap();

            let malloc_ty = i8_ptr.fn_type(&[i64_loc.into()], false);
            let malloc_fn = self
                .module
                .get_function("malloc")
                .unwrap_or_else(|| self.module.add_function("malloc", malloc_ty, None));
            let buf_call = self
                .builder
                .build_call(malloc_fn, &[size.into()], "buf")
                .unwrap();
            let buf = match buf_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                inkwell::values::ValueKind::Instruction(_) => {
                    unreachable!("algol26_string_concat: malloc returned no value")
                }
            };

            let strcpy_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
            let strcpy_fn = self
                .module
                .get_function("strcpy")
                .unwrap_or_else(|| self.module.add_function("strcpy", strcpy_ty, None));
            let strcat_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
            let strcat_fn = self
                .module
                .get_function("strcat")
                .unwrap_or_else(|| self.module.add_function("strcat", strcat_ty, None));

            self.builder
                .build_call(strcpy_fn, &[buf.into(), a.into()], "")
                .unwrap();
            self.builder
                .build_call(strcat_fn, &[buf.into(), b.into()], "")
                .unwrap();
            self.builder.build_return(Some(&buf)).unwrap();

            if let Some(bb) = saved {
                self.builder.position_at_end(bb);
            }
        }

        // ── algol26_string_to_upper / _to_lower ──
        // Allocate strlen(s)+1 bytes, walk bytes, apply the C
        // ctype function, NUL-terminate.
        for (fn_name, c_name) in [
            ("algol26_string_to_upper", "toupper"),
            ("algol26_string_to_lower", "tolower"),
        ] {
            let f_ty = i8_ptr.fn_type(&[i8_ptr.into()], false);
            let f = self.module.add_function(fn_name, f_ty, None);
            self.functions.insert(fn_name.to_string(), f);

            let i32_ty_loc = self.context.i32_type();
            let c_ty = i32_ty_loc.fn_type(&[i32_ty_loc.into()], false);
            let c_fn = self
                .module
                .get_function(c_name)
                .unwrap_or_else(|| self.module.add_function(c_name, c_ty, None));

            let saved = self.builder.get_insert_block();
            let entry = self.context.append_basic_block(f, "entry");
            let loop_bb = self.context.append_basic_block(f, "loop");
            let body_bb = self.context.append_basic_block(f, "body");
            let done_bb = self.context.append_basic_block(f, "done");
            self.builder.position_at_end(entry);

            let src = f.get_nth_param(0).unwrap().into_pointer_value();
            let i64_loc = self.context.i64_type();
            let i8_loc = self.context.i8_type();

            let strlen_ty = i64_loc.fn_type(&[i8_ptr.into()], false);
            let strlen_fn = self
                .module
                .get_function("strlen")
                .unwrap_or_else(|| self.module.add_function("strlen", strlen_ty, None));
            let len_call = self
                .builder
                .build_call(strlen_fn, &[src.into()], "len")
                .unwrap();
            let len = match len_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                inkwell::values::ValueKind::Instruction(_) => unreachable!(),
            };

            let size = self
                .builder
                .build_int_add(len, i64_loc.const_int(1, false), "size")
                .unwrap();
            let malloc_ty = i8_ptr.fn_type(&[i64_loc.into()], false);
            let malloc_fn = self
                .module
                .get_function("malloc")
                .unwrap_or_else(|| self.module.add_function("malloc", malloc_ty, None));
            let buf_call = self
                .builder
                .build_call(malloc_fn, &[size.into()], "buf")
                .unwrap();
            let buf = match buf_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                inkwell::values::ValueKind::Instruction(_) => unreachable!(),
            };

            let idx_ptr = self.builder.build_alloca(i64_loc, "idx").unwrap();
            self.builder
                .build_store(idx_ptr, i64_loc.const_zero())
                .unwrap();
            self.builder.build_unconditional_branch(loop_bb).unwrap();

            // loop: load byte; if NUL, goto done; else goto body
            self.builder.position_at_end(loop_bb);
            let i = self
                .builder
                .build_load(i64_loc, idx_ptr, "i")
                .unwrap()
                .into_int_value();
            let src_p = unsafe { self.builder.build_gep(i8_loc, src, &[i], "src_p").unwrap() };
            let byte = self
                .builder
                .build_load(i8_loc, src_p, "byte")
                .unwrap()
                .into_int_value();
            let is_end = self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::EQ,
                    byte,
                    i8_loc.const_zero(),
                    "is_end",
                )
                .unwrap();
            self.builder
                .build_conditional_branch(is_end, done_bb, body_bb)
                .unwrap();

            // body: buf[i] = c_fn(byte); idx++; goto loop
            self.builder.position_at_end(body_bb);
            let byte_wide = self
                .builder
                .build_int_z_extend(byte, i32_ty_loc, "byte32")
                .unwrap();
            let trans_call = self
                .builder
                .build_call(c_fn, &[byte_wide.into()], "trans")
                .unwrap();
            let trans = match trans_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                inkwell::values::ValueKind::Instruction(_) => unreachable!(),
            };
            let trans_byte = self
                .builder
                .build_int_truncate(trans, i8_loc, "trans8")
                .unwrap();
            let dst_p = unsafe { self.builder.build_gep(i8_loc, buf, &[i], "dst_p").unwrap() };
            self.builder.build_store(dst_p, trans_byte).unwrap();
            let i_next = self
                .builder
                .build_int_add(i, i64_loc.const_int(1, false), "i_next")
                .unwrap();
            self.builder.build_store(idx_ptr, i_next).unwrap();
            self.builder.build_unconditional_branch(loop_bb).unwrap();

            // done: buf[len] = 0; return buf
            self.builder.position_at_end(done_bb);
            let end_p = unsafe {
                self.builder
                    .build_gep(i8_loc, buf, &[len], "end_p")
                    .unwrap()
            };
            self.builder
                .build_store(end_p, i8_loc.const_zero())
                .unwrap();
            self.builder.build_return(Some(&buf)).unwrap();

            if let Some(bb) = saved {
                self.builder.position_at_end(bb);
            }
        }

        // ── algol26_string_substring(s, start, length) -> i8* ──
        // Clamps start and length to the string's bounds, mallocs,
        // memcpy's the slice, NUL-terminates. Straight-line: no
        // loop, no basic blocks beyond entry.
        {
            let i64_loc = self.context.i64_type();
            let i8_loc = self.context.i8_type();

            let sub_ty = i8_ptr.fn_type(&[i8_ptr.into(), i64_loc.into(), i64_loc.into()], false);
            let sub_fn = self
                .module
                .add_function("algol26_string_substring", sub_ty, None);
            self.functions
                .insert("algol26_string_substring".to_string(), sub_fn);

            let saved = self.builder.get_insert_block();
            let entry = self.context.append_basic_block(sub_fn, "entry");
            self.builder.position_at_end(entry);

            let s = sub_fn.get_nth_param(0).unwrap().into_pointer_value();
            let start = sub_fn.get_nth_param(1).unwrap().into_int_value();
            let len = sub_fn.get_nth_param(2).unwrap().into_int_value();
            let zero = i64_loc.const_zero();
            let one = i64_loc.const_int(1, false);

            // n = strlen(s)
            let strlen_ty = i64_loc.fn_type(&[i8_ptr.into()], false);
            let strlen_fn = self
                .module
                .get_function("strlen")
                .unwrap_or_else(|| self.module.add_function("strlen", strlen_ty, None));
            let n_call = self
                .builder
                .build_call(strlen_fn, &[s.into()], "n")
                .unwrap();
            let n = match n_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                _ => unreachable!(),
            };

            // Clamp start to [0, n]: start_b = clamp(start, 0, n)
            let start_neg = self
                .builder
                .build_int_compare(inkwell::IntPredicate::SLT, start, zero, "start_neg")
                .unwrap();
            let zero_bv: inkwell::values::BasicValueEnum = zero.into();
            let start_bv: inkwell::values::BasicValueEnum = start.into();
            let start_a = self
                .builder
                .build_select(start_neg, zero_bv, start_bv, "start_a")
                .unwrap()
                .into_int_value();
            let start_over = self
                .builder
                .build_int_compare(inkwell::IntPredicate::SGT, start_a, n, "start_over")
                .unwrap();
            let n_bv: inkwell::values::BasicValueEnum = n.into();
            let start_a_bv: inkwell::values::BasicValueEnum = start_a.into();
            let start_b = self
                .builder
                .build_select(start_over, n_bv, start_a_bv, "start_b")
                .unwrap()
                .into_int_value();

            // Clamp length to [0, n - start_b]
            let len_neg = self
                .builder
                .build_int_compare(inkwell::IntPredicate::SLT, len, zero, "len_neg")
                .unwrap();
            let len_bv: inkwell::values::BasicValueEnum = len.into();
            let len_a = self
                .builder
                .build_select(len_neg, zero_bv, len_bv, "len_a")
                .unwrap()
                .into_int_value();
            let available = self.builder.build_int_sub(n, start_b, "avail").unwrap();
            let len_over = self
                .builder
                .build_int_compare(inkwell::IntPredicate::SGT, len_a, available, "len_over")
                .unwrap();
            let available_bv: inkwell::values::BasicValueEnum = available.into();
            let len_a_bv: inkwell::values::BasicValueEnum = len_a.into();
            let len_b = self
                .builder
                .build_select(len_over, available_bv, len_a_bv, "len_b")
                .unwrap()
                .into_int_value();

            // buf = malloc(len_b + 1)
            let size = self.builder.build_int_add(len_b, one, "size").unwrap();
            let malloc_ty = i8_ptr.fn_type(&[i64_loc.into()], false);
            let malloc_fn = self
                .module
                .get_function("malloc")
                .unwrap_or_else(|| self.module.add_function("malloc", malloc_ty, None));
            let buf_call = self
                .builder
                .build_call(malloc_fn, &[size.into()], "buf")
                .unwrap();
            let buf = match buf_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                _ => unreachable!(),
            };

            // memcpy(buf, s + start_b, len_b)
            let memcpy_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into(), i64_loc.into()], false);
            let memcpy_fn = self
                .module
                .get_function("memcpy")
                .unwrap_or_else(|| self.module.add_function("memcpy", memcpy_ty, None));
            let src_p = unsafe {
                self.builder
                    .build_gep(i8_loc, s, &[start_b], "src_p")
                    .unwrap()
            };
            self.builder
                .build_call(memcpy_fn, &[buf.into(), src_p.into(), len_b.into()], "")
                .unwrap();

            // buf[len_b] = 0
            let end_p = unsafe {
                self.builder
                    .build_gep(i8_loc, buf, &[len_b], "end_p")
                    .unwrap()
            };
            self.builder
                .build_store(end_p, i8_loc.const_zero())
                .unwrap();
            self.builder.build_return(Some(&buf)).unwrap();

            if let Some(bb) = saved {
                self.builder.position_at_end(bb);
            }
        }

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
        let s_ptr = utf8_len_fn.get_nth_param(0).unwrap().into_pointer_value();
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
        let ptr = unsafe { self.builder.build_gep(i8_ty, s_ptr, &[idx], "ptr").unwrap() };
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
        let count_new = self.builder.build_int_add(count, inc, "count_new").unwrap();
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
        // ADR 0042 phase 4. `List.append` grows a heap-backed list
        // via `realloc(buffer, new_cap * elem_size)`.
        let realloc_ty = i8_ptr.fn_type(&[i8_ptr.into(), self.context.i64_type().into()], false);
        let realloc_fn = self.module.add_function("realloc", realloc_ty, None);
        self.functions.insert("realloc".to_string(), realloc_fn);

        let free_ty = self.context.void_type().fn_type(&[i8_ptr.into()], false);
        let free_fn = self.module.add_function("free", free_ty, None);
        self.functions.insert("free".to_string(), free_fn);
    }
    /// Compile a call to a backend builtin. Returns the LLVM value
    /// **and** the ALGOL26 type it corresponds to, so the caller can
    /// allocate a result slot with the correct type.
    ///
    /// Before this change the type was not reported, and
    /// `compile_builtin_call` assumed `Float` — mis-typing the slot
    /// for every Int-returning builtin (both current builtins).
    pub(super) fn compile_builtin_value(
        &self,
        name: &str,
        args: &[TypedIRValue],
    ) -> Result<(BasicValueEnum<'ctx>, Type)> {
        match name {
            "List.length" | "len" | "length" => match args.first() {
                Some(TypedIRValue::Variable(var_name, _)) => {
                    match self.list_lengths.get(var_name) {
                        Some(len) => Ok((
                            self.context.i64_type().const_int(*len as u64, false).into(),
                            Type::Int,
                        )),
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
                let utf8_len_fn =
                    self.module
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
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::Int)),
                    // algol26_strlen_utf8 returns i64; a call that
                    // produces no value means the module was built
                    // incorrectly. Fail closed rather than silently
                    // returning 0.
                    inkwell::values::ValueKind::Instruction(_) => {
                        Err(CompileError::unsupported_operation(
                            "String.length (algol26_strlen_utf8 call produced no value)",
                            "llvm",
                        ))
                    }
                }
            }
            // ADR 00XX (Path A). `Int.to_string` allocates a stack
            // buffer and formats the integer into it via `sprintf`.
            // The buffer lives for the enclosing function's lifetime,
            // which matches how the interpreter's string values behave
            // (they're owned by the caller's frame until reassigned).
            //
            // 24 bytes is enough for i64::MIN ("-9223372036854775808",
            // 20 chars) plus a NUL terminator, with room to spare.
            "String.to_upper" => {
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        "String.to_upper requires 1 argument",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s = self.compile_value(&args[0])?;
                let f = self
                    .module
                    .get_function("algol26_string_to_upper")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "internal: algol26_string_to_upper not registered",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self.builder.build_call(f, &[s.into()], "upper").unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::String)),
                    inkwell::values::ValueKind::Instruction(_) => Err(CompileError::simple(
                        "internal: to_upper returned no value",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )),
                }
            }
            "String.to_lower" => {
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        "String.to_lower requires 1 argument",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s = self.compile_value(&args[0])?;
                let f = self
                    .module
                    .get_function("algol26_string_to_lower")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "internal: algol26_string_to_lower not registered",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self.builder.build_call(f, &[s.into()], "lower").unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::String)),
                    inkwell::values::ValueKind::Instruction(_) => Err(CompileError::simple(
                        "internal: to_lower returned no value",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )),
                }
            }
            "String.substring" => {
                if args.len() != 3 {
                    return Err(CompileError::simple(
                        "String.substring requires 3 arguments (s, start, length)",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s = self.compile_value(&args[0])?;
                let start = self.compile_value(&args[1])?;
                let length = self.compile_value(&args[2])?;
                let f = self
                    .module
                    .get_function("algol26_string_substring")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "internal: algol26_string_substring not registered",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(f, &[s.into(), start.into(), length.into()], "substr")
                    .unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::String)),
                    inkwell::values::ValueKind::Instruction(_) => Err(CompileError::simple(
                        "internal: substring returned no value",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )),
                }
            }
            "String.concat" => {
                if args.len() != 2 {
                    return Err(CompileError::simple(
                        "String.concat requires 2 arguments",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let a = self.compile_value(&args[0])?;
                let b = self.compile_value(&args[1])?;
                let f = self
                    .module
                    .get_function("algol26_string_concat")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "internal: algol26_string_concat not registered",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(f, &[a.into(), b.into()], "concat")
                    .unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::String)),
                    inkwell::values::ValueKind::Instruction(_) => Err(CompileError::simple(
                        "internal: concat returned no value",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )),
                }
            }
            "Int.to_string" => {
                let arg = args.first().ok_or_else(|| {
                    CompileError::simple(
                        "Int.to_string requires 1 argument",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    )
                })?;
                let val = self.compile_value(arg)?;
                if !val.is_int_value() {
                    return Err(CompileError::simple(
                        "Int.to_string expects an Int",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let n = val.into_int_value();

                let i8_ty = self.context.i8_type();
                let buf_ty = i8_ty.array_type(24);
                let buf = self.builder.build_alloca(buf_ty, "int_str_buf").unwrap();

                let i8_ptr = self.context.ptr_type(AddressSpace::default());
                let i32_ty = self.context.i32_type();
                let sprintf_ty = i32_ty.fn_type(&[i8_ptr.into(), i8_ptr.into()], true);
                let sprintf_fn = match self.module.get_function("sprintf") {
                    Some(f) => f,
                    None => self.module.add_function("sprintf", sprintf_ty, None),
                };

                let fmt = self
                    .builder
                    .build_global_string_ptr("%lld", "fmt_i64")
                    .unwrap();

                self.builder
                    .build_call(
                        sprintf_fn,
                        &[buf.into(), fmt.as_pointer_value().into(), n.into()],
                        "int_to_string",
                    )
                    .unwrap();

                Ok((buf.into(), Type::String))
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
        // ADR 0022. `affirm` returns Void. `compile_builtin_value`
        // assumes a value-returning builtin and produces a slot to
        // store into; dispatch `affirm` separately.
        if name == "affirm" {
            return self.compile_affirm(args);
        }

        // `compile_builtin_value` reports the ALGOL26 type alongside
        // the LLVM value so the result slot is allocated with the
        // correct type. Previously the type was assumed to be Float
        // even for Int-returning builtins, which silently mis-typed
        // the slot.
        let (val, val_ty) = self.compile_builtin_value(name, args)?;
        if let Some(res_name) = result {
            if let Some(ptr) = self.variables.get(res_name).cloned() {
                self.builder.build_store(ptr, val).unwrap();
            } else {
                let alloca = self.create_entry_alloca(res_name, &val_ty);
                self.builder.build_store(alloca, val).unwrap();
                self.variables.insert(res_name.clone(), alloca);
                self.var_types.insert(res_name.clone(), val_ty);
            }
        }
        Ok(())
    }
    /// Lower `affirm(cond, msg)` to LLVM.
    ///
    /// ADR 0022. If `cond` is true, execution continues. If false,
    /// the message is printed to stderr via `printf`, then `exit(1)`
    /// terminates the process. `exit` (not a return-from-function)
    /// because an assertion failure is unrecoverable regardless of
    /// where in the call stack it occurs.
    fn compile_affirm(&mut self, args: &[TypedIRValue]) -> Result<()> {
        if args.len() != 2 {
            return Err(CompileError::simple(
                &format!(
                    "LLVM codegen: affirm requires exactly 2 arguments, got {}",
                    args.len()
                ),
                0,
                0,
                "",
                ErrorCode::E0004,
            ));
        }

        let cond_val = self.compile_value(&args[0])?;
        let msg_val = self.compile_value(&args[1])?;

        if !msg_val.is_pointer_value() {
            return Err(CompileError::simple(
                &format!(
                    "LLVM codegen: affirm message must lower to a string pointer, got {:?}",
                    msg_val
                ),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        let cond_bool = if cond_val.is_int_value() {
            let iv = cond_val.into_int_value();
            if iv.get_type().get_bit_width() == 1 {
                iv
            } else {
                self.builder
                    .build_int_compare(
                        IntPredicate::NE,
                        iv,
                        iv.get_type().const_zero(),
                        "affirm_tobool",
                    )
                    .unwrap()
            }
        } else {
            return Err(CompileError::simple(
                &format!(
                    "LLVM codegen: affirm condition must lower to a Bool, got {:?}",
                    cond_val
                ),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        };

        let current_fn = self.current_function.unwrap();
        let fail_bb = self.context.append_basic_block(current_fn, "affirm_fail");
        let ok_bb = self.context.append_basic_block(current_fn, "affirm_ok");

        self.builder
            .build_conditional_branch(cond_bool, ok_bb, fail_bb)
            .unwrap();

        // Fail path: printf("assertion failed: %s\n", msg); exit(1);
        self.builder.position_at_end(fail_bb);
        let fmt = self
            .builder
            .build_global_string_ptr("assertion failed: %s\n", "affirm_fmt")
            .unwrap();
        let printf_fn = self.module.get_function("printf").ok_or_else(|| {
            CompileError::simple(
                "LLVM codegen: printf not registered in stdlib",
                0,
                0,
                "",
                ErrorCode::E0009,
            )
        })?;
        self.builder
            .build_call(
                printf_fn,
                &[fmt.as_pointer_value().into(), msg_val.into()],
                "affirm_print",
            )
            .unwrap();
        let exit_fn = self.module.get_function("exit").ok_or_else(|| {
            CompileError::simple(
                "LLVM codegen: exit not registered in stdlib",
                0,
                0,
                "",
                ErrorCode::E0009,
            )
        })?;
        self.builder
            .build_call(
                exit_fn,
                &[self.context.i32_type().const_int(1, false).into()],
                "affirm_exit",
            )
            .unwrap();
        self.builder.build_unreachable().unwrap();

        // Continue path.
        self.builder.position_at_end(ok_bb);
        Ok(())
    }
}

// src/backends/llvm_codegen/builtins.rs

use super::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::TypedIRValue;
use inkwell::values::BasicValueEnum;
use inkwell::AddressSpace;
use inkwell::IntPredicate;

impl<'ctx> IRCodeGen<'ctx> {
    /// Map the `(key_type, value_type)` pair of a Map to the
    /// symbol-name suffix its runtime functions carry. The runtime
    /// exposes `algol26_map_<op>_<suffix>` for each shape:
    ///
    ///   (Int, Int)       -> "int"      (runtime/map.c)
    ///   (String, Int)    -> "str_int"  (runtime/map_str_int.c)
    ///
    /// Any other shape is unsupported in v1 and refused with a
    /// targeted error at each `Map.*` call site. Extending the set
    /// (e.g. (String, String)) means a new C runtime and a new arm
    /// here; the existing arms' shape only branches on the key's
    /// LLVM type (i64 vs ptr), so the pattern is mechanical.
    pub(super) fn map_suffix(key_ty: &Type, value_ty: &Type) -> Option<&'static str> {
        match (key_ty, value_ty) {
            (Type::Int, Type::Int) => Some("int"),
            (Type::String, Type::Int) => Some("str_int"),
            (Type::String, Type::String) => Some("str_str"),
            _ => None,
        }
    }

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

        // ── algol26_string_to_int ──
        // Strict i64 parser matching the interpreter's
        // `str::parse::<i64>()`. Returns 1 on success (value
        // written through the out-pointer), 0 on parse failure.
        // Same out-pointer convention as algol26_map_get_int.
        // See runtime/string.c for the contract.
        //
        //   int32_t algol26_string_to_int(const char *s, int64_t *out)
        let s2i_ty = self
            .context
            .i32_type()
            .fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let s2i_fn = self
            .module
            .add_function("algol26_string_to_int", s2i_ty, None);
        self.functions
            .insert("algol26_string_to_int".to_string(), s2i_fn);

        // ── runtime/file.c ──
        // File.read(path) -> char*            (NULL on error; caller owns)
        // File.write(path, content) -> int64_t (byte count, -1 on error)
        // File.append(path, content) -> int64_t (byte count, -1 on error)
        //
        // The C returns an error signal (NULL / -1) and the codegen
        // arm emits the abort (printf + exit(1)) so
        // the error policy stays in the LLVM backend, matching how
        // compile_affirm handles assertion failures.
        let file_read_ty = i8_ptr.fn_type(&[i8_ptr.into()], false);
        let file_read_fn = self
            .module
            .add_function("algol26_file_read", file_read_ty, None);
        self.functions
            .insert("algol26_file_read".to_string(), file_read_fn);

        let i64_ty_loc = self.context.i64_type();
        let file_write_ty = i64_ty_loc.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let file_write_fn = self
            .module
            .add_function("algol26_file_write", file_write_ty, None);
        self.functions
            .insert("algol26_file_write".to_string(), file_write_fn);

        let file_append_ty = i64_ty_loc.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let file_append_fn = self
            .module
            .add_function("algol26_file_append", file_append_ty, None);
        self.functions
            .insert("algol26_file_append".to_string(), file_append_fn);

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

        // ── algol26_float_to_string_strip ──
        // Given a buffer containing `%.6f`-formatted text, strip
        // trailing '0' characters and a single trailing '.' if the
        // fractional part became empty. Mutates in place, returns
        // void. Used by `Float.to_string` after `sprintf`.
        let strip_ty = self.context.void_type().fn_type(&[i8_ptr.into()], false);
        let strip_fn = self
            .module
            .add_function("algol26_float_to_string_strip", strip_ty, None);
        self.functions
            .insert("algol26_float_to_string_strip".to_string(), strip_fn);
        {
            let saved = self.builder.get_insert_block();
            let entry = self.context.append_basic_block(strip_fn, "entry");
            let loop_head = self.context.append_basic_block(strip_fn, "loop_head");
            let loop_check = self.context.append_basic_block(strip_fn, "loop_check");
            let strip_body = self.context.append_basic_block(strip_fn, "strip_body");
            let check_dot = self.context.append_basic_block(strip_fn, "check_dot");
            let strip_dot = self.context.append_basic_block(strip_fn, "strip_dot");
            let done = self.context.append_basic_block(strip_fn, "done");

            self.builder.position_at_end(entry);
            let buf = strip_fn.get_nth_param(0).unwrap().into_pointer_value();
            let i64_ty = self.context.i64_type();
            let i8_ty = self.context.i8_type();

            let strlen_ty = i64_ty.fn_type(&[i8_ptr.into()], false);
            let strlen_fn = self
                .module
                .get_function("strlen")
                .unwrap_or_else(|| self.module.add_function("strlen", strlen_ty, None));
            let len_call = self
                .builder
                .build_call(strlen_fn, &[buf.into()], "len")
                .unwrap();
            let len = match len_call.try_as_basic_value() {
                inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                inkwell::values::ValueKind::Instruction(_) => {
                    unreachable!("float_to_string_strip: strlen returned no value")
                }
            };
            self.builder.build_unconditional_branch(loop_head).unwrap();

            self.builder.position_at_end(loop_head);
            let cur_len_phi = self.builder.build_phi(i64_ty, "cur_len").unwrap();
            cur_len_phi.add_incoming(&[(&len, entry)]);
            let cur_len = cur_len_phi.as_basic_value().into_int_value();
            let is_zero = self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::EQ,
                    cur_len,
                    i64_ty.const_int(0, false),
                    "is_zero",
                )
                .unwrap();
            self.builder
                .build_conditional_branch(is_zero, done, loop_check)
                .unwrap();

            self.builder.position_at_end(loop_check);
            let last_idx = self
                .builder
                .build_int_sub(cur_len, i64_ty.const_int(1, false), "last_idx")
                .unwrap();
            let last_ptr = unsafe {
                self.builder
                    .build_gep(i8_ty, buf, &[last_idx], "last_ptr")
                    .unwrap()
            };
            let last_char = self
                .builder
                .build_load(i8_ty, last_ptr, "last_char")
                .unwrap()
                .into_int_value();
            let is_zero_char = self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::EQ,
                    last_char,
                    i8_ty.const_int(b'0' as u64, false),
                    "is_zero_char",
                )
                .unwrap();
            self.builder
                .build_conditional_branch(is_zero_char, strip_body, check_dot)
                .unwrap();

            self.builder.position_at_end(strip_body);
            self.builder
                .build_store(last_ptr, i8_ty.const_int(0, false))
                .unwrap();
            let next_len = self
                .builder
                .build_int_sub(cur_len, i64_ty.const_int(1, false), "next_len")
                .unwrap();
            cur_len_phi.add_incoming(&[(&next_len, strip_body)]);
            self.builder.build_unconditional_branch(loop_head).unwrap();

            self.builder.position_at_end(check_dot);
            let is_dot = self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::EQ,
                    last_char,
                    i8_ty.const_int(b'.' as u64, false),
                    "is_dot",
                )
                .unwrap();
            self.builder
                .build_conditional_branch(is_dot, strip_dot, done)
                .unwrap();

            self.builder.position_at_end(strip_dot);
            self.builder
                .build_store(last_ptr, i8_ty.const_int(0, false))
                .unwrap();
            self.builder.build_unconditional_branch(done).unwrap();

            self.builder.position_at_end(done);
            self.builder.build_return(None).unwrap();

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

        // ── algol26_string_substring ──
        // Codepoint-indexed slice: returns the `length` codepoints
        // starting at codepoint `start`. Matches the interpreter's
        // `chars().skip(start).take(length).collect()`.
        //
        // The body lives in runtime/string.c (see the comment there
        // for the codepoint-walker rationale). The previous inline
        // IR used strlen + memcpy, which is byte-indexed — it
        // returned the bytes of a codepoint rather than the
        // codepoint itself on non-ASCII input. Corpus was all ASCII
        // and missed the bug.
        let i64_loc = self.context.i64_type();
        let sub_ty = i8_ptr.fn_type(&[i8_ptr.into(), i64_loc.into(), i64_loc.into()], false);
        let sub_fn = self
            .module
            .add_function("algol26_string_substring", sub_ty, None);
        self.functions
            .insert("algol26_string_substring".to_string(), sub_fn);

        // ── algol26_string_char_at ──
        // Returns a 1-codepoint string for the i-th codepoint, or
        // NULL if i is out of range. Body lives in runtime/string.c;
        // codepoint-indexed like substring and strlen_utf8. The
        // codegen arm wraps NULL/non-NULL into Option<String>.
        let char_at_ty = i8_ptr.fn_type(&[i8_ptr.into(), i64_loc.into()], false);
        let char_at_fn = self
            .module
            .add_function("algol26_string_char_at", char_at_ty, None);
        self.functions
            .insert("algol26_string_char_at".to_string(), char_at_fn);

        // ── algol26_string_trim ──
        // Trims leading/trailing Unicode whitespace. Matches the
        // interpreter's `s.trim()`. Body in runtime/string.c;
        // codepoint walker shared with substring and char_at.
        let trim_ty = i8_ptr.fn_type(&[i8_ptr.into()], false);
        let trim_fn = self
            .module
            .add_function("algol26_string_trim", trim_ty, None);
        self.functions
            .insert("algol26_string_trim".to_string(), trim_fn);

        // ── Char bridge ──
        // algol26_char_to_string(u32) -> i8*
        // algol26_string_char_at_cp(s, i, *out) -> i32
        let c2s_ty = i8_ptr.fn_type(&[self.context.i32_type().into()], false);
        let c2s_fn = self
            .module
            .add_function("algol26_char_to_string", c2s_ty, None);
        self.functions
            .insert("algol26_char_to_string".to_string(), c2s_fn);

        let i64_loc2 = self.context.i64_type();
        let char_at_cp_ty = self
            .context
            .i32_type()
            .fn_type(&[i8_ptr.into(), i64_loc2.into(), i8_ptr.into()], false);
        let char_at_cp_fn =
            self.module
                .add_function("algol26_string_char_at_cp", char_at_cp_ty, None);
        self.functions
            .insert("algol26_string_char_at_cp".to_string(), char_at_cp_fn);

        // ── algol26_string_to_chars ──
        // Decode a UTF-8 string into a heap array of i32 codepoints.
        //   int32_t *algol26_string_to_chars(const char *s, int64_t *out_count)
        // Returns NULL for empty input (out_count = 0), else a
        // malloc'd array of *out_count codepoints. Body in
        // runtime/string.c; used by `String.to_chars` (List<Char>).
        // The compile_builtin_value arm wraps the array in the
        // {ptr, i64, i64} list descriptor.
        let to_chars_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into()], false);
        let to_chars_fn = self
            .module
            .add_function("algol26_string_to_chars", to_chars_ty, None);
        self.functions
            .insert("algol26_string_to_chars".to_string(), to_chars_fn);

        // ── algol26_string_split / algol26_string_join ──
        // split: (s, sep, *out_count) -> char**
        //   Caller owns each strdup'd element.
        // join: (items, count, sep) -> char*
        //   Caller owns the result.
        // Both match Rust's str::split / slice::join in the
        // interpreter.
        let split_ty = i8_ptr.fn_type(&[i8_ptr.into(), i8_ptr.into(), i8_ptr.into()], false);
        let split_fn = self
            .module
            .add_function("algol26_string_split", split_ty, None);
        self.functions
            .insert("algol26_string_split".to_string(), split_fn);

        let join_ty = i8_ptr.fn_type(&[i8_ptr.into(), i64_loc.into(), i8_ptr.into()], false);
        let join_fn = self
            .module
            .add_function("algol26_string_join", join_ty, None);
        self.functions
            .insert("algol26_string_join".to_string(), join_fn);

        // ── runtime/args.c ──
        // algol26_args_init(i32 argc, ptr argv) -> void
        //    Called once from the generated main wrapper.
        // algol26_args_count() -> i64
        //    Number of user arguments (argv[0] skipped).
        // algol26_args_get(i64 i) -> ptr
        //    The i-th user argument, or NULL if out of range.
        //
        // Declared unconditionally: the main wrapper calls
        // args_init for every program so argc/argv are stashed
        // in the C runtime before user main runs.
        let args_init_ty = self
            .context
            .void_type()
            .fn_type(&[self.context.i32_type().into(), i8_ptr.into()], false);
        let args_init_fn = self
            .module
            .add_function("algol26_args_init", args_init_ty, None);
        self.functions
            .insert("algol26_args_init".to_string(), args_init_fn);

        let args_count_ty = i64_loc.fn_type(&[], false);
        let args_count_fn = self
            .module
            .add_function("algol26_args_count", args_count_ty, None);
        self.functions
            .insert("algol26_args_count".to_string(), args_count_fn);

        let args_get_ty = i8_ptr.fn_type(&[i64_loc.into()], false);
        let args_get_fn = self
            .module
            .add_function("algol26_args_get", args_get_ty, None);
        self.functions
            .insert("algol26_args_get".to_string(), args_get_fn);

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
            "List.length" | "len" | "length" => {
                let arg = args.first().ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: List.length requires exactly one argument",
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    )
                })?;
                // ADR 0042 phase 2 + 4 follow-up. Two paths:
                //
                //   - Variable: read the descriptor's length field
                //     via `list_length_value`, which prefers the
                //     runtime source (correct after any `.append`)
                //     and falls back to the static hint.
                //
                //   - Anything else (a `FieldAccess` like
                //     `self.scores`, an `ArrayAccess`, a call
                //     result): compile the value — it produces the
                //     `{ptr, i64, i64}` descriptor struct — and
                //     extract field 1 directly. The previous
                //     "requires a variable argument" refusal
                //     surfaced on any record field holding a list.
                let len_val = match arg {
                    TypedIRValue::Variable(var_name, _) => {
                        self.list_length_value(var_name).ok_or_else(|| {
                            CompileError::simple(
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
                            )
                        })?
                    }
                    other => {
                        let val = self.compile_value(other)?;
                        if !val.is_struct_value() {
                            return Err(CompileError::simple(
                                &format!(
                                    "LLVM codegen: List.length argument lowered to \
                                     non-struct LLVM value (kind {:?}); expected the \
                                     list descriptor",
                                    val
                                ),
                                0,
                                0,
                                "",
                                ErrorCode::E0004,
                            ));
                        }
                        let sv = val.into_struct_value();
                        self.builder
                            .build_extract_value(sv, 1, "list_len")
                            .unwrap()
                            .into_int_value()
                    }
                };
                Ok((len_val.into(), Type::Int))
            }
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
            // `String.compare` calls C `strcmp` and normalizes the
            // result to -1/0/1. The magnitude of `strcmp`'s return
            // is implementation-defined; the sign is not. The
            // interpreter produces -1/0/1 (via Rust's `str::cmp`
            // and an `Ordering` match), so the LLVM arm normalizes
            // to match.
            "String.compare" => {
                if args.len() != 2 {
                    return Err(CompileError::simple(
                        "LLVM codegen: String.compare requires exactly 2 arguments",
                        0,
                        0,
                        "",
                        ErrorCode::E0004,
                    ));
                }
                let a_val = self.compile_value(&args[0])?;
                let b_val = self.compile_value(&args[1])?;
                if !a_val.is_pointer_value() || !b_val.is_pointer_value() {
                    return Err(CompileError::simple(
                        &format!(
                            "LLVM codegen: String.compare expected two pointer arguments, got {:?} and {:?}",
                            a_val, b_val
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let strcmp_fn = self.module.get_function("strcmp").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: strcmp not registered in stdlib",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let call = self
                    .builder
                    .build_call(strcmp_fn, &[a_val.into(), b_val.into()], "strcmp_call")
                    .unwrap();
                let raw = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                    inkwell::values::ValueKind::Instruction(_) => {
                        return Err(CompileError::unsupported_operation(
                            "String.compare (strcmp call produced no value)",
                            "llvm",
                        ));
                    }
                };
                // Normalize: (raw < 0) ? -1 : (raw > 0) ? 1 : 0
                let i32_ty = self.context.i32_type();
                let neg = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::SLT,
                        raw,
                        i32_ty.const_int(0, false),
                        "strcmp_neg",
                    )
                    .unwrap();
                let pos = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::SGT,
                        raw,
                        i32_ty.const_int(0, false),
                        "strcmp_pos",
                    )
                    .unwrap();
                let step = self
                    .builder
                    .build_select(
                        pos,
                        i32_ty.const_int(1, false),
                        i32_ty.const_int(0, false),
                        "strcmp_step",
                    )
                    .unwrap()
                    .into_int_value();
                let normalized_i32 = self
                    .builder
                    .build_select(
                        neg,
                        i32_ty.const_int(-1i64 as u64, true),
                        step,
                        "strcmp_normalized",
                    )
                    .unwrap()
                    .into_int_value();
                // ALGOL26's `Int` is i64 in LLVM; `strcmp` is i32.
                // Sign-extend so the caller reads a proper i64 -1,
                // not a 32-bit -1 whose bit pattern reads as
                // 4294967295 when reinterpreted as i64.
                let normalized_i64 = self
                    .builder
                    .build_int_s_extend(normalized_i32, self.context.i64_type(), "strcmp_i64")
                    .unwrap();
                Ok((normalized_i64.into(), Type::Int))
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
            "args" => {
                // List<String>: one entry per user argument, argv[0]
                // skipped. runtime/args.c holds the (argc, argv) the
                // main wrapper stashed via algol26_args_init.
                //
                // First runtime-length list construction in codegen:
                // the count comes from algol26_args_count(), the
                // buffer is malloc(count * sizeof(i8*)), and a loop
                // fills it with borrowed pointers from
                // algol26_args_get(i). The descriptor uses
                // { ptr, i64, i64 } = { buf, count, count } — cap
                // == len so a subsequent List.append can write in
                // place without growing.
                //
                // The interpreter rejects `args(x)` with "takes no
                // arguments"; match that arity check here.
                if !args.is_empty() {
                    return Err(CompileError::simple(
                        &format!("args: takes no arguments, got {}", args.len()),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }

                let count_fn = self
                    .module
                    .get_function("algol26_args_count")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/args.c not linked (algol26_args_count missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let get_fn = self
                    .module
                    .get_function("algol26_args_get")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/args.c not linked (algol26_args_get missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let malloc_fn = self.module.get_function("malloc").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: malloc not registered in stdlib",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;

                let i64_ty = self.context.i64_type();
                let ptr_ty = self.context.ptr_type(AddressSpace::default());

                // count = algol26_args_count()
                let count_call = self
                    .builder
                    .build_call(count_fn, &[], "args_count")
                    .unwrap();
                let count = match count_call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_args_count returned void",
                            "llvm",
                        ));
                    }
                };

                // buf = malloc(count * 8)
                let slot_bytes = self
                    .builder
                    .build_int_mul(count, i64_ty.const_int(8, false), "args_bytes")
                    .unwrap();
                let buf_call = self
                    .builder
                    .build_call(malloc_fn, &[slot_bytes.into()], "args_buf")
                    .unwrap();
                let buf = match buf_call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "malloc returned no value",
                            "llvm",
                        ));
                    }
                };

                // Loop: for i in [0, count) { buf[i] = algol26_args_get(i) }
                let current_fn = self.current_function.unwrap();
                let idx_alloca = self.builder.build_alloca(i64_ty, "args_i").unwrap();
                self.builder
                    .build_store(idx_alloca, i64_ty.const_zero())
                    .unwrap();

                let loop_bb = self.context.append_basic_block(current_fn, "args_loop");
                let body_bb = self.context.append_basic_block(current_fn, "args_body");
                let done_bb = self.context.append_basic_block(current_fn, "args_done");
                self.builder.build_unconditional_branch(loop_bb).unwrap();

                self.builder.position_at_end(loop_bb);
                let i = self
                    .builder
                    .build_load(i64_ty, idx_alloca, "args_i_val")
                    .unwrap()
                    .into_int_value();
                let in_range = self
                    .builder
                    .build_int_compare(inkwell::IntPredicate::SLT, i, count, "args_in_range")
                    .unwrap();
                self.builder
                    .build_conditional_branch(in_range, body_bb, done_bb)
                    .unwrap();

                self.builder.position_at_end(body_bb);
                let s_call = self
                    .builder
                    .build_call(get_fn, &[i.into()], "args_get")
                    .unwrap();
                let s_ptr = match s_call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_args_get returned void",
                            "llvm",
                        ));
                    }
                };
                let slot = unsafe {
                    self.builder
                        .build_gep(ptr_ty, buf, &[i], "args_slot")
                        .unwrap()
                };
                self.builder.build_store(slot, s_ptr).unwrap();
                let next_i = self
                    .builder
                    .build_int_add(i, i64_ty.const_int(1, false), "args_i_next")
                    .unwrap();
                self.builder.build_store(idx_alloca, next_i).unwrap();
                self.builder.build_unconditional_branch(loop_bb).unwrap();

                self.builder.position_at_end(done_bb);

                // Build the { ptr, i64, i64 } descriptor.
                let struct_ty = self
                    .context
                    .struct_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
                let mut sv = struct_ty.get_undef();
                sv = self
                    .builder
                    .build_insert_value(sv, buf, 0, "args_buf_field")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 1, "args_len_field")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 2, "args_cap_field")
                    .unwrap()
                    .into_struct_value();
                Ok((sv.into(), Type::list(Type::String)))
            }
            "String.charAt" => {
                // Option<String>: { i1 has_value, i8* char }.
                // runtime/string.c's algol26_string_char_at returns
                // NULL for out-of-range (negative or >= codepoint
                // count) — NOT the clamped "" that substring gives
                // — so is_null is the correct tag source.
                if args.len() != 2 {
                    return Err(CompileError::simple(
                        &format!(
                            "String.charAt expects exactly 2 arguments (s, i), got {}",
                            args.len()
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s_val = self.compile_value(&args[0])?;
                let i_val = self.compile_value(&args[1])?;
                if !s_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.charAt: first argument did not lower to a pointer",
                        "llvm",
                    ));
                }
                if !i_val.is_int_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.charAt: second argument did not lower to an integer",
                        "llvm",
                    ));
                }
                let char_at_fn = self
                    .module
                    .get_function("algol26_string_char_at")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c not linked (algol26_string_char_at missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(char_at_fn, &[s_val.into(), i_val.into()], "char_at")
                    .unwrap();
                let ptr = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_string_char_at returned void",
                            "llvm",
                        ));
                    }
                };
                // The Option tag convention is { i1 is_some, T payload }
                // — 1 means Some. `charAt` returns a non-NULL pointer
                // for valid indices, so the tag is "is_not_null", NOT
                // "is_null". Using is_null here inverted the match,
                // sending valid indices to the None arm.
                let is_some = self.builder.build_is_not_null(ptr, "char_at_some").unwrap();
                let opt_ty = match self.map_type(&Type::option(Type::String)) {
                    inkwell::types::BasicTypeEnum::StructType(s) => s,
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "Option<String> did not map to a struct",
                            "llvm",
                        ));
                    }
                };
                let undef = opt_ty.get_undef();
                let with_tag = self
                    .builder
                    .build_insert_value(undef, is_some, 0, "char_at_tag")
                    .unwrap()
                    .into_struct_value();
                let with_payload = self
                    .builder
                    .build_insert_value(with_tag, ptr, 1, "char_at_payload")
                    .unwrap()
                    .into_struct_value();
                Ok((with_payload.into(), Type::option(Type::String)))
            }
            "String.split" => {
                // Returns List<String> ({ptr, i64, i64}). The C
                // runtime fills a char** array (each element
                // strdup'd) and writes the count through the
                // out-pointer. Codegen wraps those in the list
                // descriptor — same struct shape as args()'s arm,
                // but the array is pre-filled by C, no loop
                // needed.
                if args.len() != 2 {
                    return Err(CompileError::simple(
                        &format!(
                            "String.split expects exactly 2 arguments (s, sep), got {}",
                            args.len()
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s_val = self.compile_value(&args[0])?;
                let sep_val = self.compile_value(&args[1])?;
                if !s_val.is_pointer_value() || !sep_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.split: arguments did not lower to pointers",
                        "llvm",
                    ));
                }
                let split_fn = self
                    .module
                    .get_function("algol26_string_split")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c not linked (algol26_string_split missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let i64_ty = self.context.i64_type();
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let count_alloca = self.builder.build_alloca(i64_ty, "split_count").unwrap();
                let call = self
                    .builder
                    .build_call(
                        split_fn,
                        &[s_val.into(), sep_val.into(), count_alloca.into()],
                        "split_call",
                    )
                    .unwrap();
                let arr = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_string_split returned void",
                            "llvm",
                        ));
                    }
                };
                let count = self
                    .builder
                    .build_load(i64_ty, count_alloca, "split_count_val")
                    .unwrap()
                    .into_int_value();
                // Build the {ptr, i64, i64} descriptor.
                let struct_ty = self
                    .context
                    .struct_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
                let mut sv = struct_ty.get_undef();
                sv = self
                    .builder
                    .build_insert_value(sv, arr, 0, "split_buf")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 1, "split_len")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 2, "split_cap")
                    .unwrap()
                    .into_struct_value();
                Ok((sv.into(), Type::list(Type::String)))
            }
            "String.join" => {
                // Consumes a List<String> (compiled to the
                // {ptr, i64, i64} descriptor), returns a String.
                // C side (algol26_string_join) iterates the buffer
                // and concatenates with the separator between
                // elements. Matches Rust's slice::join.
                if args.len() != 2 {
                    return Err(CompileError::simple(
                        &format!(
                            "String.join expects exactly 2 arguments (list, sep), got {}",
                            args.len()
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let list_val = self.compile_value(&args[0])?;
                if !list_val.is_struct_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.join: first argument did not lower to a list descriptor",
                        "llvm",
                    ));
                }
                let list_struct = list_val.into_struct_value();
                let buf = self
                    .builder
                    .build_extract_value(list_struct, 0, "join_buf")
                    .unwrap();
                let len = self
                    .builder
                    .build_extract_value(list_struct, 1, "join_len")
                    .unwrap();
                let sep_val = self.compile_value(&args[1])?;
                if !sep_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.join: separator did not lower to a pointer",
                        "llvm",
                    ));
                }
                let join_fn = self
                    .module
                    .get_function("algol26_string_join")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c not linked (algol26_string_join missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(join_fn, &[buf.into(), len.into(), sep_val.into()], "join")
                    .unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::String)),
                    _ => Err(CompileError::unsupported_operation(
                        "algol26_string_join returned void",
                        "llvm",
                    )),
                }
            }
            "String.char_at" => {
                if args.len() != 2 {
                    return Err(CompileError::simple(
                        &format!(
                            "String.char_at expects exactly 2 arguments, got {}",
                            args.len()
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s_val = self.compile_value(&args[0])?;
                let i_val = self.compile_value(&args[1])?;
                if !s_val.is_pointer_value() || !i_val.is_int_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.char_at arguments must be (String, Int)",
                        "llvm",
                    ));
                }
                let char_at_fn = self
                    .module
                    .get_function("algol26_string_char_at_cp")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c not linked (algol26_string_char_at_cp missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let i32_ty = self.context.i32_type();
                let out_slot = self.builder.build_alloca(i32_ty, "char_at_cp_out").unwrap();
                let call = self
                    .builder
                    .build_call(
                        char_at_fn,
                        &[s_val.into(), i_val.into(), out_slot.into()],
                        "char_at_cp",
                    )
                    .unwrap();
                let found_i32 = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_string_char_at_cp returned void",
                            "llvm",
                        ))
                    }
                };
                let found_bool = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        found_i32,
                        i32_ty.const_zero(),
                        "char_at_found",
                    )
                    .unwrap();
                let opt_ty = match self.map_type(&Type::option(Type::Char)) {
                    inkwell::types::BasicTypeEnum::StructType(st) => st,
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "Option<Char> did not map to a struct",
                            "llvm",
                        ))
                    }
                };
                let loaded = self
                    .builder
                    .build_load(i32_ty, out_slot, "char_at_val")
                    .unwrap();
                let undef = opt_ty.get_undef();
                let with_tag = self
                    .builder
                    .build_insert_value(undef, found_bool, 0, "char_at_tag")
                    .unwrap()
                    .into_struct_value();
                let with_payload = self
                    .builder
                    .build_insert_value(with_tag, loaded, 1, "char_at_payload")
                    .unwrap()
                    .into_struct_value();
                Ok((with_payload.into(), Type::option(Type::Char)))
            }
            "Char.to_string" => {
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        &format!(
                            "Char.to_string expects exactly 1 argument, got {}",
                            args.len()
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let c_val = self.compile_value(&args[0])?;
                if !c_val.is_int_value() {
                    return Err(CompileError::unsupported_operation(
                        "Char.to_string argument must be a Char",
                        "llvm",
                    ));
                }
                let c2s_fn = self
                    .module
                    .get_function("algol26_char_to_string")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c not linked (algol26_char_to_string missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(c2s_fn, &[c_val.into()], "char_to_string")
                    .unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::String)),
                    _ => Err(CompileError::unsupported_operation(
                        "algol26_char_to_string returned void",
                        "llvm",
                    )),
                }
            }
            "Char.to_int" => {
                // Zero-extend i32 codepoint to i64 Int. No C
                // runtime function — the cast is entirely in IR.
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        &format!("Char.to_int expects exactly 1 argument, got {}", args.len()),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let c_val = self.compile_value(&args[0])?;
                if !c_val.is_int_value() {
                    return Err(CompileError::unsupported_operation(
                        "Char.to_int argument must be a Char",
                        "llvm",
                    ));
                }
                let c_int = c_val.into_int_value();
                let widened = self
                    .builder
                    .build_int_z_extend(c_int, self.context.i64_type(), "char_to_int")
                    .unwrap();
                Ok((widened.into(), Type::Int))
            }
            "String.to_chars" => {
                // Returns List<Char> ({ptr, i64, i64}), with i32
                // elements. The C function allocates the array;
                // codegen wraps it in the descriptor. Mirrors
                // Map.keys exactly, except the element type is
                // i32 (Char) and the symbol is fixed.
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        &format!(
                            "String.to_chars expects exactly 1 argument, got {}",
                            args.len()
                        ),
                        0, 0, "", ErrorCode::E0002,
                    ));
                }
                let s_val = self.compile_value(&args[0])?;
                if !s_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.to_chars argument did not lower to a pointer", "llvm",
                    ));
                }
                let to_chars_fn = self
                    .module
                    .get_function("algol26_string_to_chars")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c not linked (algol26_string_to_chars missing)",
                            0, 0, "", ErrorCode::E0009,
                        )
                    })?;
                let i64_ty = self.context.i64_type();
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let count_alloca = self.builder.build_alloca(i64_ty, "to_chars_count").unwrap();
                let call = self
                    .builder
                    .build_call(to_chars_fn, &[s_val.into(), count_alloca.into()], "to_chars")
                    .unwrap();
                let arr = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_string_to_chars returned void", "llvm",
                        ));
                    }
                };
                let count = self
                    .builder
                    .build_load(i64_ty, count_alloca, "to_chars_count_val")
                    .unwrap()
                    .into_int_value();
                let struct_ty = self.context.struct_type(
                    &[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false,
                );
                let mut sv = struct_ty.get_undef();
                sv = self
                    .builder
                    .build_insert_value(sv, arr, 0, "to_chars_buf")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 1, "to_chars_len")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 2, "to_chars_cap")
                    .unwrap()
                    .into_struct_value();
                Ok((sv.into(), Type::list(Type::Char)))
            }
            "String.trim" => {
                // Trim leading/trailing Unicode whitespace. Matches
                // the interpreter's `str::trim()`. Body in
                // runtime/string.c; codepoint-walker shared with
                // substring/char_at.
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        &format!("String.trim expects exactly 1 argument, got {}", args.len()),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s_val = self.compile_value(&args[0])?;
                if !s_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.trim argument did not lower to a pointer",
                        "llvm",
                    ));
                }
                let trim_fn = self
                    .module
                    .get_function("algol26_string_trim")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c not linked (algol26_string_trim missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(trim_fn, &[s_val.into()], "trim")
                    .unwrap();
                match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => Ok((v, Type::String)),
                    _ => Err(CompileError::unsupported_operation(
                        "algol26_string_trim returned void",
                        "llvm",
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
            // ─── File.* (runtime/file.c) ───
            // The C layer returns NULL (read) or -1 (write/append)
            // on error; the arm branches to a block that prints a
            // message and exits(1), matching the interpreter's
            // Err(EvalError::Runtime(...)) abort-on-error discipline.
            // Same shape as compile_affirm's failure path.
            "File.read" => {
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        "File.read expects exactly 1 argument (path)",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let path_val = self.compile_value(&args[0])?;
                if !path_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        "File.read path did not lower to a pointer",
                        "llvm",
                    ));
                }
                let read_fn = self
                    .module
                    .get_function("algol26_file_read")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/file.c not linked (algol26_file_read missing)",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let call = self
                    .builder
                    .build_call(read_fn, &[path_val.into()], "file_read")
                    .unwrap();
                let buf = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_file_read returned void",
                            "llvm",
                        ))
                    }
                };
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let is_null = self.builder.build_is_null(buf, "file_read_null").unwrap();
                let current_fn = self.current_function.unwrap();
                let err_bb = self.context.append_basic_block(current_fn, "file_read_err");
                let ok_bb = self.context.append_basic_block(current_fn, "file_read_ok");
                self.builder
                    .build_conditional_branch(is_null, err_bb, ok_bb)
                    .unwrap();

                self.builder.position_at_end(err_bb);
                let msg = self
                    .builder
                    .build_global_string_ptr("File.read: cannot read file\n", "file_read_errmsg")
                    .unwrap();
                let printf_fn = self.module.get_function("printf").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: printf not registered",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                self.builder
                    .build_call(printf_fn, &[msg.as_pointer_value().into()], "")
                    .unwrap();
                let exit_fn = self.module.get_function("exit").unwrap();
                self.builder
                    .build_call(
                        exit_fn,
                        &[self.context.i32_type().const_int(1, false).into()],
                        "file_read_exit",
                    )
                    .unwrap();
                self.builder.build_unreachable().unwrap();

                self.builder.position_at_end(ok_bb);
                Ok((buf.into(), Type::String))
            }
            "File.write" | "File.append" => {
                let is_write = name == "File.write";
                let opname = if is_write {
                    "File.write"
                } else {
                    "File.append"
                };
                if args.len() != 2 {
                    return Err(CompileError::simple(
                        &format!("{} expects exactly 2 arguments (path, content)", opname),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let path_val = self.compile_value(&args[0])?;
                let content_val = self.compile_value(&args[1])?;
                if !path_val.is_pointer_value() || !content_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        &format!("{} arguments did not lower to pointers", opname),
                        "llvm",
                    ));
                }
                let llvm_fn_name = if is_write {
                    "algol26_file_write"
                } else {
                    "algol26_file_append"
                };
                let write_fn = self.module.get_function(llvm_fn_name).ok_or_else(|| {
                    CompileError::simple(
                        &format!("runtime/file.c not linked ({} missing)", llvm_fn_name),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let call = self
                    .builder
                    .build_call(write_fn, &[path_val.into(), content_val.into()], "file_wr")
                    .unwrap();
                let result = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("{} returned void", llvm_fn_name),
                            "llvm",
                        ))
                    }
                };
                let negative = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::SLT,
                        result,
                        self.context.i64_type().const_int(0, false),
                        "file_wr_err",
                    )
                    .unwrap();
                let current_fn = self.current_function.unwrap();
                let err_bb = self
                    .context
                    .append_basic_block(current_fn, "file_wr_err_bb");
                let ok_bb = self.context.append_basic_block(current_fn, "file_wr_ok_bb");
                self.builder
                    .build_conditional_branch(negative, err_bb, ok_bb)
                    .unwrap();

                self.builder.position_at_end(err_bb);
                let err_label = if is_write {
                    "File.write: cannot write file\n"
                } else {
                    "File.append: cannot append to file\n"
                };
                let msg = self
                    .builder
                    .build_global_string_ptr(err_label, "file_wr_errmsg")
                    .unwrap();
                let printf_fn = self.module.get_function("printf").ok_or_else(|| {
                    CompileError::simple(
                        "LLVM codegen: printf not registered",
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                self.builder
                    .build_call(printf_fn, &[msg.as_pointer_value().into()], "")
                    .unwrap();
                let exit_fn = self.module.get_function("exit").unwrap();
                self.builder
                    .build_call(
                        exit_fn,
                        &[self.context.i32_type().const_int(1, false).into()],
                        "file_wr_exit",
                    )
                    .unwrap();
                self.builder.build_unreachable().unwrap();

                self.builder.position_at_end(ok_bb);
                Ok((result.into(), Type::Int))
            }
            "String.to_int" => {
                // Strict i64 parser. The C runtime (runtime/string.c)
                // returns 1 on success with the value written through
                // an out-pointer, 0 on parse failure. The 0/1 becomes
                // an i1 tag, packed with the i64 payload into the
                // { i1, i64 } Option<Int> struct — same shape as the
                // Map.get lowering above. The interpreter's
                // `str::parse::<i64>()` is the contract: no
                // whitespace, no trailing garbage, no overflow.
                if args.len() != 1 {
                    return Err(CompileError::simple(
                        &format!(
                            "String.to_int expects exactly 1 argument, got {}",
                            args.len()
                        ),
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let s_val = self.compile_value(&args[0])?;
                if !s_val.is_pointer_value() {
                    return Err(CompileError::unsupported_operation(
                        "String.to_int argument did not lower to a pointer",
                        "llvm",
                    ));
                }
                let to_int_fn = self
                    .module
                    .get_function("algol26_string_to_int")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "runtime/string.c is not linked — \
                             algol26_string_to_int missing",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                let out_slot = self.create_entry_alloca("__s2i_out", &Type::Int);
                let call = self
                    .builder
                    .build_call(to_int_fn, &[s_val.into(), out_slot.into()], "")
                    .unwrap();
                let ok_i32 = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "algol26_string_to_int returned void",
                            "llvm",
                        ));
                    }
                };
                let ok_bool = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        ok_i32,
                        self.context.i32_type().const_zero(),
                        "s2i_ok",
                    )
                    .unwrap();
                let opt_ty = match self.map_type(&Type::option(Type::Int)) {
                    inkwell::types::BasicTypeEnum::StructType(s) => s,
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            "Option<Int> did not map to a struct",
                            "llvm",
                        ));
                    }
                };
                let loaded = self
                    .builder
                    .build_load(self.context.i64_type(), out_slot, "s2i_val")
                    .unwrap();
                let undef = opt_ty.get_undef();
                let with_tag = self
                    .builder
                    .build_insert_value(undef, ok_bool, 0, "s2i_tag")
                    .unwrap()
                    .into_struct_value();
                let with_payload = self
                    .builder
                    .build_insert_value(with_tag, loaded, 1, "s2i_payload")
                    .unwrap()
                    .into_struct_value();
                Ok((with_payload.into(), Type::option(Type::Int)))
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
            "Float.to_string" => {
                let arg = args.first().ok_or_else(|| {
                    CompileError::simple(
                        "Float.to_string requires 1 argument",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    )
                })?;
                let val = self.compile_value(arg)?;
                if !val.is_float_value() {
                    return Err(CompileError::simple(
                        "Float.to_string expects a Float",
                        0,
                        0,
                        "",
                        ErrorCode::E0002,
                    ));
                }
                let f = val.into_float_value();

                let i8_ty = self.context.i8_type();
                let buf_ty = i8_ty.array_type(32);
                let buf = self.builder.build_alloca(buf_ty, "float_str_buf").unwrap();

                let i8_ptr = self.context.ptr_type(AddressSpace::default());
                let i32_ty = self.context.i32_type();
                let sprintf_ty = i32_ty.fn_type(&[i8_ptr.into(), i8_ptr.into()], true);
                let sprintf_fn = match self.module.get_function("sprintf") {
                    Some(f) => f,
                    None => self.module.add_function("sprintf", sprintf_ty, None),
                };

                let fmt = self
                    .builder
                    .build_global_string_ptr("%.6f", "fmt_f64")
                    .unwrap();

                self.builder
                    .build_call(
                        sprintf_fn,
                        &[buf.into(), fmt.as_pointer_value().into(), f.into()],
                        "float_to_string_sprintf",
                    )
                    .unwrap();

                // Strip trailing zeros and a trailing dot.
                let strip_fn = self
                    .module
                    .get_function("algol26_float_to_string_strip")
                    .ok_or_else(|| {
                        CompileError::simple(
                            "LLVM codegen: algol26_float_to_string_strip not \
                             registered in stdlib",
                            0,
                            0,
                            "",
                            ErrorCode::E0009,
                        )
                    })?;
                self.builder
                    .build_call(strip_fn, &[buf.into()], "")
                    .unwrap();

                Ok((buf.into(), Type::String))
            }
            // ─── ADR 0027: Map<K, V> builtins (LLVM, v1) ───
            // v1 supports Map<Int, Int> only. Feature::Map is
            // currently in BackendCapabilities::llvm().supported
            // (temporarily, while we validate the lowering) — the
            // capability check will refuse maps with other key/value
            // types once we tighten it.
            "Map.length" => {
                // v1 supports (Int, Int) and (String, Int). The
                // suffix drives the runtime symbol; the call shape
                // is identical for both (map_ptr -> i64).
                let map_ptr = self.compile_value(&args[0])?;
                let recv_ty = args[0].type_of();
                let (key_ty, value_ty) = match &recv_ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("Map.length receiver is not a Map: {}", recv_ty),
                            "llvm",
                        ));
                    }
                };
                let suffix = Self::map_suffix(&key_ty, &value_ty).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "Map.length on LLVM requires Map<Int, Int> or \
                             Map<String, Int> in v1, found {}",
                            recv_ty
                        ),
                        "llvm",
                    )
                })?;
                let symbol = format!("algol26_map_length_{}", suffix);
                let len_fn = self.module.get_function(&symbol).ok_or_else(|| {
                    CompileError::simple(
                        &format!("runtime for {} not linked ({} missing)", recv_ty, symbol),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let call = self
                    .builder
                    .build_call(len_fn, &[map_ptr.into()], "map_len")
                    .unwrap();
                let v = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v,
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("{} returned void", symbol),
                            "llvm",
                        ));
                    }
                };
                Ok((v, Type::Int))
            }
            "Map.contains" => {
                // v1 supports (Int, Int) and (String, Int). The key
                // value's LLVM type follows from compile_value (i64
                // for Int, ptr for String); the runtime function's
                // declared signature matches the suffix, so no
                // branching on the key type is needed here.
                let map_ptr = self.compile_value(&args[0])?;
                let recv_ty = args[0].type_of();
                let (key_ty, value_ty) = match &recv_ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("Map.contains receiver is not a Map: {}", recv_ty),
                            "llvm",
                        ));
                    }
                };
                let suffix = Self::map_suffix(&key_ty, &value_ty).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "Map.contains on LLVM requires Map<Int, Int> or \
                             Map<String, Int> in v1, found {}",
                            recv_ty
                        ),
                        "llvm",
                    )
                })?;
                let key = self.compile_value(&args[1])?;
                let symbol = format!("algol26_map_contains_{}", suffix);
                let contains_fn = self.module.get_function(&symbol).ok_or_else(|| {
                    CompileError::simple(
                        &format!("runtime for {} not linked ({} missing)", recv_ty, symbol),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let call = self
                    .builder
                    .build_call(contains_fn, &[map_ptr.into(), key.into()], "map_has")
                    .unwrap();
                let v_i32 = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("{} returned void", symbol),
                            "llvm",
                        ));
                    }
                };
                let v_bool = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        v_i32,
                        self.context.i32_type().const_zero(),
                        "map_has_bool",
                    )
                    .unwrap();
                Ok((v_bool.into(), Type::Bool))
            }
            "Map.get" => {
                // v1 supports (Int, Int) and (String, Int). Key's
                // LLVM type comes from compile_value. Out-pointer
                // and Option<Int> construction are the same shape
                // for both suffixes.
                let map_ptr = self.compile_value(&args[0])?;
                let recv_ty = args[0].type_of();
                let (key_ty, value_ty) = match &recv_ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("Map.get receiver is not a Map: {}", recv_ty),
                            "llvm",
                        ));
                    }
                };
                let suffix = Self::map_suffix(&key_ty, &value_ty).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "Map.get on LLVM requires Map<Int, Int> or \
                             Map<String, Int> in v1, found {}",
                            recv_ty
                        ),
                        "llvm",
                    )
                })?;
                let key = self.compile_value(&args[1])?;
                let symbol = format!("algol26_map_get_{}", suffix);
                let get_fn = self.module.get_function(&symbol).ok_or_else(|| {
                    CompileError::simple(
                        &format!("runtime for {} not linked ({} missing)", recv_ty, symbol),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                // Out-pointer type follows the value type:
                // i64 for Int values, ptr (char**) for String.
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let out_slot = match &value_ty {
                    Type::Int => self
                        .builder
                        .build_alloca(self.context.i64_type(), "map_get_out")
                        .unwrap(),
                    Type::String => self.builder.build_alloca(ptr_ty, "map_get_out").unwrap(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!(
                                "Map.get on LLVM: value type {} not supported \
                                 (Int and String are)",
                                value_ty
                            ),
                            "llvm",
                        ));
                    }
                };
                let call = self
                    .builder
                    .build_call(get_fn, &[map_ptr.into(), key.into(), out_slot.into()], "")
                    .unwrap();
                let found_i32 = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("{} returned void", symbol),
                            "llvm",
                        ));
                    }
                };
                let found_bool = self
                    .builder
                    .build_int_compare(
                        inkwell::IntPredicate::NE,
                        found_i32,
                        self.context.i32_type().const_zero(),
                        "map_get_found",
                    )
                    .unwrap();
                let opt_ty = match self.map_type(&Type::option(value_ty.clone())) {
                    inkwell::types::BasicTypeEnum::StructType(s) => s,
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("Option<{}> did not map to a struct", value_ty),
                            "llvm",
                        ));
                    }
                };
                let loaded = match &value_ty {
                    Type::Int => self
                        .builder
                        .build_load(self.context.i64_type(), out_slot, "map_get_val")
                        .unwrap(),
                    Type::String => self
                        .builder
                        .build_load(ptr_ty, out_slot, "map_get_val")
                        .unwrap(),
                    _ => unreachable!("checked above"),
                };
                let undef = opt_ty.get_undef();
                let with_tag = self
                    .builder
                    .build_insert_value(undef, found_bool, 0, "opt_tag")
                    .unwrap()
                    .into_struct_value();
                let with_payload = self
                    .builder
                    .build_insert_value(with_tag, loaded, 1, "opt_payload")
                    .unwrap()
                    .into_struct_value();
                Ok((with_payload.into(), Type::option(value_ty.clone())))
            }
            "Map.insert" => {
                // Insert is void-returning but can appear in
                // expression position. v1 supports (Int, Int) and
                // (String, Int); key and value LLVM types follow
                // from compile_value, matching the runtime's
                // declared signature.
                let map_ptr = self.compile_value(&args[0])?;
                let recv_ty = args[0].type_of();
                let (key_ty, value_ty) = match &recv_ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("Map.insert receiver is not a Map: {}", recv_ty),
                            "llvm",
                        ));
                    }
                };
                let suffix = Self::map_suffix(&key_ty, &value_ty).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "Map.insert on LLVM requires Map<Int, Int> or \
                             Map<String, Int> in v1, found {}",
                            recv_ty
                        ),
                        "llvm",
                    )
                })?;
                let key = self.compile_value(&args[1])?;
                let val = self.compile_value(&args[2])?;
                let symbol = format!("algol26_map_insert_{}", suffix);
                let insert_fn = self.module.get_function(&symbol).ok_or_else(|| {
                    CompileError::simple(
                        &format!("runtime for {} not linked ({} missing)", recv_ty, symbol),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                self.builder
                    .build_call(insert_fn, &[map_ptr.into(), key.into(), val.into()], "")
                    .unwrap();
                Ok((self.context.i64_type().const_zero().into(), Type::Void))
            }
            "Map.keys" => {
                // Returns List<K>, sorted ascending by key. Matches
                // the interpreter's `keys().sort()`: integer
                // ascending for Int keys, byte-lex for String keys
                // (strcmp ≡ Rust's str::cmp for well-formed UTF-8).
                let map_ptr = self.compile_value(&args[0])?;
                let recv_ty = args[0].type_of();
                let (key_ty, value_ty) = match &recv_ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("Map.keys receiver is not a Map: {}", recv_ty),
                            "llvm",
                        ));
                    }
                };
                let suffix = Self::map_suffix(&key_ty, &value_ty).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "Map.keys on LLVM requires one of the three \
                             supported map shapes; found {}",
                            recv_ty
                        ),
                        "llvm",
                    )
                })?;
                let symbol = format!("algol26_map_keys_{}", suffix);
                let keys_fn = self.module.get_function(&symbol).ok_or_else(|| {
                    CompileError::simple(
                        &format!("runtime for Map.keys not linked ({} missing)", symbol),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let i64_ty = self.context.i64_type();
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let count_alloca = self.builder.build_alloca(i64_ty, "keys_count").unwrap();
                let call = self
                    .builder
                    .build_call(keys_fn, &[map_ptr.into(), count_alloca.into()], "keys_call")
                    .unwrap();
                let arr = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("{} returned void", symbol),
                            "llvm",
                        ));
                    }
                };
                let count = self
                    .builder
                    .build_load(i64_ty, count_alloca, "keys_count_val")
                    .unwrap()
                    .into_int_value();
                let struct_ty = self
                    .context
                    .struct_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
                let mut sv = struct_ty.get_undef();
                sv = self
                    .builder
                    .build_insert_value(sv, arr, 0, "keys_buf")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 1, "keys_len")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 2, "keys_cap")
                    .unwrap()
                    .into_struct_value();
                Ok((sv.into(), Type::list(key_ty)))
            }
            "Map.values" => {
                // Returns List<V>, sorted by key so zip(keys,
                // values) aligns. Same shape as Map.keys but the
                // element type is value_ty.
                let map_ptr = self.compile_value(&args[0])?;
                let recv_ty = args[0].type_of();
                let (key_ty, value_ty) = match &recv_ty {
                    Type::Map(k, v) => ((**k).clone(), (**v).clone()),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("Map.values receiver is not a Map: {}", recv_ty),
                            "llvm",
                        ));
                    }
                };
                let suffix = Self::map_suffix(&key_ty, &value_ty).ok_or_else(|| {
                    CompileError::unsupported_operation(
                        &format!(
                            "Map.values on LLVM requires one of the three \
                             supported map shapes; found {}",
                            recv_ty
                        ),
                        "llvm",
                    )
                })?;
                let symbol = format!("algol26_map_values_{}", suffix);
                let values_fn = self.module.get_function(&symbol).ok_or_else(|| {
                    CompileError::simple(
                        &format!("runtime for Map.values not linked ({} missing)", symbol),
                        0,
                        0,
                        "",
                        ErrorCode::E0009,
                    )
                })?;
                let i64_ty = self.context.i64_type();
                let ptr_ty = self.context.ptr_type(AddressSpace::default());
                let count_alloca = self.builder.build_alloca(i64_ty, "values_count").unwrap();
                let call = self
                    .builder
                    .build_call(
                        values_fn,
                        &[map_ptr.into(), count_alloca.into()],
                        "values_call",
                    )
                    .unwrap();
                let arr = match call.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
                    _ => {
                        return Err(CompileError::unsupported_operation(
                            &format!("{} returned void", symbol),
                            "llvm",
                        ));
                    }
                };
                let count = self
                    .builder
                    .build_load(i64_ty, count_alloca, "values_count_val")
                    .unwrap()
                    .into_int_value();
                let struct_ty = self
                    .context
                    .struct_type(&[ptr_ty.into(), i64_ty.into(), i64_ty.into()], false);
                let mut sv = struct_ty.get_undef();
                sv = self
                    .builder
                    .build_insert_value(sv, arr, 0, "values_buf")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 1, "values_len")
                    .unwrap()
                    .into_struct_value();
                sv = self
                    .builder
                    .build_insert_value(sv, count, 2, "values_cap")
                    .unwrap()
                    .into_struct_value();
                Ok((sv.into(), Type::list(value_ty)))
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
                // ADR 0042 phase 1b (ext). A list-returning
                // *builtin* (currently `args()`, the first one)
                // yields the same {ptr, i64, i64} descriptor shape
                // a list-returning *function* does. The function
                // path (`Instruction::Call`'s else branch) already
                // registers the descriptor in `list_structs` /
                // `list_arrays`; this path drifted and skipped it,
                // so the subsequent `Declare` — which expects to
                // adopt the source's descriptor — errored with
                // "list `argv` has no descriptor struct". Mirror
                // the function-path logic here.
                //
                // var_types is set to List<Unknown> (not val_ty)
                // when the shape is a list descriptor, matching
                // the function path; the Declare updates it with
                // the concrete element type.
                let mut is_list_shape = false;
                if let inkwell::types::BasicTypeEnum::StructType(st) = val.get_type() {
                    if st.count_fields() == 3 {
                        is_list_shape = true;
                        self.list_structs.insert(res_name.clone(), alloca);
                        let ptr_ty = self.context.ptr_type(AddressSpace::default());
                        let buf_slot = self
                            .builder
                            .build_struct_gep(st, alloca, 0, &format!("{}_buf_slot", res_name))
                            .unwrap();
                        let buf = self
                            .builder
                            .build_load(ptr_ty, buf_slot, &format!("{}_buf", res_name))
                            .unwrap()
                            .into_pointer_value();
                        self.list_arrays.insert(res_name.clone(), buf);
                    }
                }
                if is_list_shape {
                    self.var_types
                        .insert(res_name.clone(), Type::list(Type::Unknown));
                } else {
                    self.var_types.insert(res_name.clone(), val_ty);
                }
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

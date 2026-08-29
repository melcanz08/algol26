// ALGOL26 Standard Library - String Module
// Provides string operations via C library functions

use inkwell::values::{BasicValue, BasicValueEnum, BasicMetadataValueEnum};
use inkwell::AddressSpace;
use crate::codegen::CodeGen;
use crate::diagnostics::{CompileError, ErrorCode, Result};

#[allow(dead_code)]
impl<'ctx> CodeGen<'ctx> {
    pub fn register_string_functions(&mut self) {
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        
        // strlen(s) -> i64
        let strlen_type = i64_type.fn_type(&[ptr_type.into()], false);
        self.module.add_function("strlen", strlen_type, None);
        
        // strcat(dest, src) -> ptr
        let strcat_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
        self.module.add_function("strcat", strcat_type, None);
        
        // strcpy(dest, src) -> ptr
        let strcpy_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
        self.module.add_function("strcpy", strcpy_type, None);
        
        // toupper(c) -> int
        let toupper_type = i32_type.fn_type(&[i32_type.into()], false);
        self.module.add_function("toupper", toupper_type, None);
        
        // tolower(c) -> int
        let tolower_type = i32_type.fn_type(&[i32_type.into()], false);
        self.module.add_function("tolower", tolower_type, None);
        
        // malloc(size) -> ptr
        let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
        self.module.add_function("malloc", malloc_type, None);
    }
    
    pub fn call_string_function(
        &self, 
        name: &str, 
        args: &[BasicValueEnum<'ctx>]
    ) -> Result<BasicValueEnum<'ctx>> {
        match name {
            "String.length" => self.impl_string_length(args),
            "String.concat" => self.impl_string_concat(args),
            "String.substring" => self.impl_string_substring(args),
            "String.to_upper" => self.impl_string_to_upper(args),
            "String.to_lower" => self.impl_string_to_lower(args),
            _ => {
                if !args.is_empty() {
                    Ok(args[0])
                } else {
                    let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
                    Ok(null_ptr.as_basic_value_enum())
                }
            }
        }
    }
    
    fn impl_string_length(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        let func = self.module.get_function("strlen").ok_or_else(|| {
            CompileError::new("strlen not found", 0, 0, "", ErrorCode::E0004)
        })?;
        
        let metadata_args: Vec<BasicMetadataValueEnum> = args
            .iter()
            .map(|arg| (*arg).into())
            .collect();
        
        let call = self.builder.build_call(func, &metadata_args, "strlen_call")
            .map_err(|e| CompileError::new(
                &format!("Failed to call strlen: {}", e),
                0, 0, "",
                ErrorCode::E0001
            ))?;
        
        match call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(value) => Ok(value),
            _ => Ok(self.context.i64_type().const_int(0, false).as_basic_value_enum()),
        }
    }
    
    fn impl_string_substring(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.len() < 3 {
            let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
            return Ok(null_ptr.as_basic_value_enum());
        }
        
        let s = args[0];
        let start = args[1].into_int_value();
        let length = args[2].into_int_value();
        
        let i64_type = self.context.i64_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let malloc_func = self.module.get_function("malloc").unwrap();
        
        // buf = malloc(length + 1)
        let one = i64_type.const_int(1, false);
        let total_len = self.builder.build_int_add(length, one, "substr_len").unwrap();
        let malloc_args: Vec<BasicMetadataValueEnum> = vec![total_len.into()];
        let malloc_call = self.builder.build_call(malloc_func, &malloc_args, "malloc_substr").unwrap();
        let buf = match malloc_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        // Copy characters using a loop
        let loop_block = self.context.append_basic_block(self.current_function.unwrap(), "substr_loop");
        let after_loop = self.context.append_basic_block(self.current_function.unwrap(), "substr_done");
        
        // i = 0
        let i_ptr = self.builder.build_alloca(i64_type, "i").unwrap();
        self.builder.build_store(i_ptr, i64_type.const_int(0, false)).unwrap();
        
        self.builder.build_unconditional_branch(loop_block).unwrap();
        
        // Loop body
        self.builder.position_at_end(loop_block);
        let i_val = self.builder.build_load(i64_type, i_ptr, "i_load").unwrap().into_int_value();
        
        // if i < length
        let cond = self.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            i_val,
            length,
            "loop_cond"
        ).unwrap();
        
        let body_block = self.context.append_basic_block(self.current_function.unwrap(), "substr_body");
        let exit_block = after_loop;
        
        self.builder.build_conditional_branch(cond, body_block, exit_block).unwrap();
        
        // Copy character
        self.builder.position_at_end(body_block);
        
        // src_char = s[start + i]
        let offset = self.builder.build_int_add(start, i_val, "src_offset").unwrap();
        let src_ptr = unsafe {
            self.builder.build_gep(
                ptr_type,
                s.into_pointer_value(),
                &[offset],
                "src_ptr"
            ).unwrap()
        };
        let src_char = self.builder.build_load(i64_type, src_ptr, "src_char").unwrap();
        
        // dst_ptr = buf + i
        let dst_ptr = unsafe {
            self.builder.build_gep(
                ptr_type,
                buf,
                &[i_val],
                "dst_ptr"
            ).unwrap()
        };
        
        // dst[i] = s[start + i]
        self.builder.build_store(dst_ptr, src_char).unwrap();
        
        // i++
        let next_i = self.builder.build_int_add(i_val, one, "next_i").unwrap();
        self.builder.build_store(i_ptr, next_i).unwrap();
        
        self.builder.build_unconditional_branch(loop_block).unwrap();
        
        // Null-terminate
        self.builder.position_at_end(after_loop);
        let null_pos = unsafe {
            self.builder.build_gep(
                ptr_type,
                buf,
                &[length],
                "null_pos"
            ).unwrap()
        };
        self.builder.build_store(null_pos, i64_type.const_int(0, false)).unwrap();
        
        Ok(buf.as_basic_value_enum())
    }
    
    fn impl_string_to_upper(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.is_empty() {
            let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
            return Ok(null_ptr.as_basic_value_enum());
        }
        
        let s = args[0];
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        
        // Get functions
        let strlen_func = self.module.get_function("strlen").unwrap();
        let malloc_func = self.module.get_function("malloc").unwrap();
        let toupper_func = self.module.get_function("toupper").unwrap();
        
        // len = strlen(s)
        let strlen_args: Vec<BasicMetadataValueEnum> = vec![s.into()];
        let strlen_call = self.builder.build_call(strlen_func, &strlen_args, "upper_len").unwrap();
        let len = match strlen_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        // buf = malloc(len + 1)
        let one = i64_type.const_int(1, false);
        let total_len = self.builder.build_int_add(len, one, "upper_total").unwrap();
        let malloc_args: Vec<BasicMetadataValueEnum> = vec![total_len.into()];
        let malloc_call = self.builder.build_call(malloc_func, &malloc_args, "upper_malloc").unwrap();
        let buf = match malloc_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        // Loop: for i in 0..len { buf[i] = toupper(s[i]) }
        let loop_block = self.context.append_basic_block(self.current_function.unwrap(), "upper_loop");
        let after_loop = self.context.append_basic_block(self.current_function.unwrap(), "upper_done");
        
        let i_ptr = self.builder.build_alloca(i64_type, "upper_i").unwrap();
        self.builder.build_store(i_ptr, i64_type.const_int(0, false)).unwrap();
        self.builder.build_unconditional_branch(loop_block).unwrap();
        
        self.builder.position_at_end(loop_block);
        let i_val = self.builder.build_load(i64_type, i_ptr, "upper_i_load").unwrap().into_int_value();
        
        let cond = self.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            i_val,
            len,
            "upper_cond"
        ).unwrap();
        
        let body_block = self.context.append_basic_block(self.current_function.unwrap(), "upper_body");
        self.builder.build_conditional_branch(cond, body_block, after_loop).unwrap();
        
        self.builder.position_at_end(body_block);
        
        // src_char = s[i]
        let src_ptr = unsafe { self.builder.build_gep(ptr_type, s.into_pointer_value(), &[i_val], "upper_src").unwrap() };
        let src_byte = self.builder.build_load(i64_type, src_ptr, "upper_src_byte").unwrap();
        let src_char = self.builder.build_int_truncate(src_byte.into_int_value(), i32_type, "upper_src_char").unwrap();
        
        // c = toupper(src_char)
        let toupper_args: Vec<BasicMetadataValueEnum> = vec![src_char.into()];
        let toupper_call = self.builder.build_call(toupper_func, &toupper_args, "upper_call").unwrap();
        let upper_char = match toupper_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        // buf[i] = (char)toupper(s[i])
        let dst_ptr = unsafe { self.builder.build_gep(ptr_type, buf, &[i_val], "upper_dst").unwrap() };
        let upper_byte = self.builder.build_int_truncate(upper_char, i64_type, "upper_byte").unwrap();
        self.builder.build_store(dst_ptr, upper_byte).unwrap();
        
        // i++
        let next_i = self.builder.build_int_add(i_val, one, "upper_next").unwrap();
        self.builder.build_store(i_ptr, next_i).unwrap();
        self.builder.build_unconditional_branch(loop_block).unwrap();
        
        // Null-terminate
        self.builder.position_at_end(after_loop);
        let null_pos = unsafe { self.builder.build_gep(ptr_type, buf, &[len], "upper_null").unwrap() };
        self.builder.build_store(null_pos, i64_type.const_int(0, false)).unwrap();
        
        Ok(buf.as_basic_value_enum())
    }
    
    fn impl_string_to_lower(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.is_empty() {
            let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
            return Ok(null_ptr.as_basic_value_enum());
        }
        
        let s = args[0];
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        
        let strlen_func = self.module.get_function("strlen").unwrap();
        let malloc_func = self.module.get_function("malloc").unwrap();
        let tolower_func = self.module.get_function("tolower").unwrap();
        
        let strlen_args: Vec<BasicMetadataValueEnum> = vec![s.into()];
        let strlen_call = self.builder.build_call(strlen_func, &strlen_args, "lower_len").unwrap();
        let len = match strlen_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        let one = i64_type.const_int(1, false);
        let total_len = self.builder.build_int_add(len, one, "lower_total").unwrap();
        let malloc_args: Vec<BasicMetadataValueEnum> = vec![total_len.into()];
        let malloc_call = self.builder.build_call(malloc_func, &malloc_args, "lower_malloc").unwrap();
        let buf = match malloc_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        let loop_block = self.context.append_basic_block(self.current_function.unwrap(), "lower_loop");
        let after_loop = self.context.append_basic_block(self.current_function.unwrap(), "lower_done");
        
        let i_ptr = self.builder.build_alloca(i64_type, "lower_i").unwrap();
        self.builder.build_store(i_ptr, i64_type.const_int(0, false)).unwrap();
        self.builder.build_unconditional_branch(loop_block).unwrap();
        
        self.builder.position_at_end(loop_block);
        let i_val = self.builder.build_load(i64_type, i_ptr, "lower_i_load").unwrap().into_int_value();
        
        let cond = self.builder.build_int_compare(
            inkwell::IntPredicate::SLT,
            i_val,
            len,
            "lower_cond"
        ).unwrap();
        
        let body_block = self.context.append_basic_block(self.current_function.unwrap(), "lower_body");
        self.builder.build_conditional_branch(cond, body_block, after_loop).unwrap();
        
        self.builder.position_at_end(body_block);
        
        let src_ptr = unsafe { self.builder.build_gep(ptr_type, s.into_pointer_value(), &[i_val], "lower_src").unwrap() };
        let src_byte = self.builder.build_load(i64_type, src_ptr, "lower_src_byte").unwrap();
        let src_char = self.builder.build_int_truncate(src_byte.into_int_value(), i32_type, "lower_src_char").unwrap();
        
        let tolower_args: Vec<BasicMetadataValueEnum> = vec![src_char.into()];
        let tolower_call = self.builder.build_call(tolower_func, &tolower_args, "lower_call").unwrap();
        let lower_char = match tolower_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        let dst_ptr = unsafe { self.builder.build_gep(ptr_type, buf, &[i_val], "lower_dst").unwrap() };
        let lower_byte = self.builder.build_int_truncate(lower_char, i64_type, "lower_byte").unwrap();
        self.builder.build_store(dst_ptr, lower_byte).unwrap();
        
        let next_i = self.builder.build_int_add(i_val, one, "lower_next").unwrap();
        self.builder.build_store(i_ptr, next_i).unwrap();
        self.builder.build_unconditional_branch(loop_block).unwrap();
        
        self.builder.position_at_end(after_loop);
        let null_pos = unsafe { self.builder.build_gep(ptr_type, buf, &[len], "lower_null").unwrap() };
        self.builder.build_store(null_pos, i64_type.const_int(0, false)).unwrap();
        
        Ok(buf.as_basic_value_enum())
    }
    
    fn impl_string_concat(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.len() < 2 {
            let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
            return Ok(null_ptr.as_basic_value_enum());
        }
        
        let s1 = args[0];
        let s2 = args[1];
        
        // Get strlen function
        let strlen_func = self.module.get_function("strlen").unwrap();
        let strcpy_func = self.module.get_function("strcpy").unwrap();
        let strcat_func = self.module.get_function("strcat").unwrap();
        let malloc_func = self.module.get_function("malloc").unwrap();
        
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i64_type = self.context.i64_type();
        
        // len1 = strlen(s1)
        let len1_args: Vec<BasicMetadataValueEnum> = vec![s1.into()];
        let len1_call = self.builder.build_call(strlen_func, &len1_args, "len1")
            .unwrap();
        let len1 = match len1_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => i64_type.const_int(0, false),
        };
        
        // len2 = strlen(s2)
        let len2_args: Vec<BasicMetadataValueEnum> = vec![s2.into()];
        let len2_call = self.builder.build_call(strlen_func, &len2_args, "len2")
            .unwrap();
        let len2 = match len2_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_int_value(),
            _ => i64_type.const_int(0, false),
        };
        
        // total_len = len1 + len2 + 1
        let one = i64_type.const_int(1, false);
        let sum = self.builder.build_int_add(len1, len2, "sum").unwrap();
        let total_len = self.builder.build_int_add(sum, one, "total_len").unwrap();
        
        // buf = malloc(total_len)
        let malloc_args: Vec<BasicMetadataValueEnum> = vec![total_len.into()];
        let malloc_call = self.builder.build_call(malloc_func, &malloc_args, "malloc_buf")
            .unwrap();
        let buf = match malloc_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        // strcpy(buf, s1)
        let strcpy_args: Vec<BasicMetadataValueEnum> = vec![buf.into(), s1.into()];
        self.builder.build_call(strcpy_func, &strcpy_args, "strcpy_s1").unwrap();
        
        // strcat(buf, s2)
        let strcat_args: Vec<BasicMetadataValueEnum> = vec![buf.into(), s2.into()];
        let strcat_call = self.builder.build_call(strcat_func, &strcat_args, "strcat_s2")
            .unwrap();
        
        match strcat_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => Ok(v),
            _ => Ok(buf.as_basic_value_enum()),
        }
    }
}

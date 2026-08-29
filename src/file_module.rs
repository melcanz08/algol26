// ALGOL26 Standard Library - File I/O Module
// Provides file operations via C library functions

use inkwell::values::{BasicValue, BasicValueEnum, BasicMetadataValueEnum};
use inkwell::AddressSpace;
use crate::codegen::CodeGen;
use crate::diagnostics::Result;

#[allow(dead_code)]
impl<'ctx> CodeGen<'ctx> {
    pub fn register_file_functions(&mut self) {
        let i64_type = self.context.i64_type();
        let i32_type = self.context.i32_type();
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        
        // fopen(filename, mode) -> ptr
        let fopen_type = ptr_type.fn_type(&[ptr_type.into(), ptr_type.into()], false);
        self.module.add_function("fopen", fopen_type, None);
        
        // fclose(file) -> int
        let fclose_type = i32_type.fn_type(&[ptr_type.into()], false);
        self.module.add_function("fclose", fclose_type, None);
        
        // fgets(str, n, stream) -> ptr
        let fgets_type = ptr_type.fn_type(&[ptr_type.into(), i32_type.into(), ptr_type.into()], false);
        self.module.add_function("fgets", fgets_type, None);
        
        // malloc(size) -> ptr
        let malloc_type = ptr_type.fn_type(&[i64_type.into()], false);
        self.module.add_function("malloc", malloc_type, None);
    }
    
    pub fn call_file_function(
        &self, 
        name: &str, 
        args: &[BasicValueEnum<'ctx>]
    ) -> Result<BasicValueEnum<'ctx>> {
        match name {
            "File.write" => {
                self.impl_file_write(args)
            }
            "File.read" => {
                self.impl_file_read(args)
            }
            "File.append" => {
                self.impl_file_append(args)
            }
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
    
    fn impl_file_write(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.len() < 2 {
            return Ok(self.context.i32_type().const_int(0, false).as_basic_value_enum());
        }
        
        let path = args[0];
        let content = args[1];
        
        let i32_type = self.context.i32_type();
        
        let fopen_func = self.module.get_function("fopen").unwrap();
        let fclose_func = self.module.get_function("fclose").unwrap();
        let fputs_func = self.module.get_function("fputs").unwrap();
        
        // mode = "w"
        let mode = self.builder.build_global_string_ptr("w", "write_mode").unwrap();
        
        // file = fopen(path, "w")
        let fopen_args: Vec<BasicMetadataValueEnum> = vec![
            path.into(),
            mode.as_pointer_value().into(),
        ];
        let fopen_call = self.builder.build_call(fopen_func, &fopen_args, "fopen_write").unwrap();
        let file = match fopen_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v,
            _ => return Ok(i32_type.const_int(0, false).as_basic_value_enum()),
        };
        
        // fputs(content, file)
        let fputs_args: Vec<BasicMetadataValueEnum> = vec![
            content.into(),
            file.into(),
        ];
        self.builder.build_call(fputs_func, &fputs_args, "fputs_write").unwrap();
        
        // fclose(file)
        let fclose_args: Vec<BasicMetadataValueEnum> = vec![file.into()];
        self.builder.build_call(fclose_func, &fclose_args, "fclose_write").unwrap();
        
        Ok(i32_type.const_int(0, false).as_basic_value_enum())
    }
    
    fn impl_file_append(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.len() < 2 {
            return Ok(self.context.i32_type().const_int(0, false).as_basic_value_enum());
        }
        
        let path = args[0];
        let content = args[1];
        
        let i32_type = self.context.i32_type();
        
        let fopen_func = self.module.get_function("fopen").unwrap();
        let fclose_func = self.module.get_function("fclose").unwrap();
        let fputs_func = self.module.get_function("fputs").unwrap();
        
        // mode = "a" (append mode)
        let mode = self.builder.build_global_string_ptr("a", "append_mode").unwrap();
        
        // file = fopen(path, "a")
        let fopen_args: Vec<BasicMetadataValueEnum> = vec![
            path.into(),
            mode.as_pointer_value().into(),
        ];
        let fopen_call = self.builder.build_call(fopen_func, &fopen_args, "fopen_append").unwrap();
        let file = match fopen_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v,
            _ => return Ok(i32_type.const_int(0, false).as_basic_value_enum()),
        };
        
        // fputs(content, file)
        let fputs_args: Vec<BasicMetadataValueEnum> = vec![
            content.into(),
            file.into(),
        ];
        self.builder.build_call(fputs_func, &fputs_args, "fputs_append").unwrap();
        
        // fclose(file)
        let fclose_args: Vec<BasicMetadataValueEnum> = vec![file.into()];
        self.builder.build_call(fclose_func, &fclose_args, "fclose_append").unwrap();
        
        Ok(i32_type.const_int(0, false).as_basic_value_enum())
    }
    
    fn impl_file_read(&self, args: &[BasicValueEnum<'ctx>]) -> Result<BasicValueEnum<'ctx>> {
        if args.is_empty() {
            let null_ptr = self.context.ptr_type(AddressSpace::default()).const_null();
            return Ok(null_ptr.as_basic_value_enum());
        }
        
        let path = args[0];
        
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        
        let fopen_func = self.module.get_function("fopen").unwrap();
        let fclose_func = self.module.get_function("fclose").unwrap();
        let fgets_func = self.module.get_function("fgets").unwrap();
        let malloc_func = self.module.get_function("malloc").unwrap();
        
        // mode = "r"
        let mode = self.builder.build_global_string_ptr("r", "read_mode").unwrap();
        
        // file = fopen(path, "r")
        let fopen_args: Vec<BasicMetadataValueEnum> = vec![
            path.into(),
            mode.as_pointer_value().into(),
        ];
        let fopen_call = self.builder.build_call(fopen_func, &fopen_args, "fopen_read").unwrap();
        let file = match fopen_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v,
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        // buf = malloc(1024)
        let buf_size = i64_type.const_int(1024, false);
        let malloc_args: Vec<BasicMetadataValueEnum> = vec![buf_size.into()];
        let malloc_call = self.builder.build_call(malloc_func, &malloc_args, "read_buf").unwrap();
        let buf = match malloc_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => return Ok(ptr_type.const_null().as_basic_value_enum()),
        };
        
        // fgets(buf, 1024, file)
        let fgets_args: Vec<BasicMetadataValueEnum> = vec![
            buf.into(),
            i32_type.const_int(1024, false).into(),
            file.into(),
        ];
        let fgets_call = self.builder.build_call(fgets_func, &fgets_args, "fgets_read").unwrap();
        let result = match fgets_call.try_as_basic_value() {
            inkwell::values::ValueKind::Basic(v) => v.into_pointer_value(),
            _ => buf,
        };
        
        // fclose(file)
        let fclose_args: Vec<BasicMetadataValueEnum> = vec![file.into()];
        self.builder.build_call(fclose_func, &fclose_args, "fclose_read").unwrap();
        
        Ok(result.as_basic_value_enum())
    }
}

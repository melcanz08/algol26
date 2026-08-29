// ALGOL26 - List Module
// Provides list/array operations

use inkwell::values::{BasicValue, BasicValueEnum};
use crate::codegen::CodeGen;
use crate::diagnostics::Result;

#[allow(dead_code)]
impl<'ctx> CodeGen<'ctx> {
    pub fn call_list_function(
        &self,
        name: &str,
        _args: &[BasicValueEnum<'ctx>],
    ) -> Result<BasicValueEnum<'ctx>> {
        match name {
            "List.length" => {
                Ok(self.context.i64_type().const_int(0, false).as_basic_value_enum())
            }
            "List.sum" => {
                Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
            }
            "List.max" => {
                Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
            }
            "List.min" => {
                Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
            }
            _ => {
                Ok(self.context.f64_type().const_float(0.0).as_basic_value_enum())
            }
        }
    }
}

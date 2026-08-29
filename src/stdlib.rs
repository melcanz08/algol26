// ALGOL26 Standard Library - Math Module
// Provides mathematical functions

use crate::codegen::CodeGen;

#[allow(dead_code)]
impl<'ctx> CodeGen<'ctx> {
    pub fn register_math_functions(&mut self) {
        let f64_type = self.context.f64_type();
        
        // Math.sqrt(x)
        let sqrt_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.sqrt", sqrt_type, None);
        
        // Math.pow(x, y)
        let pow_type = f64_type.fn_type(&[f64_type.into(), f64_type.into()], false);
        self.module.add_function("Math.pow", pow_type, None);
        
        // Math.sin(x)
        let sin_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.sin", sin_type, None);
        
        // Math.cos(x)
        let cos_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.cos", cos_type, None);
        
        // Math.abs(x)
        let abs_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.abs", abs_type, None);
        
        // Math.floor(x)
        let floor_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.floor", floor_type, None);
        
        // Math.ceil(x)
        let ceil_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.ceil", ceil_type, None);
        
        // Math.exp(x)
        let exp_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.exp", exp_type, None);
        
        // Math.log(x)
        let log_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.log", log_type, None);
        
        // Math.tan(x)
        let tan_type = f64_type.fn_type(&[f64_type.into()], false);
        self.module.add_function("Math.tan", tan_type, None);
    }
}
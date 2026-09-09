// src/ffi/lowering.rs - HARDENED
// Complete FFI lowering with type checking

use crate::common::types::Type;
use crate::ffi::c::CType;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FFIFunction {
    pub algol_name: String,
    pub c_name: String,
    pub param_types: Vec<CType>,
    pub return_type: CType,
    pub variadic: bool,
}

#[derive(Debug, Clone)]
pub struct FFIRegistry {
    functions: HashMap<String, FFIFunction>,
    libraries: Vec<String>,
}

impl Default for FFIRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FFIRegistry {
    pub fn new() -> Self {
        FFIRegistry {
            functions: HashMap::new(),
            libraries: Vec::new(),
        }
    }

    pub fn register(
        &mut self,
        algol_name: &str,
        c_name: &str,
        param_types: Vec<CType>,
        return_type: CType,
        variadic: bool,
    ) {
        self.functions.insert(
            algol_name.to_string(),
            FFIFunction {
                algol_name: algol_name.to_string(),
                c_name: c_name.to_string(),
                param_types,
                return_type,
                variadic,
            },
        );
    }

    pub fn register_library(&mut self, library: &str) {
        if !self.libraries.contains(&library.to_string()) {
            self.libraries.push(library.to_string());
        }
    }

    pub fn get_function(&self, algol_name: &str) -> Option<&FFIFunction> {
        self.functions.get(algol_name)
    }

    pub fn get_c_name(&self, algol_name: &str) -> Option<&String> {
        self.functions.get(algol_name).map(|f| &f.c_name)
    }

    pub fn get_libraries(&self) -> &[String] {
        &self.libraries
    }

    pub fn is_ffi(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    pub fn validate_call(&self, algol_name: &str, arg_types: &[Type]) -> Result<(), String> {
        let func = self
            .functions
            .get(algol_name)
            .ok_or_else(|| format!("FFI function '{}' not found", algol_name))?;

        if func.variadic {
            // Can't validate variadic functions
            return Ok(());
        }

        if func.param_types.len() != arg_types.len() {
            return Err(format!(
                "FFI function '{}' expects {} arguments but got {}",
                algol_name,
                func.param_types.len(),
                arg_types.len()
            ));
        }

        for (i, (c_type, algol_type)) in func.param_types.iter().zip(arg_types).enumerate() {
            if !Self::types_compatible(c_type, algol_type) {
                return Err(format!(
                    "FFI argument {} type mismatch for '{}': C type {} vs Algol26 type {}",
                    i, algol_name, c_type, algol_type
                ));
            }
        }

        Ok(())
    }

    fn types_compatible(c_type: &CType, algol_type: &Type) -> bool {
        match (c_type, algol_type) {
            (CType::CInt, Type::Int) => true,
            (CType::CLong, Type::Int) => true,
            (CType::CLongLong, Type::Int) => true,
            (CType::CShort, Type::Int) => true,
            (CType::CFloat, Type::Float) => true,
            (CType::CDouble, Type::Float) => true,
            (CType::CBool, Type::Bool) => true,
            (CType::CString, Type::String) => true,
            (CType::CVoid, Type::Void) => true,
            (CType::CPointer(_), Type::Ptr) => true,
            (CType::CPointer(_), Type::Pointer(_)) => true,
            (CType::CConstPointer(_), Type::Ptr) => true,
            (CType::CConstPointer(_), Type::Pointer(_)) => true,
            (CType::CSizeT, Type::Int) => true,
            _ => false,
        }
    }
}

pub fn register_stdlib_functions(registry: &mut FFIRegistry) {
    // Math functions
    let math_functions = [
        ("Math.sqrt", "sqrt", vec![CType::CDouble], CType::CDouble),
        (
            "Math.pow",
            "pow",
            vec![CType::CDouble, CType::CDouble],
            CType::CDouble,
        ),
        ("Math.sin", "sin", vec![CType::CDouble], CType::CDouble),
        ("Math.cos", "cos", vec![CType::CDouble], CType::CDouble),
        ("Math.tan", "tan", vec![CType::CDouble], CType::CDouble),
        ("Math.exp", "exp", vec![CType::CDouble], CType::CDouble),
        ("Math.log", "log", vec![CType::CDouble], CType::CDouble),
        ("Math.floor", "floor", vec![CType::CDouble], CType::CDouble),
        ("Math.ceil", "ceil", vec![CType::CDouble], CType::CDouble),
        ("Math.abs", "fabs", vec![CType::CDouble], CType::CDouble),
    ];

    for (algol_name, c_name, params, ret) in math_functions {
        registry.register(algol_name, c_name, params, ret, false);
        registry.register_library("m");
    }

    // String functions
    registry.register(
        "String.length",
        "strlen",
        vec![CType::CString],
        CType::CSizeT,
        false,
    );
    registry.register_library("c");

    // Memory functions
    registry.register(
        "alloc",
        "malloc",
        vec![CType::CSizeT],
        CType::CPointer(Box::new(CType::CVoid)),
        false,
    );
    registry.register(
        "free",
        "free",
        vec![CType::CPointer(Box::new(CType::CVoid))],
        CType::CVoid,
        false,
    );
    registry.register_library("c");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_registry_with_types() {
        let mut registry = FFIRegistry::new();
        register_stdlib_functions(&mut registry);

        // Test Math.sqrt
        let func = registry.get_function("Math.sqrt").unwrap();
        assert_eq!(func.c_name, "sqrt");
        assert_eq!(func.param_types.len(), 1);
        assert_eq!(func.param_types[0], CType::CDouble);
        assert_eq!(func.return_type, CType::CDouble);

        // Test type validation
        assert!(registry.validate_call("Math.sqrt", &[Type::Float]).is_ok());
        assert!(registry.validate_call("Math.sqrt", &[Type::Int]).is_err());
    }

    #[test]
    fn test_ffi_type_validation() {
        let mut registry = FFIRegistry::new();
        registry.register(
            "test_func",
            "test_func",
            vec![CType::CInt, CType::CDouble],
            CType::CVoid,
            false,
        );

        // Correct types
        assert!(registry
            .validate_call("test_func", &[Type::Int, Type::Float])
            .is_ok());

        // Wrong types
        assert!(registry
            .validate_call("test_func", &[Type::Float, Type::Int])
            .is_err());

        // Wrong number of args
        assert!(registry.validate_call("test_func", &[Type::Int]).is_err());
    }
}

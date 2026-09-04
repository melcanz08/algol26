// ALGOL26 FFI Lowering — Foreign Function Interface handling
// Extracted from ir_codegen.rs for architectural isolation

use std::collections::HashMap;

/// Represents a registered FFI function in LLVM
#[derive(Debug, Clone)]
pub struct FFIFunction {
    /// ALGOL26 name (e.g., "Math.sqrt")
    pub algol_name: String,
    /// C symbol name (e.g., "sqrt")
    pub c_name: String,
    /// LLVM function type
    pub llvm_type: String,
}

/// FFI Registry — manages foreign function declarations
pub struct FFIRegistry {
    /// ALGOL26 name → LLVM function name
    functions: HashMap<String, String>,
    /// C library names to link against
    libraries: Vec<String>,
}

impl FFIRegistry {
    pub fn new() -> Self {
        FFIRegistry {
            functions: HashMap::new(),
            libraries: Vec::new(),
        }
    }
    
    /// Register a foreign function
    pub fn register(&mut self, algol_name: &str, c_name: &str) {
        self.functions.insert(algol_name.to_string(), c_name.to_string());
    }
    
    /// Register a library to link against
    pub fn register_library(&mut self, library: &str) {
        if !self.libraries.contains(&library.to_string()) {
            self.libraries.push(library.to_string());
        }
    }
    
    /// Get the C symbol name for an ALGOL26 function
    pub fn get_c_name(&self, algol_name: &str) -> Option<&String> {
        self.functions.get(algol_name)
    }
    
    /// Get all libraries to link against
    pub fn get_libraries(&self) -> &[String] {
        &self.libraries
    }
    
    /// Check if a function is registered as FFI
    pub fn is_ffi(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }
}

/// Register standard C library functions
pub fn register_stdlib_functions(registry: &mut FFIRegistry) {
    // Math functions
    let math_functions = [
        ("Math.sqrt", "sqrt"),
        ("Math.pow", "pow"),
        ("Math.sin", "sin"),
        ("Math.cos", "cos"),
        ("Math.abs", "fabs"),
        ("Math.floor", "floor"),
        ("Math.ceil", "ceil"),
        ("Math.exp", "exp"),
        ("Math.log", "log"),
        ("Math.tan", "tan"),
    ];
    for (algol_name, c_name) in math_functions {
        registry.register(algol_name, c_name);
        registry.register_library("m");
    }
    
    // String functions
    registry.register("String.length", "strlen");
    registry.register_library("c");
    
    // File functions
    registry.register("File.open", "fopen");
    registry.register("File.close", "fclose");
    registry.register("File.read", "fgets");
    registry.register("File.write", "fprintf");
    registry.register_library("c");
    
    // Memory functions
    registry.register("alloc", "malloc");
    registry.register("free", "free");
    registry.register_library("c");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ffi_registry_register() {
        let mut registry = FFIRegistry::new();
        registry.register("Math.sqrt", "sqrt");
        assert!(registry.is_ffi("Math.sqrt"));
        assert_eq!(registry.get_c_name("Math.sqrt"), Some(&"sqrt".to_string()));
    }
    
    #[test]
    fn test_ffi_registry_libraries() {
        let mut registry = FFIRegistry::new();
        register_stdlib_functions(&mut registry);
        assert!(registry.get_libraries().contains(&"m".to_string()));
        assert!(registry.get_libraries().contains(&"c".to_string()));
    }
}

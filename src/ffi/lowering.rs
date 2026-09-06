// ALGOL26 FFI Lowering — Foreign Function Interface handling
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct FFIFunction {
    pub algol_name: String,
    pub c_name: String,
    pub llvm_type: String,
}

#[derive(Debug, Clone)]
pub struct FFIRegistry {
    functions: HashMap<String, String>,
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
    pub fn register(&mut self, algol_name: &str, c_name: &str) {
        self.functions
            .insert(algol_name.to_string(), c_name.to_string());
    }
    pub fn register_library(&mut self, library: &str) {
        if !self.libraries.contains(&library.to_string()) {
            self.libraries.push(library.to_string());
        }
    }
    pub fn get_c_name(&self, algol_name: &str) -> Option<&String> {
        self.functions.get(algol_name)
    }
    pub fn get_libraries(&self) -> &[String] {
        &self.libraries
    }
    pub fn is_ffi(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }
    pub fn all_functions(&self) -> &HashMap<String, String> {
        &self.functions
    }
    pub fn all_functions_cloned(&self) -> HashMap<String, String> {
        self.functions.clone()
    }
}

pub fn register_stdlib_functions(registry: &mut FFIRegistry) {
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
    registry.register("String.length", "strlen");
    registry.register_library("c");
    registry.register("File.open", "fopen");
    registry.register("File.close", "fclose");
    registry.register("File.read", "fgets");
    registry.register("File.write", "fprintf");
    registry.register_library("c");
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
    #[test]
    fn test_ffi_all_functions() {
        let mut registry = FFIRegistry::new();
        register_stdlib_functions(&mut registry);
        assert!(registry.all_functions().len() >= 14);
    }
}

// ALGOL26 - Backend Contract
// Defines the interface all compilation backends must implement

use crate::semantic_ir::SemanticProgram;
use crate::diagnostics::Result;

/// Represents the output of a backend compilation
#[derive(Debug, Clone)]
pub enum BackendOutput {
    /// LLVM IR was generated
    LlvmIr,
    /// Native executable was produced
    NativeExecutable,
    /// Interpreter execution completed
    InterpreterOutput,
    /// WASM module was generated
    WasmModule,
}

/// The Backend trait defines the interface that all compilation
/// backends must implement to work with the ALGOL26 compiler.
pub trait Backend {
    /// Compile the given Semantic IR program
    fn compile(&self, ir: &SemanticProgram, output_name: &str) -> Result<BackendOutput>;
    
    /// Returns the name of this backend
    fn name(&self) -> &str;
    
    /// Returns a description of what this backend produces
    fn description(&self) -> &str;
    
    /// Returns whether this backend can run the compiled program
    fn can_execute(&self) -> bool {
        false
    }
}

/// A registry of available backends
pub struct BackendRegistry {
    backends: Vec<Box<dyn Backend>>,
}

impl BackendRegistry {
    pub fn new() -> Self {
        BackendRegistry {
            backends: Vec::new(),
        }
    }
    
    pub fn register(&mut self, backend: Box<dyn Backend>) {
        self.backends.push(backend);
    }
    
    pub fn get(&self, name: &str) -> Option<&Box<dyn Backend>> {
        self.backends.iter().find(|b| b.name() == name)
    }
    
    pub fn list(&self) -> Vec<&str> {
        self.backends.iter().map(|b| b.name()).collect()
    }
}

impl Default for BackendRegistry {
    fn default() -> Self {
        Self::new()
    }
}

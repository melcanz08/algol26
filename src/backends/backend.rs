// ALGOL26 - Backend Contract
// Defines the interface all compilation backends must implement
// Backends receive VerifiedIR — guaranteed to be semantically valid

use crate::common::diagnostics::Result;
use crate::ir::verified_ir::VerifiedIR;

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

/// The Backend trait — all backends consume VerifiedIR
pub trait Backend {
    /// Compile a verified IR program
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput>;

    /// Name of this backend
    fn name(&self) -> &str;

    /// Description of what this backend produces
    fn description(&self) -> &str;

    /// Whether this backend can execute the compiled program
    fn can_execute(&self) -> bool {
        false
    }
}

/// Registry of available backends
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

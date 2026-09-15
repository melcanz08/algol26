// src/backends/backend.rs - ALGOL26 - Backend Contract
// Defines the interface all compilation backends must implement
// Backends receive VerifiedIR — guaranteed to be semantically valid

use crate::common::diagnostics::Result;
use crate::ir::verified_ir::VerifiedIR;
use std::path::PathBuf;

/// Represents the output of a backend compilation.
///
/// Each variant carries the data the compiler driver needs to
/// continue: the path to the emitted `.ll` or `.wasm` file,
/// the interpreter's captured stdout, and so on. Before PR-13e
/// these were unit variants and callers reconstructed the path
/// or output out-of-band — which is why the trait was not the
/// real integration point.
#[derive(Debug, Clone)]
pub enum BackendOutput {
    /// LLVM IR was written to `path`.
    LlvmIr { path: PathBuf },
    /// Native executable was produced at `path`.
    NativeExecutable { path: PathBuf },
    /// Interpreter execution completed with this stdout.
    InterpreterOutput { stdout: String },
    /// WASM module was written to `path`.
    WasmModule { path: PathBuf },
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

    pub fn get(&self, name: &str) -> Option<&dyn Backend> {
        self.backends
            .iter()
            .find(|b| b.name() == name)
            .map(|v| v.as_ref())
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

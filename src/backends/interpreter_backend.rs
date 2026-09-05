// ALGOL26 - Interpreter Backend Implementation
// Implements the Backend trait for direct interpretation

use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::interpreter::Interpreter;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::verified_ir::VerifiedIR;

pub struct InterpreterBackend;

impl InterpreterBackend {
    pub fn new() -> Self {
        InterpreterBackend
    }
}

impl Backend for InterpreterBackend {
    fn compile(&self, ir: &VerifiedIR, _output_name: &str) -> Result<BackendOutput> {
        let mut interpreter = Interpreter::new(ir.program().clone());
        match interpreter.run() {
            Ok(output) => {
                if !output.is_empty() {
                    println!("{}", output);
                }
                Ok(BackendOutput::InterpreterOutput)
            }
            Err(e) => Err(CompileError::new(
                &format!("Interpreter error: {}", e),
                0,
                0,
                "",
                ErrorCode::E0001,
            )),
        }
    }

    fn name(&self) -> &str {
        "interpreter"
    }

    fn description(&self) -> &str {
        "Executes programs directly from SemanticProgram"
    }

    fn can_execute(&self) -> bool {
        true
    }
}

impl Default for InterpreterBackend {
    fn default() -> Self {
        Self::new()
    }
}

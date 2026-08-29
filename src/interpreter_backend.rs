// ALGOL26 - Interpreter Backend Implementation
// Implements the Backend trait for direct interpretation

use crate::backend::{Backend, BackendOutput};
use crate::semantic_ir::SemanticProgram;
use crate::diagnostics::Result;

pub struct InterpreterBackend;

impl InterpreterBackend {
    pub fn new() -> Self {
        InterpreterBackend
    }
}

impl Backend for InterpreterBackend {
    fn compile(&self, _ir: &SemanticProgram, _output_name: &str) -> Result<BackendOutput> {
        // The interpreter executes directly from the AST/Semantic IR
        // For now, just return success
        Ok(BackendOutput::InterpreterOutput)
    }
    
    fn name(&self) -> &str {
        "interpreter"
    }
    
    fn description(&self) -> &str {
        "Executes programs directly without compilation"
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

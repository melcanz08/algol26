
use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::interpreter::Interpreter;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::verified_ir::VerifiedIR;
pub struct InterpreterBackend;
impl InterpreterBackend { pub fn new() -> Self { Self } }
impl Default for InterpreterBackend { fn default() -> Self { Self::new() } }
impl Backend for InterpreterBackend {
    fn compile(&self, ir: &VerifiedIR, _output_name: &str) -> Result<BackendOutput> {
        let mut interpreter = Interpreter::new(ir.program().clone());
        interpreter.run().map_err(|e| CompileError::new(&format!("Runtime error: {:?}", e),0,0,"", ErrorCode::E0002))?;
        Ok(BackendOutput::InterpreterOutput)
    }
    fn name(&self) -> &str { "interpreter" }
    fn description(&self) -> &str { "Interprets SemanticProgram" }
    fn can_execute(&self) -> bool { true }
}

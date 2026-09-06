
use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::ir_codegen::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::verified_ir::VerifiedIR;
use inkwell::context::Context;
pub struct LlvmBackend;
impl Default for LlvmBackend { fn default() -> Self { Self::new() } }
impl LlvmBackend { pub fn new() -> Self { LlvmBackend } }
impl Backend for LlvmBackend {
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {
        let context = Context::create();
        let mut codegen = IRCodeGen::new(&context, "algol26_module");
        codegen.compile(ir.program()).map_err(|e| { e.display(); CompileError::new("Codegen failed",0,0,"", ErrorCode::E0002) })?;
        let ir_path = format!("{}.ll", output_name);
        codegen.module.print_to_file(&ir_path).map_err(|e| CompileError::new(&format!("emit failed: {}", e),0,0,"", ErrorCode::E0001))?;
        Ok(BackendOutput::LlvmIr)
    }
    fn name(&self) -> &str { "llvm" }
    fn description(&self) -> &str { "LLVM IR from SemanticProgram" }
    fn can_execute(&self) -> bool { true }
}

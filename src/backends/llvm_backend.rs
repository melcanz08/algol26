// ALGOL26 - LLVM Backend Implementation
// Implements the Backend trait for LLVM code generation
// Now consumes SemanticProgram via IRCodeGen

use crate::backends::backend::{Backend, BackendOutput};
use crate::ir::verified_ir::VerifiedIR;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::backends::ir_codegen::IRCodeGen;
use inkwell::context::Context;

pub struct LlvmBackend;

impl LlvmBackend {
    pub fn new() -> Self {
        LlvmBackend
    }
}

impl Backend for LlvmBackend {
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {
        let context = Context::create();
        let mut codegen = IRCodeGen::new(&context, "algol26_module");
        
        codegen.compile(ir.program()).map_err(|e| {
            e.display();
            CompileError::new("Code generation failed", 0, 0, "", ErrorCode::E0002)
        })?;
        
        let ir_path = format!("{}.ll", output_name);
        codegen.module.print_to_file(&ir_path).map_err(|e| {
            CompileError::new(&format!("Failed to emit LLVM IR: {}", e), 0, 0, "", ErrorCode::E0001)
        })?;
        
        Ok(BackendOutput::LlvmIr)
    }
    
    fn name(&self) -> &str {
        "llvm"
    }
    
    fn description(&self) -> &str {
        "Generates LLVM IR and native executables from SemanticProgram"
    }
    
    fn can_execute(&self) -> bool {
        true
    }
}

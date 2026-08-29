// ALGOL26 - LLVM Backend Implementation
// Implements the Backend trait for LLVM code generation

use crate::backend::{Backend, BackendOutput};
use crate::semantic_ir::SemanticProgram;
use crate::ast::FunctionDecl;
use crate::diagnostics::{CompileError, ErrorCode, Result};
use crate::codegen::CodeGen;
use inkwell::context::Context;

pub struct LlvmBackend {
    functions: Vec<FunctionDecl>,
}

impl LlvmBackend {
    pub fn new(functions: Vec<FunctionDecl>) -> Self {
        LlvmBackend { functions }
    }
}

impl Backend for LlvmBackend {
    fn compile(&self, _ir: &SemanticProgram, output_name: &str) -> Result<BackendOutput> {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "algol26_module");
        codegen.register_math_functions();
        codegen.register_string_functions();
        codegen.register_file_functions();
        
        // Use the original AST functions for now (until we fully separate IR from AST)
        let functions = self.functions.clone();
        codegen.compile_program(functions).map_err(|e| {
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
        "Generates LLVM IR and native executables"
    }
    
    fn can_execute(&self) -> bool {
        true
    }
}

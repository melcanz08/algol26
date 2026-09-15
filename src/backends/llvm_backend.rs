// src/backends/llvm_backend.rs

use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::llvm_codegen::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::verified_ir::VerifiedIR;
use inkwell::context::Context;

pub struct LlvmBackend;

impl Default for LlvmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl LlvmBackend {
    pub fn new() -> Self {
        LlvmBackend
    }
}

impl Backend for LlvmBackend {
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {
        // Capability check: refuse programs that use features
        // LLVM cannot lower. Moved here from
        // `Compiler::lower_to_llvm` so every caller of the trait
        // gets the check and the trait becomes the enforcement
        // point rather than an optional wrapper.
        crate::backends::capabilities::check_backend(
            ir.program(),
            &crate::backends::capabilities::BackendCapabilities::llvm(),
        )?;

        let context = Context::create();
        let mut codegen = IRCodeGen::new(&context, "algol26_module");

        codegen.compile(ir.program()).map_err(|e| {
            let error_msg = format!("Codegen failed: {}", e);
            e.display();
            CompileError::simple(&error_msg, 0, 0, "", ErrorCode::E0002)
        })?;

        // Verify BEFORE writing. If verification fails, the `.ll`
        // file never lands on disk and a user's `clang bad.ll`
        // follow-up cannot accidentally run against invalid IR.
        if let Err(e) = codegen.module.verify() {
            return Err(CompileError::simple(
                &format!("Generated LLVM IR is invalid: {}", e),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        // `.with_extension("ll")` so `foo.bar` produces `foo.ll`,
        // matching what `Compiler::lower_to_llvm` used to do.
        let ir_path = std::path::PathBuf::from(output_name).with_extension("ll");
        codegen.module.print_to_file(&ir_path).map_err(|e| {
            CompileError::simple(
                &format!("Failed to emit LLVM IR to {}: {}", ir_path.display(), e),
                0,
                0,
                "",
                ErrorCode::E0001,
            )
        })?;

        println!("[Generated LLVM IR: {}]", ir_path.display());

        Ok(BackendOutput::LlvmIr { path: ir_path })
    }

    fn name(&self) -> &str {
        "llvm"
    }
    fn description(&self) -> &str {
        "LLVM IR from SemanticProgram with validation"
    }
    fn can_execute(&self) -> bool {
        true
    }
}

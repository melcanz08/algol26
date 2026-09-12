// src/backends/wasm_backend.rs

use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::llvm_codegen::IRCodeGen;
use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::ir::verified_ir::VerifiedIR;
use inkwell::context::Context;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetTriple,
};

pub struct WasmBackend;

impl Default for WasmBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmBackend {
    pub fn new() -> Self {
        WasmBackend
    }

    fn validate_wasm_compatibility(ir: &VerifiedIR) -> Result<()> {
        // Check for unsupported operations
        for func in &ir.program().functions {
            for block in &func.blocks {
                for instr in &block.instructions {
                    match instr {
                        // Send and Receive are not supported in WASM
                        crate::ir::semantic_ir::Instruction::Send { .. } => {
                            return Err(CompileError::new(
                                "Channel send is not supported in WASM backend",
                                0,
                                0,
                                "",
                                ErrorCode::E0002,
                            ));
                        }
                        crate::ir::semantic_ir::Instruction::Receive { .. } => {
                            return Err(CompileError::new(
                                "Channel receive is not supported in WASM backend",
                                0,
                                0,
                                "",
                                ErrorCode::E0002,
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }
}

impl Backend for WasmBackend {
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {
        // Validate WASM compatibility
        Self::validate_wasm_compatibility(ir)?;

        // Initialize WebAssembly target
        Target::initialize_webassembly(&InitializationConfig::default());

        let context = Context::create();
        let mut codegen = IRCodeGen::new(&context, "algol26_wasm");

        codegen.compile(ir.program()).map_err(|e| {
            let error_msg = format!("WASM code generation failed: {}", e);
            e.display();
            CompileError::simple(&error_msg, 0, 0, "", ErrorCode::E0002)
        })?;

        let target_triple = TargetTriple::create("wasm32-unknown-unknown");

        let target = Target::from_triple(&target_triple).map_err(|e| {
            CompileError::simple(
                &format!("Failed to get WASM target: {}", e),
                0,
                0,
                "",
                ErrorCode::E0001,
            )
        })?;

        let machine = target
            .create_target_machine(
                &target_triple,
                "generic",
                "",
                inkwell::OptimizationLevel::Default,
                RelocMode::PIC,
                CodeModel::Small,
            )
            .ok_or_else(|| {
                CompileError::simple(
                    "Failed to create WASM target machine",
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        let wasm_path = format!("{}.wasm", output_name);
        machine
            .write_to_file(
                &codegen.module,
                FileType::Object,
                std::path::Path::new(&wasm_path),
            )
            .map_err(|e| {
                CompileError::simple(
                    &format!("Failed to write WASM to {}: {}", wasm_path, e),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        println!("[Generated WASM: {}]", wasm_path);

        Ok(BackendOutput::WasmModule)
    }

    fn name(&self) -> &str {
        "wasm"
    }

    fn description(&self) -> &str {
        "Generates WebAssembly modules from SemanticProgram via LLVM"
    }

    fn can_execute(&self) -> bool {
        false
    }
}

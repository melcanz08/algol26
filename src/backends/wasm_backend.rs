// ALGOL26 - WASM Backend Implementation
// Generates WebAssembly using LLVM's WebAssembly target
// Consumes SemanticProgram via IRCodeGen

use crate::backends::backend::{Backend, BackendOutput};
use crate::backends::ir_codegen::IRCodeGen;
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
}

impl Backend for WasmBackend {
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {
        // Initialize WebAssembly target
        Target::initialize_webassembly(&InitializationConfig::default());

        let context = Context::create();
        let mut codegen = IRCodeGen::new(&context, "algol26_wasm");

        // Compile using SemanticProgram (the canonical IR)
        codegen.compile(ir.program()).map_err(|e| {
            e.display();
            CompileError::new("WASM code generation failed", 0, 0, "", ErrorCode::E0002)
        })?;

        // Set WASM target
        let target_triple = TargetTriple::create("wasm32-unknown-unknown");

        let target = Target::from_triple(&target_triple).map_err(|e| {
            CompileError::new(
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
                CompileError::new(
                    "Failed to create WASM target machine",
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        // Write WASM object file
        let wasm_path = format!("{}.wasm", output_name);
        machine
            .write_to_file(
                &codegen.module,
                FileType::Object,
                std::path::Path::new(&wasm_path),
            )
            .map_err(|e| {
                CompileError::new(
                    &format!("Failed to write WASM: {}", e),
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

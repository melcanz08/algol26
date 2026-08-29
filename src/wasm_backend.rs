// ALGOL26 - WASM Backend Implementation
// Generates WebAssembly using LLVM's WebAssembly target

use crate::backend::{Backend, BackendOutput};
use crate::semantic_ir::SemanticProgram;
use crate::ast::FunctionDecl;
use crate::diagnostics::{CompileError, ErrorCode, Result};
use crate::codegen::CodeGen;
use inkwell::context::Context;
use inkwell::targets::{
    Target, TargetTriple, InitializationConfig,
    RelocMode, CodeModel, FileType,
};

pub struct WasmBackend {
    functions: Vec<FunctionDecl>,
}

impl WasmBackend {
    pub fn new(functions: Vec<FunctionDecl>) -> Self {
        WasmBackend { functions }
    }
}

impl Backend for WasmBackend {
    fn compile(&self, _ir: &SemanticProgram, output_name: &str) -> Result<BackendOutput> {
        // Initialize WebAssembly target
        Target::initialize_webassembly(&InitializationConfig::default());
        
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "algol26_wasm");
        codegen.register_math_functions();
        codegen.register_string_functions();
        codegen.register_file_functions();
        
        // Compile using AST functions
        let functions = self.functions.clone();
        codegen.compile_program(functions).map_err(|e| {
            e.display();
            CompileError::new("WASM code generation failed", 0, 0, "", ErrorCode::E0002)
        })?;
        
        // Set WASM target
        let target_triple = TargetTriple::create("wasm32-unknown-unknown");
        
        let target = Target::from_triple(&target_triple).map_err(|e| {
            CompileError::new(&format!("Failed to get WASM target: {}", e), 0, 0, "", ErrorCode::E0001)
        })?;
        
        let machine = target.create_target_machine(
            &target_triple,
            "generic",
            "",
            inkwell::OptimizationLevel::Default,
            RelocMode::PIC,
            CodeModel::Small,
        ).ok_or_else(|| {
            CompileError::new("Failed to create WASM target machine", 0, 0, "", ErrorCode::E0001)
        })?;
        
        // Write WASM object file
        let wasm_path = format!("{}.wasm", output_name);
        machine.write_to_file(
            &codegen.module,
            FileType::Object,
            std::path::Path::new(&wasm_path),
        ).map_err(|e| {
            CompileError::new(&format!("Failed to write WASM: {}", e), 0, 0, "", ErrorCode::E0001)
        })?;
        
        println!("[Generated WASM: {}]", wasm_path);
        
        Ok(BackendOutput::WasmModule)
    }
    
    fn name(&self) -> &str {
        "wasm"
    }
    
    fn description(&self) -> &str {
        "Generates WebAssembly modules via LLVM"
    }
    
    fn can_execute(&self) -> bool {
        false
    }
}

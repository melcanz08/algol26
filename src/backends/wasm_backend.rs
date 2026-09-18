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
}

impl Backend for WasmBackend {
    fn compile(&self, ir: &VerifiedIR, output_name: &str) -> Result<BackendOutput> {
        // Capability check — refuses any program whose constructs
        // the WASM backend cannot lower (channels, Result, Option,
        // spawn, ...). The supported set is declared in
        // `BackendCapabilities::wasm()` and enforced here before
        // any codegen runs.
        //
        // Note: the WASM backend reuses `IRCodeGen` — the same
        // struct the LLVM backend uses. So the fail-closed work
        // done across `value.rs`, `instruction.rs`, `terminator.rs`,
        // etc. applies here for free; there is no separate WASM
        // lowering to keep in sync.
        // (The old `validate_wasm_compatibility` only checked
        // Send/Receive and missed everything else.)
        crate::backends::capabilities::check_backend(
            ir.program(),
            &crate::backends::capabilities::BackendCapabilities::wasm(),
        )?;

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

        // Verify the generated LLVM IR before handing it to the
        // target machine. Matches the LLVM backend and catches
        // codegen bugs before they reach the writer.
        if let Err(e) = codegen.module.verify() {
            return Err(CompileError::simple(
                &format!("Generated WASM IR is invalid: {}", e),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        // wasm-ld expects a relocatable object, so write LLVM's
        // output to a `.wasm.o` intermediate first, then link it
        // into a runnable module.
        let obj_path = format!("{}.wasm.o", output_name);
        machine
            .write_to_file(
                &codegen.module,
                FileType::Object,
                std::path::Path::new(&obj_path),
            )
            .map_err(|e| {
                CompileError::simple(
                    &format!("Failed to write WASM object to {}: {}", obj_path, e),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        // Link the object into a runnable module. Undefined
        // C-library symbols (printf, exit, malloc, free, sqrt,
        // strlen, ...) become imports from `env`; the Node host
        // in `runtime/wasm/host.js` provides them.
        //   --no-entry        : no CLI `_start` entry; host calls main
        //   --allow-undefined : unresolved symbols become imports
        //   --export=main     : expose main to the host
        //   --export=memory   : expose linear memory to the host
        let wasm_path = format!("{}.wasm", output_name);
        let link = std::process::Command::new("wasm-ld")
            .arg("--no-entry")
            .arg("--allow-undefined")
            .arg("--export=main")
            .arg("--export=memory")
            .arg("-o")
            .arg(&wasm_path)
            .arg(&obj_path)
            .output()
            .map_err(|e| {
                CompileError::simple(
                    &format!(
                        "Failed to run wasm-ld: {} \
                         (install with `sudo apt install lld`)",
                        e
                    ),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )
            })?;

        if !link.status.success() {
            return Err(CompileError::simple(
                &format!(
                    "wasm-ld linking failed:\n{}",
                    String::from_utf8_lossy(&link.stderr)
                ),
                0,
                0,
                "",
                ErrorCode::E0001,
            ));
        }

        let _ = std::fs::remove_file(&obj_path);

        println!("[Generated WASM: {}]", wasm_path);

        Ok(BackendOutput::WasmModule {
            path: std::path::PathBuf::from(wasm_path),
        })
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

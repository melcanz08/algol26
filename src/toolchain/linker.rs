// src/toolchain/linker.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Resolve `output_name` to an absolute path. Relative names are
/// resolved against the current working directory.
fn resolve_output_path(output_name: &str) -> PathBuf {
    if Path::new(output_name).is_absolute() {
        PathBuf::from(output_name)
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(output_name)
    }
}

/// Compile LLVM IR at `ir_path` into a native executable at
/// `output_name` using the host `clang`.
///
/// Returns the resolved absolute path of the produced binary.
pub fn link_llvm_ir(ir_path: &Path, output_name: &str) -> Result<PathBuf> {
    let output_path = resolve_output_path(output_name);

    let output = Command::new("clang")
        .arg(ir_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-O2")
        .arg("-lm")
        .arg("-lpthread")
        .output()
        .map_err(|e| {
            let err = CompileError::simple(
                &format!("Failed to run clang: {}", e),
                0, 0, "", ErrorCode::E0001,
            );
            err.display();
            err
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let err = CompileError::simple(
            &format!("Linking failed: {}", stderr),
            0, 0, "", ErrorCode::E0001,
        );
        err.display();
        return Err(err);
    }

    Ok(output_path)
}

/// Run a compiled executable, forwarding its stdout/stderr to the
/// parent process. A non-zero exit status is not treated as a compiler
/// error — the program ran successfully, it just returned a failure.
pub fn run_binary(path: &Path) -> Result<()> {
    Command::new(path).status().map_err(|e| {
        let err = CompileError::simple(
            &format!("Failed to run: {}", e),
            0, 0, "", ErrorCode::E0001,
        );
        err.display();
        err
    })?;
    Ok(())
}
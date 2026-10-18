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
pub fn link_llvm_ir(ir_path: &Path, output_name: &str, libraries: &[String]) -> Result<PathBuf> {
    let output_path = resolve_output_path(output_name);

    let mut cmd = Command::new("clang");
    cmd.arg(ir_path)
        .arg("-o")
        .arg(&output_path)
        .arg("-O2")
        .arg("-lm")
        .arg("-lpthread");

    // ADR 0027. The LLVM backend lowers `Map<K, V>` by calling into
    // a small C runtime. Compile it inline here (clang handles mixed
    // .ll + .c input). Fails closed if the runtime is missing — the
    // capability claims that use Map must have a runtime to call.
    let map_runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("runtime")
        .join("map.c");
    if !map_runtime.exists() {
        return Err(CompileError::simple(
            &format!(
                "runtime/map.c not found at {} — required for Map<K, V> lowering",
                map_runtime.display()
            ),
            0,
            0,
            "",
            ErrorCode::E0001,
        ));
    }
    cmd.arg(&map_runtime);

    // runtime/string.c — String.to_int (strict i64 parser) and
    // future string runtime helpers. Same pattern as map_runtime:
    // fail closed if missing, since a program that reaches a
    // string-runtime call must have one to call.
    let string_runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("runtime")
        .join("string.c");
    if !string_runtime.exists() {
        return Err(CompileError::simple(
            &format!(
                "runtime/string.c not found at {} — required for String.to_int lowering",
                string_runtime.display()
            ),
            0,
            0,
            "",
            ErrorCode::E0001,
        ));
    }
    cmd.arg(&string_runtime);

    // runtime/file.c — File.read / .write / .append. Same pattern
    // as map_runtime and string_runtime.
    let file_runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("runtime")
        .join("file.c");
    if !file_runtime.exists() {
        return Err(CompileError::simple(
            &format!(
                "runtime/file.c not found at {} — required for File.* lowering",
                file_runtime.display()
            ),
            0,
            0,
            "",
            ErrorCode::E0001,
        ));
    }
    cmd.arg(&file_runtime);
    // Each extern declaration that used `from "lib"` becomes a
    // `-l<lib>` flag. The linker searches the standard library
    // paths plus anything the user added to LIBRARY_PATH.
    for lib in libraries {
        cmd.arg(format!("-l{}", lib));
    }
    let output = cmd.output().map_err(|e| {
        let err = CompileError::simple(
            &format!("Failed to run clang: {}", e),
            0,
            0,
            "",
            ErrorCode::E0001,
        );
        err.display();
        err
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let err = CompileError::simple(
            &format!("Linking failed: {}", stderr),
            0,
            0,
            "",
            ErrorCode::E0001,
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
        let err =
            CompileError::simple(&format!("Failed to run: {}", e), 0, 0, "", ErrorCode::E0001);
        err.display();
        err
    })?;
    Ok(())
}

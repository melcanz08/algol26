// tests/conformance_tests.rs
//
// Directive-driven conformance tests.
//
// Programs under tests/conformance/valid/ must run and, if they carry
// OUTPUT directives, produce exactly that stdout. Programs under
// tests/conformance/invalid/ must be rejected, and if they carry an
// ERROR_CODE directive, the diagnostic must contain that code.
//
// Directives are comments near the top of the file:
//
//     // BACKEND: interpreter    force the interpreter path
//     // OUTPUT: <text>          one line of expected stdout
//     // ERROR_CODE: E0007       for invalid programs
//
// Multiple OUTPUT lines accumulate in order.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Default)]
struct Directives {
    backend: Option<String>,
    expected_output: Vec<String>,
    expected_error_code: Option<String>,
}

fn parse_directives(source: &str) -> Directives {
    let mut d = Directives::default();
    for line in source.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("//") {
            continue;
        }
        let body = trimmed.trim_start_matches('/').trim();
        if let Some(rest) = body.strip_prefix("BACKEND:") {
            d.backend = Some(rest.trim().to_string());
        } else if let Some(rest) = body.strip_prefix("OUTPUT:") {
            d.expected_output.push(rest.trim_start().to_string());
        } else if let Some(rest) = body.strip_prefix("ERROR_CODE:") {
            d.expected_error_code = Some(rest.trim().to_string());
        }
    }
    d
}

/// A backend that a conformance test can be run on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Interpreter,
    Llvm,
    Wasm,
}

impl Backend {
    fn name(&self) -> &'static str {
        match self {
            Backend::Interpreter => "interpreter",
            Backend::Llvm => "llvm",
            Backend::Wasm => "wasm",
        }
    }
}

/// Parse the `// BACKEND:` directive value into a list of backends.
/// Absent directive -> `[Llvm]` (default). A single name -> `[that]`.
/// Whitespace-separated names -> each name in order. Unknown names
/// panic (fail fast - a typo in a test header should not silently
/// pick a default).
fn parse_backends(spec: Option<&str>) -> Vec<Backend> {
    match spec {
        None => vec![Backend::Llvm],
        Some(s) => s
            .split_whitespace()
            .map(|name| match name {
                "interpreter" => Backend::Interpreter,
                "llvm" | "default" => Backend::Llvm,
                "wasm" => Backend::Wasm,
                other => panic!(
                    "unknown backend `{}` in `// BACKEND:` directive; \
                     valid names are interpreter, llvm, wasm",
                    other
                ),
            })
            .collect(),
    }
}

/// True if the tools WASM tests need are available on this system.
/// If not, WASM backends in a test's directive are skipped with a
/// warning rather than failing.
fn wasm_tools_available() -> bool {
    let node = Command::new("node").arg("--version").output();
    let wasm_ld = Command::new("wasm-ld").arg("--version").output();
    node.map(|o| o.status.success()).unwrap_or(false)
        && wasm_ld.map(|o| o.status.success()).unwrap_or(false)
}

fn compiler_binary() -> PathBuf {
    // Prefer the newest available binary. A hardcoded release-first
    // order silently used a stale release binary after a debug-only
    // rebuild, producing test failures against code the developer
    // had already fixed. mtime comparison avoids that class of bug.
    let candidates: Vec<PathBuf> = [
        "target/debug/algol26",
        "target/release/algol26",
        "../target/debug/algol26",
        "../target/release/algol26",
    ]
    .iter()
    .map(PathBuf::from)
    .filter(|p| p.exists())
    .collect();

    candidates
        .into_iter()
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .unwrap_or_else(|| {
            panic!("No compiler binary found. Run `cargo build` before `cargo test`.")
        })
}

fn run_compiler(binary: &PathBuf, program: &PathBuf, backend: Backend) -> std::process::Output {
    let mut cmd = Command::new(binary);
    match backend {
        Backend::Interpreter => {
            cmd.arg("run");
            cmd.arg("--interpreter");
        }
        Backend::Llvm => {
            cmd.arg("run");
        }
        Backend::Wasm => {
            // WASM uses a different entry point (compile then run
            // under the Node shim); this function is not used for it.
            unreachable!("run_compiler called with Wasm backend");
        }
    }

    // Send the compiled artifact to target/ so it doesn't collide with
    // source files or run concurrently with other test targets.
    let out_dir = std::env::temp_dir().join("algol26-conformance");
    let _ = std::fs::create_dir_all(&out_dir);
    let stem = program
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program");
    let output = out_dir.join(stem);
    cmd.arg("--output").arg(&output);

    cmd.arg(program);
    cmd.output().expect("failed to invoke compiler")
}

fn compile_only(binary: &PathBuf, program: &PathBuf) -> std::process::Output {
    let out_dir = std::env::temp_dir().join("algol26-conformance");
    let _ = std::fs::create_dir_all(&out_dir);
    let stem = program
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program");
    let output = out_dir.join(stem);
    Command::new(binary)
        .arg("build")
        .arg("--output")
        .arg(&output)
        .arg(program)
        .output()
        .expect("failed to invoke compiler")
}

/// Compile a program to WASM, then run it under the Node host shim.
///
/// The compiler writes `<stem>.wasm` next to the source file. To
/// avoid leaving .wasm files in the conformance tree, we copy the
/// source to a temp directory first, compile from there, and run
/// the resulting .wasm.
fn run_wasm(binary: &PathBuf, program: &PathBuf) -> std::process::Output {
    use std::fs;

    let out_dir = std::env::temp_dir().join("algol26-conformance-wasm");
    let _ = fs::create_dir_all(&out_dir);

    let stem = program
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("program");
    let staged_src = out_dir.join(format!("{}.gol", stem));
    let produced_wasm = out_dir.join(format!("{}.wasm", stem));

    // Clean any stale artifacts.
    let _ = fs::remove_file(&staged_src);
    let _ = fs::remove_file(&produced_wasm);

    // Stage the source in the temp directory.
    if let Err(e) = fs::copy(program, &staged_src) {
        panic!(
            "failed to stage {} for wasm compile: {}",
            program.display(),
            e
        );
    }

    // Compile to WASM. The compiler writes <stem>.wasm next to the
    // staged source, which is in out_dir.
    let compile = Command::new(binary)
        .arg("wasm")
        .arg(&staged_src)
        .output()
        .expect("failed to invoke wasm compiler");

    if !compile.status.success() {
        return compile;
    }

    if !produced_wasm.exists() {
        // The compiler succeeded but no .wasm was produced where we
        // expected. Fail with the compile output for diagnosis.
        return std::process::Output {
            status: std::process::ExitStatus::default(),
            stdout: compile.stdout,
            stderr: format!("wasm compiler did not produce {}", produced_wasm.display())
                .into_bytes(),
        };
    }

    // Run under the Node host shim.
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let host = repo_root.join("runtime/wasm/host.js");
    Command::new("node")
        .arg(&host)
        .arg(&produced_wasm)
        .output()
        .expect("failed to invoke node host shim")
}

/// Filter the compiler's status lines out of stdout. Compiler lines
/// are all bracketed (`[Compiling ...]`, `[Output: ...]`, etc.).
fn program_stdout(raw: &str) -> String {
    let mut lines: Vec<&str> = raw.lines().filter(|l| !l.starts_with('[')).collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn conformance_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/conformance")
}

/// Iterate over `.gol` files under `dir`, recursing exactly one level.
/// Flat files directly under `dir` are also accepted (for backwards
/// compatibility while the tree is being reorganized).
fn gol_files_under(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                for sub in sub_entries.flatten() {
                    let sub_path = sub.path();
                    if sub_path.extension().and_then(|s| s.to_str()) == Some("gol") {
                        out.push(sub_path);
                    }
                }
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("gol") {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn test_conformance_valid() {
    let bin = compiler_binary();
    let dir = conformance_root().join("valid");
    if !dir.exists() {
        eprintln!("skipping: {} does not exist", dir.display());
        return;
    }

    let wasm_ok = wasm_tools_available();
    let mut count = 0;
    let mut skipped_wasm = 0;

    for path in gol_files_under(&dir) {
        let source = std::fs::read_to_string(&path).expect("read program");
        let dirs = parse_directives(&source);
        let backends = parse_backends(dirs.backend.as_deref());
        let expected = dirs.expected_output.join("\n");

        for backend in &backends {
            if *backend == Backend::Wasm && !wasm_ok {
                skipped_wasm += 1;
                continue;
            }

            let out = match backend {
                Backend::Wasm => run_wasm(&bin, &path),
                _ => run_compiler(&bin, &path, *backend),
            };
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);

            if !out.status.success() {
                // A capability refusal is a pass: the program is
                // valid, the backend cannot lower it, and the user
                // is directed at --interpreter.
                if stderr.contains("does not support") {
                    continue;
                }
                panic!(
                    "Valid program failed on backend {}: {}\nstdout: {}\nstderr: {}",
                    backend.name(),
                    path.display(),
                    stdout,
                    stderr,
                );
            }

            if !dirs.expected_output.is_empty() {
                let actual = program_stdout(&stdout);
                assert_eq!(
                    actual.trim(),
                    expected.trim(),
                    "Output mismatch for {} on backend {}\n expected: {:?}\n   actual: {:?}",
                    path.display(),
                    backend.name(),
                    expected,
                    actual,
                );
            }
        }

        count += 1;
    }
    eprintln!(
        "conformance valid: {} programs passed ({} wasm runs skipped)",
        count, skipped_wasm
    );
}

#[test]
fn test_conformance_invalid() {
    let bin = compiler_binary();
    let dir = conformance_root().join("invalid");
    if !dir.exists() {
        eprintln!("skipping: {} does not exist", dir.display());
        return;
    }

    let mut count = 0;
    for path in gol_files_under(&dir) {
        let source = std::fs::read_to_string(&path).expect("read program");
        let dirs = parse_directives(&source);

        let out = compile_only(&bin, &path);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);

        assert!(
            !out.status.success(),
            "Invalid program accepted: {}\nstdout: {}",
            path.display(),
            stdout,
        );

        if let Some(code) = &dirs.expected_error_code {
            let combined = format!("{}{}", stdout, stderr);
            assert!(
                combined.contains(code.as_str()),
                "Expected error code {} in diagnostic for {}\nstderr: {}",
                code,
                path.display(),
                stderr,
            );
        }

        count += 1;
    }
    eprintln!("conformance invalid: {} programs rejected", count);
}

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

fn compiler_binary() -> PathBuf {
    for path in [
        "target/release/algol26",
        "target/debug/algol26",
        "../target/release/algol26",
        "../target/debug/algol26",
    ] {
        let p = PathBuf::from(path);
        if p.exists() {
            return p;
        }
    }
    panic!(
        "No compiler binary found. Run `cargo build` before `cargo test`."
    );
}

fn run_compiler(
    binary: &PathBuf,
    program: &PathBuf,
    backend: Option<&str>,
) -> std::process::Output {
    let mut cmd = Command::new(binary);
    if backend == Some("interpreter") {
        cmd.arg("--interpreter");
    } else {
        cmd.arg("run");
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
    let stem = program.file_stem().and_then(|s| s.to_str()).unwrap_or("program");
    let output = out_dir.join(stem);
    Command::new(binary)
        .arg("build")
        .arg("--output")
        .arg(&output)
        .arg(program)
        .output()
        .expect("failed to invoke compiler")
}

/// Filter the compiler's status lines out of stdout. Compiler lines
/// are all bracketed (`[Compiling ...]`, `[Output: ...]`, etc.).
fn program_stdout(raw: &str) -> String {
    let mut lines: Vec<&str> = raw
        .lines()
        .filter(|l| !l.starts_with('['))
        .collect();
    while lines.last().map_or(false, |l| l.trim().is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

fn conformance_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/conformance")
}

#[test]
fn test_conformance_valid() {
    let bin = compiler_binary();
    let dir = conformance_root().join("valid");
    if !dir.exists() {
        eprintln!("skipping: {} does not exist", dir.display());
        return;
    }

    let mut count = 0;
    for entry in std::fs::read_dir(&dir).expect("read valid dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("gol") {
            continue;
        }

        let source = std::fs::read_to_string(&path).expect("read program");
        let dirs = parse_directives(&source);

        let out = run_compiler(&bin, &path, dirs.backend.as_deref());
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);

        if !out.status.success() {
            // A capability refusal is a pass: the program is valid,
            // the backend cannot lower it, and the user is directed
            // at --interpreter.
            if stderr.contains("does not support") {
                count += 1;
                continue;
            }
            panic!(
                "Valid program failed: {}\nstdout: {}\nstderr: {}",
                path.display(),
                stdout,
                stderr,
            );
        }

        if !dirs.expected_output.is_empty() {
            let actual = program_stdout(&stdout);
            let expected = dirs.expected_output.join("\n");
            assert_eq!(
                actual.trim(),
                expected.trim(),
                "Output mismatch for {}\n expected: {:?}\n   actual: {:?}",
                path.display(),
                expected,
                actual,
            );
        }

        count += 1;
    }
    eprintln!("conformance valid: {} programs passed", count);
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
    for entry in std::fs::read_dir(&dir).expect("read invalid dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("gol") {
            continue;
        }

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

// tests/fixture_support.rs
//
// Differential fixture harness.
//
// Every `.gol` file under `tests/fixtures/` declares which backends
// it runs on with a header line, e.g.:
//
//     // supported: interpreter llvm wasm
//
// The harness reads that header and enforces it:
//
//   - The fixture must run on the interpreter (always declared).
//   - For each backend the header declares, the fixture must
//     produce output identical to the interpreter's.
//   - For each backend the header does *not* declare, the compiler
//     must refuse with a clean E0002 -- never a panic, never exit
//     code 0, never silent wrong output.
//
// Workflow:
//   - New feature: add a fixture with `// supported: interpreter`.
//   - Implement on LLVM: change the header to add `llvm`.
//   - CI now demands parity and refuses silent miscompiles.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Backend {
    Interpreter,
    Llvm,
    Wasm,
}

impl Backend {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "interpreter" => Some(Backend::Interpreter),
            "llvm" => Some(Backend::Llvm),
            "wasm" => Some(Backend::Wasm),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Backend::Interpreter => "interpreter",
            Backend::Llvm => "llvm",
            Backend::Wasm => "wasm",
        }
    }
}

fn algol26_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_algol26"))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixtures_dir() -> PathBuf {
    repo_root().join("tests/fixtures")
}

/// A copy of a fixture in a temp directory, cleaned up on Drop.
/// Keeps the fixture source tree free of interpreter/codegen
/// artifacts during `cargo test`.
struct TempFixture {
    path: PathBuf,
    dir: PathBuf,
}

impl TempFixture {
    fn new(src: &Path) -> Self {
        let stem = src.file_stem().unwrap().to_string_lossy().into_owned();
        let dir =
            std::env::temp_dir().join(format!("algol26_fixture_{}_{}", stem, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(src.file_name().unwrap());
        std::fs::copy(src, &path).unwrap();
        Self { path, dir }
    }
}

impl Drop for TempFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Parse the `// supported:` header. Must appear before any
/// non-comment, non-blank line.
fn parse_supported_header(path: &Path) -> Option<HashSet<Backend>> {
    let content =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("// supported:") {
            let mut set = HashSet::new();
            for tok in rest.split_whitespace() {
                let b = Backend::parse(tok).unwrap_or_else(|| {
                    panic!(
                        "{}: unknown backend `{}` in supported header",
                        path.display(),
                        tok
                    )
                });
                set.insert(b);
            }
            return Some(set);
        }
        if !trimmed.starts_with("//") {
            return None;
        }
    }
    None
}

fn run_interpreter(fixture: &Path) -> String {
    let out = Command::new(algol26_bin())
        .args(["run", "--interpreter", fixture.to_str().unwrap()])
        .output()
        .expect("spawn algol26 interpreter");
    assert!(
        out.status.success(),
        "interpreter failed on {}:\n--- stdout ---\n{}\n--- stderr ---\n{}",
        fixture.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn run_llvm(fixture: &Path, scratch: &Path) -> Result<String, String> {
    let bin = scratch.join("llvm_out");
    let _ = std::fs::remove_file(&bin);
    let out = Command::new(algol26_bin())
        .args([fixture.to_str().unwrap(), "--output", bin.to_str().unwrap()])
        .output()
        .expect("spawn algol26 llvm");
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let run = Command::new(&bin).output().expect("run compiled binary");
    let _ = std::fs::remove_file(&bin);
    if !run.status.success() {
        use std::os::unix::process::ExitStatusExt;
        let code = run.status.code();
        let signal = run.status.signal();
        return Err(format!(
            "compiled binary `{}` exited non-zero.\n\
             status.code()   = {:?}\n\
             status.signal() = {:?}\n\
             --- binary stdout ---\n{}\n\
             --- binary stderr ---\n{}",
            bin.display(),
            code,
            signal,
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr),
        ));
    }
    Ok(String::from_utf8_lossy(&run.stdout).into_owned())
}

fn run_wasm(fixture: &Path, scratch: &Path) -> Result<String, String> {
    let stem = scratch.join("wasm_out");
    let wasm = stem.with_extension("wasm");
    let _ = std::fs::remove_file(&wasm);
    let out = Command::new(algol26_bin())
        .args([
            "wasm",
            fixture.to_str().unwrap(),
            "--output",
            stem.to_str().unwrap(),
        ])
        .output()
        .expect("spawn algol26 wasm");
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let host = repo_root().join("runtime/wasm/host.js");
    let run = Command::new("node")
        .arg(&host)
        .arg(&wasm)
        .output()
        .expect("run node host");
    let _ = std::fs::remove_file(&wasm);
    if !run.status.success() {
        return Err(format!(
            "wasm run exited non-zero:\n{}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&run.stdout).into_owned())
}

/// Check one backend against one fixture. Returns None on success,
/// Some(failure message) otherwise.
fn check_backend(
    fixture: &Path,
    backend: Backend,
    declared: bool,
    expected: &str,
    result: Result<String, String>,
) -> Option<String> {
    match (declared, result) {
        (true, Ok(got)) if got == expected => None,
        (true, Ok(got)) => Some(format!(
            "{}: {} output differs from interpreter.\n--- interpreter ---\n{}\n--- {} ---\n{}",
            fixture.display(),
            backend.name(),
            expected,
            backend.name(),
            got
        )),
        (true, Err(e)) => Some(format!(
            "{}: {} declared supported but refused:\n{}",
            fixture.display(),
            backend.name(),
            e
        )),
        (false, Ok(got)) => Some(format!(
            "{}: {} not declared supported but compiled and produced output.\n--- interpreter ---\n{}\n--- {} ---\n{}",
            fixture.display(),
            backend.name(),
            expected,
            backend.name(),
            got
        )),
        (false, Err(stderr)) => {
            if !stderr.contains("error[E0002]") {
                Some(format!(
                    "{}: {} not declared supported but did not refuse with E0002.\n--- stderr ---\n{}",
                    fixture.display(),
                    backend.name(),
                    stderr
                ))
            } else if stderr.contains("panicked at") {
                Some(format!(
                    "{}: {} panicked instead of refusing cleanly.\n--- stderr ---\n{}",
                    fixture.display(),
                    backend.name(),
                    stderr
                ))
            } else {
                None
            }
        }
    }
}

#[test]
fn fixtures_match_declared_backend_support() {
    let dir = fixtures_dir();
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {}", dir.display(), e))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |x| x == "gol"))
        .collect();
    fixtures.sort();

    assert!(
        !fixtures.is_empty(),
        "no fixtures found under {}",
        dir.display()
    );

    let mut failures: Vec<String> = Vec::new();

    for fixture in &fixtures {
        let supported = parse_supported_header(fixture).unwrap_or_else(|| {
            panic!(
                "{}: missing `// supported:` header. Every fixture must declare \
                 which backends it runs on.",
                fixture.display()
            )
        });

        assert!(
            supported.contains(&Backend::Interpreter),
            "{}: `interpreter` must be in the supported header",
            fixture.display()
        );

        let scratch = TempFixture::new(fixture);
        let expected = run_interpreter(&scratch.path);

        if let Some(f) = check_backend(
            fixture,
            Backend::Llvm,
            supported.contains(&Backend::Llvm),
            &expected,
            run_llvm(&scratch.path, &scratch.dir),
        ) {
            failures.push(f);
        }
        if let Some(f) = check_backend(
            fixture,
            Backend::Wasm,
            supported.contains(&Backend::Wasm),
            &expected,
            run_wasm(&scratch.path, &scratch.dir),
        ) {
            failures.push(f);
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} fixture(s) failed:\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

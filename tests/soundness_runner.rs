// tests/soundness_runner.rs — FIXED to real API
// Uses Compiler::build_semantic_ir_for which already does lex+parse+type_check+build IR

use std::fs;
use std::path::{Path, PathBuf};

use algol26::compiler::Compiler;
use algol26::ir::cfg::{build_cfgs_from_semantic_program, DataflowEngine, OwnershipTransfer};

#[derive(Debug, PartialEq, Eq)]
enum Expect {
    Compile,
    Reject { code: Option<String> },
}

fn parse_expect(source: &str) -> Option<Expect> {
    for line in source.lines().take(5) {
        let trimmed = line.trim();
        if !trimmed.starts_with("//") {
            continue;
        }
        let after = trimmed.trim_start_matches('/').trim();
        if !after.starts_with("EXPECT:") {
            continue;
        }
        let rest = after["EXPECT:".len()..].trim();
        if rest.starts_with("COMPILE") {
            return Some(Expect::Compile);
        }
        if let Some(stripped) = rest.strip_prefix("REJECT") {
            let code = stripped.trim();
            let code = if code.is_empty() {
                None
            } else {
                Some(code.to_string())
            };
            return Some(Expect::Reject { code });
        }
    }
    None
}

fn collect_gol_files() -> Vec<PathBuf> {
    let root = Path::new("tests/soundness");
    let mut files = Vec::new();
    if !root.exists() {
        return files;
    }
    fn walk(dir: &Path, acc: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    walk(&p, acc);
                } else if p.extension().and_then(|s| s.to_str()) == Some("gol") {
                    acc.push(p);
                }
            }
        }
    }
    walk(root, &mut files);
    files.sort();
    files
}

fn check_file(path: &Path) -> Result<(Expect, bool, Vec<String>), String> {
    let source = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let expect = parse_expect(&source)
        .ok_or_else(|| format!("{}: missing // EXPECT: in first 5 lines", path.display()))?;

    // Compile via the same entry point the real compiler uses
    let mut compiler = Compiler::new();
    let sem_prog = match compiler.build_semantic_ir_for(&source, &path.to_string_lossy()) {
        Ok(p) => p,
        Err(e) => {
            // Type error / semantic error — treat as rejection
            return Ok((expect, true, vec![format!("compile error: {}", e)]));
        }
    };

    let cfgs = build_cfgs_from_semantic_program(&sem_prog);
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run_all(&cfgs);

    let has_errors = result.has_errors();
    let messages = result.diagnostics.into_iter().map(|d| d.message).collect();

    Ok((expect, has_errors, messages))
}

#[test]
fn soundness_suite() {
    let files = collect_gol_files();
    assert!(
        !files.is_empty(),
        "no .gol files in tests/soundness/ — copy the 12 files first"
    );

    let mut failures = Vec::new();

    for path in &files {
        match check_file(path) {
            Ok((expect, has_errors, messages)) => {
                let ok = match expect {
                    Expect::Compile => !has_errors,
                    Expect::Reject { .. } => has_errors,
                };
                if !ok {
                    failures.push(format!(
                        "\n{}:\n  EXPECT {:?}\n  got has_errors={}\n  diagnostics:\n    {}\n",
                        path.display(),
                        expect,
                        has_errors,
                        if messages.is_empty() {
                            "(none)".to_string()
                        } else {
                            messages.join("\n    ")
                        }
                    ));
                } else {
                    println!("✓ {} — {:?}", path.display(), expect);
                }
            }
            Err(e) => failures.push(format!("\n{}: harness error: {}", path.display(), e)),
        }
    }

    if !failures.is_empty() {
        panic!(
            "soundness suite failed:{}\n\n{} files checked, {} failures",
            failures.join("\n"),
            files.len(),
            failures.len()
        );
    }
    println!(
        "\nSoundness suite: {} files, all expectations met",
        files.len()
    );
}

#[test]
fn soundness_single_file_debug() {
    if let Ok(path) = std::env::var("SOUNDNESS_FILE") {
        let p = Path::new(&path);
        let (expect, has_errors, messages) = check_file(p).expect("check_file failed");
        println!("File: {}", p.display());
        println!("Expect: {:?}", expect);
        println!("has_errors: {}", has_errors);
        for m in messages {
            println!("  - {}", m);
        }
    }
}

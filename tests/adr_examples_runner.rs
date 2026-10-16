// tests/adr_examples_runner.rs
//
// Test runner for tests/adr_examples/. Walks the directory
// recursively for `.gol` files that carry a `// EXPECT:` marker in
// their first five lines, and runs each through the same stages
// the compiler does:
//
//   1. Compiler::build_semantic_ir_for (lex, parse, type-check, IR)
//   2. build_cfgs_from_semantic_program + DataflowEngine::run_all
//
// Any error at either stage counts as `has_error = true`, which is
// what an `EXPECT: REJECT` file needs. Files without an `EXPECT:`
// marker are skipped rather than treated as failures — the older
// ADRs (0043–0047) use free-form prose comments; those files are
// documentation until they carry the marker.

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
        if rest.starts_with("COMPILE") || rest.starts_with("ACCEPT") {
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
    let root = Path::new("tests/adr_examples");
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

/// Run one file through the full pipeline. Returns `Ok(None)` for
/// files without an `// EXPECT:` marker (skipped). Otherwise
/// returns `Ok(Some((expect, had_error, messages)))`.
fn check_file(path: &Path) -> Result<Option<(Expect, bool, Vec<String>)>, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let expect = match parse_expect(&source) {
        Some(e) => e,
        None => return Ok(None),
    };

    // Stage 1: frontend through IR construction.
    let mut compiler = Compiler::new();
    let sem_prog = match compiler.build_semantic_ir_for(&source, &path.to_string_lossy()) {
        Ok(p) => p,
        Err(e) => return Ok(Some((expect, true, vec![format!("{}", e)]))),
    };

    // Stage 2: CFG-level ownership + region dataflow. This is where
    // the region-escape check (ADR 0045) and the move / init checks
    // live — the AST-level analyzer doesn't see them.
    let cfgs = build_cfgs_from_semantic_program(&sem_prog);
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run_all(&cfgs);
    let has_errors = result.has_errors();
    let messages: Vec<String> = result.diagnostics.into_iter().map(|d| d.message).collect();

    Ok(Some((expect, has_errors, messages)))
}

#[test]
fn adr_examples_suite() {
    let files = collect_gol_files();
    let mut failures = Vec::new();
    let mut run = 0usize;
    let mut skipped = 0usize;

    for path in &files {
        match check_file(path) {
            Ok(None) => {
                skipped += 1;
            }
            Ok(Some((expect, has_error, messages))) => {
                run += 1;
                let ok = match expect {
                    Expect::Compile => !has_error,
                    Expect::Reject { .. } => has_error,
                };
                if !ok {
                    failures.push(format!(
                        "\n{}:\n  EXPECT {:?}\n  got has_error={}\n  diagnostics:\n    {}\n",
                        path.display(),
                        expect,
                        has_error,
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
            "adr_examples suite failed:{}\n\n{} files run, {} skipped, {} failures",
            failures.join("\n"),
            run,
            skipped,
            failures.len()
        );
    }
    println!(
        "\nadr_examples suite: {} files run, {} skipped, all expectations met",
        run, skipped
    );
}

#[test]
fn adr_examples_single_file_debug() {
    if let Ok(path) = std::env::var("ADR_EXAMPLE_FILE") {
        let p = Path::new(&path);
        match check_file(p).expect("check_file failed") {
            Some((expect, has_error, messages)) => {
                println!("File: {}", p.display());
                println!("Expect: {:?}", expect);
                println!("has_error: {}", has_error);
                for m in messages {
                    println!("  - {}", m);
                }
            }
            None => println!("File has no EXPECT: marker: {}", p.display()),
        }
    }
}

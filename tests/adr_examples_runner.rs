// tests/adr_examples_runner.rs
//
// Test runner for tests/adr_examples/. Walks the directory
// recursively for `.gol` files that carry a `// EXPECT:` marker in
// their first five lines, and checks each through
// `Compiler::build_semantic_ir_for`.
//
// Files without a `// EXPECT:` marker are *skipped* rather than
// treated as failures. The older ADRs (0043–0047) use free-form
// prose comments instead of a marker; those files are documentation
// and are not run. ADR 0057 introduced the marker convention; its
// files are the ones this runner exercises.

use std::fs;
use std::path::{Path, PathBuf};

use algol26::compiler::Compiler;

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

/// Run one file. Returns `Ok(None)` when the file has no
/// `// EXPECT:` marker (skipped, not failed). Otherwise returns
/// `Ok(Some((expect, had_error, messages)))`.
fn check_file(path: &Path) -> Result<Option<(Expect, bool, Vec<String>)>, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    let expect = match parse_expect(&source) {
        Some(e) => e,
        None => return Ok(None),
    };

    let mut compiler = Compiler::new();
    match compiler.build_semantic_ir_for(&source, &path.to_string_lossy()) {
        Ok(_sem_prog) => Ok(Some((expect, false, vec![]))),
        Err(e) => Ok(Some((expect, true, vec![format!("{}", e)]))),
    }
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

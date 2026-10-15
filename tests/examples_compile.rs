// tests/examples_compile.rs
//
// Every `.gol` file under `examples/` should pass `algol26 check`.
// Examples are user-facing demonstrations; a broken example is a
// broken first impression.
//
// If a new example intentionally uses a feature the analyzer
// doesn't yet support, move it to `examples/experimental/` and add
// that directory to the SKIP_DIRS list below.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Directories under `examples/` that are skipped by this test.
/// Add a directory here only if every file in it is intentionally
/// non-compiling (work-in-progress, experiments).
const SKIP_DIRS: &[&str] = &[];

fn walk_gol_files(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip directories listed in SKIP_DIRS.
            if let Ok(rel) = path.strip_prefix(root) {
                if SKIP_DIRS.iter().any(|s| rel == Path::new(s)) {
                    continue;
                }
            }
            walk_gol_files(&path, root, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("gol") {
            out.push(path);
        }
    }
}

#[test]
fn all_examples_compile() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let examples_dir = root.join("examples");
    assert!(
        examples_dir.is_dir(),
        "examples/ directory not found at {}",
        examples_dir.display()
    );

    let mut files = Vec::new();
    walk_gol_files(&examples_dir, &examples_dir, &mut files);
    files.sort();

    assert!(!files.is_empty(), "no .gol files found under examples/");

    let bin = env!("CARGO_BIN_EXE_algol26");
    let mut failures: Vec<(PathBuf, String)> = Vec::new();
    let mut passed = 0;

    for path in &files {
        let output = Command::new(bin)
            .arg("check")
            .arg(path)
            .output()
            .expect("failed to run algol26 check");

        if output.status.success() {
            passed += 1;
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let rel = path.strip_prefix(&root).unwrap_or(path);
            failures.push((rel.to_path_buf(), format!("{}\n{}", stdout, stderr)));
        }
    }

    if !failures.is_empty() {
        let mut msg = format!(
            "{} of {} examples under examples/ failed `algol26 check`:\n\n",
            failures.len(),
            files.len()
        );
        for (path, err) in &failures {
            msg.push_str(&format!("════ {} ════\n{}\n\n", path.display(), err));
        }
        panic!("{}", msg);
    }

    eprintln!("examples_compile: {} files passed", passed);
}

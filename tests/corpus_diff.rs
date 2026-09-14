// tests/corpus_diff.rs

use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn corpus_all_programs_match_expected_output() {
    let dir = Path::new("tests/corpus");
    if !dir.exists() {
        return;
    }

    let mut failures: Vec<String> = Vec::new();
    let mut count = 0;

    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("gol") {
            continue;
        }
        count += 1;

        let source = fs::read_to_string(&path).unwrap();
        let known_failure = source.lines().any(|l| l.starts_with("// KNOWN_FAILURE"));

        // Respect a `// BACKEND: interpreter` directive, matching the
        // conformance harness. Default backend is LLVM.
        let backend = source
            .lines()
            .find_map(|l| l.strip_prefix("// BACKEND: "))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "llvm".to_string());

        let expected: Vec<&str> = source
            .lines()
            .filter_map(|l| l.strip_prefix("// OUTPUT: "))
            .collect();
        let expected = expected.join("\n");

        // Compile step (LLVM only — interpreter runs directly).
        let binary = path.with_extension("");
        let compile_ok = if backend == "interpreter" {
            true
        } else {
            let out = Command::new("cargo")
                .args(["run", "--quiet", "--", path.to_str().unwrap()])
                .output()
                .expect("failed to run compiler");
            if !out.status.success() {
                failures.push(format!(
                    "{}: COMPILE FAILED\n{}",
                    path.display(),
                    String::from_utf8_lossy(&out.stderr)
                ));
                false
            } else {
                true
            }
        };

        if !compile_ok {
            if known_failure {
                continue;
            }
            continue;
        }

        // Run step.
        let actual = if backend == "interpreter" {
            let out = Command::new("cargo")
                .args([
                    "run",
                    "--quiet",
                    "--",
                    "run",
                    "--interpreter",
                    path.to_str().unwrap(),
                ])
                .output()
                .expect("failed to run interpreter");
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.starts_with('['))
                .collect::<Vec<_>>()
                .join("\n")
                .trim_end()
                .to_string()
        } else {
            let out = Command::new(&binary)
                .output()
                .expect("failed to execute compiled binary");
            String::from_utf8_lossy(&out.stdout).trim_end().to_string()
        };

        // Verdict.
        if known_failure {
            let still_broken = actual != expected;
            if !still_broken {
                failures.push(format!(
                    "{}: KNOWN_FAILURE now PASSES — remove the marker and celebrate",
                    path.display()
                ));
            }
            continue;
        }

        if actual != expected {
            failures.push(format!(
                "{}: OUTPUT MISMATCH\nexpected:\n{}\nactual:\n{}",
                path.display(),
                expected,
                actual
            ));
        }
    }

    assert!(count > 0, "no .gol files found in tests/corpus/");
    assert!(
        failures.is_empty(),
        "{} of {} corpus programs failed:\n\n{}",
        failures.len(),
        count,
        failures.join("\n\n")
    );
}
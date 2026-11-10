// tests/capabilities/mod.rs
//
// Capability contract test. For each fixture program, compile it
// through every backend and assert the result matches the expected
// matrix. The matrix is loaded from expected_matrix.json and must
// match both `algol26 inspect --capabilities`'s output and the
// runtime behavior of every backend.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, serde::Deserialize)]
struct ExpectedMatrix {
    features: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug)]
struct CapabilityMismatch {
    feature: String,
    backend: String,
    expected: String,
    actual: String,
    detail: String,
}

fn load_matrix() -> ExpectedMatrix {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/cap_impl/expected_matrix.json");
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("expected_matrix.json not found: {}", e));
    serde_json::from_str(&json).expect("expected_matrix.json is malformed")
}

fn fixture_for(feature: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/cap_impl/fixtures")
        .join(format!("{}.gol", feature));
    if !path.exists() {
        panic!("missing fixture for feature `{}`: {}", feature, path.display());
    }
    path
}

fn build_with_backend(fixture: &PathBuf, backend: &str) -> (bool, String, bool) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_algol26"));
    match backend {
        "interpreter" => {
            cmd.arg("run").arg(fixture).arg("--interpreter");
        }
        "llvm" => {
            cmd.arg("build").arg(fixture).arg("--emit-llvm");
        }
        "wasm" => {
            cmd.arg("wasm").arg(fixture);
        }
        other => panic!("unknown backend `{}`", other),
    }
    let output = cmd.output().expect("failed to run algol26");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    let is_panic = combined.contains("panicked at") || combined.contains("internal error:");
    (output.status.success(), combined, is_panic)
}

#[test]
fn capability_matrix_is_accurate() {
    let matrix = load_matrix();
    let mut failures: Vec<CapabilityMismatch> = Vec::new();
    let mut checked = 0;

    for (feature, backends) in &matrix.features {
        let fixture = fixture_for(feature);
        for (backend, expected) in backends {
            checked += 1;
            let (ok, detail, is_panic) = build_with_backend(&fixture, backend);
            let actual = if is_panic { "panicked" } else if ok { "ok" } else { "refused" };
            if actual != expected {
                failures.push(CapabilityMismatch {
                    feature: feature.clone(),
                    backend: backend.clone(),
                    expected: expected.clone(),
                    actual: actual.to_string(),
                    detail: detail.chars().take(300).collect(),
                });
            }
        }
    }

    if !failures.is_empty() {
        eprintln!("\n{} capability mismatches (out of {} checks):", failures.len(), checked);
        for f in &failures {
            eprintln!(
                "  {} x {}: expected {}, got {}\n    {}",
                f.feature, f.backend, f.expected, f.actual, f.detail
            );
        }
        panic!("capability matrix is out of date");
    }

    eprintln!("all {} capability checks passed", checked);
}

#[test]
fn no_feature_is_missing_a_fixture() {
    let matrix = load_matrix();
    for feature in matrix.features.keys() {
        let fixture = fixture_for(feature);
        assert!(fixture.exists(), "missing fixture for feature `{}`", feature);
    }
}

#[test]
fn matrix_matches_inspect_capabilities() {
    let output = Command::new(env!("CARGO_BIN_EXE_algol26"))
        .args(["inspect", "--capabilities"])
        .output()
        .expect("failed to run inspect --capabilities");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let matrix = load_matrix();

    for (feature, backends) in &matrix.features {
        for (backend, expected) in backends {
            let expected_symbol = match expected.as_str() {
                "ok" => "✓",
                "refused" => "✗",
                _ => panic!("bad expected value for {}x{}: {}", feature, backend, expected),
            };
            let line = stdout
                .lines()
                .find(|l| {
                    let t = l.trim_start();
                    t.starts_with(feature)
                        && (t.get(feature.len()..feature.len() + 1) == Some(" ")
                            || t.get(feature.len()..feature.len() + 1) == Some("\t"))
                })
                .unwrap_or_else(|| panic!("feature `{}` not in inspect output", feature));
            assert!(
                line.contains(expected_symbol),
                "matrix says {}x{} is {}, but inspect says: {}",
                feature,
                backend,
                expected,
                line
            );
        }
    }
}

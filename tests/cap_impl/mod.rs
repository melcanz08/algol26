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
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/cap_impl/expected_matrix.json");
    let json = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("expected_matrix.json not found: {}", e));
    serde_json::from_str(&json).expect("expected_matrix.json is malformed")
}

fn fixture_for(feature: &str) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/cap_impl/fixtures")
        .join(format!("{}.gol", feature));
    if !path.exists() {
        panic!(
            "missing fixture for feature `{}`: {}",
            feature,
            path.display()
        );
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
            let actual = if is_panic {
                "panicked"
            } else if ok {
                "ok"
            } else {
                "refused"
            };
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
        eprintln!(
            "\n{} capability mismatches (out of {} checks):",
            failures.len(),
            checked
        );
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
        assert!(
            fixture.exists(),
            "missing fixture for feature `{}`",
            feature
        );
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
                _ => panic!(
                    "bad expected value for {}x{}: {}",
                    feature, backend, expected
                ),
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

/// For every `(feature, backend)` pair the compiler knows about,
/// the fixture MUST compile if and only if the backend's capability
/// set claims to support that feature.
///
/// This is the direct invariant the other three capability tests
/// only verify transitively. Their closure is:
///
///     fixture_compiles == expected_matrix.json
///     expected_matrix.json == inspect --capabilities
///     inspect --capabilities == BackendCapabilities::*().supported
///
/// If all three agree on the **wrong** answer, transitivity passes
/// and the invariant fails. That happened at commit 7804cfe: the
/// LLVM codegen for List.sum/max/min was written and linked, but
/// the `Feature::ListAggregates` flip in `BackendCapabilities::llvm()`
/// silently failed. The matrix stayed `refused`, `inspect` stayed
/// `refused`, and every existing test passed — while the codegen
/// sat as dead code for a full day, hiding eight subsequent bugs.
///
/// This test breaks the transitive chain by comparing
/// `fixture_compiles` directly to `BackendCapabilities::<backend>
/// .supported`, with no matrix in between. It would have failed
/// on 7804cfe with:
///
///     list.agg x llvm: compiles=true claims_supported=false
///         (codegen exists but capability gate is closed)
#[test]
fn codegen_exists_iff_capability_on() {
    use algol26::backends::capabilities::{BackendCapabilities, Feature};

    let matrix = load_matrix();
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0;

    // Feature::name() is the canonical name; matrix keys use it
    // verbatim (asserted by matrix_matches_inspect_capabilities,
    // which finds each matrix key by exact prefix in the inspect
    // output, which itself uses .name()).
    let feature_by_name: HashMap<&str, Feature> =
        Feature::all().iter().map(|f| (f.name(), *f)).collect();

    let interpreter = BackendCapabilities::interpreter();
    let llvm = BackendCapabilities::llvm();
    let wasm = BackendCapabilities::wasm();
    let backends: &[(&str, &BackendCapabilities)] = &[
        ("interpreter", &interpreter),
        ("llvm", &llvm),
        ("wasm", &wasm),
    ];

    for feature_name in matrix.features.keys() {
        let Some(feature) = feature_by_name.get(feature_name.as_str()).copied() else {
            // Matrix has a key not in Feature::all(). That's a
            // separate failure class — the matrix has drifted from
            // the enum. Report it explicitly rather than skipping.
            failures.push(format!(
                "matrix key `{}` has no matching Feature::* variant",
                feature_name
            ));
            continue;
        };

        let fixture = fixture_for(feature_name);

        for (backend_name, caps) in backends {
            checked += 1;

            let (ok, _detail, is_panic) = build_with_backend(&fixture, backend_name);
            let compiles = ok && !is_panic;
            let claims_supported = caps.supported.contains(&feature);

            if compiles != claims_supported {
                // Explain the direction. Both are bugs, but the
                // diagnosis differs:
                //   compiles && !claims  -> dead codegen; the
                //     capability flip was forgotten. This is
                //     what 7804cfe triggered.
                //   !compiles && claims  -> the capability is
                //     lying; the codegen is broken or missing.
                //     This is what `union`-on-LLVM looked like
                //     before the fix, and what `StringOps`-on-WASM
                //     looked like before the honest rollback.
                let direction = if compiles {
                    "codegen exists but capability gate is closed \
                     (add the feature to BackendCapabilities::<backend>.supported, \
                     or remove the codegen path)"
                } else {
                    "capability claims support but codegen is broken \
                     (fix the codegen, or remove the feature from \
                     BackendCapabilities::<backend>.supported)"
                };
                failures.push(format!(
                    "{} x {}: compiles={} claims_supported={}\n    {}",
                    feature_name, backend_name, compiles, claims_supported, direction
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} capability/codegen mismatch(es) out of {} checks:\n\n{}",
            failures.len(),
            checked,
            failures.join("\n\n")
        );
    }

    eprintln!(
        "all {} capability/codegen checks agree (codegen exists iff capability is on)",
        checked
    );
}

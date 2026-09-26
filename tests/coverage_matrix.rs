//! The feature × backend coverage matrix.
//!
//! One row per language feature, one column per backend. The values
//! describe what the backend *actually does* for that feature, not
//! what we wish it did.
//!
//! This is the single place to look when answering "does ALGOL26
//! support feature X on backend Y?". It was assembled from the
//! feature contracts under `docs/features/` and the capability tests
//! under `src/backends/capabilities/tests.rs`.
//!
//! ## The four backend values
//!
//! - [`Support::Full`] — works end-to-end.
//! - [`Support::Refused`] — the capability check refuses any
//!   program using this feature before lowering runs.
//! - [`Support::Partial`] — works for some operations but not
//!   others. `notes` must explain the boundary.
//! - [`Support::Unknown`] — no test currently pins this. `notes`
//!   must say what verification is needed.
//!
//! ## The `conformance_dir` field
//!
//! Points at the subdirectory of `tests/conformance/valid/` that
//! exercises this feature end-to-end. `None` means no fixture exists
//! yet — and every `None` must have a non-empty `notes` explaining
//! why. `tests/conformance_coverage.rs` enforces both directions.
//!
//! ## The `refusal_tests` field
//!
//! Names the capability tests in `src/backends/capabilities/tests.rs`
//! that pin this row's `Refused` claims. Every row whose
//! `interpreter`/`llvm`/`wasm` is `Refused` must name at least one
//! test — or be listed in `KNOWN_MISSING_REFUSALS` with a note
//! explaining the gap. Every `*_rejects_*` test in the capability
//! file must be claimed by some row. The three tests at the bottom
//! enforce both directions.
//!
//! ## Keeping this in sync
//!
//! When a feature's backend support changes:
//! 1. Update the corresponding `docs/features/<feature>.md` contract
//! 2. Update this matrix
//! 3. Update the corresponding capability test in
//!    `src/backends/capabilities/tests.rs`
//! 4. If the change is a new differential test, add it to
//!    `tests/differential/differential_true.rs`

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    Full,
    Refused,
    Partial,
    Unknown,
}

pub struct FeatureRow {
    pub name: &'static str,
    /// Subdirectory of `tests/conformance/valid/` that exercises
    /// this feature. `None` if no fixture exists yet.
    pub conformance_dir: Option<&'static str>,
    pub interpreter: Support,
    pub llvm: Support,
    pub wasm: Support,
    /// Names of capability tests in `src/backends/capabilities/tests.rs`
    /// that pin this row's `Refused` claims. Required when any
    /// backend is `Refused` and the feature is not in
    /// `KNOWN_MISSING_REFUSALS`.
    pub refusal_tests: &'static [&'static str],
    /// Required when any backend is `Partial`/`Unknown`, when
    /// `conformance_dir` is `None`, or when the row is in
    /// `KNOWN_MISSING_REFUSALS`.
    pub notes: &'static str,
}

/// Features whose matrix says `Refused` on some backend but which do
/// not yet have a capability test pinning that refusal. Every entry
/// here is a gap that should be closed. Removing a name from this
/// list requires adding the corresponding test to
/// `src/backends/capabilities/tests.rs` and naming it in the row's
/// `refusal_tests`.
pub const KNOWN_MISSING_REFUSALS: &[&str] = &[
    // Unfinished feature. No capability check exists because there
    // is no `Feature::Range` variant, and no backend supports ranges
    // end-to-end. Stays here until the feature is either implemented
    // or removed.
    "range",
];

pub const MATRIX: &[FeatureRow] = &[
    // ─── arithmetic ───
    FeatureRow {
        name: "int_arithmetic",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "float_arithmetic",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "mixed_arithmetic",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "comparison_operators",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    // ─── strings ───
    FeatureRow {
        name: "string_literal",
        conformance_dir: Some("strings"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "string_length",
        conformance_dir: Some("strings"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "string_concat",
        conformance_dir: Some("strings"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "string_substring",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
        refusal_tests: &[],
        notes: "No dedicated conformance fixture. substring exercised via \
                strings.gol but not asserted. Also unverified: LLVM/WASM lowering.",
    },
    FeatureRow {
        name: "string_case_conversion",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
        refusal_tests: &[],
        notes: "No dedicated conformance fixture for to_upper/to_lower. \
                LLVM/WASM lowering unverified.",
    },
    // ─── lists ───
    FeatureRow {
        name: "list_literal",
        conformance_dir: Some("lists"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "list_length",
        conformance_dir: Some("lists"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "list_iteration",
        conformance_dir: Some("lists"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "list_indexing",
        conformance_dir: Some("lists"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "list_print",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Unknown,
        refusal_tests: &["llvm_rejects_list_print"],
        notes: "LLVM refuses `print(list)` via capability check. No dedicated \
                conformance fixture. WASM support unverified.",
    },
    FeatureRow {
        name: "list_sum_max_min",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
        refusal_tests: &[],
        notes: "No conformance fixture. No LLVM/WASM lowering seen in codegen; \
                verify whether these builtins are lowered or refused.",
    },
    // ─── option / result ───
    FeatureRow {
        name: "option",
        conformance_dir: Some("option"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &["llvm_rejects_option_values", "wasm_rejects_option_values"],
        notes: "",
    },
    FeatureRow {
        name: "result",
        conformance_dir: Some("result"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &["llvm_rejects_result_values", "wasm_rejects_result_values"],
        notes: "",
    },
    FeatureRow {
        name: "try_catch",
        conformance_dir: Some("result"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &["llvm_rejects_result_values", "wasm_rejects_result_values"],
        notes: "",
    },
    // ─── ownership ───
    FeatureRow {
        name: "borrow",
        conformance_dir: Some("ownership"),
        interpreter: Support::Refused,
        llvm: Support::Full,
        wasm: Support::Refused,
        refusal_tests: &["interpreter_rejects_references", "wasm_rejects_references"],
        notes: "Interpreter and WASM refuse via `Feature::References` in the \
                capability check. LLVM lowers the four reference operations \
                in llvm_codegen/value.rs and the reference types in types.rs.",
    },
    FeatureRow {
        name: "region",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Partial,
        refusal_tests: &[],
        notes: "LLVM supports regions: RegionEnter/RegionExit push/pop a \
                frame and emit guarded frees for the allocations made \
                inside. WASM reuses the same IRCodeGen, so empty regions \
                work; regions that allocate are refused because WASM's \
                capability check refuses RawMemory.",
    },
    FeatureRow {
        name: "alloc_free",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Unknown,
        refusal_tests: &[],
        notes: "No conformance fixture; alloc/free exercised in interpreter \
                unit tests. WASM has malloc/free in host.js but no capability test.",
    },
    // ─── concurrency ───
    FeatureRow {
        name: "channel",
        conformance_dir: None,
        interpreter: Support::Refused,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &[
            "interpreter_rejects_channels",
            "wasm_rejects_channels",
            "llvm_rejects_channels",
        ],
        notes: "No conformance fixture. All three backends refuse channels at \
                the capability boundary. No backend models channels yet — the \
                feature is specified but unimplemented end-to-end.",
    },
    FeatureRow {
        name: "spawn",
        conformance_dir: Some("concurrency"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &["llvm_rejects_spawn", "wasm_rejects_spawn"],
        notes: "Interpreter runs sequentially; LLVM and WASM refuse (both pinned).",
    },
    FeatureRow {
        name: "parallel",
        conformance_dir: Some("concurrency"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &["llvm_rejects_parallel", "wasm_rejects_parallel"],
        notes: "",
    },
    // ─── compile-time-only ───
    FeatureRow {
        name: "trait",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "No conformance fixture; trait tests are in tests/semantics/. \
                Resolved before IR construction, so every backend supports.",
    },
    FeatureRow {
        name: "generic",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "No conformance fixture; generics exercised in examples/generics/. \
                Monomorphized before IR construction.",
    },
    FeatureRow {
        name: "method_call",
        conformance_dir: Some("method_call"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "Desugared to a function call before IR construction.",
    },
    FeatureRow {
        name: "defer",
        conformance_dir: Some("defer"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "Lowered before IR construction.",
    },
    // ─── FFI ───
    FeatureRow {
        name: "ffi",
        conformance_dir: None,
        interpreter: Support::Refused,
        llvm: Support::Full,
        wasm: Support::Refused,
        refusal_tests: &["interpreter_rejects_ffi", "wasm_rejects_ffi"],
        notes: "No conformance fixture. Only LLVM links C symbols; both \
                interpreter and WASM refuse (both pinned).",
    },
    FeatureRow {
        name: "records",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &["llvm_rejects_records", "wasm_rejects_records"],
        notes: "No conformance fixture yet. Records are interpreter-only \
                (ADR 0024). LLVM and WASM refuse `rec` programs at the \
                capability check (`Feature::Records`) rather than at \
                codegen. A fixture would need `--interpreter`; the \
                harness does not currently support per-fixture backend \
                selection.",
    },
    // ─── unsafe / range ───
    FeatureRow {
        name: "unsafe",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "Parsed but not enforced (see docs/features/unsafe.md). Currently \
                a no-op block, so every backend accepts it and no dedicated \
                fixture is meaningful.",
    },
    FeatureRow {
        name: "range",
        conformance_dir: None,
        interpreter: Support::Refused,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &[],
        notes: "Unfinished feature (see docs/features/range.md). No backend \
                supports it end-to-end, so no fixture is possible.",
    },
    FeatureRow {
        name: "command_line_args",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &["llvm_rejects_args", "wasm_rejects_args"],
        notes: "No conformance fixture — `args()` returns the process's \
                command-line arguments, which cannot be reproduced by a \
                static fixture. The interpreter returns the process's \
                arguments. LLVM and WASM refuse via the capability check \
                (`Feature::CommandLineArgs`); neither has a mechanism to \
                expose C `argc`/`argv` to the language's `main`.",
    },
];

// ─────────────────────────────────────────────────────────────────────
// Consistency tests
// ─────────────────────────────────────────────────────────────────────

use std::collections::HashSet;
use std::path::PathBuf;

#[test]
fn feature_names_are_unique() {
    let mut seen = HashSet::new();
    for row in MATRIX {
        assert!(
            seen.insert(row.name),
            "duplicate feature name in MATRIX: `{}`",
            row.name
        );
    }
}

#[test]
fn feature_names_are_snake_case() {
    for row in MATRIX {
        assert!(
            !row.name.is_empty(),
            "MATRIX contains an empty feature name"
        );
        assert!(
            row.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "feature name `{}` is not snake_case (lowercase letters, digits, underscores only)",
            row.name
        );
        assert!(
            !row.name.starts_with('_') && !row.name.ends_with('_'),
            "feature name `{}` has a leading or trailing underscore",
            row.name
        );
    }
}

#[test]
fn partial_and_unknown_supports_have_notes() {
    for row in MATRIX {
        let has_special = matches!(row.interpreter, Support::Partial | Support::Unknown)
            || matches!(row.llvm, Support::Partial | Support::Unknown)
            || matches!(row.wasm, Support::Partial | Support::Unknown);
        if has_special {
            assert!(
                !row.notes.trim().is_empty(),
                "feature `{}` has a Partial or Unknown backend but no notes",
                row.name
            );
        }
    }
}

#[test]
fn missing_conformance_dir_has_notes() {
    for row in MATRIX {
        if row.conformance_dir.is_none() {
            assert!(
                !row.notes.trim().is_empty(),
                "feature `{}` has no conformance_dir but no notes explaining why",
                row.name
            );
        }
    }
}

#[test]
fn every_feature_works_somewhere() {
    const KNOWN_UNFINISHED: &[&str] = &["range", "channel"];

    for row in MATRIX {
        if KNOWN_UNFINISHED.contains(&row.name) {
            continue;
        }
        let supported_anywhere = matches!(row.interpreter, Support::Full | Support::Partial)
            || matches!(row.llvm, Support::Full | Support::Partial)
            || matches!(row.wasm, Support::Full | Support::Partial);
        assert!(
            supported_anywhere,
            "feature `{}` is Refused or Unknown on every backend — either \
             the feature is unfinished (add to KNOWN_UNFINISHED) or the \
             matrix is wrong",
            row.name
        );
    }
}

#[test]
fn matrix_is_not_trivially_empty() {
    assert!(
        MATRIX.len() >= 20,
        "MATRIX has only {} entries; the feature set should be >= 20",
        MATRIX.len()
    );
}

// ─────────────────────────────────────────────────────────────────────
// Capability-test enforcement (Tier 5.2)
// ─────────────────────────────────────────────────────────────────────

/// Read the capability tests source at runtime. Using `std::fs`
/// rather than `include_str!` avoids any ambiguity about how the
/// path resolves when this file is included via `#[path]` from
/// `conformance_coverage.rs`.
fn capability_tests_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/backends/capabilities/tests.rs");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e))
}

/// Scan the capability tests source for `fn <name>` declarations.
/// Returns the function names in declaration order.
fn declared_test_names(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim_start();
        let Some(after_fn) = trimmed.strip_prefix("fn ") else {
            continue;
        };
        let end = after_fn
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after_fn.len());
        if end > 0 {
            names.push(after_fn[..end].to_string());
        }
    }
    names
}

#[test]
fn refusal_tests_are_declared() {
    for row in MATRIX {
        let has_refusal = matches!(row.interpreter, Support::Refused)
            || matches!(row.llvm, Support::Refused)
            || matches!(row.wasm, Support::Refused);
        if !has_refusal {
            continue;
        }
        if KNOWN_MISSING_REFUSALS.contains(&row.name) {
            continue;
        }
        assert!(
            !row.refusal_tests.is_empty(),
            "feature `{}` claims a Refused backend but declares no refusal_tests. \
             Either add a capability test and name it in this row's refusal_tests, \
             or add the feature to KNOWN_MISSING_REFUSALS.",
            row.name
        );
    }
}

#[test]
fn refusal_tests_exist() {
    let src = capability_tests_source();
    let declared: HashSet<String> = declared_test_names(&src).into_iter().collect();

    for row in MATRIX {
        for test_name in row.refusal_tests {
            assert!(
                declared.contains(*test_name),
                "feature `{}` names refusal test `{}` but it does not exist in \
                 src/backends/capabilities/tests.rs",
                row.name,
                test_name
            );
        }
    }
}

#[test]
fn no_unclaimed_refusal_tests() {
    let src = capability_tests_source();
    let declared = declared_test_names(&src);

    for test_name in declared {
        if !test_name.contains("_rejects_") {
            continue;
        }
        let claimed = MATRIX
            .iter()
            .any(|row| row.refusal_tests.contains(&test_name.as_str()));
        assert!(
            claimed,
            "capability test `{}` is not claimed by any matrix row — \
             either add it to a row's refusal_tests or delete it",
            test_name
        );
    }
}

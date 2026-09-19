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
//! ## Keeping this in sync
//!
//! When a feature's backend support changes:
//! 1. Update the corresponding `docs/features/<feature>.md` contract
//! 2. Update this matrix
//! 3. If the change is a capability check flip, update
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
    /// Required when any backend is `Partial`/`Unknown`, or when
    /// `conformance_dir` is `None`.
    pub notes: &'static str,
}

pub const MATRIX: &[FeatureRow] = &[
    // ─── arithmetic ───
    FeatureRow {
        name: "int_arithmetic",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "float_arithmetic",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "mixed_arithmetic",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "comparison_operators",
        conformance_dir: Some("arithmetic"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    // ─── strings ───
    FeatureRow {
        name: "string_literal",
        conformance_dir: Some("strings"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "string_length",
        conformance_dir: Some("strings"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "string_concat",
        conformance_dir: Some("strings"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "string_substring",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
        notes: "No dedicated conformance fixture. substring exercised via \
                strings.gol but not asserted. Also unverified: LLVM/WASM lowering.",
    },
    FeatureRow {
        name: "string_case_conversion",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
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
        notes: "",
    },
    FeatureRow {
        name: "list_length",
        conformance_dir: Some("lists"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "list_iteration",
        conformance_dir: Some("lists"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "list_indexing",
        conformance_dir: Some("lists"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "list_print",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Unknown,
        notes: "LLVM refuses `print(list)` via capability check. No dedicated \
                conformance fixture. WASM support unverified.",
    },
    FeatureRow {
        name: "list_sum_max_min",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
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
        notes: "",
    },
    FeatureRow {
        name: "result",
        conformance_dir: Some("result"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "",
    },
    FeatureRow {
        name: "try_catch",
        conformance_dir: Some("result"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "",
    },
    // ─── ownership ───
    FeatureRow {
        name: "borrow",
        conformance_dir: Some("ownership"),
        interpreter: Support::Refused,
        llvm: Support::Full,
        wasm: Support::Unknown,
        notes: "Interpreter refuses `&x` / `*r` with EvalError::Unsupported. \
                WASM support unverified.",
    },
    FeatureRow {
        name: "region",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Unknown,
        notes: "No conformance fixture; corpus_30/31 exercise regions but are \
                not conformance fixtures. LLVM treats RegionEnter/Exit as \
                no-ops, capability refuses programs that alloc. WASM unverified.",
    },
    FeatureRow {
        name: "alloc_free",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Unknown,
        notes: "No conformance fixture; alloc/free exercised in interpreter unit \
                tests. WASM has malloc/free in host.js but no capability test.",
    },
    // ─── concurrency ───
    FeatureRow {
        name: "channel",
        conformance_dir: None,
        interpreter: Support::Partial,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "No conformance fixture. Interpreter has channel instructions as \
                silent no-ops (see docs/features/channel.md). Should become \
                Refused until a real queue is implemented.",
    },
    FeatureRow {
        name: "spawn",
        conformance_dir: Some("concurrency"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "Interpreter runs sequentially; LLVM and WASM refuse.",
    },
    FeatureRow {
        name: "parallel",
        conformance_dir: Some("concurrency"),
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "",
    },
    // ─── compile-time-only ───
    FeatureRow {
        name: "trait",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "No conformance fixture; trait tests are in tests/semantics/. \
                Resolved before IR construction, so every backend supports.",
    },
    FeatureRow {
        name: "generic",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "No conformance fixture; generics exercised in examples/generics/. \
                Monomorphized before IR construction.",
    },
    FeatureRow {
        name: "method_call",
        conformance_dir: Some("method_call"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "Desugared to a function call before IR construction.",
    },
    FeatureRow {
        name: "defer",
        conformance_dir: Some("defer"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "Lowered before IR construction.",
    },
    // ─── FFI ───
    FeatureRow {
        name: "ffi",
        conformance_dir: None,
        interpreter: Support::Refused,
        llvm: Support::Full,
        wasm: Support::Refused,
        notes: "No conformance fixture; FFI exercised in examples/ffi/ and \
                tests/conformance/valid/ffi_math.gol (not currently present). \
                Only LLVM links C symbols; interpreter and WASM refuse.",
    },
    // ─── unsafe / range ───
    FeatureRow {
        name: "unsafe",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
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
        notes: "Unfinished feature (see docs/features/range.md). No backend \
                supports it end-to-end, so no fixture is possible.",
    },
];

// ─────────────────────────────────────────────────────────────────────
// Consistency tests
// ─────────────────────────────────────────────────────────────────────

use std::collections::HashSet;

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
    // Features that are known to be unfinished. If one of these starts
    // working on a backend, remove it from the list.
    const KNOWN_UNFINISHED: &[&str] = &["range"];

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

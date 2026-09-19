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
//! ## The four values
//!
//! - [`Support::Full`] — works end-to-end. A program using this
//!   feature compiles and runs on the backend, producing the same
//!   observable behavior as the interpreter (or is a compile-time
//!   feature with no runtime effect).
//! - [`Support::Refused`] — the capability check refuses any
//!   program using this feature before lowering runs. This is the
//!   correct fail-closed behavior for features the backend cannot
//!   support.
//! - [`Support::Partial`] — works for some operations but not
//!   others. The `notes` field must explain the boundary.
//! - [`Support::Unknown`] — no test currently pins this. The
//!   `notes` field must say what verification is needed.
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
//!
//! The three tests at the bottom of this file enforce internal
//! consistency (unique names, snake_case, notes on Unknown). They
//! do not yet verify the claims against the actual backends — that
//! is a later step.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    Full,
    Refused,
    Partial,
    Unknown,
}

pub struct FeatureRow {
    pub name: &'static str,
    pub interpreter: Support,
    pub llvm: Support,
    pub wasm: Support,
    /// Required when any backend is `Partial` or `Unknown`. Explains
    /// the boundary or what verification is missing.
    pub notes: &'static str,
}

pub const MATRIX: &[FeatureRow] = &[
    // ─── arithmetic ───
    FeatureRow {
        name: "int_arithmetic",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "float_arithmetic",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "mixed_arithmetic",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "comparison_operators",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    // ─── strings ───
    FeatureRow {
        name: "string_literal",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "string_length",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "string_concat",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "string_substring",
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
        notes: "No differential test confirms substring lowering on LLVM or WASM. \
                Verify with a `String.substring(s, 0, 2)` fixture before marking Full.",
    },
    FeatureRow {
        name: "string_case_conversion",
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
        notes: "String.to_upper / to_lower have no confirmed LLVM or WASM lowering.",
    },
    // ─── lists ───
    FeatureRow {
        name: "list_literal",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "list_length",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "list_iteration",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "list_indexing",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "",
    },
    FeatureRow {
        name: "list_print",
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Unknown,
        notes: "LLVM refuses `print(list)` via capability check. WASM support \
                unverified; add a capability test.",
    },
    FeatureRow {
        name: "list_sum_max_min",
        interpreter: Support::Full,
        llvm: Support::Unknown,
        wasm: Support::Unknown,
        notes: "No LLVM/WASM lowering seen in codegen; verify whether these \
                builtins are lowered or refused.",
    },
    // ─── option / result ───
    FeatureRow {
        name: "option",
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "",
    },
    FeatureRow {
        name: "result",
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "",
    },
    FeatureRow {
        name: "try_catch",
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "",
    },
    // ─── ownership ───
    FeatureRow {
        name: "borrow",
        interpreter: Support::Refused,
        llvm: Support::Full,
        wasm: Support::Unknown,
        notes: "Interpreter refuses `&x` / `*r` with EvalError::Unsupported. \
                WASM support unverified.",
    },
    FeatureRow {
        name: "region",
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Unknown,
        notes: "LLVM treats RegionEnter/Exit as no-ops, capability refuses \
                programs that alloc. WASM unverified.",
    },
    FeatureRow {
        name: "alloc_free",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Unknown,
        notes: "WASM has malloc/free in host.js but no capability test.",
    },
    // ─── concurrency ───
    FeatureRow {
        name: "channel",
        interpreter: Support::Partial,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "Interpreter has channel instructions as silent no-ops (see \
                docs/features/channel.md). Should become Refused until a \
                real queue is implemented.",
    },
    FeatureRow {
        name: "spawn",
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "Interpreter runs sequentially; LLVM and WASM refuse.",
    },
    FeatureRow {
        name: "parallel",
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "",
    },
    // ─── compile-time-only ───
    FeatureRow {
        name: "trait",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "Resolved before IR construction; every backend supports.",
    },
    FeatureRow {
        name: "generic",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "Monomorphized before IR construction.",
    },
    FeatureRow {
        name: "method_call",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "Desugared to a function call before IR construction.",
    },
    FeatureRow {
        name: "defer",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "Lowered before IR construction.",
    },
    // ─── FFI ───
    FeatureRow {
        name: "ffi",
        interpreter: Support::Refused,
        llvm: Support::Full,
        wasm: Support::Refused,
        notes: "Only LLVM links C symbols; interpreter and WASM refuse.",
    },
    // ─── unsafe / range ───
    FeatureRow {
        name: "unsafe",
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        notes: "Parsed but not enforced (see docs/features/unsafe.md). \
                Currently a no-op block, so every backend accepts it.",
    },
    FeatureRow {
        name: "range",
        interpreter: Support::Refused,
        llvm: Support::Refused,
        wasm: Support::Refused,
        notes: "Unfinished feature (see docs/features/range.md). No \
                backend supports it end-to-end.",
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
    // Sanity guard: if the matrix ever gets reset to empty, catch it.
    assert!(
        MATRIX.len() >= 20,
        "MATRIX has only {} entries; the feature set should be >= 20",
        MATRIX.len()
    );
}

//! Feature maturity ladder, derived from the coverage matrix.
//!
//! The observer's document proposed a ten-stage maturity ladder per
//! feature (Experimental → Parsed → Type-checked → ... → Stable).
//! Most of those stages are already implied by the fields on
//! `coverage_matrix::FeatureRow`. Rather than duplicate them, this
//! file derives a single maturity label from the existing data and
//! asserts it against a second, hand-written table.
//!
//! The mirror table is deliberate: adding a feature to `MATRIX`
//! requires adding it to `EXPECTED_MATURITY`. Changing a backend
//! from `Refused` to `Full` requires updating both the row and the
//! expected maturity. Two-place updates catch drift the same way
//! `docs/features/` catches it for language semantics.

use std::collections::HashSet;

#[path = "coverage_matrix.rs"]
mod coverage_matrix;

use coverage_matrix::{FeatureRow, Support, MATRIX};

/// A single label for "how far along is this feature?".
///
/// Distinct from `Support` (which is per-backend). This is a
/// summary of the row as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Maturity {
    /// No backend supports the feature end-to-end.
    Unfinished,
    /// Works on the interpreter only.
    InterpreterOnly,
    /// Works on LLVM only.
    LlvmOnly,
    /// Works on WASM only. Not currently used — WASM reuses the same
    /// `IRCodeGen` as LLVM, so a feature supported on WASM is
    /// supported on LLVM too. Kept for match totality and in case a
    /// future WASM-specific feature (e.g. bulk memory operations)
    /// breaks that symmetry.
    WasmOnly,
    /// Works on the interpreter and LLVM.
    InterpreterAndLlvm,
    /// Works on the interpreter and WASM (currently unused; kept for
    /// completeness).
    InterpreterAndWasm,
    /// Works on all three backends and has a conformance fixture.
    AllBackends,
    /// Works on every backend and has no dedicated conformance
    /// fixture. Two cases:
    ///
    /// - **No runtime behavior**: the feature is resolved before IR
    ///   construction (`trait`, `generic`, `defer`, `method_call`) or
    ///   is a runtime no-op (`unsafe`).
    /// - **Exercised by other tests**: the feature has real runtime
    ///   behavior but is covered by unit and differential tests
    ///   rather than a `.gol` conformance fixture (`region`).
    ///
    /// A feature in this category is a candidate for later promotion
    /// to `AllBackends` by adding a fixture. The distinction between
    /// `AllBackends` and `Universal` is *only* whether a fixture
    /// exists, not whether the feature is genuinely universal.
    Universal,
}

/// Derive the maturity label for a row from its existing fields.
pub fn maturity_of(row: &FeatureRow) -> Maturity {
    let interp = matches!(row.interpreter, Support::Full | Support::Partial);
    let llvm = matches!(row.llvm, Support::Full | Support::Partial);
    let wasm = matches!(row.wasm, Support::Full | Support::Partial);

    // Universal: full support everywhere, no fixture needed.
    if interp && llvm && wasm && row.conformance_dir.is_none() {
        return Maturity::Universal;
    }

    match (interp, llvm, wasm) {
        (false, false, false) => Maturity::Unfinished,
        (true, false, false) => Maturity::InterpreterOnly,
        (false, true, false) => Maturity::LlvmOnly,
        (true, true, false) => Maturity::InterpreterAndLlvm,
        (true, false, true) => Maturity::InterpreterAndWasm,
        (true, true, true) => Maturity::AllBackends,
        // LLVM+WASM without interpreter: not currently used, but
        // degrade to LlvmOnly rather than panic.
        (false, true, true) => Maturity::LlvmOnly,
        (false, false, true) => Maturity::WasmOnly,
    }
}

/// The expected maturity for every row in `MATRIX`, written down
/// explicitly. Adding a feature to the matrix requires adding an
/// entry here; changing a row's backend support requires updating
/// its entry.
pub const EXPECTED_MATURITY: &[(&str, Maturity)] = &[
    // arithmetic — full on every backend
    ("int_arithmetic", Maturity::AllBackends),
    ("float_arithmetic", Maturity::AllBackends),
    ("mixed_arithmetic", Maturity::AllBackends),
    ("comparison_operators", Maturity::AllBackends),
    // strings
    ("string_literal", Maturity::AllBackends),
    ("string_length", Maturity::AllBackends),
    ("string_concat", Maturity::AllBackends),
    ("string_substring", Maturity::InterpreterOnly),
    ("string_case_conversion", Maturity::InterpreterOnly),
    // lists
    ("list_literal", Maturity::AllBackends),
    ("list_length", Maturity::AllBackends),
    ("list_iteration", Maturity::AllBackends),
    ("list_indexing", Maturity::AllBackends),
    ("list_print", Maturity::InterpreterOnly),
    ("list_sum_max_min", Maturity::InterpreterOnly),
    // option / result
    ("option", Maturity::InterpreterOnly),
    ("result", Maturity::InterpreterOnly),
    ("try_catch", Maturity::InterpreterOnly),
    // ownership
    ("borrow", Maturity::LlvmOnly),
    ("region", Maturity::Universal),
    ("alloc_free", Maturity::InterpreterAndLlvm),
    // concurrency
    ("channel", Maturity::InterpreterOnly),
    ("spawn", Maturity::InterpreterOnly),
    ("parallel", Maturity::InterpreterOnly),
    // compile-time-only
    ("trait", Maturity::Universal),
    ("generic", Maturity::Universal),
    ("method_call", Maturity::AllBackends),
    ("defer", Maturity::AllBackends),
    // FFI
    ("ffi", Maturity::LlvmOnly),
    // unsafe / range
    ("unsafe", Maturity::Universal),
    ("range", Maturity::Unfinished),
];

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[test]
fn every_row_derives_a_maturity() {
    // Smoke test: the derivation never panics, and every row
    // produces one of the enum variants.
    for row in MATRIX {
        let m = maturity_of(row);
        let _ = format!("{} => {:?}", row.name, m);
    }
}

#[test]
fn unfinished_only_when_no_backend_supports() {
    for row in MATRIX {
        let m = maturity_of(row);
        let all_refused = !matches!(row.interpreter, Support::Full | Support::Partial)
            && !matches!(row.llvm, Support::Full | Support::Partial)
            && !matches!(row.wasm, Support::Full | Support::Partial);
        assert_eq!(
            m == Maturity::Unfinished,
            all_refused,
            "feature `{}` has maturity {:?} but backends are {:?}/{:?}/{:?}",
            row.name,
            m,
            row.interpreter,
            row.llvm,
            row.wasm
        );
    }
}

#[test]
fn no_conformance_dir_is_either_universal_or_documented() {
    for row in MATRIX {
        if row.conformance_dir.is_some() {
            continue;
        }
        let m = maturity_of(row);
        let is_documented = !row.notes.trim().is_empty();
        assert!(
            m == Maturity::Universal || is_documented,
            "feature `{}` has no conformance_dir, maturity {:?}, and no notes",
            row.name,
            m
        );
    }
}

#[test]
fn maturity_matches_expectation() {
    for (name, expected) in EXPECTED_MATURITY {
        let row = MATRIX
            .iter()
            .find(|r| r.name == *name)
            .unwrap_or_else(|| panic!("feature `{}` is in EXPECTED_MATURITY but not MATRIX", name));
        let actual = maturity_of(row);
        assert_eq!(
            actual, *expected,
            "feature `{}`: expected maturity {:?}, derived {:?}",
            name, expected, actual
        );
    }
}

#[test]
fn every_row_has_an_expected_maturity() {
    let expected_names: HashSet<&str> = EXPECTED_MATURITY.iter().map(|(n, _)| *n).collect();
    for row in MATRIX {
        assert!(
            expected_names.contains(row.name),
            "matrix row `{}` has no entry in EXPECTED_MATURITY — add one",
            row.name
        );
    }
}

#[test]
fn expected_maturity_has_no_orphans() {
    let matrix_names: HashSet<&str> = MATRIX.iter().map(|r| r.name).collect();
    for (name, _) in EXPECTED_MATURITY {
        assert!(
            matrix_names.contains(name),
            "EXPECTED_MATURITY names `{}` but no such row is in MATRIX",
            name
        );
    }
}

// tests/status_sync.rs
//
// Verifies that docs/STATUS.md's feature matrix agrees with the
// capability matrix in src/backends/capabilities/mod.rs.
//
// The docs-vs-reality drift found on 2026-10-18 (records, Option,
// List.append, match-with-bindings, String.*, Int.to_string all
// marked refused on LLVM but actually supported) had a single root
// cause: features flip in BackendCapabilities when they land, the
// capability tests keep passing (they check the matrix, not real
// compilation), and STATUS.md is never updated. The corpus doesn't
// cover the new feature on the native backend, so nothing forces
// the docs update.
//
// This test prevents that class of bug by making the two sources
// of truth mechanically checked against each other. Any feature
// that flips in BackendCapabilities::llvm().supported must have
// its STATUS.md row updated in the same commit.

use std::path::PathBuf;

use algol26::backends::capabilities::Feature;

// ─── Mapping from STATUS.md row to Feature ─────────────────────────
//
// Each entry is (status_md_row_identifier, features_that_row_covers).
// A row can map to multiple Feature variants (e.g. String.* covers
// Feature::StringOps, but the row also implicitly requires that
// the LLVM column match `supported.contains(&StringOps)`).
//
// Key is a distinctive substring of the row; the full row is matched
// by prefix. When a feature is added to Feature::all() but not to
// this table, the test fails with "unmapped feature" — that's the
// maintenance hook: adding a feature means adding a row to
// STATUS.md and an entry here.
const STATUS_ROW_TO_FEATURES: &[(&str, &[Feature])] = &[
    // (row substring, [features]) — LLVM column must be
    // "supported" iff every listed feature is supported.

    // ─── Language-level rows: no backend capability check ───
    // These describe what the language can express, not what any
    // particular backend can lower. The parser/analyzer columns
    // verify the language-level acceptance; the LLVM column is
    // out-of-scope for these rows.
    ("`while`", &[]),
    ("`for ... in`", &[]),
    ("`if` / `else`", &[]),
    ("`break` / `continue`", &[]),
    ("`defer`", &[]),
    ("Functions / procedures", &[]),
    ("Return values", &[]),
    ("Bare call statements", &[]),
    ("Lists + indexing", &[]),
    ("Strings", &[]),
    ("`String.length`", &[]),
    ("`Math.*`", &[]),
    // `match` (literal patterns) and `match` (binding patterns) are
    // language-level; the corpora pin them end-to-end. Not gated
    // by a Feature variant because the Feature enum models
    // runtime capabilities, not pattern kinds.
    ("`match` (literal", &[]),
    ("`match` (binding", &[]),
    ("Traits + impls", &[]),
    ("Visibility", &[]),
    ("Associated types", &[]),
    ("Generics", &[]),
    ("`region`", &[]),
    ("`unsafe`", &[]),
    ("`affirm", &[]),
    ("`import`", &[]),
    ("`Set<T>`", &[]),
    // ─── Backend-gated features ───
    // Each row maps to the Feature variant(s) it exercises. The
    // LLVM column in STATUS.md must be "supported" iff every
    // listed Feature is in BackendCapabilities::llvm().supported.
    ("`List.length`", &[]), // has its own lowering, not ListAggregates
    ("`List.append(x)`", &[Feature::ListAppend]),
    ("`Map<K, V>`", &[Feature::Map]),
    ("`String.to_upper`", &[Feature::StringOps]),
    ("`String.trim`", &[Feature::StringOps]),
    ("`String.split` / `.join`", &[Feature::StringSplit]),
    ("`String.to_int`", &[Feature::StringToInt]),
    ("`Int.to_string`", &[Feature::IntToString]),
    ("`File.read`", &[Feature::FileFunctions]),
    ("`Option` / `Some`", &[Feature::Option]),
    ("`Result` / `Ok`", &[Feature::Result]),
    // try/catch is refused at the capability boundary via
    // `Feature::TryCatch`. The `Feature::Result` flag covers
    // Result *values* (Ok/Error/match), which are supported on
    // LLVM; `Feature::TryCatch` covers the construct, which is
    // not. The builder emits `Instruction::TryCatchMarker` at
    // the top of the lowering so `scan_features` can see the
    // construct; the marker is what makes the two features
    // distinguishable in the IR.
    ("`try` / `catch`", &[Feature::TryCatch]),
    ("`&dyn Trait`", &[Feature::DynamicDispatch]),
    ("`rec` records", &[Feature::Records]),
    ("`union` types", &[Feature::Unions]),
    ("Structural `Copy`", &[Feature::Records]),
    ("`args()`", &[Feature::CommandLineArgs]),
    ("`String.charAt`", &[Feature::StringOps]),
    ("`&x` / `&mut x`", &[Feature::References]),
    ("`spawn` / `parallel`", &[Feature::Spawn, Feature::Fork]),
    ("`channel` / `send`", &[Feature::Channels]),
    ("`extern` (FFI)", &[Feature::Ffi]),
    ("`alloc` in `var`", &[Feature::RawMemory]),
    ("`alloc(x)` as statement", &[Feature::RawMemory]),
    ("`free`", &[Feature::RawMemory]),
];

// ─── Row parsing ────────────────────────────────────────────────────

#[derive(Debug)]
struct MatrixRow {
    feature_label: String,
    llvm_column: char, // '✅' or '⛔' (also ⚠️ and ❌)
}

fn find_status_md() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/STATUS.md")
}

fn read_status_md() -> String {
    std::fs::read_to_string(find_status_md()).expect("docs/STATUS.md must exist and be readable")
}

/// Extract every row from the "Feature Matrix" markdown table.
fn parse_matrix_rows(md: &str) -> Vec<MatrixRow> {
    let mut in_matrix = false;
    let mut rows = Vec::new();

    for line in md.lines() {
        if line.trim_start().starts_with("| Feature ") {
            in_matrix = true;
            continue;
        }
        if in_matrix && line.trim().is_empty() {
            break;
        }
        if !in_matrix {
            continue;
        }
        if !line.trim_start().starts_with('|') {
            continue;
        }
        if line.contains("|---") {
            continue; // separator
        }
        // Columns: | Feature | Parser | Analyzer | Interpreter | LLVM | Corpus |
        let parts: Vec<&str> = line.split('|').map(str::trim).collect();
        if parts.len() < 7 {
            continue;
        }
        rows.push(MatrixRow {
            feature_label: parts[1].to_string(),
            llvm_column: parts[5].chars().next().unwrap_or(' '),
        });
    }

    rows
}

// ─── The test ───────────────────────────────────────────────────────

#[test]
fn status_matrix_matches_llvm_capabilities() {
    let md = read_status_md();
    let rows = parse_matrix_rows(&md);
    assert!(
        !rows.is_empty(),
        "no Feature Matrix rows found in docs/STATUS.md"
    );

    let caps = algol26::backends::capabilities::BackendCapabilities::llvm();
    let mut errors: Vec<String> = Vec::new();

    for row in &rows {
        // Find the mapping entry whose needle is a substring of the
        // row label. A row with no mapping entry is a maintenance
        // failure: adding a feature requires adding a table entry.
        let mapping = STATUS_ROW_TO_FEATURES
            .iter()
            .find(|(needle, _)| row.feature_label.contains(*needle));

        let Some((_, features)) = mapping else {
            errors.push(format!(
                "STATUS.md row `{}` has no entry in STATUS_ROW_TO_FEATURES. \
                 Add one before this row can be checked.",
                row.feature_label,
            ));
            continue;
        };

        // A row may legitimately cover features that live in
        // different capability sets. Treat the row as "LLVM-supported"
        // iff every listed feature is supported. (Rows with empty
        // feature lists are exempt — they're language-level rows
        // with no backend capability.)
        if features.is_empty() {
            continue;
        }
        let all_supported = features.iter().all(|f| caps.supported.contains(f));
        let row_says_supported = matches!(row.llvm_column, '✅');

        if all_supported != row_says_supported {
            let feature_names: Vec<&str> = features.iter().map(|f| f.name()).collect();
            errors.push(format!(
                "STATUS.md row `{}`: LLVM column says `{}` but \
                 BackendCapabilities::llvm().supported says [{}]. \
                 Actual supported: {}. \
                 Fix the docs (or the capability) so they agree.",
                row.feature_label,
                row.llvm_column,
                feature_names.join(", "),
                features.iter().all(|f| caps.supported.contains(f)),
            ));
        }
    }

    // Inverse direction: every Feature variant should be claimed by
    // at least one STATUS.md row. A feature not mentioned in the
    // matrix is a docs gap.
    let mut covered: std::collections::HashSet<Feature> = std::collections::HashSet::new();
    for (_, features) in STATUS_ROW_TO_FEATURES {
        covered.extend(features.iter().copied());
    }
    for &f in Feature::all() {
        if !covered.contains(&f) && !features_with_no_capability_row().contains(&f) {
            errors.push(format!(
                "Feature::{} has no STATUS.md matrix row. \
                 Add a row to docs/STATUS.md and a mapping entry to \
                 STATUS_ROW_TO_FEATURES.",
                f.name(),
            ));
        }
    }

    assert!(
        errors.is_empty(),
        "STATUS.md / BackendCapabilities drift detected:\n\n{}",
        errors.join("\n\n"),
    );
}

/// Features that are legitimately not user-visible in STATUS.md —
/// they're internal (RecordByValue, GenericRecords, DynamicDispatch
/// map to rows elsewhere, etc.). Fill in as needed.
fn features_with_no_capability_row() -> Vec<Feature> {
    vec![
        Feature::RecordByValue,
        Feature::GenericRecords,
        Feature::DynamicDispatch,
        Feature::ListPrint,
        Feature::ListAggregates,
    ]
}

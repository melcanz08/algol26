//! Cross-check between `coverage_matrix::MATRIX` and the on-disk
//! conformance suite.
//!
//! Enforces three invariants:
//!
//! 1. Every matrix row with a `conformance_dir` points at a real
//!    directory under `tests/conformance/valid/`.
//! 2. Every matrix row with no `conformance_dir` has a non-empty
//!    `notes` field (this is also checked in `coverage_matrix.rs`,
//!    but duplicated here so a failure of one points at the other).
//! 3. Every directory on disk is either referenced by at least one
//!    matrix row, or explicitly listed in `NON_FEATURE_DIRS`.
//!
//! Adding a feature to the matrix without a fixture is a CI failure
//! unless the row also carries a note explaining the gap. That is
//! the "checklist, not treasure hunt" property.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[path = "coverage_matrix.rs"]
mod coverage_matrix;

use coverage_matrix::{FeatureRow, MATRIX};

/// Directories under `tests/conformance/valid/` that group fixtures
/// by category rather than by a single feature. They are exempt from
/// the "every directory is referenced by a matrix row" rule.
const NON_FEATURE_DIRS: &[&str] = &["basics", "control_flow"];

fn conformance_valid_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/valid")
}

/// Return the names (not paths) of every subdirectory under
/// `tests/conformance/valid/`. Skips `.gol` files at the top level
/// (there should be none after the restructure, but tolerate them).
fn existing_dirs() -> Vec<String> {
    let root = conformance_valid_root();
    let entries = match std::fs::read_dir(&root) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_referenced_dir_exists() {
    let existing: HashSet<String> = existing_dirs().into_iter().collect();
    let mut missing = Vec::new();

    for row in MATRIX {
        if let Some(dir) = row.conformance_dir {
            if !existing.contains(dir) {
                missing.push(format!("{} -> {}", row.name, dir));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "matrix rows point at non-existent conformance directories:\n  {}\n\
         existing directories: {:?}",
        missing.join("\n  "),
        existing.iter().collect::<Vec<_>>()
    );
}

#[test]
fn every_dir_is_referenced_or_exempt() {
    let existing = existing_dirs();
    let referenced: HashSet<&str> = MATRIX
        .iter()
        .filter_map(|row| row.conformance_dir)
        .collect();

    let mut orphans = Vec::new();
    for dir in &existing {
        if NON_FEATURE_DIRS.contains(&dir.as_str()) {
            continue;
        }
        if !referenced.contains(dir.as_str()) {
            orphans.push(dir.clone());
        }
    }

    assert!(
        orphans.is_empty(),
        "conformance directories not referenced by any matrix row and not in \
         NON_FEATURE_DIRS: {:?}\n\
         Add a matrix row that points at them, or add them to NON_FEATURE_DIRS.",
        orphans
    );
}

#[test]
fn every_missing_dir_has_notes() {
    for row in MATRIX {
        if row.conformance_dir.is_none() {
            assert!(
                !row.notes.trim().is_empty(),
                "feature `{}` has no conformance_dir but no notes — \
                 the notes field is how the gap is recorded",
                row.name
            );
        }
    }
}

#[test]
fn matrix_and_dirs_are_consistent() {
    // Cheap combined sanity check: if either side is empty, the other
    // tests pass trivially and hide real problems.
    assert!(!MATRIX.is_empty(), "MATRIX is empty");
    assert!(
        !existing_dirs().is_empty(),
        "no subdirectories under tests/conformance/valid/ — has the \
         restructure been reverted?"
    );
    let _ = std::path::Path::new::<PathBuf>(&conformance_valid_root()); // silence unused warning on Path import
}

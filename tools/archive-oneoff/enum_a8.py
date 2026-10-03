#!/usr/bin/env python3
"""
A8 part 2: coverage matrix row + maturity entry + ADR status.
"""

from pathlib import Path

MATRIX = Path("tests/coverage_matrix.rs")
MATURITY = Path("tests/coverage_maturity.rs")
ADR = Path("docs/decisions/0030-enum-types.md")


def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:300])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


# 1. KNOWN_MISSING_REFUSALS entry
patch(MATRIX, [
    (
        '''pub const KNOWN_MISSING_REFUSALS: &[&str] = &[
    // Unfinished feature. No capability check exists because there
    // is no `Feature::Range` variant, and no backend supports ranges
    // end-to-end. Stays here until the feature is either implemented
    // or removed.
    "range",
];''',
        '''pub const KNOWN_MISSING_REFUSALS: &[&str] = &[
    // Unfinished feature. No capability check exists because there
    // is no `Feature::Range` variant, and no backend supports ranges
    // end-to-end. Stays here until the feature is either implemented
    // or removed.
    "range",
    // Enum types themselves do not refuse — the `match` construct
    // does, and no `Feature::Match` variant exists to pin a per-
    // backend test against. This row claims `Refused` on LLVM/WASM
    // because matching is the interesting part of enums, and match
    // is refused there. See the row's notes for the boundary.
    "enum_types",
];''',
    ),
])

# 2. The FeatureRow
patch(MATRIX, [
    (
        '''    FeatureRow {
        name: "nominal_types",''',
        '''    FeatureRow {
        name: "enum_types",
        conformance_dir: None,
        interpreter: Support::Full,
        llvm: Support::Refused,
        wasm: Support::Refused,
        refusal_tests: &[],
        notes: "Enum declaration, resolution, and the from_ordinal / \\
                to_ordinal intrinsics compile through all three backends: \\
                at runtime an enum is an i64, and the Cast is a no-op. \\
                However, any program that MATCHES on an enum cannot run \\
                on LLVM or WASM, because those backends refuse \\`match\\` \\
                at the capability check, independent of the pattern's \\
                shape. Same limitation applies to Option/Result/Bool \\
                matches today. No conformance fixture exists because \\
                the harness does not support per-fixture backend \\
                selection. Same shape as \\`records\\` and \\`map\\`.",
    },
    FeatureRow {
        name: "nominal_types",''',
    ),
])

# 3. Maturity entry
patch(MATURITY, [
    (
        '''    ("list_append", Maturity::InterpreterOnly),
    ("nominal_types", Maturity::AllBackends),''',
        '''    ("list_append", Maturity::InterpreterOnly),
    ("enum_types", Maturity::InterpreterOnly),
    ("nominal_types", Maturity::AllBackends),''',
    ),
])

# 4. ADR status
patch(ADR, [
    (
        '''## Status

Proposed. Not yet implemented.''',
        '''## Status

Accepted. A1-A6 implemented; A7 (matrix row, feature doc) landed
together with the feature.

Implementation note: the visible portion of enum types —
`from_ordinal`, `to_ordinal`, and variant matching — works on the
interpreter. LLVM and WASM refuse the `match` construct itself,
independent of pattern shape, so a program that matches on an enum
is interpreter-only. The `enum_types` coverage matrix row reflects
this with `InterpreterOnly` and a note on the boundary.

Deferred to future ADRs: runtime out-of-range `from_ordinal`
(v1 rejects literal out-of-range only), name-based printing,
`for d in Day` iteration.''',
    ),
])


print()
print("NEXT: cargo test --release 2>&1 | grep -iE 'coverage|maturity|panicked' | head -20")

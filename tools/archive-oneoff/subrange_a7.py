#!/usr/bin/env python3
"""A7: matrix row + maturity + ADR status."""

from pathlib import Path

MATRIX = Path("tests/coverage_matrix.rs")
MATURITY = Path("tests/coverage_maturity.rs")
ADR = Path("docs/decisions/0031-subrange-types.md")


def patch(path, edits):
    src = path.read_text()
    for old, new, label in edits:
        n = src.count(old)
        if n != 1:
            print(f"FAIL: {path} — {label} matched {n} times")
            print("-" * 60)
            print(old[:300])
            print("-" * 60)
            path.write_text(src)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
        path.write_text(src)
        print(f"OK: {path} — {label}")


patch(MATRIX, [
    (
        """    FeatureRow {
        name: "nominal_types",""",
        """    FeatureRow {
        name: "subrange_types",
        conformance_dir: Some("subrange_types"),
        interpreter: Support::Full,
        llvm: Support::Full,
        wasm: Support::Full,
        refusal_tests: &[],
        notes: "",
    },
    FeatureRow {
        name: "nominal_types",""",
        "FeatureRow",
    ),
])

patch(MATURITY, [
    (
        """    ("nominal_types", Maturity::AllBackends),""",
        """    ("subrange_types", Maturity::AllBackends),
    ("nominal_types", Maturity::AllBackends),""",
        "maturity entry",
    ),
])

patch(ADR, [
    (
        """## Status

Proposed. Not yet implemented.""",
        """## Status

Accepted. A1-A7 implemented.

Subranges over Int and user enums work end-to-end on all three
backends. Literal bounds are checked at compile time; non-literal
bounds check at runtime via `Instruction::BoundsCheck`. Extraction
is `.to_base()` (parens and no-parens). Arithmetic on subranges is
deliberately rejected — see design question 6.

Deferred to a follow-up: trait-based checked arithmetic
(`Add` for a subrange returning `Result<T, RangeError>`), and a
`try_from_ordinal`-style non-panicking construction.""",
        "ADR status",
    ),
])

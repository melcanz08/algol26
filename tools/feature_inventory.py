#!/usr/bin/env python3
"""feature_inventory.py — drift detector for ALGOL26's feature inventories.

The repo has four independent inventories of "what ALGOL26 supports":

  1. docs/STATUS.md          — the canonical feature matrix
  2. docs/features/*.md      — per-feature documentation
  3. tests/conformance/      — acceptance fixtures
  4. docs/decisions/NNNN-*   — architecture decision records

These drift. A feature can be listed in STATUS.md but have no
conformance fixture; a conformance dir can exist for a feature not
listed in STATUS.md; an ADR can describe a "planned" feature that
has since shipped without STATUS.md being updated.

This script reads all four and reports mismatches. It does NOT fix
them — reconciliation is a human judgment call, logged in
docs/status/KNOWN_DRIFT.md.

The `SLUG_MAP` below is the canonical name mapping. Each entry is
one STATUS.md feature row's "concept slug" — a normalized name used
to look up the corresponding doc, conformance dir, and ADR(s). If
the checker reports a mismatch that's actually a slug error (e.g.
"conformance dir exists but not matching"), add or fix the mapping
here rather than editing the checker's matching logic.

Usage:
  tools/feature_inventory.py             # human-readable report
  tools/feature_inventory.py --check     # CI gate: exit 1 on mismatch
  tools/feature_inventory.py --raw       # raw signal dump (no
                                         # categorization)

Exit codes:
  0  — no drift found (or --check not set)
  1  — drift found and --check set
  2  — internal error (missing STATUS.md, unparseable table, etc.)
"""
import argparse
import re
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parent.parent
STATUS_MD = REPO / "docs" / "STATUS.md"

# Feature docs that are intentionally NOT in STATUS.md's matrix.
# Documented in docs/status/KNOWN_DRIFT.md with a per-item rationale.
# The reverse check ("doc exists but no STATUS.md row") skips these.
EXCLUDED_PARTIAL_FEATURES = {
    "range",  # Partial: parsed + IR, no backend executes it.
}
FEATURES_DIR = REPO / "docs" / "features"
CONFORMANCE_VALID = REPO / "tests" / "conformance" / "valid"
CONFORMANCE_INVALID = REPO / "tests" / "conformance" / "invalid"
DECISIONS_DIR = REPO / "docs" / "decisions"


# ─── The slug map ───────────────────────────────────────────────────
#
# Key: the exact feature-name text that appears in STATUS.md's matrix
# (backticks stripped, whitespace normalized).
#
# Value: a dict with:
#   "feature_doc":      expected basename (or list of basenames) under
#                       docs/features/ (no .md). None if no doc is
#                       expected (builtin primitives, variants of a
#                       larger feature, etc.). List form is for
#                       many-to-one: e.g. "Traits + impls" owns both
#                       "methods.md" and "impl_where.md".
#   "conformance_valid": expected dirname (or list) under
#                       tests/conformance/valid/. Same list form.
#   "adr_hint":         an ADR number or number list that clearly
#                       covers this feature. None if no ADR is expected.
#                       (Checks the ADR NNNN-*.md file exists.)
#
# The map is the *canonical* answer. When the checker reports a
# mismatch, first check whether the SLUG_MAP is wrong. Only edit
# SLUG_MAP when a feature's canonical home has genuinely moved.

SLUG_MAP = {
    # ─── Control flow ──────────────────────────────────────────
    "`while`":                                 {"feature_doc": None, "conformance_valid": "control_flow", "adr_hint": None},
    "`for ... in`":                            {"feature_doc": None, "conformance_valid": "control_flow", "adr_hint": None},
    "`if` / `else`":                           {"feature_doc": None, "conformance_valid": "control_flow", "adr_hint": None},
    "`break` / `continue`":                    {"feature_doc": None, "conformance_valid": "control_flow", "adr_hint": None},
    "`defer`":                                 {"feature_doc": "defer", "conformance_valid": "defer", "adr_hint": None},

    # ─── Functions ─────────────────────────────────────────────
    "Functions / procedures":                  {"feature_doc": None, "conformance_valid": "basics", "adr_hint": None},
    "Return values":                           {"feature_doc": None, "conformance_valid": "basics", "adr_hint": None},
    "Bare call statements":                    {"feature_doc": None, "conformance_valid": "basics", "adr_hint": None},

    # ─── Collections ───────────────────────────────────────────
    "Lists + indexing":                        {"feature_doc": ["list", "list_llvm"], "conformance_valid": "lists", "adr_hint": None},
    "`List.length` / `List.sum`":              {"feature_doc": "list", "conformance_valid": "lists", "adr_hint": None},
    "`List.append(x)`":                        {"feature_doc": "list", "conformance_valid": "lists", "adr_hint": "0028"},
    "`Map<K, V>`":                             {"feature_doc": "map", "conformance_valid": None, "adr_hint": "0027"},
    "`Set<T>`":                                {"feature_doc": "set_types", "conformance_valid": "set_types", "adr_hint": "0032"},

    # ─── Strings ───────────────────────────────────────────────
    "Strings":                                 {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`String.length`":                         {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`String.charAt`":                         {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`String.to_upper` / `.to_lower` / `.concat` / `.substring`": {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`String.trim`":                           {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`String.split` / `.join`":                {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`Int.to_string`":                         {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`String.to_int`":                         {"feature_doc": "string", "conformance_valid": "strings", "adr_hint": None},
    "`File.read` / `.write` / `.append`":      {"feature_doc": "string", "conformance_valid": None, "adr_hint": None},

    # ─── Math ──────────────────────────────────────────────────
    "`Math.*`":                                {"feature_doc": None, "conformance_valid": "arithmetic", "adr_hint": None},

    # ─── Types ─────────────────────────────────────────────────
    "`Option` / `Some` / `None`":              {"feature_doc": "option", "conformance_valid": "option", "adr_hint": None},
    "`match` (literal patterns)":              {"feature_doc": None, "conformance_valid": "basics", "adr_hint": None},
    "`match` (binding patterns)":              {"feature_doc": None, "conformance_valid": "basics", "adr_hint": None},
    "`Result` / `Ok` / `Error`":               {"feature_doc": "result", "conformance_valid": "result", "adr_hint": "0030"},
    "`try` / `catch`":                         {"feature_doc": "result", "conformance_valid": "result", "adr_hint": None},
    "Traits + impls":                          {"feature_doc": ["trait", "methods", "impl_where"], "conformance_valid": ["methods", "method_call", "impl_where"], "adr_hint": "0033"},
    "`&dyn Trait` / `&mut dyn Trait`":         {"feature_doc": "dyn_trait", "conformance_valid": "dyn_trait", "adr_hint": "0038"},
    "Visibility (`pub`)":                      {"feature_doc": "visibility", "conformance_valid": "visibility", "adr_hint": "0039"},
    "Associated types":                        {"feature_doc": "associated_types", "conformance_valid": "assoc_types", "adr_hint": "0041"},
    "Generics":                                {"feature_doc": "generic", "conformance_valid": "basics", "adr_hint": None},
    "`rec` records":                           {"feature_doc": "record", "conformance_valid": "basics", "adr_hint": "0024"},
    "`union` types":                           {"feature_doc": "union", "conformance_valid": None, "adr_hint": "0055"},
    "Structural `Copy`":                       {"feature_doc": None, "conformance_valid": "ownership", "adr_hint": "0026"},

    # ─── Memory / concurrency ──────────────────────────────────
    "`region`":                                {"feature_doc": "region", "conformance_valid": None, "adr_hint": "0007"},
    "`unsafe`":                                {"feature_doc": "unsafe", "conformance_valid": None, "adr_hint": "0009"},
    "`&x` / `&mut x` / `*r` / `AddrOf`":       {"feature_doc": "borrow", "conformance_valid": "ownership", "adr_hint": ["0019", "0043"]},
    "`spawn` / `parallel`":                    {"feature_doc": "spawn", "conformance_valid": "concurrency", "adr_hint": "0008"},
    "`channel` / `send` / `receive`":          {"feature_doc": "channel", "conformance_valid": "concurrency", "adr_hint": "0020"},
    "`alloc` in `var` position":               {"feature_doc": "alloc_free", "conformance_valid": None, "adr_hint": "0015"},
    "`alloc(x)` as statement":                 {"feature_doc": "alloc_free", "conformance_valid": None, "adr_hint": "0015"},
    "`free`":                                  {"feature_doc": "alloc_free", "conformance_valid": None, "adr_hint": "0015"},

    # ─── FFI / modules ─────────────────────────────────────────
    "`extern` (FFI)":                          {"feature_doc": "ffi", "conformance_valid": None, "adr_hint": None},
    "`import`":                                {"feature_doc": None, "conformance_valid": None, "adr_hint": None},

    # ─── Assertions / args ─────────────────────────────────────
    "`affirm(cond, msg)`":                     {"feature_doc": None, "conformance_valid": None, "adr_hint": "0022"},
    "`args()`":                                {"feature_doc": None, "conformance_valid": None, "adr_hint": "0023"},

    # ─── New rows added in the feature-inventory reconciliation ──
    # These STATUS.md rows didn't exist before; the features were
    # documented + tested but absent from the canonical matrix.
    "`enum` types":                            {"feature_doc": "enum_types", "conformance_valid": "enum_types", "adr_hint": "0030"},
    "`nominal` types":                         {"feature_doc": "nominal_types", "conformance_valid": "nominal_types", "adr_hint": "0029"},
    "`subrange` types":                        {"feature_doc": "subrange_types", "conformance_valid": "subrange_types", "adr_hint": "0031"},
    "Associated constants":                    {"feature_doc": "associated_constants", "conformance_valid": "associated_constants", "adr_hint": "0062"},
    "Where-clauses on impls":                  {"feature_doc": "impl_where", "conformance_valid": "impl_where", "adr_hint": "0025"},
}


def parse_status_matrix():
    """Extract feature rows from STATUS.md's canonical matrix.

    Returns a list of dicts: {"name": <feature name>, "row_text": <full row>}.
    Raises SystemExit(2) if STATUS.md or its matrix can't be found.

    Two row shapes appear in STATUS.md:

      | `while` | ✅ | ...       (backticked feature name)
      | Functions / procedures | ✅ | ...   (bare feature name)

    Both are valid. The parser stops at the first non-table, non-blank
    line after the table's first row.

    Blank lines *before* the table are tolerated (STATUS.md has a
    blank line between "## Feature Matrix" and the `| Feature |`
    header row).
    """
    if not STATUS_MD.exists():
        print(f"ERROR: {STATUS_MD} not found", file=sys.stderr)
        sys.exit(2)

    text = STATUS_MD.read_text(encoding="utf-8")
    lines = text.splitlines()

    # Locate the "## Feature Matrix" header, then scan forward to the
    # start of the table (first line starting with "| Feature |").
    in_matrix = False
    table_started = False
    features = []
    for line in lines:
        stripped = line.strip()
        if not in_matrix:
            if stripped == "## Feature Matrix":
                in_matrix = True
            continue

        # In the matrix section.
        if not table_started:
            if stripped.startswith("| Feature |"):
                table_started = True
                continue
            if stripped.startswith("##"):
                # Left the section without finding the table — error.
                break
            # Blank line or prose before the header — skip.
            continue

        # Table has started. Separator row?
        if stripped.startswith("|---"):
            continue

        # A data row?
        if stripped.startswith("|"):
            # Extract the first cell (feature name). The cell contains
            # the exact STATUS.md row text (backticks and all). We keep
            # it verbatim because that's what the SLUG_MAP keys are.
            # Splitting on the first `|` that isn't escaped is fine —
            # feature cells never contain `|`.
            parts = stripped.split("|", 2)
            if len(parts) >= 2:
                cell = parts[1].strip()
                # Skip the header row (in case separator wasn't caught).
                if cell == "Feature":
                    continue
                if cell:
                    features.append({"name": cell, "row_text": line})
        else:
            # First non-| line after table started — end of table.
            break

    if not features:
        print("ERROR: no feature rows found in STATUS.md matrix", file=sys.stderr)
        sys.exit(2)
    return features


def list_feature_docs():
    if not FEATURES_DIR.exists():
        return set()
    return {p.stem for p in FEATURES_DIR.glob("*.md") if p.stem != "README"}


def list_conformance_valid():
    if not CONFORMANCE_VALID.exists():
        return set()
    return {p.name for p in CONFORMANCE_VALID.iterdir() if p.is_dir()}


def list_conformance_invalid_prefixes():
    if not CONFORMANCE_INVALID.exists():
        return set()
    # Invalid fixtures are per-feature-name.gol; group by first segment.
    names = set()
    for p in CONFORMANCE_INVALID.glob("*.gol"):
        first = p.stem.split("_")[0]
        names.add(first)
    return names


def list_decision_files():
    if not DECISIONS_DIR.exists():
        return set()
    return {p.name for p in DECISIONS_DIR.glob("*.md") if p.name != "README.md"}


def adr_exists(number):
    for name in list_decision_files():
        if name.startswith(f"{int(number):04d}-"):
            return True
    return False


def compute_mismatches():
    """Return a dict of mismatch categories to lists."""
    features = parse_status_matrix()
    feature_docs = list_feature_docs()
    conformance_valid = list_conformance_valid()
    decision_files = list_decision_files()

    # Names in STATUS.md that aren't handled by the slug map at all.
    unmapped = []

    # Per-feature checks.
    missing_doc = []
    missing_conformance = []
    missing_adr = []

    # Reverse checks (something exists but STATUS.md doesn't list it).
    doc_not_in_status = set(feature_docs) - EXCLUDED_PARTIAL_FEATURES
    conformance_not_in_status = set(conformance_valid) - EXCLUDED_PARTIAL_FEATURES

    for feat in features:
        name = feat["name"]
        entry = SLUG_MAP.get(name)
        if entry is None:
            unmapped.append(name)
            continue

        if entry["feature_doc"]:
            # feature_doc can be a str or list[str] (many-to-one — e.g.
            # "Traits + impls" also owns "methods.md" and "impl_where.md").
            slugs = entry["feature_doc"]
            if isinstance(slugs, str):
                slugs = [slugs]
            for slug in slugs:
                if slug in feature_docs:
                    doc_not_in_status.discard(slug)
                else:
                    missing_doc.append((name, slug))

        if entry["conformance_valid"]:
            # conformance_valid can be a str (one dir) or list[str]
            # (many-to-one — e.g. "Traits + impls" maps to both
            # "methods" and "method_call").
            slugs = entry["conformance_valid"]
            if isinstance(slugs, str):
                slugs = [slugs]
            for slug in slugs:
                if slug in conformance_valid:
                    conformance_not_in_status.discard(slug)
                else:
                    missing_conformance.append((name, slug))

        if entry["adr_hint"]:
            # adr_hint can be a str or list[str] (many-to-one — e.g.
            # references point at both ADR 0019 and ADR 0043).
            hints = entry["adr_hint"]
            if isinstance(hints, str):
                hints = [hints]
            for num in hints:
                if not adr_exists(num):
                    missing_adr.append((name, num))

    return {
        "features_parsed": features,
        "unmapped_features": unmapped,
        "missing_doc": missing_doc,
        "missing_conformance": missing_conformance,
        "missing_adr": missing_adr,
        "doc_not_in_status": sorted(doc_not_in_status),
        "conformance_not_in_status": sorted(conformance_not_in_status),
    }


def print_report(mismatches):
    print("=" * 70)
    print("feature_inventory.py — ALGOL26 feature inventory drift report")
    print("=" * 70)
    print()
    print(f"Features in STATUS.md matrix: {len(mismatches['features_parsed'])}")
    print()

    def section(title, items):
        if not items:
            return
        print(f"{title} ({len(items)}):")
        for item in items:
            print(f"  - {item}")
        print()

    section("Features in STATUS.md with no SLUG_MAP entry (BUG in this script)",
            mismatches["unmapped_features"])

    section("Features in STATUS.md with missing feature docs",
            [f"{name}  ->  docs/features/{slug}.md" for name, slug in mismatches["missing_doc"]])

    section("Features in STATUS.md with missing conformance dirs",
            [f"{name}  ->  tests/conformance/valid/{slug}/" for name, slug in mismatches["missing_conformance"]])

    section("Features in STATUS.md with missing ADRs",
            [f"{name}  ->  docs/decisions/{num}-*.md" for name, num in mismatches["missing_adr"]])

    section("Feature docs not referenced by any STATUS.md row",
            mismatches["doc_not_in_status"])

    section("Conformance valid dirs not referenced by any STATUS.md row",
            mismatches["conformance_not_in_status"])

    total = (
        len(mismatches["unmapped_features"])
        + len(mismatches["missing_doc"])
        + len(mismatches["missing_conformance"])
        + len(mismatches["missing_adr"])
        + len(mismatches["doc_not_in_status"])
        + len(mismatches["conformance_not_in_status"])
    )
    print(f"TOTAL: {total} mismatch(es)")
    return total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true",
                    help="exit 1 if any mismatch is found (CI-friendly)")
    ap.add_argument("--raw", action="store_true",
                    help="dump raw signals without categorization")
    args = ap.parse_args()

    mismatches = compute_mismatches()

    if args.raw:
        for k, v in mismatches.items():
            print(f"{k}: {v}")
        return

    total = print_report(mismatches)

    if args.check and total > 0:
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()

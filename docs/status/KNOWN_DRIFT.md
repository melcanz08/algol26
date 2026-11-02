# Known feature-inventory drift

This file records *intentional* differences between the four
inventories that `tools/feature_inventory.py` checks:

1. `docs/STATUS.md` — canonical feature matrix
2. `docs/features/*.md` — per-feature documentation
3. `tests/conformance/` — acceptance fixtures
4. `docs/decisions/NNNN-*.md` — architecture decision records

**Unintentional drift belongs in the report, not here.** The checker
finds mismatches; the reconciliation commit resolved every one. What
remains are features where the four inventories legitimately
disagree on scope.

## Intentional aliases

### `range` — documented but not in STATUS.md matrix

**Status:** partial feature. Parsed, has an IR representation, no
backend executes it. `docs/features/range.md` says so explicitly:

> Status: partial — parsed, IR exists, backends refuse

`STATUS.md`'s matrix lists *shipped* features only. `range` isn't
shipped, so it's absent by design, not drift.

The checker's reverse check flags `range` as "feature doc exists
but not referenced by any STATUS.md row." That's a correct
observation of an intentional state. It's listed in
`EXCLUDED_PARTIAL_FEATURES` in `tools/feature_inventory.py` so
`--check` mode exits 0 when it's the only finding.

When `range` gets a working backend, it moves into the matrix; the
checker's `--check` mode will pass on any matrix row that names it,
and `range` can be removed from `EXCLUDED_PARTIAL_FEATURES`.

### Sub-feature docs

Three feature docs describe *sub-features* of larger concepts and
are aliased under the parent's STATUS.md row by `SLUG_MAP` in
`tools/feature_inventory.py`:

- **`methods.md`** — aliased under `Traits + impls`. Method
  resolution and method-call syntax are part of the trait system.
- **`impl_where.md`** — aliased under `Traits + impls`. Where-
  clause checking on impls is part of the trait system.
- **`list_llvm.md`** — aliased under `Lists + indexing`.
  LLVM-specific list-lowering details; the *list* feature is the
  top-level concern.

**Not aliased** (they have their own STATUS.md rows):

- `associated_types.md` — has an `Associated types` matrix row.
- `associated_constants.md` — has an `Associated constants` row
  added in this reconciliation.

The `SLUG_MAP` is the source of truth for aliasing. If a
sub-feature doc is promoted to a top-level feature, add a
STATUS.md row and remove the alias from `SLUG_MAP`.

## History — the 2026-11-02 reconciliation

Starting point: **15 mismatches** across the four inventories.

| # | Mismatch type | Count | Resolution |
|---|---|---|---|
| A | Phantom ADR references — STATUS.md cited 0019, 0020, 0021; none existed | 1 group | Wrote all three ADRs, backfilled |
| B | Feature docs missing from STATUS.md matrix | 5 | Added 5 matrix rows |
| C | Feature docs are sub-features, not top-level | 3 | Aliased via SLUG_MAP |
| D | Conformance dirs missing from STATUS.md matrix | 6 | Same as B/C — resolved alongside |
| E | Feature with no ADR — `associated constants` | 1 | Wrote ADR 0062 |

After reconciliation: **0 real drift**. The checker's remaining
`range` finding is intentional (partial feature, excluded).

## Running the checker

    python3 tools/feature_inventory.py            # human report
    python3 tools/feature_inventory.py --check    # CI gate

`--check` exits 0 when no drift is found, 1 when drift is found.
`range` is excluded via `EXCLUDED_PARTIAL_FEATURES` in the checker;
it's documented here so a future contributor knows why.

## Adding a new feature — the checklist

When you implement a feature:

1. **Add a matrix row** to `docs/STATUS.md`'s Feature Matrix table.
2. **Add a SLUG_MAP entry** in `tools/feature_inventory.py` — the
   key is the verbatim matrix-row feature name, the value names
   the expected doc / conformance dir / ADR(s).
3. **Add the feature doc** under `docs/features/`.
4. **Add conformance fixtures** under `tests/conformance/valid/`
   (or an end-to-end test in the appropriate test module).
5. **Write an ADR** recording the decision.

Then run `python3 tools/feature_inventory.py --check`. Zero
mismatches means all four inventories are aligned.

## Why this file exists

The four inventories can drift silently. A feature doc can be
written and shipped, but never added to STATUS.md. An ADR can be
referenced from STATUS.md before it's written. A conformance
fixture can exist for a feature the matrix doesn't list.

Before this file and the checker, drift was only discovered by
chance — see `LANGUAGE_GAPS.md`'s G4 correction for the canonical
example: `List.append` existed, worked, and was tested, but
`docs/features/list.md` didn't list it, which produced a false
"gap" report. The fix is a mechanical check (`feature_inventory.py`)
plus a human reconciliation log (this file).

The `--check` mode gives CI a way to fail on new drift. This file
gives humans a place to record intentional aliases so they don't
turn into silent regressions.

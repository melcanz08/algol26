# Subsystem status & investigations

Focused, in-progress notes about individual subsystems. These are
*narrower* than [`../STATUS.md`](../STATUS.md), which is the canonical
feature matrix. When an investigation here concludes, its outcome
should be folded into `../STATUS.md` (and the corresponding
`../features/<name>.md` / `../decisions/NNNN-*.md` updated if the
decision changed).

| File | Subsystem | State |
|------|-----------|-------|
| [diagnostics.md](diagnostics.md) | Diagnostics & spans | Investigation (Phase 0) |
| [KNOWN_DRIFT.md](KNOWN_DRIFT.md) | Feature-inventory reconciliation log | Current |

## Feature inventory drift detection

`tools/feature_inventory.py` checks the four independent inventories
that describe "what ALGOL26 supports":

1. `docs/STATUS.md` — canonical feature matrix
2. `docs/features/*.md` — per-feature documentation
3. `tests/conformance/` — acceptance fixtures
4. `docs/decisions/NNNN-*.md` — architecture decision records

Run it to find mismatches:

    python3 tools/feature_inventory.py            # report
    python3 tools/feature_inventory.py --check    # CI gate

Intentional aliases (sub-feature docs, partial features) are
recorded in [`KNOWN_DRIFT.md`](KNOWN_DRIFT.md). Unintentional
drift should be fixed by updating the missing inventory — usually
by adding a `STATUS.md` matrix row or a `SLUG_MAP` entry.

When adding a new feature, follow the checklist in
[`KNOWN_DRIFT.md`](KNOWN_DRIFT.md) §"Adding a new feature".

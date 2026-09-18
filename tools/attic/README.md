# tools/attic — retired one-shot patch scripts

This directory contains `apply_*.py` scripts that were used
historically to apply one-off edits to the ALGOL26 tree. They
are **not** part of the build, not run by CI, and not maintained.

## Why they exist

Before this project had a stable CI pipeline and a reliable
local `cargo fmt` / `cargo clippy` workflow, changes that
touched many files at once were applied via scripts rather than
by editing each file. Each script targeted a specific
refactor (a PR number, a fix phase, a step number, etc.).

## Why they're archived

Two problems made them worse than useless:

1. **No clear ordering.** Multiple scripts claimed to be
   "final" or "v2" (see `apply_final_docs_sync.py` vs
   `apply_final_docs_sync_v2.py`, `apply_docs_sync.py` vs
   `apply_docs_sync_v3.py`). It was impossible to know which
   one had already run, in what order, against which commit.

2. **Duplicate of what git does.** Every script's changes were
   already committed. Re-running any of them would either be a
   no-op (if the content matched) or corrupt the tree (if it
   didn't). The commits themselves are the source of truth.

## What replaced them

- **`cargo fmt`** for style changes — deterministic, idempotent,
  runs in CI.
- **`git rebase` / `git cherry-pick`** for propagating changes
  across branches.
- **Hand edits** for anything subtler, reviewed in small commits.

## If you need to know what a script did

Look at its git history:

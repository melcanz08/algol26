#!/usr/bin/env bash
# tools/capture_baseline.sh
#
# Dumps the exact stdout/stderr/exit-code of every corpus program
# through both backends and the checker. Used as a golden snapshot
# during refactors. Paths are normalized so the output is portable
# across machines and worktrees.
#
# Usage:
#   tools/capture_baseline.sh > /tmp/baseline.txt
#   diff tools/golden_baseline.txt <(tools/capture_baseline.sh)

set -uo pipefail

BIN=./target/debug/algol26

if [ ! -x "$BIN" ]; then
    echo "error: $BIN not built (run 'cargo build' first)" >&2
    exit 1
fi

{
    echo "# baseline captured at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "# git HEAD: $(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo

    for f in $(find tests/corpus -name '*.gol' | sort); do
        echo "======================================================================"
        echo "FILE: $f"
        echo "======================================================================"

        echo "--- check ---"
        "$BIN" check "$f" 2>&1
        echo "check_exit=$?"
        echo

        echo "--- run --interpreter ---"
        "$BIN" run --interpreter "$f" 2>&1
        echo "interp_exit=$?"
        echo

        echo "--- run (llvm) ---"
        "$BIN" run "$f" 2>&1
        echo "llvm_exit=$?"
        echo
    done
} | sed "s|$PWD|REPO|g"
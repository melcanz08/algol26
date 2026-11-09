#!/usr/bin/env bash
# Lint + check every .gol file in src/chaincss/ in one pass.
#
# The two most common port-file typos — Map<...] with wrong closer
# and a missing `}` in a while/proc body — are caught by the
# linter and the compiler respectively. Running both checks
# together after every write saves round-trips.
#
# Usage: tools/check_all.sh
#        (run from anywhere; cd's to repo root first)

set -e
cd "$(dirname "$0")/.." || exit 1

echo "== bracket lint =="
for f in src/chaincss/*.gol; do
    python3 tools/lint_generic_brackets.py "$f"
done
for f in tests/chaincss/*.gol tests/chaincss/compiler_bugs/*.gol; do
    if [ -f "$f" ]; then
        python3 tools/lint_generic_brackets.py "$f"
    fi
done
echo "  all clean"

echo "== compiler check =="
for f in src/chaincss/*.gol; do
    ./target/release/algol26 check "$f"
done
echo "  all clean"

# Test files are checked by their own runners (run_parity.sh etc.)
# because they use bare imports that need a specific working directory.
# Do not check them here — the false positives are noisy.


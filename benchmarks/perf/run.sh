#!/usr/bin/env bash
# ALGOL26 LLVM backend vs. C (-O2) performance comparison.
# Run from the repo root: bash benchmarks/perf/run.sh [reps]
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT" || { echo "cannot cd to repo root: $ROOT"; exit 1; }

REPS="${1:-3}"
ALGOL26=./target/release/algol26
[[ -x "$ALGOL26" ]] || ALGOL26=./target/debug/algol26
[[ -x "$ALGOL26" ]] || { echo "build first: cargo build --release"; exit 1; }
command -v clang >/dev/null || { echo "clang not found"; exit 1; }

OUT=/tmp/algol26_bench
mkdir -p "$OUT"

min_ms() {
    local n="$1"; shift
    local best=999999999
    for ((i=0;i<n;i++)); do
        local a b
        a=$(date +%s%N)
        "$@" >/dev/null 2>&1
        b=$(date +%s%N)
        local ms=$(( (b - a) / 1000000 ))
        (( ms < best )) && best=$ms
    done
    echo "$best"
}

printf '%-10s  %10s  %10s  %8s\n' "benchmark" "ALGOL26" "C -O2" "ratio"
printf '%-10s  %10s  %10s  %8s\n' "---------" "--------" "-------" "-----"

for name in loop array string; do
    gol="benchmarks/perf/$name.gol"
    c="benchmarks/perf/$name.c"
    [[ -f "$gol" ]] || { echo "missing $gol"; exit 1; }
    [[ -f "$c" ]] || { echo "missing $c"; exit 1; }

    clang -O2 -o "$OUT/c_bin" "$c"
    "$ALGOL26" build "$gol" -o "$OUT/gol_bin" >/dev/null 2>&1

    l_ms=$(min_ms "$REPS" "$OUT/gol_bin")
    c_ms=$(min_ms "$REPS" "$OUT/c_bin")
    ratio=$(awk -v l="$l_ms" -v c="$c_ms" 'BEGIN{ if (c>0) printf "%.1f", l/c; else print "inf" }')

    printf '%-10s  %8dms  %8dms  %7sx\n' "$name" "$l_ms" "$c_ms" "$ratio"
done

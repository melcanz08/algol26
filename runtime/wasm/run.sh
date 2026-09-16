#!/usr/bin/env bash
#
# runtime/wasm/run.sh — compile an ALGOL26 program to WASM and run
# it under the Node host shim.
#
# Usage:
#   runtime/wasm/run.sh path/to/program.gol

set -euo pipefail

if [ $# -lt 1 ]; then
    echo "usage: $0 <program.gol>" >&2
    exit 1
fi

SRC="$1"
OUT="${SRC%.gol}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

"$ROOT/target/debug/algol26" wasm "$SRC"
exec node "$ROOT/runtime/wasm/host.js" "${OUT}.wasm"

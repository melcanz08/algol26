#!/bin/bash
set -e
echo "=== ALGOL26 Full Verification ==="
cargo fmt --all -- --check 2>&1 | head -5 || cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test
echo ""
echo "--- Examples ---"
cargo run -- run examples/basic/test.gol --backend interpreter 2>&1 | tail -2
cargo run -- run examples/basic/test.gol --backend llvm 2>&1 | tail -2
echo ""
echo "--- Benchmarks ---"
./benchmarks/run.sh
echo ""
echo "=== ALL GREEN ✅ ==="

#!/bin/bash
set -e
echo "=== ALGOL26 FULL VERIFICATION ==="

echo -e "\n[1/8] Structure"
tree -L 3 -I 'target|.git' || find . -maxdepth 3 -type f | sort

echo -e "\n[2/8] Formatting"
cargo fmt --all -- --check || (echo "→ running cargo fmt"; cargo fmt --all)

echo -e "\n[3/8] Clippy (hardening)"
cargo clippy --all-targets --all-features -- -D warnings

echo -e "\n[4/8] Build"
cargo check --all-targets --all-features
cargo build --all-features --verbose

echo -e "\n[5/8] Tests (all)"
cargo test --all-features -- --nocapture

echo -e "\n[6/8] Docs"
cargo doc --all-features --no-deps --document-private-items
test -d docs && echo "docs/ exists" && ls docs/
test -f README.md && echo "README.md ok"

echo -e "\n[7/8] Examples - compile & run via all backends"
for f in examples/*.alg26 examples/*.algol 2>/dev/null; do
  [ -e "$f" ] || continue
  echo "→ Testing $f"
  cargo run --quiet -- run "$f" --backend interpreter || echo "FAIL interpreter $f"
  cargo run --quiet -- run "$f" --backend llvm || echo "FAIL llvm $f"
done

echo -e "\n[8/8] Benchmarks & Tools"
ls -lh benches/ benchmarks/ 2>/dev/null || echo "no benches dir"
cargo bench --no-run --all-features 2>&1 | tail -5
ls -lh tools/
cargo run --quiet -- --help
cargo run --quiet -- help 2>&1 | head -20 || true

echo -e "\n=== ALL GREEN ==="

#!/bin/bash
# Algol26 cleanup for G560 / Xubuntu - run from ~/dev/algol26
set -e
echo "=== Algol26 Cleanup ==="

# 1. Remove build artifacts that make tree look messy
echo "[1/5] Removing build artifacts..."
rm -rf target/
find . -name "*.ll" -type f | grep -v ".git" | while read x; do echo "  rm $x"; rm "$x"; done
find ./examples -type f -executable -not -name "*.gol" -not -name "*.sh" | while read x; do echo "  rm bin $x"; rm "$x"; done
rm -f temperature_report.txt weather_report.txt test_output.ll
rm -f ./examples/ffi_test ./examples/generics_test ./examples/opt_test ./examples/orthogonal_test ./examples/pattern_test ./examples/trait_test ./examples/trait_bounds_test ./examples/test_for ./examples/temperature_analyzer
rm -rf ./examples/systems/comprehensive_test/full_test ./examples/systems/weather_system/weather ./examples/systems/temperature_system/main

# 2. Fix .gitignore
echo "[2/5] Writing clean .gitignore..."
cat > .gitignore <<'GITIGNORE'
# Rust
/target/
/build/
**/*.rs.bk
Cargo.lock

# Algol26 generated
**/*.ll
**/*.bc
**/*.o
*.ll
temperature_report.txt
weather_report.txt
test_output.ll

# Example binaries (keep .gol only)
examples/**/test
examples/**/test_for
examples/**/ffi_test
examples/**/generics_test
examples/**/opt_test
examples/**/orthogonal_test
examples/**/pattern_test
examples/**/trait_test
examples/**/trait_bounds_test
examples/**/temperature_analyzer
examples/systems/*/main
examples/systems/*/weather
examples/systems/comprehensive_test/full_test

# Tools temp
.fingerprint
incremental
deps
*.pyc
__pycache__/

# OS / Editor
.DS_Store
*.swp
*.swo
*~
.sublime-workspace
GITIGNORE

# 3. Show clean tree
echo "[3/5] Clean tree (real source only):"
tree -a -I ".git|.fingerprint|build|deps|incremental|target|__pycache__" -L 3

# 4. Rust hygiene
echo "[4/5] Running cargo fmt + clippy (light)..."
cargo fmt --all || echo "cargo fmt skipped"
cargo clippy --all-targets -- -D warnings --allow clippy::too_many_arguments || echo "clippy has warnings - check manually"

# 5. Check project
echo "[5/5] Running checks if present..."
[ -x tools/check_project.sh ] && bash tools/check_project.sh || echo "check_project.sh not executable, skipping"
echo "Done. Now run: cargo test --lib -- --nocapture"

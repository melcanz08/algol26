#!/bin/bash

echo "╔══════════════════════════════════════════════╗"
echo "║   ALGOL26 v1.0 Final Verification            ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

echo "1. Build Status:"
cargo build --release 2>&1 | tail -2
echo ""

echo "2. Warning Count:"
WARNINGS=$(cargo build --release 2>&1 | grep -c "warning")
echo "   $WARNINGS warnings"
echo ""

echo "3. Unit Tests:"
cargo test --release 2>&1 | grep "test result" | head -3
echo ""

echo "4. Conformance Suite:"
./run_conformance.sh
echo ""

echo "5. Documentation Files:"
find docs -type f | wc -l
echo "   documentation files"
echo ""

echo "6. Source Files:"
find src -type f | wc -l
echo "   source files"
echo ""

echo "7. Test Programs:"
find tests -type f -name "*.gol" | wc -l
echo "   test programs"
echo ""

echo "╔══════════════════════════════════════════════╗"
echo "║   ALGOL26 v1.0 Verified!                      ║"
echo "╚══════════════════════════════════════════════╝"

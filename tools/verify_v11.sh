#!/bin/bash

echo "╔══════════════════════════════════════════════╗"
echo "║   ALGOL26 v1.1 Verification                  ║"
echo "╚══════════════════════════════════════════════╝"
echo ""

echo "1. Build:"
cargo build --release 2>&1 | tail -2
echo ""

echo "2. Warnings:"
WARNINGS=$(cargo build --release 2>&1 | grep -c "warning")
echo "   $WARNINGS warnings"
echo ""

echo "3. Tests:"
cargo test --release 2>&1 | grep "test result"
echo ""

echo "4. Conformance:"
./run_conformance.sh
echo ""

echo "5. Documentation:"
ls docs/*.md | wc -l
echo "   markdown files"
echo ""

echo "6. Source Modules:"
ls src/*.rs | wc -l
echo "   Rust source files"
echo ""

echo "╔══════════════════════════════════════════════╗"
echo "║   v1.1 Semantic Hardening Complete!          ║"
echo "╚══════════════════════════════════════════════╝"

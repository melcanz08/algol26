#!/bin/bash

echo "╔══════════════════════════════════════════╗"
echo "║   ALGOL26 v0.8 Conformance Test Suite    ║"
echo "╚══════════════════════════════════════════╝"
echo ""

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0

# Test valid programs
echo "Valid Programs:"
echo "────────────────"
for file in tests/programs/valid/*.gol; do
    if algol26 "$file" --emit-llvm > /dev/null 2>&1; then
        echo -e "  ${GREEN}✅${NC} $(basename $file)"
        PASS=$((PASS+1))
    else
        echo -e "  ${RED}❌${NC} $(basename $file)"
        FAIL=$((FAIL+1))
    fi
done
echo ""

# Test invalid programs
echo "Invalid Programs (should fail):"
echo "────────────────"
for file in tests/programs/invalid/*.gol; do
    if algol26 "$file" > /dev/null 2>&1; then
        echo -e "  ${RED}❌${NC} $(basename $file) - should have failed!"
        FAIL=$((FAIL+1))
    else
        echo -e "  ${GREEN}✅${NC} $(basename $file)"
        PASS=$((PASS+1))
    fi
done
echo ""

echo "╔══════════════════════════════════════════╗"
echo "║   Results: $PASS passed, $FAIL failed          ║"
echo "╚══════════════════════════════════════════╝"

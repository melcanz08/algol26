#!/bin/bash
echo "=== ALGOL26 Level 2/3 Hardening Audit ==="
echo ""

echo "1. PANIC SURFACE (unwrap/expect/panic in src/)"
grep -rn "unwrap()\|expect(\|panic!(\|todo!()\|unimplemented!" src --include="*.rs" | grep -v "test" 
echo ""
echo "Count:"
grep -rn "unwrap()\|expect(\|panic!(" src --include="*.rs" | wc -l

echo ""
echo "2. UNSAFE BLOCKS"
grep -rn "unsafe" src --include="*.rs"
echo "Count: $(grep -rn "unsafe" src --include="*.rs" | wc -l)"

echo ""
echo "3. BORROW CHECKER CORE"
wc -l src/semantics/escape.rs src/semantics/race.rs src/semantics/semantic.rs src/semantics/semantic_builder.rs 2>/dev/null

echo ""
echo "4. DIFFERENTIAL COVERAGE"
cargo test differential -- --nocapture 2>&1 | tail -20

echo ""
echo "5. FUZZ - No panic guarantee"
cargo test fuzz -- --nocapture 2>&1 | tail -20

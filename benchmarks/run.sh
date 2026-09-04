#!/bin/bash
# ALGOL26 Benchmark Runner

echo "=== ALGOL26 Compiler Benchmarks ==="
echo ""
echo "Program | Bytes | Compile Time | RSS"
echo "--------|-------|--------------|-----"

for program in benchmarks/programs/*.gol; do
    name=$(basename "$program" .gol)
    bytes=$(wc -c < "$program")
    
    # Verify it compiles first
    if target/debug/algol26 "$program" >/dev/null 2>&1; then
        # Measure
        result=$(/usr/bin/time -f "%e|%M" target/debug/algol26 "$program" 2>&1 >/dev/null)
        time=$(echo "$result" | cut -d'|' -f1)
        rss=$(echo "$result" | cut -d'|' -f2)
        printf "%-8s | %5d | %12s | %s KB\n" "$name" "$bytes" "${time}s" "$rss"
    else
        echo "$name: FAILED to compile"
    fi
done

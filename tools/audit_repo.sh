#!/bin/bash
echo "=== CARGO.TOML ==="
cat Cargo.toml
echo -e "\n=== TOP LEVEL ==="
ls -lh
echo -e "\n=== SRC STRUCTURE ==="
find src -type f | sort
echo -e "\n=== TESTS ==="
find tests -type f | sort
ls -lh tests/
echo -e "\n=== EXAMPLES ==="
ls -lh examples/ 2>/dev/null || echo "no examples/"
find examples -type f 2>/dev/null | sort
echo -e "\n=== BENCHMARKS / BENCHES ==="
ls -lh benches/ 2>/dev/null; ls -lh benchmarks/ 2>/dev/null; ls -lh bench/ 2>/dev/null
find . -maxdepth 2 -name "*bench*" -type f | sort
echo -e "\n=== DOCS ==="
ls -lh docs/ 2>/dev/null || echo "no docs/"
find docs -type f 2>/dev/null | sort
ls -lh *.md
echo -e "\n=== TOOLS ==="
ls -lh tools/
cat tools/*.sh 2>/dev/null | head -100
echo -e "\n=== DEAD CODE CHECK ==="
echo "-- files not imported anywhere? --"
grep -r "mod " src --include="*.rs" | sort
echo -e "\n=== DOCS CONTENT QUICK SCAN ==="
for f in README.md docs/*.md *.md 2>/dev/null; do [ -f "$f" ] && echo "--- $f ---" && head -30 "$f"; done

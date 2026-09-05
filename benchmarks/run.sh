#!/bin/bash
echo "=== ALGOL26 Benchmarks ==="
for program in benchmarks/programs/*.gol; do
  [ -e "$program" ] || continue
  name=$(basename "$program" .gol)
  echo -n "$name: "
  if cargo run --quiet -- run "$program" --backend llvm 2>&1 | grep -q "Output"; then
    echo "OK"
  else
    cargo run -- run "$program" --backend llvm 2>&1 | tail -3
  fi
done

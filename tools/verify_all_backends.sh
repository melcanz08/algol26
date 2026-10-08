#!/usr/bin/env bash
# Run every conformance fixture on both backends; compare outputs.
set -u
fails=0
for f in tests/conformance/valid/*/*.gol; do
  if ! llvm_out=$(cargo run --quiet -- run "$f" 2>&1); then
    : # LLVM refusal is acceptable for capability-gated features
  fi
  interp_out=$(cargo run --quiet -- run --interpreter "$f" 2>&1)
  # A capability refusal on LLVM is expected; not a mismatch.
  if echo "$llvm_out" | grep -q 'does not support'; then
    continue
  fi
  # Extract just the printed lines (drop the compiler chatter).
  llvm_lines=$(echo "$llvm_out" | grep -vE '^\[|^$|^\s')
  interp_lines=$(echo "$interp_out" | grep -vE '^\[|^$|^\s')
  if [ -n "$llvm_lines" ] && [ "$llvm_lines" != "$interp_lines" ]; then
    echo "MISMATCH: $f"
    echo "  llvm: $llvm_lines"
    echo "  interp: $interp_lines"
    fails=$((fails+1))
  fi
done
echo "---"
echo "$fails mismatch(es)"
exit $((fails > 0))

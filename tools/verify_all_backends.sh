#!/usr/bin/env bash
# Run every conformance fixture on both backends; compare outputs.
set -u
fails=0
vacuous=0
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
  # A fixture that produces no output on either backend compiles and
  # runs but exercises nothing observable. Reported as a signal, not
  # a failure — the fixture may legitimately be a pure "does it
  # compile?" check. See enum_types/variant_value.gol for an example
  # of a fixture that was promoted from vacuous to observable.
  if [ -z "$llvm_lines" ] && [ -z "$interp_lines" ]; then
    echo "VACUOUS: $f (neither backend produced output)"
    vacuous=$((vacuous+1))
  fi
done
echo "---"
echo "$fails mismatch(es), $vacuous vacuous"
exit $((fails > 0))

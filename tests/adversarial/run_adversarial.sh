#!/usr/bin/env bash
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_DIR="$ROOT/tests/adversarial"
BIN="${ALGOL26_BIN:-$ROOT/target/debug/algol26}"
[[ -x "$BIN" ]] || BIN="${ALGOL26_BIN:-$ROOT/target/release/algol26}"

if [[ ! -x "$BIN" ]]; then
  echo "ALGOL26 binary not found. Set ALGOL26_BIN or build the project first."
  exit 2
fi

pass=0; fail=0; review=0

for test in "$TEST_DIR"/*.gol; do
  name="$(basename "$test")"
  expected="$(grep -m1 '^// EXPECT:' "$test" | sed 's#^// EXPECT: ##')"
  out="$(mktemp)"
  # Parse optional backend directive. Default: LLVM.
  backend="$(grep -m1 '^// BACKEND:' "$test" | sed 's#^// BACKEND: ##')"
  backend="${backend:-llvm}"

  if [[ "$backend" == "interpreter" ]]; then
      "$BIN" --interpreter "$test" >"$out" 2>&1
  else
      "$BIN" "$test" >"$out" 2>&1
  fi
  status=$?

  case "$expected" in
    REJECT)
      if [[ $status -ne 0 ]]; then echo "PASS-CORRECT $name"; ((pass++))
      else echo "FAIL-ACCEPTED $name"; ((fail++)); fi ;;
    ACCEPT)
      if [[ $status -eq 0 ]]; then echo "PASS-CORRECT $name"; ((pass++))
      else echo "FAIL-WRONG $name"; ((fail++)); fi ;;
    RUNTIME-TRAP)
      # Compilation must succeed; the compiled binary must then trap.
      if [[ $status -ne 0 ]]; then
        echo "FAIL-COMPILE $name (expected runtime trap, compilation failed)"
        ((fail++))
      else
        bin="${test%.gol}"
        if [[ -x "$bin" ]]; then
          run_status=0
          "$bin" >/dev/null 2>&1 || run_status=$?
          if [[ $run_status -ne 0 ]]; then
            echo "PASS-CORRECT $name (trapped, exit $run_status)"
            ((pass++))
          else
            echo "FAIL-NO-TRAP $name (compiled, ran cleanly, expected trap)"
            ((fail++))
          fi
        else
          echo "FAIL-NO-BINARY $name"
          ((fail++))
        fi
      fi ;;
    "ACCEPT-DEFER")
      # Positive control with defer semantics — accepts, runs,
      # cleanup fires before the return value is produced.
      if [[ $status -eq 0 ]]; then echo "PASS-CORRECT $name"; ((pass++))
      else echo "FAIL-WRONG $name"; ((fail++)); fi ;;
    REVIEW|"REJECT OR RUNTIME-TRAP"|"REJECT OR REQUIRE UNSAFE")
      echo "REVIEW $name (expected: $expected)"
      ((review++)) ;;
    *) echo "REVIEW $name (unknown expectation: $expected)"; ((review++)) ;;
  esac
  rm -f "$out"
done

echo
echo "Results: pass=$pass fail=$fail review=$review"
exit $(( fail > 0 ? 1 : 0 ))

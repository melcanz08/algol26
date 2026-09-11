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
  "$BIN" "$test" >"$out" 2>&1
  status=$?

  case "$expected" in
    REJECT)
      if [[ $status -ne 0 ]]; then echo "PASS-CORRECT $name"; ((pass++))
      else echo "FAIL-ACCEPTED $name"; ((fail++)); fi ;;
    ACCEPT)
      if [[ $status -eq 0 ]]; then echo "PASS-CORRECT $name"; ((pass++))
      else echo "FAIL-WRONG $name"; ((fail++)); fi ;;
    "REJECT OR RUNTIME-TRAP"|"REJECT OR REQUIRE UNSAFE"|"ACCEPT-DEFER")
      echo "REVIEW $name (expected: $expected)"
      ((review++)) ;;
    *) echo "REVIEW $name (unknown expectation: $expected)"; ((review++)) ;;
  esac
  rm -f "$out"
done

echo
echo "Results: pass=$pass fail=$fail review=$review"
exit $(( fail > 0 ? 1 : 0 ))

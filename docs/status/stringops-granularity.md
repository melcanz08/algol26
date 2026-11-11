# StringOps granularity gap

**Status:** RESOLVED (commit 12a68ab, 2026-11-12) — Option B: all 6 missing symbols ported to host.js
**Discovered:** session ending 2026-11-11
**Affects:** `src/backends/capabilities/mod.rs`, `wasm()`

## The problem

`Feature::StringOps` bundles several distinct string operations under
one capability gate:

| Operation | Backed by | Works on WASM today? |
|---|---|---|
| `String.length` | `algol26_strlen_utf8` | ✅ (host.js provides it) |
| `String.compare` | `strcmp` | ✅ (host.js provides it) |
| `String.concat` | inline IR in `register_stdlib` | ✅ (no runtime call) |
| `String.substring` | `algol26_string_substring` | ❌ (missing from host.js) |
| `String.charAt` | `algol26_string_char_at` | ❌ (missing) |
| `String.trim` | `algol26_string_trim` | ❌ (missing) |
| `String.to_upper` | `algol26_string_to_upper` | ❌ (missing) |
| `String.to_lower` | `algol26_string_to_lower` | ❌ (missing) |
| `String.charAt` (Char) | `algol26_string_char_at_cp` | ❌ (missing) |
| `Char.to_string` | `algol26_char_to_string` | ❌ (missing) |
| `String.to_chars` | `algol26_string_to_chars` | ❌ (missing) |

Because the gate is all-or-nothing, `wasm()` has to choose between:

- **Claim `StringOps` supported** → 8 of 10 operations produce modules
  that fail at load time with `unresolved imports`. That's the
  category-2 bug we just reverted.
- **Refuse `StringOps`** → `String.length` and `String.compare` are
  refused even though they work. Over-refusing.

The current state (post-commit XXX) is the second: WASM refuses
`StringOps` entirely.

## Fix options

**Option A — split the feature.** Introduce `Feature::StringLength`
(gates `String.length` + `String.compare` + `String.concat`) and keep
`Feature::StringOps` for the ported subset. Update:
- `capabilities/mod.rs`: `Feature` enum, `wasm()`, `llvm()`,
  `interpreter()`
- `capabilities/scan.rs`: what IR constructs trigger each feature
- `capabilities/tests.rs`: new tests per feature
- `tests/coverage_matrix.rs`: matrix row per feature
- `tests/coverage_maturity.rs`: maturity per feature
- `tests/cap_impl/expected_matrix.json`: fixture per feature
- Every fixture whose `// supported:` header references string ops

This is the correct fix but touches many files. ~2 hours.

**Option B — keep `StringOps` monolithic, port all symbols.** Port
the 8 missing symbols to `host.js`, then re-flip `StringOps` for
WASM in one commit. ~2.5 hours (ports are mechanical, but each needs
a fixture to verify).

**Option C — split at the `builtins.rs` level.** Mark each builtin
with the feature it belongs to, and have `scan_features` derive the
set from the builtins used. Removes the need for a hand-maintained
feature table. Most architecturally correct but requires refactoring
`scan.rs`. ~4 hours.

## Recommendation

Option B for the next session — ports are mechanical and they unlock
the full StringOps surface for WASM in one go. Option A can follow if
partial string support becomes a common source of user confusion.

## Fixtures that would benefit

None of these have WASM headers today; adding them as part of the
port work will prevent the same drift from recurring:
- tests/fixtures/string_substring.gol (if it exists)
- tests/fixtures/string_case_conversion.gol (if it exists)
- tests/fixtures/string_trim.gol (if it exists)

If they don't exist, add one fixture per ported operation.

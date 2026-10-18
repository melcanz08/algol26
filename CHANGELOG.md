# Changelog

## Unreleased

### Added

- **Sum types (ADR 0055)** — `union` declarations with payloads
  and recursive self-reference, construction (`Json.Num(42)`),
  pattern matching (`case Num(n) then ...`), guards
  (`case Pos(n) if n > 0 then ...`), exhaustive checking, and
  cross-file imports. Interpreter-only; LLVM and WASM refuse at
  the capability boundary. See `examples/json/` for the end-to-end
  demonstration.

- **`String.charAt(s, i) -> Option<String>`** — codepoint-indexed
  access to a single character. `Some(1-codepoint string)` for
  valid indices, `None` for out-of-range (negative or past the
  end). Works on interpreter and LLVM. The C implementation
  (`algol26_string_char_at` in `runtime/string.c`) shares the
  codepoint-walking discipline with `substring` and
  `strlen_utf8`; the LLVM arm wraps the pointer in
  `{ i1, i8* }`. Corpus pin:
  `tests/corpus/corpus_50_string_char_at_utf8.gol`.

### Fixed

- **ADR 0043 — shared borrows are now checked at last use.** Prior
  to this change, a write to a variable with a live shared borrow
  was accepted — a soundness hole. `val r := &x; x := 2; print(*r)`
  now rejects at the write; `val r := &x; print(*r); x := 2` still
  compiles. Uses inside loops or `defer` bodies conservatively
  extend the borrow to the enclosing scope's exit. (commit `6819517`)
- **ADR 0057 — operator-at-end-of-line continuations now compile.**
  `val total := a +` followed by an indented `b` previously produced
  a spurious `Indent`, which `parse_unary` rejected. The lexer now
  suppresses `Indent`/`Dedent` when the previous line ends with a
  binary operator. The `parallel ... and` form is preserved by
  excluding `And`/`Or` from the continuation set. The
  `statement_start_column` field is now reset at the top of each
  function declaration, closing a cross-function state leak.
  `tests/adr_examples/0057_boundaries/pass_op_at_end_of_line.gol`
  flips from `EXPECT: REJECT` to `EXPECT: COMPILE`. (commit `123de0b`)
- **`String.substring` now indexes by codepoint, not bytes, on LLVM.**
  The interpreter's `substring` uses `chars().skip(start).take(length)`
  — codepoint-indexed. LLVM's inline IR used `strlen` + `memcpy` —
  byte-indexed. On non-ASCII input the two backends disagreed:
  `"héllo".substring(2, 1)` returned `"l"` on the interpreter and
  the second byte of `"é"` (invalid UTF-8) on LLVM. The old lowering
  was replaced by a C codepoint walker in `runtime/string.c`; the
  Rust side keeps only the declaration. Corpus was all ASCII and
  missed the divergence — `tests/corpus/corpus_49_string_substring_utf8.gol`
  now pins the UTF-8 cases. (commit `8518ca7`)
- **Differential test harness prefers the debug binary.** An
  in-test `find_compiler()` preferred `target/release/algol26`,
  which is only rebuilt on explicit `cargo build --release`. Any
  source change between a release build and a `cargo test` run was
  invisible to the differential tests — the harness ran against a
  stale binary. Surfaced while fixing the `substring` divergence
  (six differential tests failed on a `multiple definition` linker
  error caused by the stale binary still emitting the deleted
  inline IR). Candidate order flipped to debug-first, matching
  `differential_true.rs`. (commit `ea60478`)


### Changed

- Renamed the `procedure` keyword to `proc`. Programs using
  `procedure` must be updated to `proc`.

## [0.8.0-hardening] - 2026-05-13
### Level 5 Hardening Complete

#### 5.1 Negative Corpus (20 files)
- Expanded from 6 -> 20 invalid programs covering:
  - `07_missing_return`, `08_arity_mismatch`, `09_duplicate_decl`, `10_assign_to_val`
  - `11_break_outside_loop`, `12_defer_in_global`, `13_invalid_return_type`
  - `14_mut_borrow_of_val`, `15_redecl_function`, `16_invalid_for_type`
  - `17_deref_non_ref`, `18_invalid_binary_string_int`, `19_use_after_scope`
  - `20_invalid_nested_if_defer`, plus original 6 (`undefined_var`, `type_mismatch`, `invalid_syntax`, `invalid_array_index`, `double_borrow`, `use_after_move`)
- Enforcement: `test_negative_corpus_no_ice` asserts total>=20, invalid>=18, no ICE
- Invalid check uses **OR**: `SemanticIRBuilder diags OR SemanticAnalyzer Err` — required because borrow/move (E0007) is caught by Analyzer, while break/defer are caught by IR

#### 5.3 Stress
- `test_stress_100_vars_single_scope`, `test_stress_10_level_nested_if_for_defer_break_return`
- `test_stress_10_level_closure_capture`, nested functions, control flow, multiple imports

#### 5.4 Optimizer Safety
- Fixed string concat codegen (`String.concat` vs `+`)
- `test_optimization_preserves_borrow_semantics`: borrow ops before=2 after=2
- `test_optimization_diff_interpreter_complex` + preserves semantics for lists/strings/arithmetic
- Idempotent optimizer

#### 5.5 Diagnostics Quality
- `test_diag_double_borrow_e0007_with_help` — mentions var name + suggestion
- `test_diag_use_after_move_has_line` — E0007 with help

#### Tests
- `integration_tests`: **27/27 PASS**
- `release_hardening`: **17/17 PASS**

## [0.7.0] - Previous
- List ops, FFI isolation, interpreter hardening

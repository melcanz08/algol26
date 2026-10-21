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

- **`args()` on LLVM** — command-line arguments now work on both
  native backends. The `main` wrapper (`i32 @main(i32, ptr)`)
  forwards `(argc, argv)` to `runtime/args.c` via
  `algol26_args_init` before user main runs; `args()` returns a
  `List<String>` of user arguments (`argv[0]` skipped, matching
  the interpreter). The CLI now forwards post-`--` program
  arguments to the compiled binary (`Compiler::compile` and
  `run_binary` take `program_args`). First builtin to return a
  `List<T>` on LLVM — surfaced a gap in `compile_builtin_call`'s
  result-storage path, which now mirrors `Instruction::Call`'s
  list-shape descriptor registration. Corpus pin:
  `tests/corpus/corpus_51_args_llvm.gol`.

- **`Map<String, Int>` on LLVM** — extends the Map lowering beyond
  the initial `Map<Int, Int>` shape. `runtime/map_str_int.c` is a
  new typed runtime (strdup'd keys, FNV-1a hashing over the string
  bytes, `strcmp` equality, grow-on-full via rehashing — unlike
  `runtime/map.c`, which retains its fixed-16 capacity limitation).
  The four `Map.*` arms and the literal construction in `value.rs`
  now dispatch via a shared `map_suffix(key_ty, value_ty)` helper,
  so adding a third shape is one arm in the helper plus one C file.
  `Map<String, String>` and `Map.keys`/`.values` remain
  unimplemented on LLVM. Corpus pin:
  `tests/corpus/corpus_52_map_str_int_llvm.gol` (exercises insert,
  overwrite, get hit/miss, contains, length, and a 20-entry
  capacity-growth case). Also corrects stale STATUS.md prose that
  still described `Map` as interpreter-only.

- **`Map<Int, Int>` capacity growth.** `runtime/map.c` had a
  fixed 16-entry capacity; inserting a 17th entry entered an
  infinite loop in the linear-probe search (no empty slot to
  terminate on). The interpreter's `HashMap` grows unboundedly,
  so `Map<Int, Int>` with more than 16 entries worked on the
  interpreter and hung on LLVM — a differential failure the
  corpus never caught (`corpus_44_map_llvm` uses fewer than 16
  entries). `runtime/map.c` now doubles capacity and rehashes,
  matching `runtime/map_str_int.c` and the interpreter. New
  corpus pin: `tests/corpus/corpus_53_map_int_int_growth.gol`
  (25 entries, all retrievable). Standalone C test also added
  (`runtime/map.c` previously had none).

- **`String.trim` on LLVM** — trims leading/trailing Unicode
  whitespace, matching the interpreter's `str::trim()`. The C
  implementation (`algol26_string_trim` in `runtime/string.c`)
  walks codepoints (shared with `substring` and `char_at`) and
  classifies Unicode White_Space — not just ASCII — so it agrees
  with Rust's `char::is_whitespace` on NBSP, NEL, EN QUAD through
  HAIR SPACE, line/paragraph separators, IDEOGRAPHIC SPACE, and
  the ASCII range. Using `ctype.h`'s `isspace` would have diverged
  on non-ASCII whitespace and been unsafe on UTF-8 lead bytes.
  C test covers 17 cases including all Unicode ranges and UTF-8
  content preservation. Corpus pin:
  `tests/corpus/corpus_54_string_trim_llvm.gol`.
  `String.split` and `.join` remain interpreter-only.

- **`String.split` and `String.join` on LLVM** — the last two
  interpreter-only builtins now work on LLVM. `runtime/string.c`
  gains `algol26_string_split` (codepoint-aware when the separator
  is empty, byte-exact substring match otherwise, includes empty
  segments — matching Rust's `str::split`) and
  `algol26_string_join` (iterates a `List<String>` descriptor,
  concatenates with separator between elements — matching Rust's
  `slice::join`). The C test covers 20 cases including empty
  separator on UTF-8 input (`"héllo".split("")` → 7 codepoints
  with leading/trailing empties) and empty-list/empty-element join
  behavior. The `.split` arm returns a `{ptr, i64, i64}`
  descriptor built from the C's pre-filled `char**`; the `.join`
  arm extracts buffer and length from the compiled `List<String>`
  and calls the C. `Feature::StringSplit` moves into
  `llvm().supported`. Corpus pin:
  `tests/corpus/corpus_55_string_split_join_llvm.gol`. After this,
  no language feature is interpreter-only — every construct works
  on LLVM except those the capability matrix explicitly refuses
  (unions, channels, `try`/`catch`, `Map` shapes beyond Int/String
  keys, `Map.keys`/`.values`, and the documented `File.read`
  UTF-8 divergence).

- **`Map<String, String>` on LLVM** — extends the map family to
  a third realistic shape. `runtime/map_str_str.c` owns both keys
  and values (both `strdup`'d); insert-overwrite frees the old
  value, grow-rehash frees old key/value pairs. `get` returns an
  owned copy (the interpreter's `entries.get(&key).clone()`
  semantics), so subsequent map mutations cannot invalidate a
  previously-returned string — verified by the C test's explicit
  copy-semantics case. The `.get` codegen arm now branches on the
  value type to choose the out-pointer type (`i64` vs `ptr`) and
  the `Option<Int>` / `Option<String>` construction; the other
  three `Map.*` arms were already shape-agnostic via
  `map_suffix`. `Map<Int, String>` and `Map<Bool, T>` remain
  refused at codegen. Corpus pin:
  `tests/corpus/corpus_56_map_str_str_llvm.gol`.

- **`Map.keys` / `Map.values` on LLVM** — the last two map
  methods now work on all three supported shapes. Six new C
  functions (`algol26_map_{keys,values}_{int,str_int,str_str}`)
  collect the map's contents, `qsort` ascending (int64 numeric,
  string `strcmp` — matching the interpreter's `MapKey` derived
  `Ord` / Rust's `str::cmp` byte-lex), and return a malloc'd
  array with an out-pointer count. String elements are `strdup`'d
  so the caller owns them (per the `String.split` convention —
  the resulting `List<String>` holds owned pointers that are
  never freed, matching the existing leak trade-off). `values`
  sorts by key, so `zip(keys, values)` aligns. Six standalone C
  tests confirm empty-map, sort order, alignment, overwrite, and
  grow-past-capacity behavior. Codegen splits the previously
  combined `Map.keys | Map.values` arm into two, each dispatching
  by `map_suffix` and building a `List<K>` / `List<V>`
  descriptor from the returned array and out-pointer count.
  Corpus pin: `tests/corpus/corpus_57_map_keys_values_llvm.gol`
  (all three shapes in one program).

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

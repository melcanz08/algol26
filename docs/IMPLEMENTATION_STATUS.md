# ALGOL26 Implementation Status

Last updated: 2026-09-14

This document records what actually works, verified by the differential
corpus in `tests/corpus/`. A feature is only listed as "works" if there
is at least one corpus program exercising it end-to-end.

## Legend

- ✅ **Works** — verified by corpus program(s)
- ⚠️ **Interpreter only** — works in the interpreter, LLVM refuses or miscompiles
- ❌ **Parser only** — parser and analyzer accept it, no backend runtime
- ⛔ **Not supported** — explicitly refused with a clear error
- ❓ **Untested** — no corpus coverage yet

## Feature Matrix

| Feature | Parser | Analyzer | Interpreter | LLVM | Corpus |
|---------|--------|----------|-------------|------|--------|
| `while` | ✅ | ✅ | ✅ | ✅ | 18–22, 34–36 |
| `for ... in` | ✅ | ✅ | ✅ | ✅ | 01, 02, 07, 12, 17 |
| `if` / `else` | ✅ | ✅ | ✅ | ✅ | 02, 19 |
| `break` / `continue` | ✅ | ✅ | ✅ | ✅ | 19, 22 |
| `defer` | ✅ | ✅ | ✅ | ✅ | 06, 12, 33 |
| Functions / procedures | ✅ | ✅ | ✅ | ✅ | 03–05, 11, 16, 34–36 |
| Return values | ✅ | ✅ | ✅ | ✅ | 04, 05, 09 |
| Bare call statements | ✅ | ✅ | ✅ | ✅ | 33 (fixed 2026-09-14) |
| Lists + indexing | ✅ | ✅ | ✅ | ✅ | 01, 07, 15 |
| `List.length` / `List.sum` | ✅ | ✅ | ✅ | ✅ | 07, 15 |
| Strings | ✅ | ✅ | ✅ | ✅ | 08, 15 |
| `String.length` / `to_upper` / `to_lower` | ✅ | ✅ | ✅ | ⚠️ | 08 |
| `Math.*` | ✅ | ✅ | ✅ | ⚠️ | — |
| `Option` / `Some` / `None` | ✅ | ✅ | ✅ | ❌ | 13 |
| `match` (literal patterns) | ✅ | ✅ | ✅ | ✅ | — |
| `match` (binding patterns) | ✅ | ✅ | ✅ | ⛔ | 13 |
| `Result` / `Ok` / `Error` | ✅ | ✅ | ✅ | ⛔ | 14 |
| `try` / `catch` | ✅ | ✅ | ✅ | ⛔ | 14 |
| Traits + impls | ✅ | ✅ | ✅ | ⚠️ | 28, 29, 37 |
| `region` | ✅ | ✅ | ✅ | ⚠️ | 30, 31 |
| `unsafe` | ✅ | ✅ | ✅ | ⚠️ | 32, 33 |
| `spawn` / `parallel` | ✅ | ✅ | ✅ | ⛔ | — |
| `channel` / `send` / `receive` | ✅ | ✅ | ❌ | ❌ | 23–26 |
| `alloc` in `var` position | ❌ | — | — | — | — |
| `alloc(x)` as statement | ✅ | ✅ | ✅ | ❓ | — |
| `free` | ✅ | ✅ | ✅ | ❓ | — |
| `extern` (FFI) | ✅ | ✅ | ⛔ | ✅ | — |
| `import` | ✅ | ✅ | ✅ | ✅ | — |

## Known Gaps

Gaps are documented by `tests/corpus/*.gol` programs marked
`// KNOWN_FAILURE:` or `// BACKEND: interpreter`.

### LLVM backend gaps (interpreter works)

- **`match` with pattern bindings** (`corpus_13`). LLVM refuses with a
  clear error. Root cause: the IR builder records bindings in its
  internal scope but does not emit `Declare` instructions for them,
  so the LLVM codegen cannot resolve the binding.
- **Iterating over a list parameter** (`corpus_10`). LLVM's iterator
  setup is only done for locally-created list literals. Lists passed
  as function arguments have no iterator state.
- **`try` / `catch` and `Result` values** (`corpus_14`). No LLVM
  lowering. Refused cleanly.

### Parser gaps

- **`alloc` cannot appear in `var` position.** `parse_stmt` handles
  `alloc(x)` as a bare statement producing a `FunctionCall`, but
  `parse_expr` does not recognize the `Alloc` token, so
  `val p := alloc(8)` fails to parse.

### Unimplemented features

- **Channels** (`corpus_23`–`corpus_26`). Parser and analyzer
  support `channel c: T`, `send c, v`, and `receive c into x`.
  The IR builder pushes `SemanticInstruction::Send` / `Receive`,
  but no backend executes them. `send` and `receive` are effectively
  no-ops.

## Conventions

**Language design note:** methods are called as `x.method(x)` — the
receiver is passed explicitly as the first argument. Zero-argument
methods are called as `x.method()`. There is no implicit `self`.

**Backend selection:** programs that require interpreter-only features
declare `// BACKEND: interpreter` at the top of the file. The
`corpus_diff` harness runs them through the interpreter; others run
through LLVM by default.

## How to add a feature to this document

1. Write a corpus program in `tests/corpus/` that exercises the feature.
2. Run `cargo test --test corpus_diff`.
3. If it passes, mark the feature ✅ in the matrix.
4. If it fails in a backend, mark it ⚠️ or ⛔, add a `// BACKEND:` or
   `// KNOWN_FAILURE:` header, and add a bullet under "Known Gaps."
5. Update the "Last updated" date.

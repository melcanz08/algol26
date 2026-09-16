# ALGOL26 Implementation Status

Last updated: 2026-09-14

This document records what actually works, verified by the differential
corpus in `tests/corpus/`. A feature is only listed as "works" if there
is at least one corpus program exercising it end-to-end.

## Known divergences between backends

The interpreter and LLVM backends agree on observable behavior for
all corpus programs. The following non-corpus divergences are
documented so future work can close them:

- **Region auto-free does not track outer-pointer overwrite.**
  If a `region` block reassigns a `var` whose pointer was allocated
  *outside* the region, the outer allocation is not freed at
  region exit. Workaround: do not reuse an outer pointer variable
  as region-local scratch storage — declare a fresh `var` inside
  the block.
- **Variadic FFI argument *types* are not validated.** `extern "C"
  function printf(fmt: String, ...)` accepts any number of arguments
  at or above the fixed count, and the analyzer records every
  argument's type in the type table. The extra arguments' types are
  not checked against the format string — that is C-level undefined
  behavior, and no compiler catches it. Callers must match the
  format specifiers to the argument types themselves.
- **WASM output requires a host shim.** The generated `.wasm`
  module imports `printf`, `exit`, `malloc`, `free`, and the C
  math library; it cannot execute without a host that provides
  those symbols.

## Deferred runtime modules (wired but not shared)

The modules `src/runtime/region.rs` and `src/runtime/region_memory.rs`
implement a `std::alloc`-based region allocator with parent/child
cascade on free. They are not used by the current pipeline: the
interpreter has its own heap and the LLVM backend relies on libc.
These modules are the seed of a shared runtime that a future
backend could consume.

## Legend

- ✅ **Works** — verified by corpus program(s)
- ⚠️ **Interpreter only** — works in the interpreter, LLVM refuses or miscompiles
- ❌ **Parser only** — parser and analyzer accept it, no backend runtime
- ⛔ **Not supported** — explicitly refused with a clear error
- 🔶 **Syntax only** — parses and analyzes; no backend executes it
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
|  `alloc` in `var` position | ❌ | — | — | — | — |
| `alloc(x)` as statement | ✅ | ✅ | ✅ | ❓ | — |
| `free` | ✅ | ✅ | ✅ | ❓ | — |
| `extern` (FFI) | ✅ | ✅ | ⛔ | ✅ | — |
| `import` | ✅ | ✅ | ✅ | ✅ | — |

## Known Gaps

Gaps are documented by `tests/corpus/*.gol` programs marked
`// KNOWN_FAILURE:` or `// BACKEND: interpreter`.

### Analyzer soundness gaps (accepted, not yet fixed)

These are cases where the analyzer accepts programs that a stricter
borrow system would reject. They are documented rather than silently
patched because fixing each one correctly requires design decisions
that belong in their own ADR, and past attempts have regressed real
programs.

- **`&mut x` in a call argument is not registered as a borrow.**
  Writing `f(&mut x)` does not mark `x` as mut-borrowed, so
  `f(&mut x); g(&mut x)` in the same scope compiles even though both
  functions may write through the same reference. A naive fix
  (marking the borrow in `Expr::MutBorrow`) makes
  `increment(&mut value); print(value);` fail, because the current
  borrow model is fully lexical and has no way to release a borrow
  when the statement that created it completes. A proper fix needs
  statement-scoped release (small, ~2 days) or full non-lexical
  lifetimes (large). Not a memory-safety hole in the current
  runtime — references are addresses, not aliased Rust-style
  references — but looser than Rust.

- **`escape.rs` is not wired into the pipeline.** The module
  implements a reference-outlives-scope analysis, but no pass
  constructs an `EscapeAnalyzer` or consumes its output. Escape
  detection is therefore not part of the compiler's safety story
  yet, despite being referenced by ADR-0005. Either wire it up or
  delete it; right now it is unused.

- **`flow_analyzer.rs` is a stub.** Definite-assignment analysis,
  reachability of variable uses, and borrow-state joins at CFG
  merge points are not implemented. See the module doc comment for
  the explicit statement of scope.

### IR correctness (audited and fixed)

Fixed in the Phase 3 audit:

- Constant propagation no longer leaks constants across branch
  blocks. Before the fix, a conditional assignment in one branch
  could rewrite a join-block use with the wrong value.
- Verifier recursively checks `Some`/`None`/`Ok`/`Error` payloads.
  Before the fix, an ill-typed payload inside an `Option` or
  `Result` constructor was silently accepted.
- Verifier rejects `Float` arguments for `Int` parameters. Now
  matches the analyzer's `can_coerce_to` rules.
- Optimizer skips folding large `Int` arithmetic where `f64`
  intermediates would lose precision (above 2^53).

Deferred (design/cleanup, not correctness bugs):

- `VerifiedIR::from_verify_pass` is a `pub(crate)` typestate hole;
  relies on caller discipline.
- `builtins.rs` signature table is hand-synced with
  `builder/build.rs`; no test enforces the sync.
- Dominance-aware constant propagation would allow cross-block
  folding (currently disabled to preserve correctness).
- Data-flow joins at CFG merges use first-visited-wins, not a
  proper fixed-point.
- `Spawn`/`Fork` capture semantics are not verified.

### Backend audit (Phase 4)

Fixed:

- LLVM codegen: `NotEqual` on pointers now emits `NE` (was `EQ`).
- LLVM codegen: switch with no default emits `unreachable` (was an
  arbitrary block).
- LLVM codegen: list reassignment rebuilds the array instead of
  leaving `list_arrays` pointing at the old allocation.
- LLVM codegen: `Some`/`Ok`/`Error`/`None` are refused at capability
  check instead of silently unwrapped. The interpreter handles them.
- LLVM codegen: `alloc`/`free` are refused (neither backend has a
  heap model).
- Production LLVM path now goes through `Backend::compile`, so
  `module.verify()` and the capability check run on the same path
  users exercise.
- Interpreter: iteration limit raised from 10_000 to 100_000_000;
  `Fork` runs every branch sequentially (was: only the first);
  Math builtins complete; `List.max`/`min`, `String.substring`
  added; Math builtins accept `Int` inputs.
- WASM: dead `validate_wasm_compatibility` replaced by the
  capability system; `module.verify()` runs before write.
- Capability: `scan_call_name` uses `builtin_signatures()` instead
  of a prefix match — user-defined `String.helper` is no longer
  misclassified.
- `BackendOutput` now carries real data (paths, stdout).

Open (not fixed):

- `IteratorNext` fallback guesses arrays when `IteratorInit` did
  not record the iterator. Root cause: list-typed function
  parameters are not handled by `IteratorInit`. Two corpus
  programs hit this. Fix requires extending `IteratorInit`.
- Interpreter `eval_call` swallows user-function errors — the
  caller sees `Void` after an `eprintln!`. Requires `eval_*` to
  return `Result`.
- WASM output has unresolved C library imports (`printf`, `exit`,
  `sqrt`, `strlen`, `strcat`). Module is not executable without a
  host shim. Needs a design decision.
- `InterpreterBackend` clones the program per compile.
- Interpreter runtime errors are `Debug`-formatted.

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

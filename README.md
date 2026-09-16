# ALGOL26

**ALGOL 58 reimagined for 2026.**

A historically inspired systems programming language combining ALGOL's
clarity with indentation, compile-time safety, deterministic resource
management, safe concurrency, and native compilation via LLVM.

> **Control without unsafe defaults.**

**Version:** v0.8.0 · **License:** MIT · **Built with:** Rust 1.70+ and LLVM 17

---

## Quick Start

```bash
# Build
cargo build

# Run the test suite (337 tests, including a 37-program corpus)
cargo test --all-features

# Run a program through LLVM
./target/debug/algol26 run examples/basic/test.gol

# Or run through the interpreter
./target/debug/algol26 run --interpreter examples/basic/test.gol

# Just type-check
./target/debug/algol26 check examples/basic/test.gol
```

Run `./target/debug/algol26 --help` for the full CLI.

## Example

```gol
function add(x: Int, y: Int) -> Int
    return x + y

procedure main
    val result := add(5, 3)
    print(result)
```

## Language at a Glance

- **Indentation-based**, like Python — no braces, no semicolons
- **Immutable by default** (`val`), opt-in mutability (`var`)
- **Statically typed** with inference: `Int`, `float`, `Bool`, `String`,
  `List<T>`, `Option<T>`, `Result<T, E>`
- **Borrow checking** and **move semantics** enforced at compile time
- **Region-based memory** — no garbage collector
- **Traits and impls** with explicit receiver passing: `x.method(x)`
- **`defer`** for LIFO cleanup, **`try`/`catch`** for `Result` handling
- **`match`** on `Option`, `Result`, and literals
- **FFI** via `extern "C"` for calling into C libraries
- **`spawn`** and **`parallel`** blocks for structured concurrency
- **`region`** blocks and `alloc` / `free` for manual memory

> **Note on execution coverage:**
>
> - **`alloc` / `free`** work end-to-end through both backends.
>   The interpreter uses a simulated heap; LLVM lowers to libc
>   `malloc` / `free` (Step 5 wiring, tag `step5-done`).
> - **`region` blocks** work end-to-end with auto-free on both
>   backends. `RegionExit` frees the region's allocations in the
>   interpreter (Step 3 wiring) and in the LLVM backend (Step 6
>   wiring, tag `step6-done`). Explicit `free(p)` inside a region
>   is idempotent — LLVM nulls the pointer after freeing so
>   auto-free skips it. The one remaining asymmetry (reassigning a
>   `var` pointer inside a region leaks the earlier allocation in
>   LLVM but not in the interpreter) is documented in
>   `docs/IMPLEMENTATION_STATUS.md`.
> - **`extern "C"` FFI** works through LLVM. `as "symbol"` renaming
>   and `from "library"` linking are honored (Step 4b wiring, tag
>   `step4b-done`). Variadic externs (`...`) parse but do not
>   validate argument types — a known gap.
> - The WASM backend produces a module, but the module has
>   unresolved C-library imports (`printf`, `exit`, `sqrt`,
>   `strlen`, `strcat`, `malloc`, `free`) and is not directly
>   executable. See `docs/IMPLEMENTATION_STATUS.md` for the
>   corpus-verified state of every feature.

## Backends

| Backend | Output | Status |
|---------|--------|--------|
| Interpreter | Direct execution (semantic oracle) | Complete |
| LLVM | Native executable | Most features; refuses some (see below) |
| WASM | `.wasm` module | Compilation only; execution not wired up |

**For the accurate, corpus-verified feature matrix, see
[`docs/IMPLEMENTATION_STATUS.md`](docs/IMPLEMENTATION_STATUS.md).**
That file is the single source of truth for what works where.

## Known Limitations

These are verified gaps, tracked by corpus programs:

- **LLVM does not support `match` with pattern bindings.** Use the
  interpreter. (`tests/corpus/corpus_13_match_option.gol`)
- **LLVM does not support `try`/`catch` or `Result` values.** Use the
  interpreter. (`tests/corpus/corpus_14_try_catch.gol`)
- **LLVM cannot iterate over a list passed as a function parameter.**
  Use the interpreter. (`tests/corpus/corpus_10_deep_control.gol`)
- **Channels (`channel`, `send`, `receive`) parse and analyze but have
  no backend runtime.** Programs run but the operations are no-ops.
  (`tests/corpus/corpus_23`–`corpus_26`)
- **`alloc` cannot appear in a `var` declaration.** Only as a bare
  statement: `alloc(8)` works; `val p := alloc(8)` does not parse.

## Testing

```bash
cargo test --all-features
```

The suite has 14 test binaries covering:

- 137 unit tests (types, lexer, parser, IR, verifier, FFI)
- 43 differential tests (interpreter vs LLVM vs WASM)
- 49 semantic tests (borrow checker, traits, ownership)
- 40 IR tests (verification, optimization, defer, short-circuit)
- 27 integration tests (conformance, hardening, stress)
- 20 backend tests (independence, oracle)
- **37 corpus programs** in `tests/corpus/` — differential harness in
  `tests/corpus_diff.rs` runs every program and compares output

The corpus is the most important test suite. Every feature in
`IMPLEMENTATION_STATUS.md` is backed by at least one corpus program.
Adding a feature means adding a corpus program.

## How to Add a Corpus Program

1. Write a `.gol` program in `tests/corpus/`
2. Header lines declare expected behavior:
   ```
   // OUTPUT: expected line 1
   // OUTPUT: expected line 2
   ```
3. If the program requires the interpreter, add:
   ```
   // BACKEND: interpreter
   ```
4. If the program documents a known compiler bug, add:
   ```
   // KNOWN_FAILURE: short description
   ```
5. Run `cargo test --test corpus_diff`. If it passes, commit.

## Documentation

| Document | Purpose |
|----------|---------|
| [`docs/IMPLEMENTATION_STATUS.md`](docs/IMPLEMENTATION_STATUS.md) | Feature matrix + known gaps |
| [`docs/decisions/`](docs/decisions/) | Architecture Decision Records (0001–0009) |
| [`docs/releases/`](docs/releases/) | Release notes |
| [`docs/archive/`](docs/archive/) | Superseded docs, kept for history |
| [`docs/README.md`](docs/README.md) | Full index of all docs |

## Architecture

```
src/
├── common/        — types, diagnostics, span
├── frontend/      — lexer, parser, AST, module loader
├── semantics/     — analyzer (types, borrow, traits, race) + IR builder
├── ir/            — semantic IR, verifier, optimizer, loop desugar
├── backends/      — interpreter, LLVM codegen, WASM
├── runtime/       — region memory
├── compiler/      — pass pipeline, scheduler, registry
├── diagnostics/   — error rendering
└── ffi/           — C type definitions, FFI registry
```

Compilation pipeline:

```
Lex → Parse → Desugar → Expand Impl → Monomorphize → Type Check
→ Safety Check → Build IR → Verify → Optimize → Verify → Lower to Backend
```

## Safety Guarantees

| Guarantee | Enforced by |
|-----------|-------------|
| Type safety | Semantic analyzer |
| Immutability | Semantic analyzer |
| Bounds checking | IR verifier + runtime |
| Use-after-move | Analyzer + IR verifier |
| Borrow checking | Semantic analyzer |
| Trait bounds | Trait registry |
| IR well-formedness | VerifiedIR wrapper |
| No-panic on malformed input | Fuzz tests (700 iterations) |

See [`docs/decisions/`](docs/decisions/) for the reasoning behind each
guarantee.

## Contributing

See [`docs/README.md`](docs/README.md) for the doc taxonomy and how to
update each type. In short:

- **Reference docs** (like `IMPLEMENTATION_STATUS.md`) are audited against
  the code and must be accurate.
- **ADRs and release notes** are frozen — never updated, kept as history.
- **Superseded docs** move to `docs/archive/` rather than being deleted.

## License

MIT — Rommel Edorot Caneos

## Acknowledgments

ALGOL 58 (inspiration) · Python (indentation) · Rust (implementation)
· LLVM 17 (native backend)
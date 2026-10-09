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

# Run the test suite
cargo test --all-targets

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
rec Point
    x: Int
    y: Int

function sum_point(p: Point) -> Int
    return p.x + p.y

proc main
    val p := Point { x: 3, y: 4 }
    print(sum_point(p))

    match p
        case Point { x, y }
            print(x * y)
```

## Language at a Glance

- **Indentation-based**, like Python — no braces, no semicolons
- **Immutable by default** (`val`), opt-in mutability (`var`)
- **Statically typed** with inference. Scalar types: `Int`, `Float`,
  `Bool`, `String`. Container and structured types: `List<T>`,
  `Map<K, V>`, `Option<T>`, `Result<T, E>`, and `rec`-declared records.
- **`rec` structured data types** — declared fields, construction literals,
  field access, field assignment, pattern matching, cross-module use
- **`Map<K, V>`** — key-value container with `insert`, `get`, `contains`,
  `keys`, `values`, `length`. Keys are `Int`, `String`, or `Bool`.
- **`List.append(x)`** — dynamic growth; requires a `var` receiver
- **Structural `Copy`** — a record is `Copy` iff every field is `Copy`.
  `Point { x: Int, y: Int }` copies; `Person { name: String, age: Int }`
  moves. Non-`Copy` values follow the move/borrow rules.
- **Borrow checking** and **move semantics** enforced at compile time
- **Region-based memory** — no garbage collector
- **Traits and impls** — declared with `impl Trait for Type`, called
  with method syntax: `x.method()`
- **Generics** — `function first<T>(xs: List<T>) -> Option<T>`; type
  parameters bind from arguments, including inside container types
- **`defer`** for LIFO cleanup, **`try`/`catch`** for `Result` handling
- **`match`** on `Option`, `Result`, records, and literals
- **FFI** via `extern "C"` for calling into C libraries
- **`spawn`** and **`parallel`** blocks for structured concurrency
- **`region`** blocks and `alloc` / `free` for manual memory
- **`affirm(cond, msg)`** — always-on runtime assertions; a failing
  `affirm` prints the message and exits non-zero
- **`args()`** — command-line arguments; pass them after `--` on the CLI
- **String builtins**: `String.split`, `String.join`, `String.trim`,
  `String.substring`, `String.to_upper`, `String.to_lower`,
  `String.to_int`, `Int.to_string`

> **Note on execution coverage:**
>
> - **`alloc` / `free`** work end-to-end through both backends.
>   The interpreter uses a simulated heap; LLVM lowers to libc
>   `malloc` / `free`.
> - **`region` blocks** work end-to-end with auto-free on both
>   backends. Explicit `free(p)` inside a region is idempotent —
>   LLVM nulls the pointer after freeing so auto-free skips it.
> - **`extern "C"` FFI** works through LLVM. `as "symbol"` renaming,
>   `from "library"` linking, and variadic arity checking are all
>   implemented. Variadic argument *types* are not validated against
>   the format string — that is C-level UB.
> - **WASM** produces a runnable module. After linking via
>   `wasm-ld`, the module executes through a Node host shim at
>   `runtime/wasm/host.js` that provides the C library imports.
>   Run a WASM build with `runtime/wasm/run.sh <file.gol>`.
>
> Several features are interpreter-only in the current version —
> `Map<K, V>`, `List.append`, `Option`, `Result` with `try`/`catch`,
> and `String.*` conversions among them. LLVM and WASM refuse these
> at the capability boundary and print an `E0002` error suggesting
> `--interpreter`. The matrix below is the authoritative list.
>
> `rec` records are **no longer** on this list. Every record
> boundary — parameter, return, nested field, in a list, method
> with `&self` or by-value `self` — works on both interpreter and
> LLVM. Verified by probe.

## Backends

| Backend | Output | Role |
|---------|--------|------|
| Interpreter | Direct execution (semantic oracle) | Reference backend. Executes every feature the language specifies, in the subset where a runtime model exists. |
| LLVM | Native executable | Native compilation for the feature subset the capability check permits. Refuses records, maps, `List.append`, `Option`, `Result`, and channels at the boundary. |
| WASM | `.wasm` module | Compiles and runs via the Node host shim. Supports a similar subset to LLVM. |

**The authoritative feature × backend matrix is
[`docs/STATUS.md`](docs/STATUS.md).**
It is kept in sync with the code by the capability tests in
`src/backends/capabilities/tests.rs` — every "refused" claim in the
matrix is pinned by a test, and a build fails if the two drift.

## Inspector

`algol26 inspect` exposes the compiler's internal state, one flag
per intermediate representation:

| Command | Shows |
|---------|-------|
| `inspect --tokens <file>` | Lexer output |
| `inspect --ast <file>` | Parsed AST (source-shaped) |
| `inspect --ir <file>` | Semantic IR (source-shaped, with instructions) |
| `inspect --cfg <file>` | CFG view — blocks and their successors |
| `inspect --passes` | Registered passes and their contracts |
| `inspect --capabilities` | Feature × backend capability matrix |
| `inspect --type-table <file>` | Analyzer type-table completeness check |

Plus `--timing` for per-phase compile durations, on the LLVM path:

```bash
./target/debug/algol26 build --timing file.gol
```

## Known Limitations

These are verified gaps in the current version.

- **Chained method calls on field accesses fail on LLVM.**
  `x.field.method()` and `a.b.c.method()` parse and run on the
  interpreter, but LLVM codegen emits malformed IR for the field-
  access receiver shape and fails with an `E0002` "Call parameter
  type" error. Workaround: bind the field to a local first
  (`val tmp := x.field; tmp.method()`). Index-receiver and
  method-then-field chains (`arr[0].method()`, `x.m().field`) work
  on both backends. The parser is not the problem — the README
  previously described this as a parse limitation, which was
  stale.
- **Nested `import` inside a `procedure` works; top-level imports
  work; imported files' own imports are followed recursively.**
  Only circular imports are rejected.
- **Channels have no backend runtime.** All three backends refuse
  channel programs at the capability boundary before execution.
  See ADR 0020.
- **No general pointer-lifetime enforcement.** Region exit frees
  allocations, but no check rejects a pointer value that outlives
  its source region. See `docs/features/region.md`, section
  "What is not enforced today".
- **Diagnostic spans in imported files are attributed to the
  importing file's path.** When a nested module fails to compile,
  the error's line:column points at the wrong source file. The
  message text is correct; only the location is misattributed.
- **`output/json.gol`-style recursive structured data is not
  expressible.** The language has no sum types; recursive values
  need a design decision that belongs in its own ADR.

## Testing

```bash
cargo test --all-targets
```

Test coverage is spread across several complementary suites:

- **Unit tests** for each subsystem (lexer, parser, analyzer,
  builder, verifier, optimizer, interpreter).
- **Differential tests** — interpreter vs LLVM vs WASM output on
  the same programs.
- **Semantic tests** — borrow checker, trait registry, ownership.
- **IR tests** — verification, optimization, defer lowering,
  short-circuit CFG.
- **Integration tests** — negative diagnostics, hardening stress
  tests, release-mode safety guarantees.
- **Backend tests** — capability-matrix pinning, oracle tests,
  WASM isolation.
- **Property and fuzz tests** — malformed input never panics.
- **Conformance tests** — the fixture suite under `tests/conformance/`.
- **Corpus tests** — 39 programs in `tests/corpus/` are compiled
  and run through the differential harness in `tests/corpus_diff.rs`.

Features landed since the corpus was assembled (records, maps,
`List.append`, traits, generics) are covered primarily by interpreter
unit tests (`src/backends/interpreter/tests.rs`), analyzer tests
(`src/semantics/analyzer/tests.rs`), and capability tests
(`src/backends/capabilities/tests.rs`). Adding a corpus program is
still the strongest form of end-to-end coverage for a new feature,
but it is not the only mechanism.

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
| [`docs/STATUS.md`](docs/STATUS.md) | Feature matrix + known gaps |
| [`docs/pass-contracts.md`](docs/pass-contracts.md) | Compiler pass contracts and pipeline rules |
| [`docs/decisions/`](docs/decisions/) | Architecture Decision Records (0001–0048) |
| [`docs/features/`](docs/features/) | Per-feature reference docs |
| [`docs/releases/`](docs/releases/) | Release notes |
| [`docs/archive/`](docs/archive/) | Superseded docs, kept for history |
| [`docs/README.md`](docs/README.md) | Full index of all docs |
| [`docs/status/safety-guarantees.md`](docs/status/safety-guarantees.md) | The language's four safety claims, what enforces each, where they end |
| [`docs/status/analyzer-verifier-partition.md`](docs/status/analyzer-verifier-partition.md) | Which safety rules the analyzer owns vs. the verifier |
| [`docs/status/llvm-support.md`](docs/status/llvm-support.md) | What LLVM refuses and why, per feature |

## Architecture

```
src/
├── common/        — types, diagnostics, span
├── frontend/      — lexer, parser, AST, module loader
├── semantics/     — analyzer (types, borrow, traits, race) + IR builder
├── ir/            — semantic IR, verifier, optimizer, loop desugar
├── backends/      — interpreter, LLVM codegen, WASM
├── compiler/      — pass pipeline, scheduler, registry
├── diagnostics/   — error rendering
└── ffi/           — C type definitions, FFI registry
```

Compilation pipeline (see `src/compiler.rs`, `compile()`):

```
Lex → Parse → Process Imports → Desugar → Expand Impl → Assign ExprIds
→ Type Check → Type Table Complete → Build Semantic IR
→ Verify → Optimize → Re-verify → Lower to Backend
```

Every stage after the frontend runs through the pass pipeline in
`src/compiler/`. Passes declare a contract (input level, output
level, kind) that the scheduler enforces — see
`docs/pass-contracts.md`. ADR 0017 split verification into a
promotion pass (`SemanticIr → VerifiedIr`) and a re-check pass
(`VerifiedIr → VerifiedIr`) so the pipeline can re-verify after
optimization without pretending the level changed.

## Safety Guarantees

| Guarantee | Enforced by |
|-----------|-------------|
| Type safety | Semantic analyzer |
| Immutability | Semantic analyzer |
| Bounds checking | IR verifier + runtime |
| Use-after-move | Analyzer + IR verifier |
| Borrow checking | Semantic analyzer (conservative; see below) |
| Trait bounds | Trait registry (name resolution only — see ADR 0025) |
| IR well-formedness | `VerifiedIR` typestate + executable-IR invariants |
| No-panic on malformed input | Fuzz tests |

> **The borrow checker is conservative in the current version.**
> It accepts some programs a stricter borrow system would reject.
> The known gaps are enumerated in
> [`docs/status/safety-guarantees.md`](docs/status/safety-guarantees.md)
> §"Known gaps" (by review ID: B1, B2, B3, B5). Fixing them
> requires design decisions that belong in their own ADRs (0043,
> 0044, 0045, 0046).

## Contributing

See [`docs/README.md`](docs/README.md) for the doc taxonomy and how to
update each type. In short:

- **Reference docs** (like `STATUS.md`) are audited against
  the code and must be accurate.
- **ADRs** are append-only in spirit: supersede with a new ADR rather
  than editing a decided one.
- **Release notes** are frozen — never updated, kept as history.
- **Superseded docs** move to `docs/archive/` rather than being deleted.

## License

MIT — Rommel Edorot Caneos

## Acknowledgments

ALGOL 58 (inspiration) · Python (indentation) · Rust (implementation)
· LLVM 17 (native backend)
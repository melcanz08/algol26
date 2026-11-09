# ALGOL26

**ALGOL 58 reimagined for 2026.**

A historically inspired systems programming language combining ALGOL's
clarity with indentation, compile-time safety, deterministic resource
management, safe concurrency, and native compilation via LLVM.

> **Control without unsafe defaults.**

**The language is under active development.** Its primary dogfooding
target is ChainCSS, a production CSS-in-JS compiler being migrated
from TypeScript to ALGOL26 phase-by-phase, with byte-for-byte
differential parity as the exit criterion at each stage. The
migration's gap log lives at
[`LANGUAGE_GAPS.md`](LANGUAGE_GAPS.md).

**Version:** v0.8.0 · **License:** MIT · **Built with:** Rust 1.70+ and LLVM 17

---

## Quick Start

**Prerequisites:** Rust 1.70+, LLVM 17 (with `clang` and `wasm-ld` on
`PATH` for the native and WASM backends).

```bash
# Clone and build
git clone <repo-url> algol26
cd algol26
cargo build --release

# Run a program through LLVM
./target/release/algol26 run examples/basic/test.gol

# Or run through the interpreter (runs every feature)
./target/release/algol26 run --interpreter examples/basic/test.gol

# Just type-check
./target/release/algol26 check examples/basic/test.gol

# Run the test suite
cargo test --all-targets
```

Run `./target/release/algol26 --help` for the full CLI.

## Example

Records, `fn`/`proc`, pattern matching, defer, and spawn — one program.

```gol
rec ShoppingCart
    items: List<String>
    total: Int

fn checkout(cart: ShoppingCart) -> Int
    cart.total

proc main
    var cart := ShoppingCart { items: ["apple"], total: 5 }

    // checkout consumes cart; a second use is a compile error:
    val result := checkout(cart)
    // print(cart.total)   // <- use of moved value
    print(Int.to_string(result))

    // spawn requires val capture; a var capture is rejected:
    val prefix := "task"
    spawn do
        print(prefix)

    // defer runs LIFO at scope exit:
    defer print("cleanup")
```

Every one of those restrictions is enforced by the analyzer, not
by convention. That is what "control without unsafe defaults"
means in practice. See `docs/status/safety-guarantees.md`.

## Language at a Glance

- **Indentation-based** by default, like Python — no semicolons. An
  **alternate brace-based form** is also supported (ADR 0054); the two
  forms are semantically identical. See `examples/braces/` and
  `tests/brace_examples.rs`.
- **Immutable by default** (`val`), opt-in mutability (`var`)
- **Statically typed** with inference. Scalar types: `Int`, `Float`,
  `Char`, `Bool`, `String`. Container and structured types: `List<T>`,
  `Map<K, V>`, `Option<T>`, `Result<T, E>`, and `rec`-declared records.
- **`Char`** — a Unicode scalar value, distinct from `Int`. Literals
  are `'a'`, `'\n'`, `'😀'`; comparisons (`=`, `<`, `<=`, `>`, `>=`)
  work; no arithmetic (use `Char.to_int` for codepoint math).
  `String.char_at(s, i)` returns `Option<Char>`;
  `Char.to_string` encodes to UTF-8.
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
- **Generics** — `fn first<T>(xs: List<T>) -> Option<T>`; type
  parameters bind from arguments, including inside container types
- **`defer`** for LIFO cleanup, **`try`/`catch`** for `Result` handling
- **`match`** on `Option`, `Result`, records, unions, and literals
  — with guards (`case Num(n) if n > 0 then ...`) and
  exhaustiveness checking
- **Warnings** — `--no-warn=CODE` suppresses specific warnings,
  `--deny-warnings` promotes all warnings to errors. W0001 warns
  when a mixed-Void `match` discards a non-Void arm's value. See
  ADR 0063.
- **FFI** via `extern "C"` for calling into C libraries
- **`spawn`** and **`parallel`** blocks for structured concurrency
- **`region`** blocks and `alloc` / `free` for manual memory
- **`affirm(cond, msg)`** — always-on runtime assertions; a failing
  `affirm` prints the message and exits non-zero
- **`args()`** — command-line arguments; pass them after `--` on the CLI
- **String builtins**: `String.split`, `String.join`, `String.trim`,
  `String.substring`, `String.to_upper`, `String.to_lower`,
  `String.to_int`, `Int.to_string`

## Backends

Three backends serve distinct purposes. **The interpreter is the
semantic oracle** — it runs every feature the language specifies, and
serves as the reference implementation for differential testing.
**LLVM and WASM are optimized subsets** — faster and native, but
restricted to the feature set the capability check permits. A
program that works under LLVM is guaranteed to work under the
interpreter; the converse is not true.

| Backend | Output | Role |
|---------|--------|------|
| Interpreter | Direct execution | **Semantic oracle.** Runs every feature. The reference for differential testing. |
| LLVM | Native executable | **Native fast path.** Refuses `alloc`-family, `String.trim`/`.split`/`.join`, `args()`, `try`/`catch`, and unions at the capability boundary. |
| WASM | `.wasm` module | **Portable fast path.** Same capability model as LLVM. Runs via the Node host shim at `runtime/wasm/host.js`. |

**The authoritative feature × backend matrix is
[`docs/STATUS.md`](docs/STATUS.md).**
It is kept in sync with the code by two complementary mechanisms:
the capability tests in `src/backends/capabilities/tests.rs` (every
"refused" claim in the matrix is pinned by a test), and
`tools/feature_inventory.py` (which verifies the four feature
inventories — `STATUS.md`, feature docs, conformance fixtures, and
ADRs — agree with each other). A build fails if the matrix and the
code drift, and `python3 tools/feature_inventory.py --check` fails
if the four inventories drift.

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
> "Conservative" here means it *rejects some programs that are
> actually safe* — it errs on the side of rejection. The safety
> claims in this table hold **as stated** for what is implemented;
> they are not claims of full Rust-equivalence. The specific gaps
> are enumerated in
> [`docs/status/safety-guarantees.md`](docs/status/safety-guarantees.md)
> §"Known gaps" (by review ID: B1, B2, B3, B5). Fixing them requires
> design decisions that belong in their own ADRs (0043–0046).

## Testing

> **Want to contribute in five minutes?** Add a corpus program —
> see "How to Add a Corpus Program" below. Fastest path from
> reader to contributor.

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
  tests, release-mode safety guarantees, warning-system behavior.
- **Backend tests** — capability-matrix pinning, oracle tests,
  WASM isolation.
- **Property and fuzz tests** — malformed input never panics.
- **Conformance tests** — the fixture suite under `tests/conformance/`.
- **Corpus tests** — 39 programs in `tests/corpus/` are compiled
  and run through the differential harness in `tests/corpus_diff.rs`.

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
| [`docs/decisions/`](docs/decisions/) | Architecture Decision Records (0001–0063) |
| [`docs/features/`](docs/features/) | Per-feature reference docs |
| [`docs/releases/`](docs/releases/) | Release notes |
| [`docs/archive/`](docs/archive/) | Superseded docs, kept for history |
| [`docs/README.md`](docs/README.md) | Full index of all docs |
| [`docs/status/safety-guarantees.md`](docs/status/safety-guarantees.md) | The language's four safety claims, what enforces each, where they end |
| [`docs/status/analyzer-verifier-partition.md`](docs/status/analyzer-verifier-partition.md) | Which safety rules the analyzer owns vs. the verifier |
| [`docs/status/llvm-support.md`](docs/status/llvm-support.md) | What LLVM refuses and why, per feature |
| [`docs/status/KNOWN_DRIFT.md`](docs/status/KNOWN_DRIFT.md) | Feature-inventory reconciliation log |
| [`LANGUAGE_GAPS.md`](LANGUAGE_GAPS.md) | The dogfooding gap log — open and resolved language gaps from the ChainCSS port |

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

tree-sitter-algol26/ — Tree-sitter grammar (editor highlighting only;
                       not part of the compiler binary)
editors/sublime/     — Sublime Text syntax definition
runtime/             — C runtime (String, Map, File, etc.)
tools/               — Developer utilities (lint, feature-inventory)
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

## Known Limitations

These are verified gaps in the current version.

- **Tree-sitter indentation form.** The grammar at
  `tree-sitter-algol26/` currently covers the brace form only; the
  indentation scanner (`INDENT`/`DEDENT`) is not yet implemented.
  Indentation-form source will highlight but not fully parse in
  Tree-sitter-backed editors.
- **Tree-sitter keyword scopes.** The grammar also does not tokenize
  several keywords (`pub`, `and`, `or`, `not`, `self`, `Self`); those
  tokens render in the default text color in tree-sitter-backed
  editors. Sublime Text is unaffected — it uses
  `editors/sublime/ALGOL26.sublime-syntax`. Tracked as
  `LANGUAGE_GAPS.md` G33.
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
- **Union programs are interpreter-only.** LLVM and WASM refuse
  them at the capability boundary (`Feature::Unions`).

## Contributing

See [`docs/README.md`](docs/README.md) for the doc taxonomy and how to
update each type. In short:

- **Reference docs** (like `STATUS.md`) are audited against
  the code and must be accurate.
- **ADRs** are append-only in spirit: supersede with a new ADR rather
  than editing a decided one.
- **Release notes** are frozen — never updated, kept as history.
- **Superseded docs** move to `docs/archive/` rather than being deleted.

If you're adding a feature, run
`python3 tools/feature_inventory.py` before committing — it will
flag any drift between `STATUS.md`, feature docs, conformance
fixtures, and ADRs. The `--check` flag makes it suitable for CI.

## License

MIT — Rommel Caneos

## Acknowledgments

ALGOL 58 (inspiration) · Python (indentation) · Rust (implementation)
· LLVM 17 (native backend)
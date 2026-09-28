# ALGOL26 Implementation Status

Last updated: 2026-09-27

This document records what actually works, verified by the
differential corpus in `tests/corpus/` and the capability tests in
`src/backends/capabilities/tests.rs`. A feature is only listed as
"works" if there is at least one corpus program or end-to-end test
exercising it.

## Recent changes (through 2026-09-27)

### Records (ADR 0024)

- **`rec` structured data types.** Declaration, construction
  (`Point { x: 1, y: 2 }`), field read/write (`p.x`, `p.x := v`),
  pattern matching (`case Point { x, y }`), function parameters,
  nested records, and cross-module use.
- **Interpreter-only.** LLVM and WASM refuse programs using records
  at the capability check (`Feature::Records`).
- **Cross-module merge** (`d7b9e4d`). `process_imports` now merges
  `imported_program.records` alongside imported functions. A record
  declared in an imported file was previously invisible to the
  importing module.
- **Not yet supported:** structural `Copy` for all-`Copy` records
  (v1 records are move-only), field-level borrows (`&p.x`),
  auto-derived traits, nested destructuring beyond one level.

### Track B builtins

Five conversion and string-manipulation builtins landed. All are
interpreter-only; LLVM and WASM refuse at the capability boundary.

| Builtin | Signature | Commit |
|---------|-----------|--------|
| `Int.to_string` | `(Int) -> String` | `7ea08d3` |
| `String.to_int` | `(String) -> Option<Int>` | `7ea08d3` |
| `String.trim` | `(String) -> String` | `7778200` |
| `String.split` | `(String, String) -> List<String>` | `d32fb1c` |
| `String.join` | `(List<String>, String) -> String` | `ee87239` |

### Assertion, arguments, and diagnostics

- **`affirm(cond, msg)`** (ADR 0022). Always-on runtime assertion.
  Supported by every backend: the interpreter returns
  `EvalError::Runtime` on false; LLVM emits a conditional branch to
  `printf` + `exit(1)`.
- **`args()`** (ADR 0023). Command-line arguments as
  `List<String>`. Interpreter-only. The CLI passes program arguments
  after a `--` separator.
- **Diagnostics: filename in errors** (`95329e0`). `CompileError`
  gained an `Option<String> file` field. Errors render
  `--> file:12:5` instead of `--> 12:5`.
- **Diagnostics: unresolvable named types** (`bc94173`). A
  `TypeSyntax::Named(name)` where `name` is multi-character, not a
  primitive, not a record, and not a type parameter now returns
  `Err` instead of silently producing `Type::Unknown`. Closes the
  class of failure where a resolution error surfaces three steps
  downstream.

### Convergence work (ADRs 0013–0018)

- **Canonical pipeline** (ADR 0018). One `Compiler::run_pipeline`
  shared by `compile`, `run_interpreter`, and `compile_to_wasm`.
- **VerifiedIR typestate** (ADR 0017). `Program::verified: bool`
  replaced by an `IrState` enum (`Absent` / `Built` / `Verified`).
  `VerifyIrPass` promotes; `ReVerifyPass` re-checks after optimize.
- **Per-function dataflow** (ADR 0016). Every function is analyzed;
  parameters seed the entry state; diagnostics are deduplicated.
- **Executable-IR invariant check** (ADR 0014).
  `crate::ir::verifier::invariants` rejects `Type::TypeVar` in
  executable IR.
- **Generic instantiation closure** (ADR 0013).
  `InstantiationPlan::close` materializes transitive specializations.
  The pipeline no longer calls a pre-typecheck `Monomorphizer`.

### Unsafe, references, channels, iterator metadata

- **Unsafe enforcement** (ADR 0015). Raw pointer dereference, `alloc`,
  and `free` require an `unsafe` block.
- **References** (ADR 0019). LLVM supports `&x`, `&mut x`, `*r`,
  `AddrOf`. The interpreter refuses them at the capability check
  (`Feature::References`).
- **Channels** (ADR 0020). All three backends refuse channel
  programs at the capability check. The interpreter's channel
  instruction arms return `EvalError::Unsupported` as a defensive
  guard.
- **Iterator metadata** (ADR 0021). `Declare` records the LLVM array
  type; `IteratorInit` and `IteratorNext` fail closed when metadata
  is missing rather than guessing.

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
- **Write-through-`&mut` is accepted but not lowered.** Assigning
  to a variable whose declared type is `MutBorrow(T)` is the
  language's write-through syntax. The analyzer and verifier
  implement the rule; neither backend lowers it. See ADR 0010
  Phase 2 for the corrected premise and the fix plan.
- **Variadic FFI argument *types* are not validated.** `extern "C"
  function printf(fmt: String, ...)` accepts any number of arguments
  at or above the fixed count. The extra arguments' types are not
  checked against the format string — that is C-level undefined
  behavior.
- **WASM execution requires the Node host shim.** The compiler
  links `.wasm` output via `wasm-ld`; the resulting module is
  runnable through `runtime/wasm/host.js`, which provides the C
  library imports. The shim's `malloc` is a bump allocator
  (no `free`).

## Compiler infrastructure

The compiler's phase sequence is visible and enforced through the
pass pipeline (`src/compiler/pipeline.rs`, `scheduler.rs`,
`registry.rs`, `pass.rs`).

- **Pass contracts.** Every pass declares a `PassContract`
  (`id`, `kind`, `input`/`output` IR level, and prose fields for
  `requires` / `guarantees` / `may_change` / `must_preserve`).
  See `docs/pass-contracts.md`.
- **Enforcement.** `PipelineBuilder::validate_chain` enforces
  chain continuity and level advancement at build time.
  `Scheduler::run` enforces Transform-must-be-followed-by-
  Verification at run time. `tests/pass_contracts.rs` enforces
  non-empty metadata for every registered pass.
- **Every compile path routes through the pipeline.**
  `compile`, `run_interpreter`, `compile_to_wasm`, and
  `inspect --ir` / `--type-table` all invoke passes via the
  scheduler.
- **`--timing`.** Prints per-phase compile durations: lex, parse,
  imports, desugar, expand, type_check, type_table_complete,
  ir_build, verify_pre, optimize, verify_post, lower.
- **`inspect` subcommands.** `--tokens`, `--ast`, `--ir`, `--cfg`,
  `--passes`, `--capabilities`, `--type-table`.

## Legend

- ✅ **Works** — verified by corpus program(s) or end-to-end test
- ⚠️ **Interpreter only** — works in the interpreter; LLVM refuses
  or miscompiles
- ❌ **Parser only** — parser and analyzer accept it; no backend
  runtime
- ⛔ **Not supported** — explicitly refused with a clear error
- ❓ **Untested** — no corpus or end-to-end coverage

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
| Bare call statements | ✅ | ✅ | ✅ | ✅ | 33 |
| Lists + indexing | ✅ | ✅ | ✅ | ✅ | 01, 07, 15 |
| `List.length` / `List.sum` | ✅ | ✅ | ✅ | ✅ | 07, 15 |
| Strings | ✅ | ✅ | ✅ | ✅ | 08, 15 |
| `String.length` / `to_upper` / `to_lower` | ✅ | ✅ | ✅ | ⛔ | 08 |
| `String.trim` / `.split` / `.join` | ✅ | ✅ | ✅ | ⛔ | — |
| `String.to_int` / `Int.to_string` | ✅ | ✅ | ✅ | ⛔ | — |
| `File.read` / `.write` / `.append` | ✅ | ✅ | ✅ | ⛔ | — |
| `Math.*` | ✅ | ✅ | ✅ | ⚠️ | — |
| `Option` / `Some` / `None` | ✅ | ✅ | ✅ | ⛔ | 13 |
| `match` (literal patterns) | ✅ | ✅ | ✅ | ✅ | — |
| `match` (binding patterns) | ✅ | ✅ | ✅ | ⛔ | 13 |
| `Result` / `Ok` / `Error` | ✅ | ✅ | ✅ | ⛔ | 14 |
| `try` / `catch` | ✅ | ✅ | ✅ | ⛔ | 14 |
| Traits + impls | ✅ | ✅ | ✅ | ⚠️ | 28, 29, 37 |
| `rec` records | ✅ | ✅ | ✅ | ⛔ | — |
| `region` | ✅ | ✅ | ✅ | ✅ | 30, 31 |
| `unsafe` | ✅ | ✅ | ✅ | ✅ | 32, 33 |
| `affirm(cond, msg)` | ✅ | ✅ | ✅ | ✅ | — |
| `args()` | ✅ | ✅ | ✅ | ⛔ | — |
| `&x` / `&mut x` / `*r` / `AddrOf` | ✅ | ✅ | ⛔ | ✅ | — |
| `spawn` / `parallel` | ✅ | ✅ | ✅ | ⛔ | — |
| `channel` / `send` / `receive` | ✅ | ✅ | ⛔ | ⛔ | 23–26 |
| `alloc` in `var` position | ✅ | ✅ | ✅ | ⛔ | — |
| `alloc(x)` as statement | ✅ | ✅ | ✅ | ⛔ | — |
| `free` | ✅ | ✅ | ✅ | ⛔ | — |
| `extern` (FFI) | ✅ | ✅ | ⛔ | ✅ | — |
| `import` | ✅ | ✅ | ✅ | ✅ | — |
| `Map<K, V>` | Interpreter | LLVM/WASM refused | ADR 0027 |

**Note on canonical IR names.** The IR builder emits
`BorrowShared` / `BorrowMutable` / `ReadReference` / `SendChannel`
/ `ReceiveChannel`. User-facing syntax is unchanged.

**Note on refusal markers.** ⛔ means the backend refuses via the
capability check rather than silently failing or producing wrong
code. See `src/backends/capabilities/` and
`inspect --capabilities`.

## Known Gaps

Gaps are documented by `tests/corpus/*.gol` programs marked
`// KNOWN_FAILURE:` or `// BACKEND: interpreter`, or by capability
tests in `src/backends/capabilities/tests.rs`.

### Analyzer soundness gaps

Cases where the analyzer accepts programs a stricter borrow system
would reject. Documented rather than silently patched because
fixing each one correctly requires design decisions that belong in
their own ADRs.

- **`&mut x` in a call argument is not registered as a borrow.**
  Writing `f(&mut x)` does not mark `x` as mut-borrowed, so
  `f(&mut x); g(&mut x)` compiles even though both functions may
  write through the same reference. A proper fix needs
  statement-scoped release or full non-lexical lifetimes.
- **No general pointer-lifetime enforcement.** Region exit frees
  allocations, but no check rejects a pointer value that outlives
  its source region. See `docs/features/region.md`, section
  "What is not enforced today".
- **Trait bounds are name-resolved only** (ADR 0025). A
  `where T: TraitName` clause is accepted if `TraitName` is
  declared; whether the concrete type substituted for `T`
  implements the trait is not checked.

### IR correctness

Fixed in earlier sessions:

- Constant propagation no longer leaks constants across branch
  blocks.
- Verifier recursively checks `Some`/`None`/`Ok`/`Error` payloads.
- Verifier rejects `Float` arguments for `Int` parameters.
- Optimizer skips folding large `Int` arithmetic above 2^53.
- CFG verifier rules for `Fork` shape (ADR 0011).
- Region-boundary `break`/`continue` leak fixed at the analyzer
  level.
- Canonical IR variants landed (`BorrowShared`, `ReadReference`,
  `SendChannel`, `ReceiveChannel`).

Deferred (design, not correctness bugs):

- `builtin_signatures_match_analyzer_table` enforces the sync
  between the verifier's `builtin_signatures()` table and the
  analyzer's `register_builtin_functions()`. It does **not**
  enforce the third site — `SemanticIRBuilder::build_impl`'s
  `function_types` registrations. Adding a builtin requires
  updating all three; only the first two are tested against
  each other.
- Dominance-aware constant propagation would allow cross-block
  folding (currently disabled to preserve correctness).
- `Spawn`/`Fork` capture semantics are not verified. The
  `CaptureMode` field intended to carry this information was
  removed; adding real capture verification requires its own
  ADR. See `docs/decisions/0008-concurrency-model.md`.

### Backend audit

Fixed (cumulative):

- LLVM codegen: `NotEqual` on pointers now emits `NE` (was `EQ`).
- LLVM codegen: switch with no default emits `unreachable`.
- LLVM codegen: list reassignment rebuilds the array.
- LLVM codegen: `Some`/`Ok`/`Error`/`None` refused at the
  capability check instead of silently unwrapped.
- LLVM codegen: `Declare` records the array type in
  `list_array_types` so `IteratorInit` finds it (ADR 0021).
- Production LLVM path goes through `Backend::compile`, so
  `module.verify()` and the capability check run on the path
  users exercise.
- Interpreter: `Fork` runs every branch sequentially; Math
  builtins complete; `List.max`/`min`, `String.substring` added.
- WASM: `validate_wasm_compatibility` replaced by the capability
  system; `module.verify()` runs before write.
- Capability: `scan_call_name` uses `builtin_signatures()` instead
  of a prefix match.
- Capability: `Feature::References` refuses interpreter reference
  programs before execution (ADR 0019).
- Capability: `Feature::Channels` refuses channel programs on all
  backends (ADR 0020).
- `BackendOutput` carries real data (paths, stdout).
- Interpreter: `File.read` / `.write` / `.append` dispatch
  implemented (`957fbe7`). Previously the capability matrix
  advertised `FileFunctions` but the dispatcher had no arms.

Open (not fixed):

- WASM output has unresolved C library imports (`printf`, `exit`,
  `sqrt`, `strlen`, `strcat`). Module is not executable without
  the Node host shim.
- `InterpreterBackend` clones the program per compile.
- Interpreter runtime errors are `Debug`-formatted and double-
  prefixed (`Runtime error: runtime error: ...`).
- Errors inside imported files render the *importing* file's name,
  not the file where the error text lives. Per-node file
  provenance does not exist in the AST or `Span` yet.

### LLVM backend gaps (interpreter works)

- **`match` with pattern bindings** (`corpus_13`). The IR builder
  records bindings in its internal scope but does not emit
  `Declare` instructions for them.
- **Iterating over a list parameter** (`corpus_10`). Iterator
  setup is only done for locally-created list literals.
- **`try` / `catch` and `Result` values** (`corpus_14`). No LLVM
  lowering; refused cleanly.
- **Records** (`ADR 0024`). Refused at the capability check.
- **Track B builtins.** Refused at the capability check
  (`Feature::Conversions` for `Int.to_string` / `String.to_int`;
  `StringFunctions` for the others).

### Reserved-word collisions

`end`, `from`, `case`, `in`, `do`, `as`, and the rest of the
keyword set cannot be used as field names, variable names, or
parameter names. Two collisions surfaced during Track A:

- `Segment { start, end }` — `end` is a block terminator.
- `function find_char(s, from, ch)` — `from` is an FFI keyword.

Neither is a bug; both are ergonomics costs. A future ADR could
add contextual keywords or an escape mechanism (`r#name`), but
v1 requires distinct names.

### Unimplemented features

- **Channels** (`corpus_23`–`corpus_26`). All three backends
  refuse channel programs at the capability boundary. See
  ADR 0020.

## Conventions

**Language design note:** methods are called as `x.method(x)` —
the receiver is passed explicitly as the first argument.
Zero-argument methods are called as `x.method()`. There is no
implicit `self`.

**Backend selection:** programs that require interpreter-only
features declare `// BACKEND: interpreter` at the top of the file.
The `corpus_diff` harness runs them through the interpreter;
others run through LLVM by default.

**Capability truth:** for every feature, the capability matrix in
`src/backends/capabilities/mod.rs` is the single source of truth.
A backend that cannot lower a feature refuses at the capability
check rather than at codegen.

## See also

- `docs/decisions/` — Architecture Decision Records (0001–0025)
- `docs/pass-contracts.md` — the pass contract model
- `docs/features/` — per-feature contracts
- `README.md` — user-facing overview

## How to add a feature to this document

1. Write a corpus program in `tests/corpus/` that exercises the
   feature, or an end-to-end test in the relevant test module.
2. Run `cargo test --test corpus_diff` (or the relevant suite).
3. If it passes, mark the feature ✅ in the matrix.
4. If it fails in a backend, mark it ⚠️ or ⛔, add a
   `// BACKEND:` or `// KNOWN_FAILURE:` header, and add a bullet
   under "Known Gaps."
5. If the feature adds a new `Feature` variant, add a matching
   row to `tests/coverage_matrix.rs` and an entry to
   `tests/coverage_maturity.rs`'s `EXPECTED_MATURITY`.
6. Update the "Last updated" date.
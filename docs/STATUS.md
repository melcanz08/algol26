# ALGOL26 Status

> **Canonical status document.** This is the single place to look for
> "what works today". It is verified by the differential corpus in
> `tests/corpus/`, the conformance fixtures in `tests/conformance/`,
> and the feature × backend matrix in `tests/coverage_matrix.rs`.
>
> Subsystem investigations live under [`status/`](status/):
> [`diagnostics.md`](status/diagnostics.md).
>
> Superseded docs are preserved (with headers) under
> [`archive/`](archive/).

Last updated: 2026-10-01

This document records what actually works, verified by the
differential corpus in `tests/corpus/`, the conformance fixtures in
`tests/conformance/`, the interpreter test suite in
`src/backends/interpreter/tests.rs`, the analyzer tests in
`src/semantics/analyzer/tests.rs`, and the capability tests in
`src/backends/capabilities/tests.rs`. A feature is only listed as
"works" if there is at least one end-to-end test exercising it.

## Recent changes (through 2026-10-01)

### Associated types (ADR 0041)

- **Trait-declared associated types.** A trait declares `type Item`
  in its body; each impl binds it with `type Item := ConcreteType`.
- **Projections in signatures.** `Self::Item` inside a trait method
  is legal type syntax; `C::Item` for a where-clause-bound type
  variable is also legal (`function head<C>(c: C) -> C::Item where
  C: Container`).
- **Normalization at specialization.** The analyzer stores a
  projection symbolically; the base becomes concrete at a generic
  call site, and the projection reduces to the impl's binding. The
  IR verifier rejects any unnormalized projection that survives to
  executable IR.
- **Validation.** An impl must define every trait-declared
  associated type and no others. Inherent impls cannot declare
  associated types.
- **Not yet supported:** associated type bounds, associated type
  defaults, supertrait associated types, generic associated types.

### Visibility (`pub`, ADR 0039)

- **Item-level visibility.** Every top-level declaration carries a
  `Visibility`. Default is private; `pub` opts a declaration into the
  module's public surface. Scope is the file.
- **Record fields have their own visibility.** A public record can
  have private fields; only the record's own `impl` (in its
  declaring file) can read or write them.
- **Trait methods and trait-impl methods are always public.** A
  redundant `pub` in either position is a parse error. Inherent impl
  methods respect the default-private rule.
- **Enforcement at resolution.** The analyzer compares the current
  function's module against the item's declaring module at every
  cross-module function call, record construction, and field read.
  Diagnostics name the item, its defining module, and the access
  site (`E0013`).
- **Not yet supported:** scoped visibility (`pub(crate)`),
  declared modules, `private` / `internal` synonyms. Traits and
  impls are not forwarded through imports yet (pre-existing gap).

### Dynamic dispatch (ADR 0038)

- **`&dyn Trait` / `&mut dyn Trait`.** Implicit coercion from
  `&T` / `&mut T` when `T: Trait`. Method calls on a `dyn Trait`
  receiver dispatch through a fat-pointer vtable at runtime.
  See [`docs/features/dyn_trait.md`](features/dyn_trait.md).
- **Fat-pointer representation.** `{ data: ptr, vtable: ptr }`.
  Vtables are emitted per `(trait, concrete)` pair as
  internal-linkage `[N x ptr]` constants with the mangled name
  `__algol26_vtable_{trait_id}_{concrete_mangled}`.
- **Object safety.** A trait is usable as `dyn Trait` iff every
  method has a `&Self` or `&mut Self` receiver. Traits with
  by-value receivers are rejected at the coercion site with
  `E0002` naming the offending method.
- **Dedicated coercion diagnostic.** A `&T → &dyn Trait` coercion
  whose `T` does not implement the trait produces `E0012`, which
  names the concrete type and the trait in the message.
- **All three backends.** Interpreter dispatches by reconstructing
  the mangled impl name at runtime; LLVM and WASM share an
  indirect-call lowering through the vtable slot.
- **Not yet supported:** owned dynamic dispatch (`Box<dyn Trait>`
  or equivalent), trait-object upcasting, generic trait objects,
  runtime type identity (`Any` / `TypeId` / downcasting), dynamic
  library loading. See ADR 0038 §Scope boundary for the full list.

### Records (ADR 0024) and structural Copy (ADR 0026)

- **`rec` structured data types.** Declaration, construction
  (`Point { x: 1, y: 2 }`), field read/write (`p.x`, `p.x := v`),
  pattern matching (`case Point { x, y }`), function parameters,
  nested records, and cross-module use.
- **Structural `Copy`** (ADR 0026). A record is `Copy` iff every
  field is `Copy`. `Point { x: Int, y: Int }` copies;
  `Person { name: String, age: Int }` moves. The property is a
  compile-time predicate on the type, not a declaration the
  programmer writes.
- **Interpreter-only.** LLVM and WASM refuse programs using records
  at the capability check (`Feature::Records`).
- **Cross-module merge** (`c8c15fc`). `process_imports` merges
  `imported_program.records` alongside imported functions. A record
  declared in an imported file is visible to the importing module.
- **Signature resolution.** Records named in function signatures
  (`-> Option<Sale>`, `p: Point`) resolve through the analyzer's
  and builder's `resolve_type_syntax`. All pure-syntax `to_type()`
  call sites were replaced in `5b07b27`, `c8c15fc`, and `9ee2797`.
- **Trait method dispatch on records** (`f5bc75d`).
  `impl Show for Sale` + `sale.show()` resolves the impl-mangled
  name (`Sale_show`) before falling through to the builtin path.
- **Not yet supported:** field-level borrows (`&p.x`), auto-derived
  traits, nested destructuring beyond one level.

### Map (ADR 0027) and List.append (ADR 0028)

- **`Map<K, V>`** (ADR 0027). Literal syntax `Map { "a": 1 }` and
  `Map<String, Int> {}`, methods `insert`, `get`, `contains`,
  `keys`, `values`, `length`. Keys restricted to `Int`, `String`,
  or `Bool`. `insert` requires a `var` receiver and clears the
  analyzer's static length tracking for the receiver.
- **`List.append(x)`** (ADR 0028). Mutating method, requires a
  `var` receiver. Invalidates the analyzer's statically-tracked
  length for that binding so a now-valid index past the old length
  isn't falsely rejected.
- **Interpreter-only.** LLVM and WASM refuse both at the capability
  check (`Feature::Map`, `Feature::ListAppend`).

### Traits and generics in real programs

- **`impl Trait for Type`** — declared methods are renamed to
  `<Type>_<method>` by `expand_impl_methods` and registered in
  `function_types`. Method call syntax is `x.method()`; the
  receiver is passed implicitly.
- **`function first<T>(xs: List<T>) -> Option<T>`** — generic
  parameters bind through `unify_types` in the analyzer, which
  recurses into container types. A call with `List<Int>` binds
  `T = Int`. The specialization signature path also resolves
  record type arguments through `resolve_type_syntax`.

### Module system

- **Top-level imports** (fixed in `9ee2797`). `import "path"` at
  the top of a file is now processed by `process_imports`, matching
  the in-body form. Previously top-level imports were parsed but
  silently ignored.
- **Nested imports.** An imported file's own `import` statements
  are followed recursively. Cycle detection uses the loader's
  `import_stack`; duplicate-file detection uses a `visited` set
  keyed by canonical path so diamonds merge each file once.

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
  `Err` instead of silently producing `Type::Unknown`.

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

### Compiler bug fixes discovered by real programs

The CLI and sales-report exercises (see the `algol26-cli`
directory) surfaced and fixed a series of latent bugs. Each was
invisible to single-file tests because the interaction of features
only appears when they are combined in a real program:

- `process_imports` dropped records from imported files (`c8c15fc`)
- Function signature types in the builder used pure-syntax `to_type`
  (`5b07b27`, `c8c15fc`)
- `TypedIRValue::type_of()` had no arm for `FieldAccess`, `Array`,
  or `Range`; the trailing `_ => Type::Unknown` catch-all is now
  removed so a missing arm fails to compile (`b88efcc`)
- Analyzer's and builder's `VarDecl` annotation resolution used
  pure-syntax `to_type`
- `resolve_method_call` short-circuited for records before trying
  the impl-mangled form (`f5bc75d`)
- Top-level imports were parsed but not processed (`9ee2797`)
- Generic parameters inside container types were not bound
- `Unknown` in a parameter type needed to act as a wildcard in the
  call-coercion check
- `val x := f()` executed `f()` three times (double-push plus
  Declare-embedded call)
- DCE removed a `Declare` whose value was a call, dropping the
  side effect
- The interpreter test helper skipped `expand_impl_methods`, so
  the harness didn't exercise the pipeline the CLI uses
- Remaining `to_type` call sites in the builder (`9ee2797`)
- Nested imports were one level deep

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
| `List.append(x)` | ✅ | ✅ | ✅ | ⛔ | — |
| `Map<K, V>` | ✅ | ✅ | ✅ | ⛔ | — |
| Strings | ✅ | ✅ | ✅ | ✅ | 08, 15 |
| `String.length` | ✅ | ✅ | ✅ | ✅ | 08 |
| `String.to_upper` / `.to_lower` / `.concat` / `.substring` | ✅ | ✅ | ✅ | ⛔ | — |
| `String.trim` / `.split` / `.join` | ✅ | ✅ | ✅ | ⛔ | — |
| `String.to_int` / `Int.to_string` | ✅ | ✅ | ✅ | ⛔ | — |
| `File.read` / `.write` / `.append` | ✅ | ✅ | ✅ | ⛔ | — |
| `Math.*` | ✅ | ✅ | ✅ | ✅ | — |
| `Option` / `Some` / `None` | ✅ | ✅ | ✅ | ⛔ | 13 |
| `match` (literal patterns) | ✅ | ✅ | ✅ | ✅ | — |
| `match` (binding patterns) | ✅ | ✅ | ✅ | ⛔ | 13 |
| `Result` / `Ok` / `Error` | ✅ | ✅ | ✅ | ⛔ | 14 |
| `try` / `catch` | ✅ | ✅ | ✅ | ⛔ | 14 |
| Traits + impls | ✅ | ✅ | ✅ | ✅ | 28, 29, 37 |
| `&dyn Trait` / `&mut dyn Trait` | ✅ | ✅ | ✅ | ✅ | — |
| Visibility (`pub`) | ✅ | ✅ | ✅ | ✅ | — |
| Associated types | ✅ | ✅ | ✅ | ✅ | — |
| Generics | ✅ | ✅ | ✅ | ✅ | — |
| `rec` records | ✅ | ✅ | ✅ | ⛔ | — |
| Structural `Copy` | ✅ | ✅ | ✅ | ⛔ | — |
| `region` | ✅ | ✅ | ✅ | ✅ | 30, 31 |
| `unsafe` | ✅ | ✅ | ✅ | ✅ | 32, 33 |
| `affirm(cond, msg)` | ✅ | ✅ | ✅ | ✅ | — |
| `args()` | ✅ | ✅ | ✅ | ⛔ | — |
| `&x` / `&mut x` / `*r` / `AddrOf` | ✅ | ✅ | ⛔ | ✅ | — |
| `spawn` / `parallel` | ✅ | ✅ | ✅ | ⛔ | — |
| `channel` / `send` / `receive` | ✅ | ✅ | ⛔ | ⛔ | 23–26 |
| `alloc` in `var` position | ✅ | ✅ | ✅ | ✅ | — |
| `alloc(x)` as statement | ✅ | ✅ | ✅ | ✅ | — |
| `free` | ✅ | ✅ | ✅ | ✅ | — |
| `extern` (FFI) | ✅ | ✅ | ⛔ | ✅ | — |
| `import` | ✅ | ✅ | ✅ | ✅ | — |
| `Set<T>` | ✅ | ✅ | ✅ | ✅ | — |

**Note on canonical IR names.** The IR builder emits
`BorrowShared` / `BorrowMutable` / `ReadReference` / `SendChannel`
/ `ReceiveChannel`. User-facing syntax is unchanged.

**Note on refusal markers.** ⛔ means the backend refuses via the
capability check rather than silently failing or producing wrong
code. See `src/backends/capabilities/` and
`inspect --capabilities`.

**Note on the `Traits + impls` row.** Traits resolve before IR
construction (`expand_impl_methods` runs in `prepare_frontend`).
The `✅` for LLVM means a trait-using program compiles and runs
when its impl bodies use features LLVM supports. A `show()`
implementation that returns a `String` is fine; one that
constructs a record would be refused because records are refused.

## Known Gaps

Gaps are documented by `tests/corpus/*.gol` programs marked
`// KNOWN_FAILURE:` or `// BACKEND: interpreter`, or by capability
tests in `src/backends/capabilities/tests.rs`.

### Analyzer soundness gaps

Cases where the analyzer accepts programs a stricter borrow system
would reject. Documented rather than silently patched because
fixing each one correctly requires design decisions that belong in
their own ADRs.

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
- `type_of()` is exhaustive over `TypedIRValue`; the
  `_ => Type::Unknown` catch-all is gone, so a future variant
  that lacks an arm fails to compile rather than silently
  producing `Unknown`.

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
  implemented (`957fbe7`).

Open (not fixed):

- WASM output has unresolved C library imports (`printf`, `exit`,
  `sqrt`, `strlen`, `strcat`). Module is not executable without
  the Node host shim.
- `InterpreterBackend` clones the program per compile.
- Interpreter runtime errors are `Debug`-formatted and double-
  prefixed (`Runtime error: runtime error: ...`).
- Errors inside imported files render the *importing* file's name,
  not the file where the error text lives. Per-node file
  provenance does not exist in the AST or `Span` yet. Surfaced
  again during the CLI exercises — a type error in
  `data/parser.gol` reported a location in `sales.gol`.

### LLVM backend gaps (interpreter works)

- **`match` with pattern bindings** (`corpus_13`). The IR builder
  records bindings in its internal scope but does not emit
  `Declare` instructions for them.
- **Iterating over a list parameter** (`corpus_10`). Iterator
  setup is only done for locally-created list literals.
- **`try` / `catch` and `Result` values** (`corpus_14`). No LLVM
  lowering; refused cleanly.
- **Records** (ADR 0024). Refused at the capability check.
- **`Map<K, V>`** (ADR 0027). Refused at the capability check.
- **`List.append`** (ADR 0028). Refused at the capability check.
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

### Language gaps

- **Chained method calls on field accesses.** `x.field.method()`
  is rejected by the parser. Workaround: bind the field to a
  local (`val tmp := x.field`), then call the method.
- **Recursive structured data (sum types).** The language has no
  sum types; a recursive value like a JSON tree is not
  expressible. This is the last item on the outsider's proposal
  that is blocked on a language feature rather than a runtime
  or backend.

### Unimplemented features

- **Channels** (`corpus_23`–`corpus_26`). All three backends
  refuse channel programs at the capability boundary. See
  ADR 0020.
- **Sum types.** Not designed yet. No ADR.

## Conventions

**Language design note:** trait methods are declared with an
`impl Trait for Type` block and called as `x.method()`. The
receiver is passed implicitly; there is no user-visible `self`
parameter. `expand_impl_methods` inserts it during the frontend
normalization step.

**Backend selection:** programs that require interpreter-only
features declare `// BACKEND: interpreter` at the top of the file.
The `corpus_diff` harness runs them through the interpreter;
others run through LLVM by default.

**Capability truth:** for every feature, the capability matrix in
`src/backends/capabilities/mod.rs` is the single source of truth.
A backend that cannot lower a feature refuses at the capability
check rather than at codegen.

**Testing coverage:** features that predate the current corpus are
covered by corpus programs. Features landed since (records, maps,
`List.append`, traits, generics, structural `Copy`) are covered by
interpreter unit tests (`src/backends/interpreter/tests.rs`),
analyzer tests (`src/semantics/analyzer/tests.rs`), and capability
tests (`src/backends/capabilities/tests.rs`). Adding a corpus
program is still the strongest form of end-to-end coverage, but it
is no longer the only mechanism.

## See also

- `docs/decisions/` — Architecture Decision Records (0001–0028)
- `docs/pass-contracts.md` — the pass contract model
- `docs/features/` — per-feature contracts
- `README.md` — user-facing overview

## How to add a feature to this document

1. Write a corpus program in `tests/corpus/` that exercises the
   feature, or an end-to-end test in the relevant test module
   (interpreter, analyzer, capability).
2. Run `cargo test --all-targets` (or the specific test file).
3. If it passes, mark the feature ✅ in the matrix.
4. If it fails in a backend, mark it ⚠️ or ⛔, add a
   `// BACKEND:` or `// KNOWN_FAILURE:` header, and add a bullet
   under "Known Gaps."
5. If the feature adds a new `Feature` variant, add a matching
   row to `tests/coverage_matrix.rs` and an entry to
   `tests/coverage_maturity.rs`'s `EXPECTED_MATURITY`.
6. Update the "Last updated" date.
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

Last updated: 2026-10-17

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

### Result and error handling (ADR 0030)

- **`Result<T, E>`** — `Ok(v)` / `Error(e)` construct the tagged
  union `{ i1 is_ok, T ok, E err }`. `match` on a `Result`
  switches on the tag and extracts the payload into the arm's
  binding. Both interpreter and LLVM support the construction
  and matching.
- **`try` / `catch`** — interpreter-only. LLVM and WASM refuse
  programs using `try`/`catch` at the capability boundary via
  `Feature::TryCatch`, before codegen runs. Result *values*
  (`Ok`/`Error` construction, `match` on `Result`) remain
  supported on LLVM via the separate `Feature::Result` gate; only
  the `try`/`catch` construct is interpreter-only. The builder
  emits `Instruction::TryCatchMarker` at the top of the lowering
  so the capability scan can distinguish the construct from a
  hand-written `match` on a `Result`.
- **Not yet supported:** `?`-style propagation (not part of the
  language), converting between `Result` and `Option` (both exist
  but no builtin), LLVM/WASM lowering for `try`/`catch`.

### Sum types (ADR 0055)

- **Union declarations.** `union Json { Null Num(Int) Str(String) }`
  declares a tagged-union type. Variants may carry payloads of any
  type, including the union itself through an indirection
  (`Arr(List<Json>)`, `Obj(Map<String, Json>)`).
- **Construction.** `Json.Num(42)` and payloadless `Json.Null` are
  recognized by the analyzer and lowered by the builder to
  `Instruction::UnionConstruct`.
- **Pattern matching.** `match j ... case Num(n) then ...` extracts
  the payload and binds it with its declared type. Every variant
  must be covered, or a `case _` fallback must be present.
- **Guards.** `case Pos(n) if n > 0 then ...` evaluates the
  condition with the payload bindings in scope; if the guard is
  false, the switch falls through to the next case. Guarded arms
  do not count as exhaustiveness coverage.
- **Cross-file unions.** A `pub union` declared in an imported
  file is merged into the top-level program and usable from
  function signatures.
- **Interpreter-only.** LLVM and WASM refuse union programs at the
  capability boundary (`Feature::Unions`) with an `E0002` error.
  See `examples/json/` for the end-to-end demonstration. This
  refusal covers the union *representation*; the `match`
  mechanism itself works on all three backends for plain enums,
  `Option`, `Result`, and literal patterns — see
  `corpus_61_enum_match_llvm`, `corpus_43_match_bindings_llvm`,
  `corpus_45_result_llvm`. "Match on unions with payloads is
  refused on LLVM" is really "union values are refused on LLVM."
- **Not yet supported:** nested patterns (`case Pair(Some(x), _)`),
  reference patterns (`case &Num(n)`), LLVM/WASM codegen for
  unions.

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
- **LLVM and WASM support** (ADR 0036). Records lower to LLVM
  structs. Construction, field read/write, and the ABI for
  parameters, returns, and nested fields all work on both
  compiled backends. Verified end-to-end with `Point { x: 3, y: 4 }
  → p.x + p.y` printing `7` natively.
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
- **Partially supported:** field-level borrows (`&p.x`). The
  *write* side is place-granular (ADR 0044 stage 2, commit
  `6819517`): `p.x := ...` conflicts with a live borrow of `p.x`
  but not with a borrow of `p.y`. The *read* side is still
  variable-granular; completing it is ADR 0044 follow-up.
- **Not yet supported:** auto-derived traits, nested destructuring
  beyond one level.

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
- **`List.append` is supported on LLVM and WASM** (ADR 0042
  phase 4). It grows a stack-backed buffer to the heap, reallocates
  when full, and mutates in place when spare capacity exists.
  Verified end-to-end with `var xs: List<Int> := [1,2,3]; xs.append(4)`
  printing the updated length natively.
- **`Map<K, V>` on LLVM and WASM.** LLVM supports `Map<Int, Int>`
  and `Map<String, Int>` (ADR 0027 + a `_str_int` extension of the
  runtime). Other key/value shapes — `Map<String, String>`,
  `Map<Int, String>`, `Map<Bool, T>` — are refused at codegen with
  a targeted error. WASM refuses all `Map` shapes at the capability
  check. `Map.keys` / `.values` are interpreter-only on LLVM.

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

Five conversion and string-manipulation builtins landed.

- `Int.to_string` has an LLVM lowering (`sprintf` into a stack
  buffer) and works on both native backends.
- `String.to_upper` / `.to_lower` / `.concat` / `.substring` / `.charAt` / `.trim`
  have LLVM lowerings and work on both native backends.
- `String.to_int` has an LLVM lowering (`runtime/string.c`, strict
  i64 parser matching the interpreter's `str::parse::<i64>()`) and
  works on both native backends.
- `String.split` and `String.join` work on both native backends.
  `runtime/string.c`'s `algol26_string_split` (codepoint-aware on
  empty separator, byte-exact substring match otherwise) and
  `algol26_string_join` (iterates a `List<String>`, concatenates
  with separator) match the interpreter's Rust `str::split` and
  `slice::join` behavior.
- `File.read` / `.write` / `.append` work on both native backends
  via `runtime/file.c`. Signatures match the interpreter: read
  returns `String`, write/append return the byte count as `Int`.
  See "Known Gaps" for the UTF-8 divergence below.

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
  `List<String>`. Works on both native backends: the interpreter
  reads from its `command_line_args`; LLVM forwards `(argc, argv)`
  through the generated `main` wrapper to `runtime/args.c`. The CLI
  passes program arguments after a `--` separator. `argv[0]` (the
  program name) is skipped on both backends.
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
| `List.append(x)` | ✅ | ✅ | ✅ | ✅ | — |
| `Map<K, V>` | ✅ | ✅ | ✅ | ✅ | — |
| Strings | ✅ | ✅ | ✅ | ✅ | 08, 15 |
| `String.length` | ✅ | ✅ | ✅ | ✅ | 08 |
| `String.charAt` | ✅ | ✅ | ✅ | ✅ | — |
| `String.to_upper` / `.to_lower` / `.concat` / `.substring` | ✅ | ✅ | ✅ | ✅ | — |
| `String.trim` | ✅ | ✅ | ✅ | ✅ | — |
| `String.split` / `.join` | ✅ | ✅ | ✅ | ✅ | — |
| `Int.to_string` | ✅ | ✅ | ✅ | ✅ | — |
| `String.to_int` | ✅ | ✅ | ✅ | ✅ | — |
| `File.read` / `.write` / `.append` | ✅ | ✅ | ✅ | ✅ | — |
| `Math.*` | ✅ | ✅ | ✅ | ✅ | — |
| `Option` / `Some` / `None` | ✅ | ✅ | ✅ | ✅ | 13 |
| `match` (literal patterns) | ✅ | ✅ | ✅ | ✅ | — |
| `match` (binding patterns) | ✅ | ✅ | ✅ | ✅ | 13 |
| `Result` / `Ok` / `Error` | ✅ | ✅ | ✅ | ✅ | 14 |
| `try` / `catch` | ✅ | ✅ | ✅ | ⛔ | 14 |
| Traits + impls | ✅ | ✅ | ✅ | ✅ | 28, 29, 37 |
| `&dyn Trait` / `&mut dyn Trait` | ✅ | ✅ | ✅ | ✅ | — |
| Visibility (`pub`) | ✅ | ✅ | ✅ | ✅ | — |
| Associated types | ✅ | ✅ | ✅ | ✅ | — |
| Generics | ✅ | ✅ | ✅ | ✅ | — |
| `rec` records | ✅ | ✅ | ✅ | ✅ | — |
| `union` types | ✅ | ✅ | ✅ | ✅ | — |
| Structural `Copy` | ✅ | ✅ | ✅ | ✅ | — |
| `region` | ✅ | ✅ | ✅ | ✅ | 30, 31 |
| `unsafe` | ✅ | ✅ | ✅ | ✅ | 32, 33 |
| `affirm(cond, msg)` | ✅ | ✅ | ✅ | ✅ | — |
| `args()` | ✅ | ✅ | ✅ | ✅ | — |
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

### `File.read` UTF-8 validation

The interpreter uses `std::fs::read_to_string`, which rejects
invalid UTF-8 with an error. LLVM's `File.read` (via `runtime/file.c`)
is byte-transparent — it returns whatever bytes are in the file
without validating. Reading a binary file or a file with malformed
UTF-8 succeeds on LLVM and aborts on the interpreter.

This is a deliberate v1 simplification. A UTF-8 validator in C
(~40 lines, rejecting overlong encodings, surrogates, and
truncated sequences) would close the gap; it's tracked as a
follow-up. Programs that read only well-formed UTF-8 (source files,
configuration, generated data) see identical behavior on both
backends.


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

- **`Map<K, V>` v1 partial lowering.** LLVM lowers three shapes:
  `Map<Int, Int>` (via `runtime/map.c`), `Map<String, Int>` (via
  `runtime/map_str_int.c`), and `Map<String, String>` (via
  `runtime/map_str_str.c`, which `strdup`s both keys and values
  and returns owned copies from `get`). Other key/value
  combinations (`Map<Int, String>`, `Map<Bool, T>`) are refused at
  codegen with a targeted error. `Map.keys` and
  `Map.values` are supported on all three shapes: keys sort
  ascending (int numeric, string byte-lex, matching the
  interpreter's `keys().sort()`), and values are sorted by key
  so `zip(keys, values)` aligns. Both supported
  shapes grow via rehashing when full, matching the interpreter's
  unbounded `HashMap`. Workaround for unsupported shapes: run
  through `--interpreter`.

- **`String.join` codegen.** The builtin is accepted by the
  capability check but has no `compile_builtin_value` arm in the
  LLVM backend; a program using it fails at codegen with `E0004`
  "unhandled builtin" rather than a clean `E0002` refusal.
  Workaround: run through `--interpreter`. Fix: either add a
  lowering (walk the list, concatenate with the separator) or
  classify `String.join` under `Feature::StringSplit` in
  `scan_call_name` so it's refused at the capability boundary.

- **Iterating over a list parameter** (`corpus_10`). Iterator
  setup is only done for locally-created list literals.
- **`try` / `catch` and `Result` values** (`corpus_14`). No LLVM
  lowering; refused cleanly.
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

- **`&mut self` method calls on complex receivers.**
  `x.field.bump()` where `bump` takes `&mut self` is rejected
  by the analyzer with E0007. Workaround: bind the field to a
  `var` local (`var tmp := x.field`), then call the method on
  that binding. `&self` and builtin methods on field-access
  receivers (`x.field.read()`, `x.field.trim()`) work without
  workaround. Regression coverage:
  tests/conformance/valid/methods/method_on_field.gol,
  method_on_field_literal.gol, method_on_field_control.gol,
  method_builtin_on_field.gol. Negative case:
  tests/conformance/invalid/method_on_field_mut_rejected.gol.
- **`proc` ending in a `match` is rejected by return-path
  analysis.** A `proc f(x: T) -> U` whose body is `match x
  case A then <expr> case B then <expr>` (no explicit `return`
  in the arms) is rejected with "may not return a value on all
  paths": `check_all_paths_return` doesn't recognize that a
  trailing `match` whose arms all produce values counts as
  returning on all paths. Workaround: use `case X then return
  expr` in each arm — confirmed to work by
  `corpus_61_enum_match_llvm`. Fix is a follow-up in the
  analyzer.

- **Brace-form gradebook output differs from indentation form.** The
  Honor Roll section of `examples/braces/gradebook.gol` is empty at
  runtime, while `examples/gradebook.gol` (indentation form) prints
  `Alice (91)`. The two files are supposed to be semantically
  identical (ADR 0054 alternate brace-block syntax).

  Minimal repro: a brace-form `proc` with a `val` list, a `for`
  loop, and an `if` that appends to the list compiles and runs, but
  the appended element doesn't appear in the returned list. The
  equivalent indentation form works correctly.

  Separately, a brace-form `proc` with `var` and a trailing
  expression as return value is rejected with E0002 ("may not
  return a value on all paths"), while the same shape in
  indentation form is accepted. The return-path analysis
  (`check_all_paths_return`) doesn't recognize a brace-form
  trailing expression as a return.

  These are ADR 0054 brace-form parser/analyzer gaps. The fixtures
  under `examples/braces/` were written during ADR 0054 and never
  fully validated against their indentation counterparts.

  Regression test: `tests/brace_examples.rs` (currently
  `#[ignore]`d). Un-ignore when the parser bugs are fixed.

- **LLVM codegen: variable shadowing across scopes reads the
  wrong alloca.** A variable declared in a later block can
  overwrite an earlier block's binding of the same name in
  `IRCodeGen::variables` — a flat `HashMap` — because
  `compile_function` iterates `func.blocks` in **creation
  order**, not execution order. A block whose source-level scope
  ended may still be compiled after a block that redeclares the
  same name.

  Concrete repro: `examples/wordfreq.gol`'s `to_sorted_list`
  has `match counts.get(key) case Some(n)` inside a `for` loop,
  then `val n := List.length(entries)` after the loop. On LLVM,
  the `WordCount { count: n }` construction inside the match
  body reads `%n10` (the outer `val n`'s alloca) instead of
  `%n` (the match binding's). The outer alloca is undefined at
  that point in execution, so the count is uninitialized memory
  (observed: garbage in the 126000-126113 range, apparent heap
  address fragments). The interpreter is correct — it uses a
  scope-chained environment, not a flat map.

  Verified in `examples/wordfreq.ll` (commit 6b4c024):
  `some_bind_n` stores to `%n` but `blk_43` loads from `%n10`.

  Isolation: removing the outer `val n`, or renaming it, fixes
  LLVM output. The trigger is the same-name shadowing, not the
  record type — the earlier diagnosis ("record field layout
  resembling map entry layout") was wrong.

  Fix (follow-up session; not a one-liner):
    (a) compile blocks in dominance/RPO order so an inner
        binding's block is compiled before its outer-redeclaring
        sibling, or
    (b) give `IRCodeGen` a scope stack that mirrors the IR
        builder's `push_scope`/`pop_scope` — requires the
        builder to emit scope markers, or the codegen to
        reconstruct scopes from the CFG, or
    (c) mangle match/if/for binding names in the IR builder
        (`translate_match` et al.) so shadowing produces
        distinct LLVM alloca names.

  Option (c) is the smallest but requires the builder's scope
  map to support aliasing `"n"` → `"n__match_N"` for the body.
  Options (a) and (b) are the architecturally correct fixes.

### Unimplemented features

- **Channels** (`corpus_23`–`corpus_26`). All three backends
  refuse channel programs at the capability boundary. See
  ADR 0020.

### Large output: File.append and String building

`File.append` is O(n) per call in both backends: `runtime/file.c`'s
`algol26_file_append` does `fopen(path, "ab")` + `fwrite` of only
the new bytes + `fclose`, and the interpreter's `File.append`
uses `OpenOptions::new().append(true).open(path)` with the same
syscall shape. There is no whole-file rewrite, no read-then-write,
no quadratic behavior.

`List.append` (used to accumulate output fragments) is O(1)
amortized: `emit_list_append` (ADR 0042 phase 4) doubles capacity
on growth and writes in place when spare capacity exists — the
same profile as `Vec::push`.

The recommended pattern for building large output (LLVM IR,
JSON, etc.) is therefore O(total length) end-to-end:

    var parts: List<String> := []
    for line in lines
        parts.append(line)                 // O(1) amortized
    val output := String.join(parts, "\n")  // O(total length)
    File.write("out.ll", output)            // O(total length)

`String.concat` in a loop *is* O(n^2), but that pattern has never
been the natural way to build output — `List<String>` +
`String.join` is.

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

### WASM string UTF-8 gap

`String.char_at`, `String.trim`, `String.to_chars`, and
`Char.to_string` lower to `algol26_string_char_at`,
`algol26_string_trim`, `algol26_string_to_chars`, and
`algol26_char_to_string` on the WASM backend (see the host's
unresolved-import diagnostics). None of these symbols are
exported by `runtime/wasm/host.js`. A WASM build of any program
using them succeeds at compile time (`algol26 wasm` emits the
.wasm), but the host refuses to instantiate the module, printing
`[wasm host] unresolved imports: ...` at startup. Closing this
requires exporting all four functions from `runtime/wasm/host.js`
with the same contracts as the C implementations, or adding a
WASM capability gate to refuse these builtins at scan time.

### Result<T, E> codegen gap (LLVM)

Three conformance fixtures fail on the LLVM backend with an
`unreachable!` in `src/backends/llvm_codegen/types.rs` on
`Type::Unknown`:

  - tests/conformance/valid/result/result_try.gol
  - tests/conformance/valid/result/catch_named_binding.gol
  - (one more; run `bash tools/verify_all_backends.sh` for the full list)

The interpreter produces correct output for all three. Reaching
codegen with an unresolved type means the IR verifier accepted a
program whose `Result`-propagation shapes it does not actually
model. The fix is to tighten `verify_ir` to reject the shapes,
or to teach the analyzer to normalize `Type::Unknown` before IR
construction. Pre-existing; not caused by the String.to_chars
landing.

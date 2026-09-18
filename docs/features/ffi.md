# Feature: FFI (`extern "C"` / `unsafe`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is FFI in ALGOL26, and where does it live?"

## Summary

FFI (Foreign Function Interface) is how ALGOL26 code calls into
C libraries and, in principle, how external code can call into
ALGOL26-compiled functions. The current implementation covers the
**import** direction only: ALGOL26 declares an `extern "C"`
function, the analyzer type-checks calls to it, and the LLVM backend
lowers the declaration and the calls to the corresponding C symbols.

FFI is the only feature in the language that explicitly crosses the
memory-safety boundary. Every other feature is checked end-to-end by
the analyzer and the verifier; FFI cannot be, because the foreign
code is outside the compiler's reach. The `unsafe` keyword marks the
boundary; ADR 0009 discusses why it exists.

## Syntax

Basic declaration:

```gol
extern "C" function sqrt(x: Float) -> Float
```

Symbol renaming (`as`):

```gol
extern "C" function fast_sqrt(x: Float) -> Float as "sqrt"
```

The ALGOL26 name is `fast_sqrt`; the C symbol emitted is `sqrt`.

Library binding (`from`):

```gol
extern "C" from "m" function sqrt(x: Float) -> Float
```

The linker adds `-lm` when building the executable.

Variadic declaration (`...`):

```gol
extern "C" function printf(fmt: String, ...) -> Int
```

Calling an extern function:

```gol
val x := sqrt(2.0)
printf("value: %f\n", x)
```

Calls are syntactically identical to normal function calls. The
`unsafe` keyword is not required at the call site — the `extern`
declaration is what marks the boundary.

**Note:** the `unsafe` keyword exists in the language (ADR 0009)
and appears in examples (`unsafe_simple.gol`,
`unsafe_in_fn.gol`, both in `tests/corpus/`), but its interaction
with FFI declarations is not currently enforced. See Open
Questions.

## Typing rules

An `extern "C"` declaration introduces a function signature that
is otherwise identical to a normal `function` declaration. The
argument and return types are type-checked against the call sites
the same way.

### FFI type mapping

Not every ALGOL26 type has a C equivalent. `src/ffi/lowering.rs`
defines the mapping; tests in `ffi::lowering::tests` assert it:

| ALGOL26 type | C type | Notes |
|---|---|---|
| `Int` | `int64_t` / `long long` | |
| `Float` | `double` | |
| `Bool` | `int` | C has no native bool in the ABI |
| `String` | `char*` | Null-terminated; ALGOL26 owns the buffer |
| `*Unknown` | `void*` | Raw pointer |
| `*T` | `T*` | Typed pointer |
| `Void` | `void` | Only valid as return type |

Types that **cannot** cross the FFI boundary:

- `List<T>`, `Option<T>`, `Result<T, E>`, `Channel<T>` — composite
  types with no C equivalent.
- `&T` / `&mut T` — references are a compile-time concept; the FFI
  boundary sees only pointers.
- `Tuple`, `Array<T, N>` — no C equivalent.

Passing a forbidden type to an `extern` function is a type error at
the call site. `test_ffi_type_validation` in `src/ffi/lowering.rs`
covers this.

### Variadic declarations

An `extern "C"` function may declare `...` as its final parameter
to indicate C-style variadic arguments. Calls to a variadic extern
function are checked for:

- The declared (non-variadic) parameters must match.
- Additional arguments are untyped — the analyzer accepts any
  number and any type, matching C's variadic ABI.

`test_variadic_extern_accepts_extra_args` and
`test_variadic_extern_rejects_too_few_args` in
`src/semantics/analyzer/` pin this behavior.

The `SemanticProgram` carries a `variadic_functions: HashSet<String>`
field. The IR verifier's arity check relaxes for names in this set.

## IR representation

Extern functions appear in `SemanticProgram.functions` with
`is_extern: true`. Their `blocks` are empty — the function has no
body to lower.

Additional state on `SemanticProgram`:

| Field | Purpose |
|---|---|
| `ffi_symbols: HashMap<String, String>` | ALGOL26 name -> C symbol |
| `ffi_libraries: Vec<String>` | Libraries to link with (`-l<name>`) |
| `variadic_functions: HashSet<String>` | Names declared variadic |

The `ffi_symbols` map is populated by the IR builder when a
declaration uses `as "sym"`. If no `as` is present, no entry is
created and the C symbol equals the ALGOL26 name.

The `ffi_libraries` vector is populated for each `from "lib"`
clause and consumed by the linker driver in
`src/toolchain/linker.rs`.

### IR verifier rules

The verifier checks calls to extern functions the same way it checks
calls to any function: arity match, argument type compatibility,
return type propagation. Variadic externs are exempt from the arity
check for arguments beyond the declared parameters.

Extern functions have no body to verify. The verifier skips them:

```rust
if func.is_extern { return Ok(()); }
```

## CFG representation

Extern functions have no blocks, so they never appear in the CFG.
Calls to them are translated to `CfgInstruction::Call`, which
records the callee name and argument names for dataflow purposes.

The CFG does not model FFI boundaries differently from ordinary
calls. It does not know that the callee has no body.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | **Unsupported** | `interpreter_rejects_ffi` in `src/backends/capabilities/tests.rs` |
| LLVM | **Supported** | `llvm_accepts_ffi` in `src/backends/capabilities/tests.rs` |
| WASM | **Unsupported** | No WASM FFI lowering; capability check refuses |

### LLVM

The LLVM backend handles FFI in three places:

1. **Declaration.** Each `extern` function is declared in the LLVM
   module with the correct C symbol name (`ffi_symbols` lookup) and
   the correct signature (using the FFI type mapping).
2. **Call lowering.** A call to an extern function lowers to a
   direct `call` instruction against the declared function.
3. **Linker.** `src/toolchain/linker.rs` invokes `clang` with the
   `.ll` file and any `-l<lib>` flags from `ffi_libraries`.

Variadic functions lower to variadic LLVM functions (`fn_type` with
`is_var_args: true`). The call site does not need special lowering
because LLVM's `call` handles varargs natively.

### Interpreter

The interpreter refuses any program using FFI. When the capability
check runs, it produces a compile error before the interpreter
starts. This is correct fail-closed behavior.

Attempting to run an FFI program with `--interpreter` fails with a
capability diagnostic naming the offending construct.

### WASM

The WASM backend has no FFI lowering. WASM modules import functions
from a host environment rather than linking against a system
library. A future implementation would need a host-provided import
table; this is not implemented. The capability check refuses.

## Diagnostics

FFI-related error codes currently emitted:

**None in the `E-XXX-NNN` format.** FFI errors — unknown library,
forbidden type at the boundary, unresolved symbol at link time — are
produced as free-form `CompileError::simple(...)` values with
generic codes (`E0001`, `E0004`, `E0009`).

**This is the fifth feature (after traits, generics, defer, spawn)
with the same diagnostic gap.** A future Tier 2 sweep would give
FFI errors distinct codes like `E-FFI-001` (forbidden type),
`E-FFI-002` (unknown symbol at link time), `E-FFI-003` (library not
found).

## Safety

FFI is the **only** feature in ALGOL26 that can violate memory
safety. The language's guarantees — no use-after-free, no data
races, no null dereferences — are enforced by the analyzer for
ALGOL26 code, but cannot extend across the boundary.

The `unsafe` keyword exists to mark this boundary (ADR 0009), but
its exact interaction with FFI is not currently enforced:

- An `extern "C"` declaration is allowed at the top of a file
  without an enclosing `unsafe` block.
- A call to an extern function is allowed at any call site, not
  just inside an `unsafe` block.
- The examples `unsafe_simple.gol` and `unsafe_in_fn.gol` show the
  `unsafe` keyword in use, but the corpus does not have a test
  that asserts `unsafe` is *required* for FFI.

**This is the most important open question in this contract.** If
`unsafe` is meant to gate FFI (as it does in Rust), then the current
behavior is a safety gap: FFI can be used without the syntactic
marker that warns the reader a safety boundary is being crossed.

If `unsafe` is meant to gate something else (raw pointer arithmetic,
memory unmapping, etc.), then the ADR should clarify. The current
state is ambiguous.

## Test coverage

Current coverage across the tree:

**FFI lowering (`src/ffi/lowering.rs::tests`):**

- `test_ffi_registry_with_types` — the type mapping is exercised
- `test_ffi_type_validation` — forbidden types are rejected

**Frontend (`tests/frontend/ffi_test.rs`):**

- `test_parse_simple_ffi` — basic `extern "C"` declaration
- `test_parse_symbol_renaming` — the `as "sym"` syntax

**Analyzer:**

- `test_variadic_extern_accepts_extra_args`
- `test_variadic_extern_rejects_too_few_args`

**Capability tests (`src/backends/capabilities/tests.rs`):**

- `interpreter_rejects_ffi`
- `llvm_accepts_ffi`

**Corpus:**

- `examples/ffi/ffi_test.gol`
- `tests/conformance/valid/ffi_math.gol`
- `tests/programs/valid/ffi_math.gol`

**Adversarial (none directly):** the adversarial suite does not
have FFI cases; the boundary is not exercised by an out-of-bounds
call.

### Gaps

- **No test that an `extern` call with the wrong argument count
  fails.** Non-variadic externs should be checked for arity; the
  analyzer does this for normal functions but a dedicated extern
  test would pin it.
- **No test for `extern` with `unsafe`.** Whether `unsafe` is
  required, forbidden, or ignored in FFI declarations is not
  covered by tests.
- **No test for a missing library at link time.** Passing
  `from "nonexistent"` should produce a linker error; not covered.
- **No test for a symbol that does not exist at link time.**
  Declaring `extern "C" function does_not_exist()` should fail at
  link time on any real system; not covered.
- **No differential test.** The interpreter refuses FFI, so a
  differential test is impossible by construction. The right
  equivalent is a compile-only test that asserts the LLVM path
  succeeds and the interpreter path is refused.
- **No test for calling a variadic function through an `as`
  renaming.** `extern "C" function my_printf(fmt: String, ...)
  as "printf"` — the combination of renaming and variadic is
  untested.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
FFI (extern "C")
    semantics:   Partial (unsafe gating unverified)
    parsed:      yes
    typed:       yes (type mapping enforced)
    validated:   yes (forbidden types rejected)
    IR:          yes (is_extern + ffi_symbols + ffi_libraries)
    verified:    yes (calls type-checked; body N/A)
    interpreter: unsupported (correct refusal)
    LLVM:        supported
    WASM:        unsupported (correct refusal)
    optimized:   N/A
```

FFI is the only feature in this directory that is supported on
**exactly one** backend. This is by design: C interop is a
platform-specific concern, and a portable interpreter or WASM
module cannot in general resolve a C symbol.

The single-backend support is fine as long as the capability check
refuses the other backends correctly, which it does.

## Checklist for related features

If you are adding a feature *like* FFI (a compile-time declaration
of an external entity with a type-checked interface and a
backend-specific lowering), you need to touch:

1. `src/frontend/lexer/` — keywords (`extern`, `as`, `from` if new).
2. `src/frontend/parser/` — parse the declaration syntax.
3. `src/frontend/ast.rs` — AST nodes for the declaration.
4. `src/semantics/analyzer/` — type-check declarations and calls.
5. `src/ffi/` — a registry and lowering module.
6. `src/ir/semantic_ir.rs` — a field on `SemanticProgram` (or a
   new `Instruction` variant) to carry the declaration metadata.
7. `src/ir/verifier/` — rules for calls to the new entity.
8. `src/backends/<target>/` — lowering on the supporting backend.
9. `src/backends/capabilities/scan.rs` — declare support (usually
   a single backend).
10. `src/backends/capabilities/tests.rs` — accept/reject per backend.
11. `src/toolchain/linker.rs` — pass any library/flag to the linker.
12. `tests/frontend/` — parse tests.
13. `tests/conformance/valid/<feature>.gol`.
14. `examples/` — a runnable example.
15. `docs/features/<feature>.md` — this file.

## Open questions

- **Is `unsafe` required for FFI?** This is the biggest open
  question. In Rust, FFI requires `unsafe`; the ALGOL26 examples
  use `unsafe` for something (raw pointer ops?), but the corpus
  does not have a test asserting `unsafe` is required at an `extern`
  call site. Resolving this determines whether the current design
  is a safety hole or correct.

- **Should there be an FFI type-checking pass that verifies the
  C ABI layout?** The current type mapping (`Int` -> `int64_t`,
  etc.) is asserted by tests, but a struct passed by value, a
  `#pragma pack` difference, or an ABI mismatch between the
  declared signature and the actual C symbol would not be caught.
  A `c-bindgen`-style tool that generates ALGOL26 `extern`
  declarations from a `.h` file would reduce the risk. Not
  implemented.

- **Should `unsafe` blocks be required at FFI call sites, or only
  at declaration?** Rust requires it at both. Requiring only at
  declaration is more ergonomic; requiring at both makes the
  boundary more visible. Currently neither is enforced.

- **Should ALGOL26 functions be exportable?** The current
  implementation covers import only. Exporting (declaring an
  ALGOL26 function as `extern "C"` so C code can call it) is a
  natural extension. Not implemented.

- **Should there be a `c_sizeof` or similar builtin?** Foreign
  structures need size information to allocate the right buffer.
  Currently the user must hard-code sizes. Not implemented.

- **Where do FFI declarations live?** Currently at module top
  level. Should they be importable from a shared header-style
  module? Not implemented.

- **Is the linker driver robust against missing `clang`?**
  `src/toolchain/linker.rs` runs `clang` as a subprocess. If
  `clang` is not installed, the compiler errors. This is fine for
  a systems language, but the error message quality is worth
  checking.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/decisions/0009-unsafe.md` — the design decision behind `unsafe`
- `docs/no-panic-policy.md` — rules about panics and errors at the FFI boundary
- `docs/features/alloc_free.md` — (to be written) the memory primitives that FFI interacts with
- `src/ffi/c.rs`, `src/ffi/lowering.rs`, `src/ffi/mod.rs` — FFI implementation
- `src/ir/semantic_ir.rs` — `ffi_symbols`, `ffi_libraries`, `variadic_functions`
- `src/toolchain/linker.rs` — the clang invocation
- `tests/frontend/ffi_test.rs`
- `examples/ffi/ffi_test.gol`
- `tests/conformance/valid/ffi_math.gol`

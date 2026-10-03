# FFI soundness

Tests in this directory exercise the **static** FFI boundary: which
ALGOL26 types may cross an `extern "C"` declaration, and how call-site
arity/variadics are enforced.

They are run by `tests/soundness_runner.rs`, which executes:

    Compiler::build_semantic_ir_for(source)
      → build_cfgs_from_semantic_program
      → DataflowEngine::new(OwnershipTransfer).run_all

That pipeline is **front-end only**. It has no LLVM backend, no
linker, no runtime, and no heap model.

## What is covered

- Composite types (`List<T>`, `Option<T>`) rejected at the boundary
- References (`&T`, `&mut T`) rejected at the boundary
- Nominal record types rejected at the boundary
- Arity checking for non-variadic externs
- Positive: scalar, pointer, string, and variadic call sites accepted

See `docs/features/ffi.md` §"FFI type mapping" for the authoritative
list of allowed and forbidden types.

## What is **not** covered (and why)

The following classes of FFI bug are **runtime properties** and
cannot be detected by this runner. They remain an open gap:

| Class | Why the runner can't see it | What would be needed |
|---|---|---|
| Double free across FFI | Ownership dataflow treats extern calls as opaque; no heap model | An ownership pass that understands `malloc`/`free` semantics, or a runtime harness with ASan |
| Dangling pointer returned from C | The returned `*T` carries no region/lifetime information | Extend `SemanticProgram` to model FFI return lifetimes |
| Struct layout mismatch through `void*` | The type checker only sees `*Unknown` | Cross-check with a C header, or a `#[repr(C)]`-style layout assertion |
| `unsafe` enforcement at FFI call sites | Currently documented as "not enforced" in `docs/features/ffi.md` | See `ffi_unsafe_not_enforced.gol` below |

If/when any of the above become statically detectable, add tests
here. If they become runtime-checkable, they belong in a separate
`tests/ffi_runtime/` harness — *not* in this directory, because
`soundness_runner.rs` will not run them.

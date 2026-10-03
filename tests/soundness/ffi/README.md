# FFI soundness

Tests in this directory exercise the **static** FFI boundary. They
are run by `tests/soundness_runner.rs`, which executes:

    Compiler::build_semantic_ir_for(source)
      → build_cfgs_from_semantic_program
      → DataflowEngine::new(OwnershipTransfer).run_all

That pipeline is **front-end only** — no LLVM, no linker, no runtime.

## What is actually enforced today

| Property | Test | Status |
|---|---|---|
| Arity checked for non-variadic externs | `arity_mismatch_rejected.gol` | ✅ enforced |
| Variadic externs accept extra args | `variadic_extra_args_ok.gol` | ✅ enforced |
| Scalar FFI (`Int`/`Float`/`Bool`) accepted | `scalar_argument_ok.gol` | ✅ enforced |
| `String` maps to `char*` | `string_argument_ok.gol` | ✅ enforced |

## What is enforced

All FFI-boundary checks below are enforced at **declaration site**
by `src/semantics/analyzer/items.rs` in `analyze_function`:

- `List<T>`, `Option<T>`, `Map<K,V>`, `Result<T,E>`, `Record`, `Array`,
  `Tuple`, `Channel<T>` — rejected as FFI parameter or return types
- `&T`, `&mut T` — rejected (references are compile-time only)
- `Void` as a parameter — rejected (`Void` is only valid in return position)

See `docs/status/ffi-boundary.md` for the closing note on this gap.

## What this runner cannot cover

Runtime FFI properties — double-free, dangling pointers returned
from C, layout mismatches through `void*` — are **not** testable
here because the pipeline has no heap model, no linker, and no
runtime. When those become checkable they belong in a separate
`tests/ffi_runtime/` harness, not this directory.
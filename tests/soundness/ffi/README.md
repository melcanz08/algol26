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

## Known gaps

The following are **documented as enforced in `docs/features/ffi.md`
but are not enforced in the analyzer today**. Each has a
characterization test here pinning the buggy behavior. When the
check lands, flip the test's `EXPECT:` line back to `REJECT`.

- `List<T>` argument to an `extern "C"` function —
  `list_argument_rejected.gol`
- `Option<T>` argument — `option_argument_rejected.gol`
- `&T` / `&mut T` argument — `reference_argument_rejected.gol`

See [`docs/status/ffi-boundary.md`](../../../docs/status/ffi-boundary.md)
for the diagnosis and the intended fix location
(`src/semantics/analyzer/expr.rs`, extern call-site path).

## What this runner cannot cover

Runtime FFI properties — double-free, dangling pointers returned
from C, layout mismatches through `void*` — are **not** testable
here because the pipeline has no heap model, no linker, and no
runtime. When those become checkable they belong in a separate
`tests/ffi_runtime/` harness, not this directory.
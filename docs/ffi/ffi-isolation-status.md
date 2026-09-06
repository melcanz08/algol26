# FFI Isolation — Status

**Date**: 2026-09-05
**Version**: v0.8.1

## Current State: ✅ ISOLATED

| Component | Status | Notes |
|-----------|--------|-------|
| `src/ffi/c.rs` | ✅ Isolated | CType, FFIInfo types |
| `src/ffi/lowering.rs` | ✅ Isolated | FFIRegistry + register_stdlib_functions + all_functions() + Clone |
| `ir_codegen.rs register_stdlib` | ✅ Uses registry | Calls FFIRegistry, iterates all_functions_cloned() for LLVM decls |
| `is_ffi` detection | ✅ Uses registry | is_ffi_call() delegates to ffi_registry.is_ffi() |

## What Changed

- IRCodeGen owns `ffi_registry: FFIRegistry`
- `register_stdlib()` creates registry via `register_stdlib_functions(&mut registry)`
- Math decls created from registry, not hardcoded array
- Added Clone derives to FFIRegistry/FFIFunction
- Tests: ffi_math.gol (4.0,8.0,0.0) + test_ffi_all_functions

## Verification

- cargo test --lib — 44 passed
- cargo test --test backends_tests — 19 passed
- list_print.gol and ffi_math.gol green

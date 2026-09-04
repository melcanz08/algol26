# FFI Isolation — Status

**Date**: 2026-09-04
**Version**: v0.8.0

## Current State: PARTIAL ISOLATION

| Component | Status | Notes |
|-----------|--------|-------|
| `src/ffi/c.rs` | ✅ Isolated | CType, FFIInfo types |
| `src/ffi/lowering.rs` | ✅ Created | FFIRegistry, register_stdlib_functions |
| `ir_codegen.rs` register_stdlib | ⚠️ Still hardcoded | Working but not using registry |

## What's Done

1. ✅ Created `src/ffi/lowering.rs` with:
   - `FFIRegistry` struct
   - `register_stdlib_functions()` function
   - FFI function tracking

2. ✅ Created `src/ffi/mod.rs` exposing both `c` and `lowering`

## What Remains

1. ⚠️ Replace hardcoded `register_stdlib` in `ir_codegen.rs` with `FFIRegistry`
2. ⚠️ Use `register_stdlib_functions()` instead of manual LLVM function creation
3. ⚠️ Use `FFIRegistry.is_ffi()` for extern function detection

## Why It's Still Hardcoded

The current `register_stdlib` works correctly (132 tests pass).
Replacing it with FFIRegistry requires:
- Modifying how LLVM function types are created
- Updating all call sites
- Testing all math/string/file functions still work

This is a **2-3 hour refactoring** that can be done in a future session.

## Recommendation

For v0.8.0, the FFI registry exists and is documented.
The hardcoded `register_stdlib` is the **temporary working solution**.
Future work: Replace hardcoded version with registry-based version.

# List Printing - v0.8.1 Bugfix

**Status**: Bugfix for v0.8.0, not a language change
**Date**: 2026-09-05

## Behavior

`print(List<T>)` is defined as:

- `print([1.0, 2.0, 3.0])` → `[1.0, 2.0, 3.0]\n`
- `val arr := [1.0, 2.0, 3.0]; print(arr)` → `[1.0, 2.0, 3.0]\n`
- `print(arr[idx])` → element via GEP

This matches interpreter oracle (`interpreter::display` for List).

## Backend Implementation

LLVM `IRCodeGen`:
- `emit_print_list_literal(elements: &[TypedIRValue])`
- `emit_print_list_var(var_name: &str)` iterates `list_arrays[var]` with `build_gep`
- `ArrayAccess` lowered via `f64_type.array_type(len)` + GEP

WASM and Interpreter already supported this via Display trait.

## Tests

- `tests/programs/valid/list_print.gol`
- `tests/backends/oracle_test.rs::test_oracle_print_list_var`
- `tests/backends/oracle_test.rs::test_oracle_print_list_literal`
- `tests/backends/oracle_test.rs::test_oracle_array_access`

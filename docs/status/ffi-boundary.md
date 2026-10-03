# FFI boundary — current state

## Summary

`docs/features/ffi.md` claims:

> Passing a forbidden type to an `extern` function is a type error
> at the call site. `test_ffi_type_validation` in
> `src/ffi/lowering.rs` covers this.

**Neither sentence is true as written.**

## What is actually validated

`FFIRegistry::validate_call` (`src/ffi/lowering.rs`) checks argument
types against a *hardcoded* registry entry. It is exercised by
`test_ffi_type_validation` and `test_ffi_registry_with_types` — both
of which construct the registry in-test and call `validate_call`
directly.

The registry is populated by `register_stdlib_functions` for the
`Math.*` surface. **User-declared `extern "C"` functions never reach
`validate_call`.** The analyzer's extern handling lives at
`src/semantics/analyzer/items.rs:493` and covers declaration and
variadic arity; the call-site path in
`src/semantics/analyzer/expr.rs` does not consult the FFI type map.

## What gets through today

Programs of this shape compile with **zero diagnostics**:

```gol
extern "C" function takes_list(x: List<Int>) -> Void
extern "C" function takes_option(x: Option<Int>) -> Void
extern "C" function takes_ref(x: &Int) -> Void

procedure main
    val xs := [1, 2, 3]
    takes_list(xs)          // List<Int> crosses the boundary
    takes_option(Some(5))   // Option<Int> crosses the boundary
    var n := 5
    takes_ref(&n)           // &Int crosses the boundary
end
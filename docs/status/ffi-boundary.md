# FFI boundary — current state

# FFI boundary — closed

**Status:** Fixed. The declaration-site FFI boundary check landed in
the `fix/ffi-boundary` branch. See the "History" section at the
bottom of this file for the original gap analysis.

## Current behavior

User-declared `extern "C"` functions are now validated at
**declaration site** in `src/semantics/analyzer/items.rs`:
`analyze_function` rejects any parameter or return type that has no
C ABI representation. The predicate is `Type::is_ffi_compatible` in
`src/common/types.rs`.

Pinned by `tests/soundness/ffi/{list,option,reference}_argument_rejected.gol`.

## History

This file was created during a repo-cleanup pass to document a real
gap: user-declared `extern "C"` functions were never validated
against the FFI type map — only the hardcoded `Math.*` stdlib
registry was. The three characterization tests pinned the bug. They
now pin the fix.

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
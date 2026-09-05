# List Ops — Fixed

## Before: 80% TEMPORARY
evaluate_list_operation only handled TypedIRValue::List literal, ignored Variable.
List.sum(arr) returned 0.0, List.length(arr) returned 0.

## After: ✅ GENERIC
- evaluate_list_operation() for const literals (existing)
- evaluate_list_operation_var() for runtime vars using list_arrays/list_lengths
- Emits LLVM loop with phi nodes for idx and acc, GEP load, fadd/select for max/min
- Handles both Call (result assignment) and compile_value (expression) paths

## Verified
- list_sum.gol: 3.0, 6.0, 3.0, 1.0, 30.0, 2.0
- list_print.gol still [1.0,2.0,3.0]
- backends_tests 19 passed

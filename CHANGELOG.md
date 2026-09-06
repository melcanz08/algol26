# Changelog

## [0.8.0-hardening] - 2026-05-13
### Level 5 Hardening Complete

#### 5.1 Negative Corpus (20 files)
- Expanded from 6 -> 20 invalid programs covering:
  - `07_missing_return`, `08_arity_mismatch`, `09_duplicate_decl`, `10_assign_to_val`
  - `11_break_outside_loop`, `12_defer_in_global`, `13_invalid_return_type`
  - `14_mut_borrow_of_val`, `15_redecl_function`, `16_invalid_for_type`
  - `17_deref_non_ref`, `18_invalid_binary_string_int`, `19_use_after_scope`
  - `20_invalid_nested_if_defer`, plus original 6 (`undefined_var`, `type_mismatch`, `invalid_syntax`, `invalid_array_index`, `double_borrow`, `use_after_move`)
- Enforcement: `test_negative_corpus_no_ice` asserts total>=20, invalid>=18, no ICE
- Invalid check uses **OR**: `SemanticIRBuilder diags OR SemanticAnalyzer Err` — required because borrow/move (E0007) is caught by Analyzer, while break/defer are caught by IR

#### 5.3 Stress
- `test_stress_100_vars_single_scope`, `test_stress_10_level_nested_if_for_defer_break_return`
- `test_stress_10_level_closure_capture`, nested functions, control flow, multiple imports

#### 5.4 Optimizer Safety
- Fixed string concat codegen (`String.concat` vs `+`)
- `test_optimization_preserves_borrow_semantics`: borrow ops before=2 after=2
- `test_optimization_diff_interpreter_complex` + preserves semantics for lists/strings/arithmetic
- Idempotent optimizer

#### 5.5 Diagnostics Quality
- `test_diag_double_borrow_e0007_with_help` — mentions var name + suggestion
- `test_diag_use_after_move_has_line` — E0007 with help

#### Tests
- `integration_tests`: **27/27 PASS**
- `release_hardening`: **17/17 PASS**

## [0.7.0] - Previous
- List ops, FFI isolation, interpreter hardening

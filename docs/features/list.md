# Feature: List<T>

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is `List<T>` in ALGOL26, and where does it live?"

## Summary

`List<T>` is ALGOL26's dynamically-sized, ordered sequence type.
Elements are of a single type `T`; there is no `List<Any>` in the
current language. Lists are constructed with `[a, b, c]` syntax and
indexed with `list[i]`.

Unlike `Array<T, N>` (fixed size, element type known at compile time),
`List<T>` has no compile-time length. The interpreter represents both
as a Rust `Vec<RuntimeValue>`; the LLVM backend treats `List` as a
pointer plus a length that is tracked separately.

## Syntax

Construction:

```gol
val empty   := []
val ints    := [1, 2, 3]
val floats  := [1.0, 2.0, 3.0]
val nested  := [[1, 2], [3, 4]]
```

Element type is inferred from the contents. Where inference fails
(empty list in an untyped context), an explicit annotation is needed:

```gol
val empty: List<Int> := []
```

Indexing:

```gol
val first := ints[0]
ints[1] := 42
```

Iteration:

```gol
for x in ints do
    print(x)
```

## Typing rules

| Expression | Type |
|---|---|
| `[]` in a context requiring `List<T>` | `List<T>` |
| `[a, b, c]` where `a, b, c: T` | `List<T>` |
| `list[i]` where `list: List<T>`, `i: Int` | `T` |
| `List.length(list)` | `Int` |

Type coercion is covariant in the element type: `List<Int>` coerces
to `List<Float>`. Nested lists coerce element-wise.

Heterogeneous list literals (`[1, 2.0, 3]`) are rejected — the
analyzer requires all elements to unify to a single `T`. To mix
numeric types, annotate explicitly (`[1.0, 2.0, 3.0]`) or cast.

Location in the type system: `src/common/types.rs`, variant
`Type::List(Box<Type>)`.
Constructor: `Type::list(element_type)`.

Parsing: `Type::from_str` accepts both `List<T>` and `list[T]` syntax.

## Ownership

A `List<T>` is a **non-`Copy`** type. Assigning a list to another
variable moves it; the original is no longer usable:

```gol
val a := [1, 2, 3]
val b := a            // move
print(a)              // compile error: use after move
```

Sharing without moving requires a borrow:

```gol
val a := [1, 2, 3]
val b := &a           // shared borrow
print(List.length(b)) // ok while a is alive
```

Mutable access through `&mut` follows the standard rules: while a
`&mut List` is live, the list cannot be read or moved.

Element-wise ownership: if `T` is non-`Copy` (e.g. `List<String>`),
indexing returns a copy only if `T: Copy`. For non-`Copy` elements,
indexing requires a borrow to read the element in place, or a move
to extract it:

```gol
val strings := ["a", "b"]
val first := strings[0]     // compile error: cannot move out of index
val r := &strings[0]        // ok: borrow
```

## IR representation

In `src/ir/semantic_ir.rs`:

| Concept | Variant |
|---|---|
| List literal | `TypedIRValue::List(Vec<TypedIRValue>, Type)` |
| Array literal (fixed-size, lowered same way) | `TypedIRValue::Array(Vec<TypedIRValue>, Type, usize)` |
| Read `list[i]` | `TypedIRValue::ArrayAccess { array, index, element_type }` |
| Write `list[i] := v` | `Instruction::ArrayAssign { array, index, value }` |
| Iteration setup | `Instruction::IteratorInit { iterator, iterable }` |
| Iteration step | `Terminator::IteratorNext { iterator, target, body_block, exit_block }` |

`List` and `Array` share all downstream IR — the interpreter treats
them identically, and the LLVM backend distinguishes them only in
lowering of the length, which is static for `Array` and dynamic for
`List`.

### IR verifier rules

The verifier enforces:

- `TypedIRValue::List(elems, T)` requires all `elems` to verify and
  their types to coerce to `T`.
- `TypedIRValue::ArrayAccess` requires `array` to be a `List<T>` or
  `Array<T, N>`, `index` to be `Int`, and returns `element_type`.
  The static check does **not** prove `0 <= index < length`.
- `Instruction::ArrayAssign` requires the target to be a mutable
  `List<T>` and `value` to coerce to `T`.
- `Instruction::IteratorInit` requires `iterable` to be a `List<T>`.
  The iterator binds a loop variable of type `T` in the body block.

## Bounds checking

Out-of-bounds access is a **runtime error**, not a compile error.
The language cannot prove `0 <= i < len(list)` in general, so the
check is deferred to the interpreter (and to a runtime check in the
LLVM backend when the case is not statically eliminable).

Interpreter behavior:

- Negative index → `EvalError::Runtime("array index N is negative")`.
- Index >= length → `EvalError::Runtime("array index N out of bounds (length L)")`.
- Access on a non-list value → `EvalError::TypeMismatch`.

See `src/backends/interpreter/eval.rs`, `TypedIRValue::ArrayAccess` arm.

LLVM behavior: `List` bounds checking is on the runtime-check list for
Tier 2 (fail-closed audit) — see the roadmap. Until that lands, LLVM
does not enforce bounds for `List`; use the interpreter when
correctness matters more than performance.

## Builtins

In `src/ir/verifier/builtins.rs` and `src/backends/interpreter/eval.rs`:

| Builtin | Signature | Behavior |
|---|---|---|
| `List.length` | `List<T> -> Int` | Number of elements |
| `len` / `length` | `List<T> -> Int` or `String -> Int` | Alias for `List.length` (or `String.length`) |
| `List.sum` | `List<Int>|List<Float> -> Float` | Sum as `Float` |
| `List.max` | `List<Int>|List<Float> -> Float` | Max as `Float`, `-inf` on empty |
| `List.min` | `List<Int>|List<Float> -> Float` | Min as `Float`, `+inf` on empty |

`List.sum`/`max`/`min` return `Float` unconditionally, even for
`List<Int>`. This is a known design choice, not an oversight — see
`docs/decisions/0003-type-system.md` for the numeric tower.

Method-call syntax desugars to function calls: `nums.length` and
`nums.length()` both become `Call(List.length, [nums])`. See
`tests/ir/borrow_deref_addrof_test.rs::test_method_call_desugars_to_function_call`.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported | `interpreter_accepts_list_print` in `src/backends/capabilities/tests.rs` |
| LLVM | **Partial** | Length yes, printing no — see capability tests `llvm_rejects_list_print` |
| WASM | **Unverified** | No test currently pins `List` support on WASM. |

The interpreter represents `List<T>` as
`RuntimeValue::List(Vec<RuntimeValue>)` in
`src/backends/interpreter/runtime.rs`.

`runtime_eq` structural comparison handles `List` element-wise:
two lists are equal if they have the same length and all elements
compare equal recursively.

### LLVM partial support

`List.length` lowers to a compile-time constant when the list is a
literal, or a runtime count when the list is dynamic. The codegen
tracks static lengths in `self.list_lengths` and errors with
`E0004` if `List.length` is called on a list of unknown length:

```
LLVM codegen: List.length called on unknown list 'X' (known lists: [...])
```

`print(list)` is refused by the LLVM capability check. The interpreter
prints `[1, 2, 3]`; LLVM has no lowering for aggregate printing yet.

This asymmetry is one of the concrete cases `docs/architecture-direction.md`
points at: a feature can be supported on one backend and refused on
another, as long as the refusal is a compile error rather than a
silent fallback.

## Optimizer rules

None implemented yet. Candidates:

- Constant folding of `[1, 2, 3].length` to `3` (already happens at
  codegen time for LLVM, but not in the IR optimizer).
- Dead-list elimination: `val unused := [1, 2, 3]` where `unused` has
  no side-effecting uses.

## Safety

- No implicit null: `List<T>` is always a valid list.
- Bounds are checked at runtime (interpreter) or by the capability
  check (LLVM refuses programs using list printing, but does not
  yet refuse programs indexing a list out of bounds).
- Move semantics are enforced by the ownership analyzer; using a
  moved list is a compile error.
- Element-type homogeneity is enforced at parse/analysis time; a
  heterogeneous literal is rejected before IR construction.

## Test coverage

Current coverage across the tree:

- `tests/corpus/corpus_01_sum_list.gol` — sum of list
- `tests/corpus/corpus_15_list_of_strings.gol` — list of strings
- `tests/conformance/valid/lists.gol`
- `tests/conformance/valid/list_print.gol`
- `tests/conformance/valid/list_sum.gol`
- `tests/conformance/valid/list_length_builtin.gol`
- `tests/differential/differential_true.rs::test_differential_list_length_builtin`
- `tests/differential/differential_true.rs::test_differential_array_sum`
- `tests/differential/differential_true.rs::test_differential_for_int_list`
- `tests/differential/differential_true.rs::test_differential_for_float_list_with_if`
- `tests/differential/differential_true.rs::test_differential_for_int_list_count`
- `src/backends/capabilities/tests.rs`: accept (interpreter) + reject (LLVM) for list printing
- `src/semantics/analyzer/`: tests for array/list type checks
- `src/ir/verifier/`: `verifier_rejects_iterator_init_on_non_list`

### Gaps

- No test for empty list (`[]`) as a value.
- No test for nested lists (`[[1, 2], [3, 4]]`) — construction parses,
  but I have not verified evaluation.
- No test for `List.sum`/`max`/`min` on empty lists (returns `0.0`,
  `-inf`, `+inf` respectively — this should be pinned).
- No test for negative index or out-of-bounds on a `List` (only on
  `Array`).
- No test for `List<Float>` sum returning correct `Float`.
- No test for `List<String>` printing.
- No per-backend fixture under `tests/conformance/`.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
List<T>
    semantics:   Stable
    parsed:      yes
    typed:       yes
    validated:   yes
    IR:          yes
    verified:    yes
    interpreter: supported
    LLVM:        partial (length yes; print no; bounds unchecked)
    WASM:        unverified
    optimized:   no rules
```

The partial LLVM support is the concrete case Tier 2 (fail-closed
audit) will resolve: LLVM either supports a list operation fully, or
refuses the whole program with a capability error — never silently
produces a wrong result.

## Checklist for related features

If you are adding a feature *like* `List<T>` (a dynamically-sized
container), you need to touch:

1. `src/common/types.rs` — new `Type` variant + constructor + parsing
   + `can_coerce_to` + `common_supertype` + `contains_type_var` +
   `substitute` + `inner_type` + `Display`.
2. `src/ir/semantic_ir.rs` — new `TypedIRValue` variant(s) + new
   `Instruction` variant(s) for mutation if applicable.
3. `src/ir/verifier/` — rules for the new variants.
4. `src/semantics/analyzer/` — type-checking rules, plus ownership
   rules for mutation and move.
5. `src/backends/interpreter/runtime.rs` — a `RuntimeValue` variant.
6. `src/backends/interpreter/eval.rs` — construction, indexing, iteration.
7. `src/backends/interpreter/mod.rs` — any new `Instruction` handling.
8. `src/backends/interpreter/runtime.rs` — extend `runtime_eq`.
9. `src/backends/llvm_codegen/` — lowering, or a capability refusal if
   lowering is deferred.
10. `src/backends/capabilities/scan.rs` — declare which backends
    support it.
11. `src/backends/capabilities/tests.rs` — accept/reject tests per backend.
12. `src/ir/verifier/builtins.rs` — signatures for any new builtins.
13. `tests/conformance/valid/<feature>.gol` — the program fixture.
14. `tests/corpus/` — one end-to-end example program.
15. `docs/features/<feature>.md` — this file.

## Open questions

- **Should `List.sum`/`max`/`min` be parameterized over `T`?** Currently
  they always return `Float`. A `List<Int>.sum()` returning `Int` would
  be more natural, but requires either overload resolution or a trait
  bound. Not currently supported.
- **Should element-typed indexing be a first-class operation?** Currently
  `list[i]` returns `T` by value, which forces a copy for `T: Copy` and
  rejects the operation for non-`Copy` `T`. Borrowing through an index
  (`&list[i]`) exists but is a distinct construct.
- **Should there be a `List<Result<T, E>>` convenience?** Common pattern
  in other languages (`collect()`, `sequence()`, `try_collect()`). Not
  currently provided as a builtin.
- **Should bounds checks be statically eliminable?** A simple range
  analysis on the index expression would eliminate the check for
  `for i in 0..list.length` loops. Not implemented.
- **Should LLVM represent `List<T>` as a struct (ptr, len) or as a
  plain pointer with the length tracked in a side table?** The current
  codegen uses the side-table approach for `List.length`. The struct
  approach would be cleaner for Tier 7 (canonical IR) but is a
  larger change.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/option.md` — the sibling container type
- `docs/features/result.md` — the sibling container type
- `docs/decisions/0003-type-system.md`
- `docs/decisions/0005-ownership-model.md`
- `src/common/types.rs`
- `src/ir/semantic_ir.rs`
- `src/backends/interpreter/eval.rs`
- `tests/corpus/corpus_01_sum_list.gol`

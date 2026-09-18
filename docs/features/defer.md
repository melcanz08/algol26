# Feature: Defer (`defer ...`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is `defer` in ALGOL26, and where does it live?"

## Summary

`defer` schedules a statement to run when the enclosing scope exits.
The statement executes on:

- Normal fall-through at the end of the block.
- An explicit `return` from the enclosing function.
- A `break` or `continue` that exits the enclosing loop body.

Multiple `defer`s in the same scope run in **LIFO order** — the
last one declared runs first. This is the same discipline as Rust's
`Drop` and Go's `defer`, chosen so that paired setup/teardown
naturally nests:

```gol
defer close_a()
defer close_b()
// ... on exit, close_b runs first, then close_a
```

Defer is a **lowering feature**, not an IR feature. By the time
`SemanticProgram` is constructed, every `defer` has been rewritten
into explicit instructions placed before the corresponding exit
points. There is no `Instruction::Defer` variant.

## Syntax

Basic form:

```gol
procedure main
    defer print("cleanup")
    print("body")
```

Output: `body` then `cleanup`.

Multiple defers:

```gol
procedure main
    defer print("first")
    defer print("second")
    defer print("third")
```

Output: `third`, `second`, `first`. LIFO order.

Defer inside a nested scope:

```gol
procedure main
    if true then
        defer print("inner")
        print("in if")
    // "inner" runs here
    print("after")
```

The deferred statement belongs to the innermost enclosing block, not
to the function. See `test_defer_in_nested_scope` in
`tests/ir/defer_lowering_test.rs`.

Defer inside a loop:

```gol
procedure main
    var i := 0
    while i < 3 do
        defer print("iter")
        print(i)
        i := i + 1
```

The `defer` runs once per loop iteration, at the end of the body.
See `test_defer_with_loop`.

## Typing rules

A `defer` statement's expression must be a statement (not a bare
value). The type of the deferred expression is discarded — deferred
print statements return `Void`, deferred calls may return any type,
but the return value is ignored.

`defer` has no return type. A `defer` at statement position is
type-checked normally for its own body, then treated as a `Void`
statement for the enclosing block.

The parser rejects `defer` at expression position — see
`test_do_at_statement_position_is_rejected` in the parser tests,
which confirms that the counterpart `do` keyword is also
statement-only.

## Lowering

**This is the most important section of this contract.** Because
defer is lowered, its semantics are defined by the lowering pass,
not by any IR instruction.

The lowering happens in `src/ir/` — the file
`tests/ir/defer_lowering_test.rs` tests the result. Based on the
test names (`test_defer_preserves_order`, `test_defer_with_return`,
`test_defer_with_break`, `test_defer_with_loop`), the pass:

1. **Collects** all `defer` statements in a block, in source order.
2. **At every exit point** from the block, splices the deferred
   statements in reverse order (LIFO).
3. **Exit points** are: the end of the block (fall-through),
   every `return`, every `break`, every `continue`.

The result is a `SemanticBlock` with the deferred instructions
duplicated at each exit point. There is no special IR representation
for the defer itself.

**Consequences of the lowering approach:**

- A loop body with `defer` duplicates the deferred instructions
  once per exit — but a loop has only one exit and one body, so
  the duplication is not per-iteration, it is per-exit-point.
  The runtime cost per iteration is the cost of executing the
  deferred instruction, same as if it were written inline.
- A block with many exit points (e.g. an `if` inside an `if`
  inside a loop) duplicates the deferred instructions at each
  exit. Code size grows with exit count, not with defer count.
- The verifier sees only ordinary instructions. There is no
  `defer`-specific verifier rule.

**Implication for the capacity of the current design:** a
`defer` at the top of a large function with many early returns
produces as many copies of the deferred code as there are return
statements. For small deferred bodies this is fine; for large
deferred bodies it could be significant. This is a place where a
future optimizer pass (e.g. jump-to-epilogue) would help. Not
currently implemented.

## Ownership

### Defer captures

A deferred statement can capture variables from its enclosing
scope. The analyzer treats this capture like any other use of the
variable at the defer's source position — with one important
difference discussed below.

```gol
procedure main
    val x := 42
    defer print(x)
    // x is captured by the defer
```

The capture affects move analysis. `test_defer_capture_blocks_move_in_nested_scope`
in `src/semantics/analyzer/` confirms that a variable captured by a
`defer` cannot be moved while the defer is pending. This is
**stronger** than a normal borrow: the deferred statement might run
at any exit point, so the captured variable must remain available
until the defer fires.

**Example:**

```gol
procedure main
    val s := "hello"
    defer print(s)
    consume(s)   // compile error: s is captured by the pending defer
```

The capture is released when the defer fires (at scope exit). After
that point, the variable is available for further use, but the
scope is also ending, so this is rarely observable.

**Contrast with normal borrow:**

```gol
val s := "hello"
val r := &s
consume(s)   // also a compile error (s is borrowed)
```

Both cases prevent the move. The defer case is documented as a
capture because the analyzer's diagnostic refers to the defer, not
to a borrow.

### Move inside defer

```gol
procedure main
    val s := "hello"
    defer consume(s)
```

Here the `defer` itself will move `s` when it fires. This is legal —
the analyzer allows the move because it happens at the defer's
execution point, not before. The move is scheduled, not immediate.

After the defer is declared, `s` is not usable in the enclosing
scope, because the pending defer will eventually consume it. This
is a **deferred move**: the same as if the move were written at
the exit point.

See `test_uncaptured_move_in_nested_scope_accepted` for the case
where a variable is moved in a nested scope that has no pending
defer referencing it, and `test_borrow_in_defer_after_move` in
`tests/semantics/borrow_checker_extra_test.rs` for the case where a
borrow is created inside the defer.

### Interaction with return values

```gol
function f() -> Int
    defer print("cleanup")
    return 42
```

The `return 42` triggers the defer before returning. The defer
cannot modify the return value (there is no named return value
syntax that would allow this). The return value is computed first,
then the defer runs, then the function returns.

See `tests/conformance/valid/defer_return.gol`,
`defer_explicit_return.gol`, and `defer_fallthrough.gol` for the
three exit forms.

## IR representation

**None.** After lowering, defer is not visible in the IR.

The IR contains the *result* of the lowering: duplicated instructions
at each exit point, in the correct order. `TypedIRValue` has no
`Defer` variant; `Instruction` has no `Defer` variant; the CFG
builder has no `CfgInstruction::Defer`.

This is the same discipline as traits and generics: the feature is
resolved before IR construction, so every backend supports it for
free. `defer` is written in source, lowered in the frontend, and
the backends see only the ordinary instructions it produces.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | Supported | Runs the lowered IR — no defer-specific code |
| LLVM | Supported | Runs the lowered IR — no defer-specific code |
| WASM | Supported | Runs the lowered IR — no defer-specific code |

Because defer is lowered, no backend needs defer-specific logic.
The differential tests for defer (`test_differential_defer`,
`test_differential_defer_with_return` in
`tests/differential/differential_true.rs`) exercise the two
backends and confirm they agree.

## Diagnostics

Defer-related error codes currently emitted:

**None in the `E-XXX-NNN` format.** The prior report claimed
`E-DEFER-001` exists, but grep across the codebase shows no such
code. Errors related to defer are:

- Move-after-defer-capture errors, which use the general move
  codes `E-MOVE-001` / `E-MOVE-002`.
- Parser errors for `defer` in the wrong position, which use the
  generic syntax error code.

**This is the same diagnostic gap as traits and generics.** A
distinct `E-DEFER-001` would be useful for the specific case
"variable captured by pending defer cannot be moved". Currently
that error is reported as a move error, which is technically
correct but loses the connection to the defer.

## Safety

- No panic on nested defers. The LIFO order is well-defined.
- No leak on early return. The lowering splices the deferred
  statements at every return, so an early return still runs them.
- No interaction with region memory. A deferred statement that
  uses a region-local pointer is checked by the escape analysis
  before lowering — a defer that would use a freed pointer is a
  compile error, not a runtime use-after-free.

The capture analysis is the main safety property: a variable
captured by a pending defer cannot be moved out from under it.

## Test coverage

Current coverage across the tree:

**Lowering tests (`tests/ir/defer_lowering_test.rs`):**

- `test_defer_in_nested_scope` — defer belongs to innermost block
- `test_defer_preserves_order` — LIFO order
- `test_multiple_defers_in_same_scope` — same
- `test_defer_with_return` — explicit return triggers defer
- `test_defer_with_break` — break triggers defer
- `test_defer_with_loop` — loop body defer runs per iteration

**Analyzer tests (`src/semantics/analyzer/`):**

- `test_defer_capture_blocks_move_in_nested_scope` — capture prevents move
- `test_uncaptured_move_in_nested_scope_accepted` — no capture, no block

**Borrow tests (`tests/semantics/borrow_checker_extra_test.rs`):**

- `test_borrow_across_defer`
- `test_borrow_in_defer_after_move`

**Differential tests (`tests/differential/differential_true.rs`):**

- `test_differential_defer` — basic LIFO
- `test_differential_defer_with_return` — explicit return

**Conformance fixtures (`tests/conformance/valid/`):**

- `defer_return.gol`
- `defer_explicit_return.gol`
- `defer_fallthrough.gol`

**Corpus:**

- `corpus_06_defer_order.gol`
- `corpus_12_defer_in_loop.gol`

**Adversarial (`tests/adversarial/`):**

- `19_defer_captured_then_move.gol`
- `26_defer_return_value.gol`
- `35_nested_defer.gol`
- `36_defer_then_move_in_loop.gol`

### Gaps

- **No test for `continue` triggering a defer.** `break` and
  `return` are tested; `continue` is not. It should behave the
  same as `break` (defer runs before the loop continues), but this
  is not pinned.
- **No test for a defer inside a `for` loop** (only `while` is
  covered by `corpus_12_defer_in_loop.gol`).
- **No test for a deferred statement that itself contains a defer.**
  Nested defers (defer inside a block that is itself deferred) are
  not covered.
- **No test for defer inside a region.** Regions and defer both
  fire on scope exit; the interaction is not tested.
- **No test for a defer whose body panics.** The language has no
  panics, but a defer whose body triggers a runtime error (e.g.
  division by zero in the interpreter) is not covered.
- **No test for the exact error message when a captured variable
  is moved.** The analyzer rejects the program, but the diagnostic
  text is not asserted on.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
defer
    semantics:   Stable
    parsed:      yes
    typed:       yes
    validated:   yes (capture analysis)
    IR:          N/A (lowered before IR construction)
    verified:    N/A
    interpreter: supported
    LLVM:        supported
    WASM:        supported
    optimized:   no epilogue coalescing
```

Defer is the simplest feature in this directory: one keyword, one
lowering, no backend work. Its entire complexity is in the capture
analysis and the LIFO ordering.

## Checklist for related features

If you are adding a feature *like* defer (a compile-time
transformation of a statement into other statements), you need to
touch:

1. `src/frontend/lexer/` — new keyword if needed.
2. `src/frontend/parser/` — parse the new statement form.
3. `src/frontend/ast.rs` — new AST node (usually one that never
   reaches the analyzer's later phases — it is rewritten before
   IR construction).
4. `src/semantics/analyzer/` — capture analysis if the feature
   captures variables, and any type-checking of the rewritten form.
5. The lowering pass — usually a new file under `src/ir/` or a
   phase of the IR builder.
6. `tests/ir/<feature>_lowering_test.rs` — the lowering is the
   feature, so the tests live here.
7. `docs/features/<feature>.md` — this file.

Note: this is the **shortest checklist** of any feature contract
written so far. The absence of items 5–11 from `list.md`'s
checklist (IR variants, verifier rules, per-backend lowering)
reflects that lowered features do not touch the backends.

## Open questions

- **Should `E-DEFER-001` exist?** The move-after-capture error is
  currently reported as a move error. A distinct code would make
  it clear the move is blocked by a defer, not by a borrow. This
  is the same Tier 2 item flagged in traits and generics.

- **Should defers be coalesced?** If a function has twenty
  `return` statements and one `defer`, the deferred body is
  duplicated twenty times. A jump-to-epilogue pass would replace
  the duplication with a single deferred block and a branch. Not
  implemented; not currently a bottleneck.

- **Should `defer` be allowed at function top level?** Yes — this
  is the common case (`defer print("cleanup")` at the top of
  `main`). The current parser allows it. Worth confirming with a
  test if not already covered.

- **Should `defer` interact with `Result` error propagation?** If
  a function returns `Result<T, E>` and uses `try`, does the defer
  fire on error return? Presumably yes, because `try` desugars to
  a return of `Error(e)`, and the lowering splices defers at every
  return. Not tested.

- **What happens if a deferred statement has side effects that
  depend on a variable modified after the defer was declared?**
  ```gol
  var x := 0
  defer print(x)
  x := 42
  // prints 0 or 42?
  ```
  The answer depends on whether defer captures by value or by
  reference. Currently, since the defer is lowered by duplicating
  the deferred statement at exit points, and the exit runs after
  `x := 42`, it prints `42`. But this is worth a test.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/features/region.md` — another scope-exit mechanism (they
  fire independently, but worth understanding together)
- `src/frontend/parser/stmt.rs` — defer parsing
- `src/ir/` — the lowering pass
- `tests/ir/defer_lowering_test.rs` — the lowering tests
- `tests/soundness/` — no defer-specific fixtures yet
- `tests/corpus/corpus_06_defer_order.gol`

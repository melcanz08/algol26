# Feature: Spawn / Parallel (concurrency)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is `spawn`/`parallel` in ALGOL26, and where does it live?"

## Summary

ALGOL26 has two concurrency constructs:

- `spawn` — a single deferred task. Its body runs concurrently with
  the code that follows the spawn block.
- `parallel` — a group of two or more branches that run concurrently
  and join at the end of the block.

Both are **task-based**, not thread-based. A task is not an OS
thread; the language does not require a specific threading
implementation. What the language guarantees is:

1. **Independence.** Two tasks cannot share mutable state through a
   variable binding. State is either `val` (immutable, shareable)
   or moved into a task via channel.
2. **Deterministic results at join points.** When `parallel` joins,
   the values that survive the join are the ones the analyzer
   permitted.

The concurrency model is described in
`docs/decisions/0008-concurrency-model.md`. This contract documents
the current implementation.

## Syntax

Spawn:

```gol
procedure main
    print("before")
    spawn
        print("spawned")
    print("after")
```

Parallel:

```gol
procedure main
    print("start")
    parallel
        print("A")
        print("B")
    print("end")
```

Combined with channels (the common pattern):

```gol
procedure main
    val ch: Channel<Int> := channel
    spawn
        send ch, 42
    val got := receive ch
    print(got)
```

See `tests/corpus/corpus_26_spawn_channel.gol`.

## Typing rules

`spawn` and `parallel` are statements, not expressions. They have no
type. A block terminated by a `spawn` or `parallel` block is `Void`.

The **body** of a `spawn` or `parallel` block is type-checked
normally. A `return` inside a spawn body returns from the spawned
task, not from the enclosing function — the two return types are
independent. The analyzer currently requires the spawned block to be
`Void` (or to end without producing a value); a spawn block with a
trailing expression is a compile error.

## IR representation

In `src/ir/semantic_ir.rs`, two `Terminator` variants:

| Concept | Variant |
|---|---|
| `spawn` | `Terminator::Spawn { entry_block: usize }` |
| `parallel` | `Terminator::Fork { blocks: Vec<usize>, join_block: usize }` |

A `Spawn` terminator ends the current block and continues execution
at `entry_block`. The semantics are: **the current task forks a new
task that starts at `entry_block`, and then continues in the current
task immediately after the spawn**. There is no explicit join — a
spawn is a one-way hand-off.

A `Fork` terminator ends the current block and continues execution
at the first block in `blocks`. The remaining blocks run
concurrently, and all of them join at `join_block`. The semantics
are: **run every branch of `blocks` concurrently; when all have
finished, resume at `join_block`**.

The IR comment in the interpreter's mod.rs summarizes:

> Not supported: parallel execution. Spawn and Fork run sequentially;
> the interpreter does not create OS threads.

This is deliberate. The IR *represents* concurrency, but the
backends do not have to *implement* concurrency. An interpreter that
runs the branches in source order produces the same output as a
concurrent implementation for any program that respects the
language's independence rule.

### IR verifier

The verifier does **not** check spawn/fork capture semantics. The
module doc in `src/ir/verifier/mod.rs` states this explicitly:

> Still not verified: `Spawn`/`Fork` capture semantics (ownership
> transfer into a spawned block).

This is a known gap. It means the verifier trusts the analyzer to
have rejected any capture of a shared mutable variable. If the
analyzer misses a case, the IR verifier will not catch it.

## Analyzer

The analyzer does two things for spawn/parallel:

1. **Type check** the spawned block as a `Void` statement.
2. **Race detection** via `src/semantics/race/`.

### Race detection

The race detector walks the AST and, for each `spawn` or `parallel`
block, records which variables the parent task accesses and which
the spawned task accesses. If both access the same variable and at
least one access is a mutation (`var` read during a spawn, or any
write), a race is reported.

The rule is **conservative**: a `var` read in the parent during a
spawn is flagged, even if the spawn never writes to it. This is the
intent of `test_var_read_during_spawn_is_conservatively_flagged`.
The conservative rule means fewer valid programs are accepted, but
no race slips through.

Test cases in `src/semantics/race/tests.rs`:

- `test_no_race_with_empty_functions` — empty spawn is fine
- `test_read_only_access_no_race` — two reads of a `val` do not race
- `test_read_write_race_detected` — read + write of a `var`
- `test_write_write_race_detected` — two writes of a `var`
- `test_val_sharing_is_not_a_race` — sharing a `val` is safe
- `test_var_read_during_spawn_is_conservatively_flagged` — read of
  a `var` in the parent while a spawn runs is flagged
- `test_double_borrow_in_parallel` (in
  `tests/semantics/borrow_checker_extra_test.rs`) — a `&mut` and a
  `&` in two parallel branches is rejected

### Capture rules

The analyzer's capture rule for a spawned block is: the block may
**read `val` bindings** freely, and may **move `var` bindings and
non-`Copy` values** into itself. It may not:

- Read a `var` binding that the parent also reads (conservative flag).
- Borrow a place that the parent also borrows.
- Share a `&mut` borrow between two branches.

The safe pattern for cross-task communication is a channel: move the
channel into the spawn and send/receive through it.

## Backends

| Backend | Support | Evidence |
|---|---|---|
| Interpreter | **Sequential** | Runs spawn/fork branches in source order |
| LLVM | **Unsupported** | Capability check refuses `spawn` and `parallel` |
| WASM | **Unsupported** | Same, per capability matrix |

### Interpreter

The interpreter executes spawn and fork sequentially. The relevant
code is in `src/backends/interpreter/mod.rs`:

```rust
Some(Terminator::Spawn { entry_block }) => {
    current = *entry_block;
}

Some(Terminator::Fork { blocks, join_block }) => {
    if let Some((first, rest)) = blocks.split_first() {
        if !rest.is_empty() {
            pending_forks.push((rest.to_vec(), *join_block));
        }
        current = *first;
    } else {
        current = *join_block;
    }
}
```

For `Spawn`, the interpreter simply jumps to `entry_block` and runs
it. The spawned task executes **before** the code that follows the
spawn block, because the interpreter is single-threaded. This is
observably different from a real concurrent implementation if the
parent and spawn share mutable state — which the language forbids.
For programs that respect the independence rule, sequential
execution produces the same output as concurrent execution.

For `Fork`, the interpreter uses a **pending_forks queue**. The
first branch runs; when it reaches the join block, the queue is
consulted. If more branches remain, the next branch is run instead
of joining. When all branches have run, the join block executes.

The observable ordering is:

```
parallel
    print("A")
    print("B")
```

prints `A` then `B`. A concurrent implementation might print either
order; the interpreter always prints source order. The differential
test `test_parallel_interpreter_sequential` pins this exact
behavior:

```rust
assert_eq!(interp.trim(), "start\nA\nB\nend");
```

### LLVM

The LLVM backend refuses any program using `spawn` or `parallel`.
The capability check produces a compiler error naming the construct:

```
does not support: spawn
does not support: parallel
```

The refusal happens **before** lowering, so the LLVM codegen has no
spawn/fork handling. This is correct fail-closed behavior: a program
that spawns threads cannot be compiled to LLVM without an explicit
threading model, and the capability check refuses rather than
silently serializing.

Differential tests:

- `test_spawn_llvm_refused` — asserts the refusal message
- `test_parallel_llvm_refused` — asserts the refusal message

### WASM

Same as LLVM — refused via capability check. WASM has no native
threading in the default compilation target; the capability check
reflects this.

## Diagnostics

Concurrency-related error codes currently emitted:

**None in the `E-XXX-NNN` format.** Like traits, generics, and defer,
concurrency errors are reported as free-form strings through the
analyzer's `Result<(), String>` path.

Examples of concurrency error messages:

- "data race on variable 'x'" — from `src/semantics/race/`
- "mutually exclusive borrow in parallel branches" — from
  `borrow_checker_extra_test` (message text unverified, but the test
  asserts `result.is_err()`)

**This is the fourth feature (after traits, generics, defer) with
the same diagnostic gap.** Bringing all four into a coded system is
a single Tier 2 sweep.

## Safety

- **No data races at runtime** — the analyzer rejects any program
  where two tasks could access the same mutable state.
- **No torn reads** — the independence rule means no two tasks
  touch the same memory location simultaneously.
- **No deadlock detection** — a `receive` on a channel that never
  receives will block forever. The analyzer cannot prove a receiver
  will run, so deadlock is not rejected at compile time. This is a
  runtime hazard, not a compile-time error. No timeout mechanism
  exists.
- **No panics from concurrency** — the interpreter serializes, so
  no runtime concurrency primitive can fail. The LLVM/WASM refusal
  means no codegen path can panic on a concurrency construct.

## Test coverage

Current coverage across the tree:

**Analyzer tests (`src/semantics/race/tests.rs`):**

- `test_no_race_with_empty_functions`
- `test_read_only_access_no_race`
- `test_read_write_race_detected`
- `test_write_write_race_detected`
- `test_val_sharing_is_not_a_race`
- `test_var_read_during_spawn_is_conservatively_flagged`
- `test_merge_access_function`

**Borrow tests (`tests/semantics/borrow_checker_extra_test.rs`):**

- `test_double_borrow_in_parallel`

**Capability tests (`src/backends/capabilities/tests.rs`):**

- `llvm_rejects_spawn`

**Differential tests (`tests/differential/differential_true.rs`):**

- `test_spawn_llvm_refused`
- `test_spawn_interpreter_sequential`
- `test_parallel_llvm_refused`
- `test_parallel_interpreter_sequential`

**Corpus:**

- `corpus_26_spawn_channel.gol` — spawn + channel

**Adversarial (`tests/adversarial/`):**

- `20_spawn_mutable_alias.gol` — mutable alias through spawn
- `21_two_spawn_writers.gol` — two spawns writing the same var
- `22_safe_read_sharing.gol` — read-only sharing across spawn
- `40_spawn_reads_through_alias.gol` — reading through an alias

### Gaps

- **No test for a `parallel` block whose branches assign to
  different variables.** The analyzer should accept this (no race);
  no test confirms it.
- **No test for a `parallel` block whose branches share a `val`.**
  Should be accepted; not covered by a full-program test.
- **No test for a `spawn` inside a `parallel`.** Nested concurrency
  is not covered.
- **No test for the exact error message** when a race is detected.
  The analyzer rejects the program, but the diagnostic text is not
  asserted on.
- **No per-backend conformance fixture.** Since LLVM/WASM refuse,
  any such fixture would use `// BACKEND: interpreter` as its
  marker.
- **No test for `spawn` with a non-`Void` body.** The analyzer
  should reject this; not tested.
- **No test that a `return` inside a spawn returns from the spawn,
  not the enclosing function.** Semantically important, not tested.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
spawn / parallel
    semantics:   Stable (one open question: deadlock detection)
    parsed:      yes
    typed:       yes
    validated:   yes (race detector)
    IR:          yes (Terminator::Spawn, Terminator::Fork)
    verified:    partial (capture semantics not verified)
    interpreter: supported (sequential execution)
    LLVM:        unsupported (correct refusal)
    WASM:        unsupported (correct refusal)
    optimized:   no rules
```

Concurrency is unusual among the feature contracts: it is fully
supported on the analyzer and one backend, but refused on two.
The refusals are correct — the language does not commit to a
threading model, and neither LLVM nor WASM has one by default.

The design allows the interpreter to be a *serialized* model of
concurrency: it runs the branches in source order. This is sound
for any program that respects the language's independence rule,
because the independence rule guarantees the observation order does
not affect the result.

## Checklist for related features

If you are adding a feature *like* spawn/parallel (a scope that
runs concurrently with the parent, with a defined capture rule),
you need to touch:

1. `src/frontend/lexer/` — new keyword.
2. `src/frontend/parser/` — parse the new block syntax.
3. `src/frontend/ast.rs` — new AST node.
4. `src/ir/semantic_ir.rs` — new `Terminator` variant.
5. `src/ir/verifier/terminator.rs` — verification rules.
6. `src/semantics/race/` — capture analysis for the new block.
7. `src/semantics/analyzer/` — type-check the block, integrate
   with the race detector.
8. `src/backends/interpreter/mod.rs` — execution model (sequential
   or with a scheduler).
9. `src/backends/capabilities/scan.rs` — declare backend support.
10. `src/backends/capabilities/tests.rs` — accept/reject per backend.
11. `tests/semantics/` — analyzer-level tests.
12. `tests/differential/` — refusals and interpreter-sequential checks.
13. `tests/corpus/` — end-to-end program.
14. `docs/features/<feature>.md` — this file.

## Open questions

- **Should deadlock detection exist?** A `receive` on a channel that
  never receives blocks forever. The analyzer cannot prove this in
  general, but for simple cases (single-task program, no spawn,
  receive at the top level) it could reject. Not currently
  implemented.

- **Should there be a `select` construct?** A way to wait on
  multiple channels and proceed with whichever one is ready. Would
  be a new feature contract describing how it interacts with
  `parallel` and the ownership model.

- **Should spawn/fork capture semantics be verified in the IR?**
  Currently the IR verifier does not check this, and the module doc
  says so explicitly. Adding the check would make the verifier
  standalone — a program that is malformed at the IR level (with
  a shared mutable capture) would be rejected even if the analyzer
  had a bug. This is a Tier 2 item.

- **Should LLVM have a threading implementation?** The language
  commits to tasks, not threads. LLVM could lower `Spawn` to a
  `pthread_create` call in a runtime library, but this requires:
  (1) picking a threading model, (2) shipping a runtime library
  with the compiled binary, (3) defining memory ordering for
  channel operations. Not currently planned. If added, it would
  be a substantial extension of this feature.

- **Is the sequential interpreter truly equivalent to a concurrent
  one?** For programs that respect the independence rule, yes: no
  two tasks can observe each other's writes through shared
  variables, so the observation order is irrelevant. But channel
  ordering is *not* pinned by the language — a `receive` on a
  channel with multiple sends has undefined order in a concurrent
  implementation, and the interpreter picks source order. This is a
  place where the interpreter is *more* deterministic than the
  language guarantees. Worth documenting in
  `docs/decisions/0008-concurrency-model.md` if not already there.

- **What happens if a spawn body has a side effect and the parent
  also has a side effect?** The interpreter runs them in source
  order (spawn body first, then parent's continuation). The
  language's semantics say "concurrently" but do not pin an order.
  The interpreter chooses one. A conforming implementation could
  choose differently. This means the *observable* behavior of a
  spawn with side effects is not pinned by the language — a
  program that relies on the ordering is technically undefined.
  Worth documenting explicitly.

## See also

- `docs/architecture-direction.md` — the feature contract pattern itself
- `docs/decisions/0008-concurrency-model.md` — the design decision
- `docs/features/channel.md` — the cross-task communication mechanism
- `docs/features/borrow.md` — the borrow rules that apply across tasks
- `src/ir/semantic_ir.rs` — `Terminator::Spawn`, `Terminator::Fork`
- `src/ir/verifier/terminator.rs` — verifier rules
- `src/semantics/race/` — the race detector
- `src/backends/interpreter/mod.rs` — the sequential execution model
- `src/backends/capabilities/scan.rs` — LLVM/WASM refusals
- `tests/differential/differential_true.rs` — the sequential/refused tests
- `tests/corpus/corpus_26_spawn_channel.gol`

# ADR 0065: Spawn / Fork — Task Model and Backend Lowering

Status: Proposed (2026-11-12). Extends ADR 0008 and ADR 0011.

## Context

ADR 0008 established `spawn` and `parallel` as the language's
two concurrency constructs. ADR 0011 documented the current
`Fork` execution model in detail: the interpreter runs branch
bodies sequentially through the `pending_forks` worklist, and
the CFG verifier enforces a strict shape (`branches must end
in `Jump(join)`, no early returns, no nested forks`).

Current capability state:

| Feature | Interpreter | LLVM | WASM |
|---------|-------------|------|------|
| `spawn` | ✅ (synchronous inline) | ❌ | ❌ |
| `parallel` / `fork` | ✅ (sequential branches) | ❌ | ❌ |

The interpreter supports them by running them sequentially,
which is observationally equivalent to concurrent execution
for any program that respects the language's independence
rule. LLVM and WASM refuse both at the capability boundary.

This ADR makes three decisions:

1. **What `spawn` and `parallel` mean as language constructs**
   (the semantics the interpreter has always approximated).
2. **What the LLVM lowering is** (the first real concurrency).
3. **What the WASM lowering is** (and whether it happens at all).

It also **defers** any changes to the interpreter. The
synchronous model is the semantic reference. Real concurrency
is a *performance* feature, not a *correctness* feature, and
the interpreter is intentionally not optimized for throughput.

## Decision

### Language semantics

A `spawn do ... ` block spawns a **task**. The task runs
concurrently with the enclosing function, captures variables
per the race detector's rules, and returns no value.

A `parallel do A and B and C` block is equivalent to:

```
spawn_task(A)      // task 1
spawn_task(B)      // task 2
spawn_task(C)      // task 3
join_all           // wait for all three
```

`parallel` is sugar for concurrent-then-join. It is not a
task group (Go's `errgroup`, Kotlin's `coroutineScope`) — it
has no error propagation, no cancellation, no ordering
guarantees between branches. That is a future ADR.

The `Fork` terminator (currently branch-blocks-with-a-join)
is retained as the *internal* representation of `parallel`.
ADR 0011's verifier rules continue to enforce its shape. A
future ADR may canonicalize `Fork` into `SpawnTask` /
`JoinTask` (Option A in ADR 0011), but that is a code-shape
refactor, not a semantic change.

### Interpreter — unchanged

The interpreter keeps its synchronous `pending_forks` model.
Reasons:

- **It is the reference semantics.** Any program that runs on
  the interpreter and respects the independence rule produces
  the same observable behavior as a truly concurrent run.
- **It is fast enough.** Interpreters are for correctness and
  debugging, not throughput.
- **It is testable.** A concurrent interpreter is a source of
  flaky tests and race conditions in the compiler itself.

`spawn`'s synchronous inline execution (documented in ADR
0011's open question 5) is a known limitation. A `return`
inside `spawn` becomes the whole function's return value.
This is *not* the correct task semantics. It is deferred
with a documented gap: the interpreter's `spawn` is
"sync inline," not "task spawn." The LLVM lowering (below)
uses real task semantics, so a program that relies on
`return`-inside-`spawn` compiles differently on the two
backends. This ADR does not fix it — it names it, and the
fixture set should not exercise it.

### LLVM — real pthreads

LLVM gets real concurrency via `pthread_create`,
`pthread_join`, and a channel runtime built on `pthread_mutex`
plus `pthread_cond`. All under `runtime/thread.c`, linked via
`src/toolchain/linker.rs` with `-lpthread` added to the link
line.

`spawn` lowers to:

```llvm
define void @parent() {
  %closure = alloca <captured var types>
  ; store captured vars into closure
  %tid = call i32 @pthread_create(
      ptr %t_handle, ptr null, ptr @task_body, ptr %closure)
  ; spawner continues immediately
  ...
}

define void @task_body(ptr %closure) {
  ; load captured vars
  ; run body
  ; free closure
  ret void
}
```

`parallel` lowers to N × `pthread_create`, then N ×
`pthread_join` at the join block. The join is a barrier, not
a task-dependency graph. This matches `parallel`'s semantics
exactly.

**Soundness.** The analyzer's race detector rejects programs
where two concurrent contexts conflict on the same variable.
This is a compile-time guarantee. On LLVM, that guarantee
extends to runtime: no data races can occur in a program that
compiles. This is the load-bearing invariant of this ADR. If
the race detector is *unsound* (accepts a program that has a
data race), LLVM's output has undefined behavior. The race
detector's test suite is therefore part of the LLVM lowering
contract.

**Scope of the pthread runtime:**

| Construct | C runtime function |
|-----------|-------------------|
| `spawn` | `pthread_create` (no library function; codegen emits directly) |
| `parallel` | N× `pthread_create` + N× `pthread_join` |
| `channel<T>` | `algol26_channel_new(elem_size, capacity)` |
| `send ch, v` | `algol26_channel_send(ch, &v)` |
| `receive ch -> x` | `algol26_channel_recv(ch, &x)` |

The channel runtime is a bounded ring buffer with mutex +
condvar. Not lock-free — a first implementation that is
*correct* is more valuable than one that is fast.

### WASM — permanently refused

WASM has no threads. `spawn` and `parallel` cannot be lowered
to a WASM module that runs in a browser or in Node's default
`WebAssembly.instantiate`.

The *technically possible* path:

- Enable `SharedArrayBuffer` (requires COOP/COEP headers)
- Spawn WebWorkers
- Share the WASM module's memory across workers
- Implement channel state in shared memory with `Atomics.wait`

This is a substantial host-side lift. It requires a browser
or Node environment with specific security headers, and it
makes `host.js` order-of-magnitude more complex.

**Decision: WASM permanently refuses `spawn` / `parallel`.**

The capability matrix becomes the honest answer: these
features work on interpreter + LLVM, never on WASM. This
matches the precedent of `raw-memory` on WASM (permanent
refusal). It is documented in `docs/status/`.

A future ADR may revisit this if WASM threads (which are on
the standards track) become usable in the target runtime. For
now, the capability is closed.

## Ownership and safety

### Capture rules

A `spawn` or `parallel` body **captures** variables from the
enclosing scope. The rules:

1. **Immutable reads are allowed.** If `x` is `val` and `Copy`,
   the spawned body sees `x`.
2. **Moves are allowed.** If the body consumes `x` (passes it
   to a function by value, or assigns it away), the outer
   binding is marked moved after the spawn.
3. **Mutable access across a spawn is rejected** unless the
   variable is transferred exclusively. Two branches that
   both touch `var x` — even just reading — are rejected by
   the race detector.
4. **Channels are the escape hatch.** Capturing a channel for
   `send` / `receive` does not count as a conflicting access
   (ADR 0064 Session 2).

### Existing tests

The fixture set already exercises the negative cases:

- `tests/adr_examples/0047_concurrency/fail_two_spawn_writers.gol`
- `tests/adr_examples/0047_concurrency/fail_var_shared_with_spawn.gol`
- `tests/adr_examples/0047_concurrency/pass_val_shared_with_spawn.gol`

These should continue to pass unchanged. The LLVM codegen
does not need to enforce these rules — the analyzer does,
before the IR is built. Codegen sees only programs that
respect the independence rule.

## Implementation roadmap

Four sessions. Each ends green. Each is committed
independently. Sessions 1–2 land before any LLVM work.

### Session 1 — `pthread` link infrastructure

Scope:

- `runtime/thread.c`: empty file with a `#include <pthread.h>`
  and one dummy function `algol26_thread_smoke()` that
  creates and joins a thread.
- `src/toolchain/linker.rs`: add `runtime/thread.c` and
  `-lpthread` to the link line.
- One test: a trivial program that runs through LLVM and
  exits 0. The link must succeed with `-lpthread`.
- No new features enabled.

Effort: ~1 hour. Purely infrastructure. The hard part is
making the link work on the target system.

### Session 2 — Channel runtime in C

Scope:

- `runtime/channel.c`: `algol26_channel_new(elem_size,
  capacity)`, `algol26_channel_send`, `algol26_channel_recv`,
  `algol26_channel_free`.
- Mutex + condition variable ring buffer. Bounded; capacity
  passed by caller (defaults to a codegen-chosen constant).
- Unit tests in `runtime/channel_test.c` compiled to a test
  binary by `build.rs`.
- No compiler integration yet.

Effort: ~2-3 hours. This is real C. The correctness bar is
high — a channel implementation with a deadlock is worse than
no channel implementation.

### Session 3 — `spawn` lowering on LLVM

Scope:

- `src/backends/llvm_codegen/terminator.rs`: lower
  `Terminator::Spawn { entry_block }` to a `pthread_create`
  call. The closure is a stack `alloca` in the parent; the
  task body's entry block is wrapped in a function that
  loads from the closure.
- Capture analysis: which variables does the spawned body
  read? The IR builder already computes this for the race
  detector; reuse that information.
- Capability: `Feature::Spawn` joins `llvm()`.
- Tests: two positive fixtures (spawn-with-immutable-capture,
  spawn-with-move-capture). Existing negative fixtures
  continue to pass (analyzer rejects them before IR).
- Matrix, maturity, STATUS updates.

Effort: ~4-6 hours. This is the first real codegen for
concurrency. Expect bugs in the closure shape.

### Session 4 — `parallel` lowering on LLVM

Scope:

- `src/backends/llvm_codegen/terminator.rs`: lower
  `Terminator::Fork { blocks, join_block }` to N ×
  `pthread_create` at the fork point, and N × `pthread_join`
  at the join point.
- Capability: `Feature::Fork` joins `llvm()`.
- Tests: `parallel_straightline.gol` and its ADR-example
  counterparts.

Effort: ~3-4 hours. Smaller than Session 3 because the
closure shape is identical.

### Session 5 (optional) — Channel lowering on LLVM

Blocked on ADR 0064 Sessions 1–2.

Scope:

- `ChannelDecl` → `algol26_channel_new` call.
- `SendChannel` / `ReceiveChannel` → runtime calls.
- Capability: `Feature::Channels` joins `llvm()`.

Effort: ~3 hours. Follows the spawn lowering's template.

## Risks

**The biggest risk: the race detector is unsound.** If a
program compiles through the analyzer but has a data race
when run concurrently on LLVM, the compiler is unsound.
This would be a serious bug. Mitigations:

- The race detector's existing test suite is treated as a
  gating criterion for Session 3.
- Session 3's fixtures are all *positive* cases (programs
  the detector accepts). Negative cases remain
  interpreter-only.
- A follow-up session (not in this ADR) audits the detector
  against a set of known-racing programs and confirms every
  one is rejected.

**The second risk: `pthread` link portability.** The link
step uses `cc`. On some systems, `-lpthread` is needed; on
others, it's implicit. Session 1 must verify this.

**The third risk: `spawn`'s synchronous-inline semantics
(interpreter) vs real-task semantics (LLVM).** A program
that uses `return` inside a `spawn` body compiles differently
on the two backends. This ADR does not fix it. The
conformance fixture set must not contain such a program.
A future ADR can revisit `spawn`'s interpreter semantics.

## Alternatives considered

### Alternative 1 — Cooperative task scheduler

Run tasks on a single thread with `yield` points at channel
operations. No `pthread`. Portable to WASM.

Rejected because:

- It requires the language to have `yield` in the surface
  syntax (or an implicit yield at every function call,
  which changes performance characteristics).
- It doesn't give *real* parallelism. `spawn` becomes a
  coroutine spawn, which is a different feature.
- It is a bigger language change than pthreads.

A future ADR could add coroutines as a separate feature.

### Alternative 2 — Worker pool + futures

Spawn tasks onto a fixed thread pool. `join` waits on a
future. This is Rust's async model, Go's goroutine model
(simplified), Kotlin's coroutine model.

Rejected because:

- It requires a full futures / async runtime in the
  generated code. That is compiler architecture, not
  a codegen change.
- It requires `await` in the surface syntax for `receive`
  to be useful. That is a language change.
- The current language's `spawn` and `parallel` are simpler
  than futures, and pthreads is the simpler lowering.

### Alternative 3 — Refuse WASM only for `fork`, allow
`spawn` via single-task worker

In WASM, `spawn do ... ` becomes a coroutine run on the main
thread. `parallel do A and B` still refuses.

Rejected because:

- Half-implementing `spawn` on WASM is a maintenance trap.
  Code that assumes real concurrency will be wrong on WASM.
- The capability matrix should give one answer per feature
  per backend, not a per-construct answer that depends on
  what the construct does.
- The `spawn`-as-coroutine semantic is a *different feature*
  (see Alternative 1). Not a partial implementation.

## Open questions

1. **Task-count limit.** `parallel` with 100 branches spawns
   100 threads. Do we cap this? A first implementation
   doesn't, and documents the risk. A future ADR may add
   a worker pool.
2. **`join` on a task that panicked.** Task failure semantics
   are undefined. A first implementation aborts the process
   (matching the interpreter's panic behavior). A future ADR
   may add `Result`-returning joins.
3. **Cancellation.** No cancellation mechanism. `spawn` is
   fire-and-forget (or waitable at the implicit join), but
   there is no way to *abort* a running task. This is a
   substantial feature — out of scope.
4. **The interpreter's `spawn` inline semantics.** Documented
   in ADR 0011's open question 5. Not fixed by this ADR.
5. **`-lpthread` on macOS.** Should be implicit. Verify in
   Session 1.

## See also

- `docs/decisions/0008-concurrency-model.md` — parent ADR
- `docs/decisions/0011-phase4-task-model.md` — `Fork` shape
- `docs/decisions/0064-channels.md` — channel runtime
- `src/ir/semantic_ir/terminators.rs` — `Spawn`, `Fork`
- `src/backends/interpreter/mod.rs:251` — synchronous
  `pending_forks` model
- `src/ir/cfg_verifier.rs` — CFG shape rules from ADR 0011
- `src/semantics/race/analyze.rs` — race detector
- `tests/adr_examples/0047_concurrency/` — existing
  soundness fixtures
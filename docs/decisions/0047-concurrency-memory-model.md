# ADR 0047: Concurrency memory model

Status: Accepted (design)

## Context

ALGOL26 has concurrency syntax — `spawn`, `parallel`, `channel`,
`send`, `receive` — and a race detector that produces diagnostics.
It does not have a written memory model. That gap is now the
blocking one: without a defined model, any future implementation
of `spawn` on a compiled backend would be free to invent its own
ordering, and two backends would disagree about what a program
means.

Current state, for the record:

- **Interpreter.** `parallel` blocks run **sequentially** in source
  order. `spawn` bodies are executed eagerly. There is no real
  concurrency. Output from a `parallel` block is deterministic:
  the first block's statements, then the second's, etc.
- **LLVM.** `spawn` and `parallel` are refused by the capability
  check with a clear diagnostic. No threads.
- **WASM.** Same — refused by the capability check.
- **Channels.** Refused by every backend. The parser and analyzer
  accept the syntax; the corpus entries for channels are marked
  `KNOWN_FAILURE: channel runtime not implemented in any backend`.
- **Race detector.** A source-level, per-function, name-based
  approximation. It rejects some correct programs (conservative
  `var`-sharing policy) and misses some incorrect ones (no
  interprocedural tracking, no field-level disambiguation). See
  `src/semantics/race/`.

`docs/decisions/0008-concurrency-model.md` describes the intended
task model. It does not define the memory model: it says nothing
about happens-before, atomicity, or what constitutes a data race.

This ADR fills that gap. It does **not** implement concurrency;
it commits the semantics that future implementations must honor.

## Decision

Adopt the **data-race-free (DRF) synchronized memory model** —
the same model used by C++11 onward, Rust, Java 5 onward, and Go.

The model has three parts:

1. **A definition of happens-before.** A partial order over
   operations in a program.
2. **A definition of data race.** Two accesses to the same memory
   location, at least one of which is a write, **unordered by
   happens-before**.
3. **The DRF guarantee.** If a program has no data races (per the
   definition above), it behaves as if all its operations executed
   in *some* total order consistent with each thread's program
   order and with the happens-before relation — i.e., as if
   sequentially consistent.

Programs that have data races have undefined behavior. The
compiler may assume the absence of races when optimizing, exactly
as C++ and Rust do. This is the choice that makes the model
implementable on real hardware and optimizable.

## Happens-before rules

Happens-before (`→`) is the transitive closure of the following
edges. Within a single thread, program order is a happens-before
edge.

### `spawn`

    spawn
        <body>

- The parent's operations **before** the `spawn` statement `→` the
  first operation in the spawned body.
- The last operation in the spawned body `→` the parent's
  operations **after** the `spawn` statement.

In other words, `spawn` is a full synchronization point in both
directions: the child sees everything the parent did before it,
and the parent sees everything the child did after it. This makes
the common "spawn a task, wait for the result" pattern race-free
by construction.

`parallel` blocks are semantically equivalent to `spawn` per block
with an implicit join at the end:

    parallel
        A
        B
    C

means:

    spawn { A }
    spawn { B }
    <join both>
    C

The parent's operations before the `parallel` happen-before the
start of every block; every block's last operation happens-before
the parent's operations after the `parallel`. Blocks do not
happen-before each other — that is what "parallel" means.

### `send` / `receive` on channels

A `send(ch, v)` that is matched by a `receive(ch) into x` forms a
happens-before edge:

    send(ch, v)  →  receive(ch) into x

The receiver observes `v` in `x` with all writes that happened in
the sender's thread before the `send`. This is the standard
message-passing semantics — channels are the primary safe
primitive for cross-thread communication in ALGOL26.

If multiple `send`s and `receive`s can match, the happens-before
edges are the ones from the actual matching pairs at runtime.

### Transitivity

Happens-before is transitive. If A → B and B → C then A → C.

### No edge without a primitive

Two operations that are not related by a chain of the above edges
are **unordered** by happens-before. Unordered is not the same as
concurrent — the model does not say they ran at overlapping times,
only that the program provides no ordering guarantee between them.
If both access the same location and at least one is a write,
that is a data race.

## Atomicity

Not all reads and writes are made equal by the model.

**Atomic (indivisible) by default:**

- Reads and writes of `Int` (64-bit)
- Reads and writes of `Float` (64-bit)
- Reads and writes of `Bool`
- Reads and writes of `Ptr` and pointer-like scalars
- Reads and writes of enum ordinals (which lower to `Int`)

A read of one of these types observes either the value before a
concurrent write or the value after — never a torn value.

**Not atomic:**

- Composite values: records, `List<T>`, `Array<T, N>`, `Map<K, V>`,
  `Set<T>`, `String`, `Option<T>`, `Result<T, E>`.
- Field access `b.x` is a read of a scalar **if** `b.x` is scalar
  typed; the "composite not atomic" rule applies to the composite
  as a whole, not to its individual scalar fields.

Writing a record from one thread and reading it from another
without a happens-before edge is undefined behavior even if the
record is only eight bytes. The model does not specify word-sized
composite atomicity.

**No atomic instructions.** ALGOL26 v1 has no `atomic` type, no
compare-and-swap, no fence primitive. Synchronization is done via
`spawn`/`parallel` join and `channel`. A future ADR may add
atomics if a use case requires them.

## Data race definition

A **data race** occurs when:

1. Two accesses touch the same memory location, **and**
2. At least one of them is a write, **and**
3. The two accesses are not ordered by happens-before.

Both must be *concurrent* per the model, not per wall-clock
overlap. Two accesses that never actually run simultaneously at
the hardware level can still be a data race if the model provides
no ordering between them — the model is what the compiler assumes
when optimizing, not what the hardware happens to do.

Races are undefined behavior. A program with a race may produce
any result, including "correct on this machine today and wrong on
the next machine."

## The DRF guarantee

> If a program has no data races, it behaves as if all its
> operations executed in some total order that is consistent with
> each thread's program order and with happens-before.

This is the guarantee that makes concurrency tractable for
programmers: they reason about orderings at the synchronization
primitives, not about what the hardware does. It is what justifies
aggressive optimization (reordering, caching in registers, common
subexpression elimination across unsynchronized code).

The guarantee is stated but not proven here. Proofs for the
equivalent model in C++ and Rust appear in the literature; ALGOL26
adopts the same model and inherits the same argument. The
contribution of this ADR is committing to the model, not deriving
it.

## What this means for the current interpreter

The interpreter runs `parallel` blocks sequentially and `spawn`
bodies eagerly. That is a **valid implementation** of the memory
model for any program with no data races:

- Sequential execution produces *a* total order.
- It is consistent with every thread's program order (each thread
  runs in source order).
- It is consistent with every happens-before edge (every edge
  corresponds to a program-order relationship the interpreter
  respects — a spawn's body runs after the spawn statement and
  before the statement that follows it; a `parallel` block runs
  between the parent's pre- and post- statements).

So the interpreter is correct with respect to this ADR. No
behavior change is required, and no program currently accepted by
the interpreter needs to change.

The interpreter does not implement `channel`. Per this ADR, the
happens-before edges from `send`/`receive` are part of the model,
but until a backend implements them, no program can rely on them.
That is a **feature gap**, not a semantic conflict.

## What this means for LLVM and WASM

When `spawn` and `parallel` are implemented on a compiled backend,
they must honor the happens-before rules above. On native LLVM
this means real threads (pthreads, std::thread, or similar) with
appropriate memory fences at spawn and join boundaries. On WASM
this means threads, which the WebAssembly standard now supports
via shared memory and atomics — but the host environment must
opt in.

Implementing either is a separate project and requires:

- A runtime library providing thread creation, join, and channel
  buffers.
- Codegen changes: spawn emits a thread-start call; the spawn
  completion boundary emits a join.
- A decision about how `parallel` blocks are scheduled.
- Channel implementation (a queue with the happens-before
  semantics described above; usually a mutex-protected buffer or
  a lock-free queue with acquire/release semantics).

None of that is decided here. What is decided is what the
implementations must *mean* when they exist.

## What this means for `unsafe`

Operations inside `unsafe` blocks are **outside the memory-model
guarantee**. Sharing a raw pointer across threads and dereferencing
it without a happens-before edge is undefined behavior, and the
compiler provides no protection. This matches ADR 0015's framing:
`unsafe` is a promise by the programmer that they have verified
the local invariants manually.

The interaction to be explicit about:

- A `&T` or `&mut T` shared across a `spawn` boundary is
  permitted by the type system only if the borrow checker can
  prove the lifetime is compatible. This is a static check; it
  says nothing about whether concurrent access is race-free at
  the model level.
- The race detector is the source-level approximation of the
  data-race rule. It is conservative: it rejects some race-free
  programs and misses some races (interprocedural writes, field-
  level aliasing). See "Interaction with the race detector"
  below.

## Interaction with the race detector

The race detector (`src/semantics/race/`) approximates the
data-race rule at the source level. This ADR specifies the rule
it is approximating; it does not change the detector.

Known conservatism (false positives):

- **`var` bindings shared with a spawn are rejected outright**,
  even when the analyzed program only reads them. Rationale: the
  detector is per-function and does not follow writes through
  function calls, so it cannot prove no mutation reaches the
  binding. Workaround: use `val` when the binding is not
  reassigned.

Known incompleteness (false negatives):

- **Writes through function calls are invisible.** If `f()`
  mutates a global-ish binding and `f()` is called from a spawn,
  the detector does not see the mutation.
- **Field-level aliasing is not distinguished.** `b.x` and `b.y`
  are both recorded as accesses to `b`, so disjoint-field
  operations may be over-rejected rather than correctly accepted.
- **Reference aliasing is not tracked.** Two references to the
  same location are two different names to the detector.

Closing these is the same project as ADR 0044 (place-based
borrows). When place-based tracking lands, the detector can be
rewritten on top of it and the conservative `var`-sharing policy
can be removed. Until then, the detector is a sound-but-incomplete
approximation and its limitations are documented in its source.

## Consequences

**Positive.**

- Future implementations of `spawn`, `parallel`, and `channel`
  have a well-defined target.
- Programs written today that avoid sharing mutable state across
  a `spawn` are correct with respect to a standard model; they
  will behave the same on any conforming implementation.
- The current interpreter can be justified against the model
  rather than being a special case.
- Optimizations that assume the absence of data races are
  permitted, matching C++ and Rust.

**Negative.**

- Programs with data races are undefined behavior. This is a real
  sharp edge; C++ and Rust have the same edge and the same
  learning curve.
- No atomics in v1. Programs that need lock-free algorithms have
  to fall back to `unsafe`, which is outside the guarantee.
- Channel semantics are committed but not implemented. Any
  program relying on them today does not compile on any backend.

**Neutral.**

- The model is deliberately boring. It is the same model the
  other modern languages chose; the value is in committing to it,
  not in being novel.

## Alternatives considered

### A. Sequential-only semantics

Say that ALGOL26's `parallel` and `spawn` have **only** the
sequential interpretation and no multi-threaded semantics at all.
This is what the interpreter does today; this ADR would simply
freeze that as the final answer.

**Rejected** because the syntax advertises concurrency. Freezing
"parallel means sequential" makes the feature honest by making it
useless; it also forecloses implementing real concurrency later
without a semantic break.

### B. Total-store-order (TSO) model

Adopt a stronger model where reads and writes have a total order
across all threads, as x86 hardware effectively provides.

**Rejected** because it forbids the optimizations that modern
compilers rely on (store-load reordering, register caching across
unsynchronized code). Every conforming compiler would either
have to disable those optimizations or violate the model. The DRF
model gives programmers a simpler mental model than raw TSO while
allowing standard optimization.

### C. Opt-in parallelism only via channels

Remove `spawn` and `parallel` from the language; make `channel`
the only concurrency primitive. Channels provide implicit
synchronization, so no memory model beyond "channel operations
synchronize" is needed.

**Rejected** because `spawn` is already in the language and
because structured parallelism (`parallel` blocks) is the more
common shape for the "do these N things, then continue" pattern.
Channel-only forces users into explicit thread-and-queue plumbing
for what should be a block form.

### D. Sequential consistency for all operations

Every operation is globally ordered. Simplest mental model, no
fences needed at the language level.

**Rejected** because it forbids reordering entirely, which makes
the compiler unable to optimize standard scalar code even when
no concurrency is present. This is the model Rust, C++, and Java
all moved away from for the same reason.

## Open questions

- **Do `parallel` blocks share a memory pool or own separate
  ones?** The model is agnostic; it only requires that happens-
  before edges are respected. The implementation decides.
- **What is the join behavior of `parallel` when a block panics
  or diverges?** `Never`-typed blocks need a semantics; the
  current language has no panic mechanism, so this is deferred.
- **Does `spawn` return a handle?** The current syntax does not
  — the spawn point synchronizes with the parent's next
  statement. A handle-based `spawn` (async-style) would change
  the happens-before rules and is a separate proposal.
- **Is `Channel<T>` a type or a value?** The parser treats it as
  a declaration (`channel c Int`), matching the corpus fixtures.
  The type-system interaction is unresolved.
- **Atomics.** If a use case for lock-free algorithms arises,
  the model can be extended with an `atomic` type modifier and
  acquire/release/relaxed orderings, in the same way C++ did.

## See also

- `docs/decisions/0008-concurrency-model.md` — the task model this
  ADR supplements
- `docs/decisions/0011-phase4-task-model.md` — task model details
- `docs/decisions/0015-unsafe-enforcement.md` — what `unsafe` does
  and does not guarantee
- `docs/decisions/0044-place-based-borrows.md` — the analysis that
  will subsume the race detector's name-based checks
- `src/semantics/race/` — the current race detector
- C++11 memory model (Boehm & Adve, 2008) — the model this ADR
  adopts
- Rust's memory model documentation — same model, different
  surface syntax

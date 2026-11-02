# ADR 0020 — Channels

## Status

Accepted. Not implemented on any backend. Backfilled 2026-11-02.

## Context

ALGOL26 has `spawn` / `parallel` (`ADR 0008`) for concurrency.
Channels — `channel`, `send`, `receive` — were planned as the
communication primitive between spawned tasks.

The parser and analyzer accept the syntax. No backend executes it.
STATUS.md referenced "ADR 0020" for the decision, but the ADR was
never written.

## Decision

**Three channel forms** are part of the language *syntax*:

    var c := channel<Int>()
    send c, 42
    var x := receive c

**Capability matrix:**

- **Interpreter:** refused via `Feature::Channels`. The
  interpreter's channel instruction arms
  (`Instruction::ChannelDecl`, `SendChannel`, `ReceiveChannel`)
  are implemented but return `EvalError::Unsupported` as a
  defensive guard if reached; the capability check fires first.
- **LLVM:** refused via `Feature::Channels`.
- **WASM:** refused via `Feature::Channels`.

**Rationale for refusing on all backends:** channels require a
runtime scheduler and synchronization primitives (mutexes,
condvars, or lock-free queues) that would each need their own
ADR. Implementing channels properly means deciding:

1. **Blocking vs. non-blocking.** Should `receive` block until a
   value is available? Should there be a `try_receive`?
2. **Buffered vs. unbuffered.** Should `channel<Int>()` be a
   rendezvous (rendezvous semantics, like Go's unbuffered
   channels) or buffered with a size parameter
   (`channel<Int>(10)`)?
3. **Close semantics.** Should channels be closeable? What does
   a `receive` on a closed channel return?
4. **Runtime shape.** Each backend needs its own channel
   implementation. The LLVM/WASM paths would need libc-level
   thread or async primitives; the interpreter would need its
   own cooperative scheduler.

Each of those is an ADR of its own. Refusing cleanly is better
than a half-implemented channel that deadlocks unpredictably.

## Consequences

### Positive

- The syntax exists, so programs can be written against it
  today. Porting them to a backend that implements channels later
  requires no source change.
- The capability matrix is honest: all three backends refuse
  cleanly.
- `spawn` / `parallel` (ADR 0008) works independently of
  channels, so concurrent-with-no-communication programs run on
  every backend today.

### Negative

- Programs that use channels can only be parsed, not run.
  Users need to know that `--interpreter run` fails on channel
  programs.
- The channel instruction arms exist in the interpreter without
  a real lowering. That's untested code that will rot unless the
  feature is implemented or the arms are deleted.

### Deferred

- The four design questions above (blocking, buffering, close,
  runtime) each require their own ADR.
- Which backend implements channels first (interpreter is the
  natural choice — cooperative scheduler; LLVM and WASM follow).

## References

- `ADR 0008` — concurrency model
- `ADR 0047` — concurrency memory model
- `docs/features/channel.md` — syntax and current status
- `docs/features/spawn.md` — the spawn/parallel primitive that
  channels will communicate between

## Backfill note

Backfilled 2026-11-02 during the feature-inventory
reconciliation. The feature is documented and parsed but
deliberately unimplemented; the ADR was never written to record
the *why*. This records it now.

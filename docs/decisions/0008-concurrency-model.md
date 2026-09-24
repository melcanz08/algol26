# D008: Concurrency Model

> **Status note (2026-09-16)**: Data-race detection is
> implemented (see `src/semantics/race/`), though it is
> deliberately conservative and per-function only. `spawn`
> and `parallel` execute through the interpreter and are
> refused by the LLVM backend. Channels parse and analyze
> but have no backend runtime. The syntax shown below
> predates the current parser: `spawn` takes an indented
> body (no `do`/`end`).
> **Status note (2026-09-24)**: Capture-mode modeling was
> considered during the IR builder's design. A `CaptureMode`
> enum (`Read` / `Write` / `Move`) was added to the builder's
> `VariableInfo` type, intended to be consumed by a future
> escape analysis. No consumer was ever written, and the
> `escape.rs` module that would have provided one was deleted
> as part of the Convergence Map cleanup. The field has been
> removed — a dormant field that looks like it might be
> consumed is worse than no field, because the next reader
> assumes capture handling exists.
>
> ALGOL26 does not currently commit to capture-mode semantics.
> The race detector (`src/semantics/race/`) is the concurrency-
> safety boundary; it reasons conservatively about which
> variables each branch reads or writes and rejects programs
> where two branches touch the same variable in a conflicting
> way. If capture semantics becomes a committed language
> feature, it should start with an ADR describing what captures
> mean (shared borrow? move? copy?) before any field is added
> back.

## Status
🟨 Partial (syntax implemented, safety not enforced)

## Decision
Message passing with channels as primary concurrency mechanism.

## Model
```gol
// Spawn concurrent block
spawn do
    // concurrent execution
```

```gol
// Channel communication
channel ch
send ch, value
receive ch
```

## Safety Rules
1. No shared mutable state
2. Ownership transfer via channels
3. Immutable data can be shared
4. Compile-time race detection

## Future
- Data race prevention
- Deadlock detection
- Structured concurrency
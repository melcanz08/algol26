# D008: Concurrency Model

> **Status note (2026-09-16)**: Data-race detection is
> implemented (see `src/semantics/race/`), though it is
> deliberately conservative and per-function only. `spawn`
> and `parallel` execute through the interpreter and are
> refused by the LLVM backend. Channels parse and analyze
> but have no backend runtime. The syntax shown below
> predates the current parser: `spawn` takes an indented
> body (no `do`/`end`).

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
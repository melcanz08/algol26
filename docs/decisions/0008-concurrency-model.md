# D008: Concurrency Model

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
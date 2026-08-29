# D005: Ownership Model

## Status
🟨 Partial (basic ownership, move semantics)

## Decision
Three ownership states:
- Owned (default)
- Borrowed (future)
- Moved

## Safety Guarantees
- No use-after-move
- No double free
- Deterministic cleanup
- Compile-time verification

## Examples
```gol
var x := 42.0  // Owned
var y := x     // Move - x invalid
// x cannot be used here
```
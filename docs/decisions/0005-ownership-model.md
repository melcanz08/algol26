# D005: Ownership Model

> **Status note (2026-09-16)**: Borrowing is implemented,
> not "future." The syntax in the example matches the
> current parser. Known limitation: `&mut x` passed as a
> call argument is not registered as a borrow (see
> `IMPLEMENTATION_STATUS.md`).

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
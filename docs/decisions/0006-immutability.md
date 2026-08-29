# D006: Immutability

## Status
✅ Implemented

## Decision
- `val` for immutable variables
- `var` for mutable variables
- Immutability is default philosophy

## Rationale
1. Prevents accidental mutation
2. Easier reasoning
3. Thread safety
4. Functional programming influence

## Examples
```gol
val pi := 3.14159  // Immutable
var counter := 0   // Mutable
```
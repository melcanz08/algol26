# D004: Memory Model

## Status
🟨 Partial (ownership tracking, move semantics implemented)

## Context
Need a deterministic memory model without garbage collection.

## Decision
ALGOL26 uses ownership-based memory management with:
- Single owner per value
- Scope-based cleanup
- Move semantics
- Borrowing (future)

## Rationale
1. Deterministic performance
2. No GC pauses
3. Compile-time safety
4. C/C++ level control

## Examples
```gol
// Ownership
var buffer := allocate(1024)
// buffer owns memory

// Move
var new_owner := move(buffer)
// buffer invalid after move

// Scope cleanup
region r do
    var temp := allocate(100)
    // temp freed when region exits
end region
```
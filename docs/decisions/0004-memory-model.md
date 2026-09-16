# D004: Memory Model

> **Status note (2026-09-16)**: Borrowing is implemented,
> not "future." The syntax shown below predates the current
> parser: `region r` takes an indented body (no `do`/`end
> region`), `alloc(n)` / `free(p)` are the memory builtins,
> and move happens implicitly on assignment (there is no
> `move(...)` call). See `IMPLEMENTATION_STATUS.md` for the
> current memory model and known limitations.

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
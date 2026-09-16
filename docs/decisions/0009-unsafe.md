# D009: Unsafe Boundary

> **Status note (2026-09-16)**: `unsafe` blocks and FFI are
> both implemented. `extern "C"` supports `as "symbol"`
> renaming and `from "library"` linking (tag
> `step4b-done`). The syntax shown below predates the current
> parser: `unsafe` takes an indented body (no `do`/`end
> unsafe`).

## Status
🔲 Planned

## Decision
Explicit unsafe blocks for low-level operations.

## Concept
```gol
unsafe do
    // Raw pointers
    // Manual memory
    // FFI calls
    // Hardware access
end unsafe
```

## Principles
1. Explicit opt-in
2. Localized and auditable
3. Documented in code
4. Compiler can identify unsafe regions

## Safety Boundary
```
SAFE ALGOL26          UNSAFE ALGOL26
    │                     │
    │                     │
ownership             raw pointers
bounds                pointer arithmetic
regions               manual memory
channels              FFI
type safety           hardware access
```
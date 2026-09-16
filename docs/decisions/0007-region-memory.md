# D007: Region Memory

> **Status note (2026-09-16)**: Regions are implemented
> end-to-end. `RegionExit` auto-frees region-scoped
> allocations on both the interpreter and LLVM backends
> (tags `step3-done`, `step6-done`). The syntax shown below
> predates the current parser: use `region r` + indented
> body, and `alloc(n)` (not `allocate(n)`).

## Status
🔲 Planned (module created, not integrated)

## Decision
Region-based memory management for grouped allocations.

## Concept
```gol
region r do
    var temp1 := allocate(100)
    var temp2 := allocate(200)
    // both freed when region exits
end region
```

## Rationale
1. Bulk deallocation
2. Deterministic cleanup
3. Better cache locality
4. No GC pauses

## Trade-offs
- Requires region lifetime tracking
- May over-allocate
- Less flexible than manual memory
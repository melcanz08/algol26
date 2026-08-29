# The ALGOL26 Contract

## What ALGOL26 Guarantees

If a program successfully passes semantic analysis, the compiler guarantees:

### Type Safety
- Values have valid types
- No type mismatches at runtime
- No implicit unsafe conversions

### Memory Safety
- Immutable bindings (`val`) cannot be mutated
- Moved values cannot be reused
- Array accesses are bounds-checked
- No use-after-move

### Explicit Absence
- No null as a general-purpose value
- Optional values use `Option<T>`
- `Some(value)` and `None` are explicit

### Explicit Errors
- Recoverable failure uses `Result<T, E>`
- `Ok(value)` and `Error(value)` are explicit
- Callers cannot ignore failure silently

### Ownership
- Single owner per value
- Move semantics for ownership transfer
- Copy semantics for simple assignments

## What ALGOL26 Does NOT Yet Guarantee

### Not Yet Proven
- Race freedom (foundation only, not enforced)
- No undefined behavior (requires formal proof)
- Deadlock prevention
- Full borrow checking

### Currently In Progress
- Complete pattern matching exhaustiveness
- Full function return value extraction (math works, user-defined pending)
- Region-based memory management

## Safety Layers

ALGOL26 provides safety through multiple compiler-enforced layers:

1. **Static type checking** - Compile-time type validation
2. **Explicit optional values** - `Option<T>` for absence
3. **Explicit error values** - `Result<T, E>` for failures
4. **Immutable bindings** - `val` prevents mutation
5. **Ownership/move checking** - Use-after-move detection
6. **Bounds checking** - Array access safety
7. **Escape analysis** - Reference scope tracking (foundation)
8. **Region-based management** - Grouped allocation (foundation)
9. **Concurrency safety** - Race detection (foundation)

## The Unsafe Boundary

Unsafe operations are explicitly separated from safe code:

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

## Trust Model

The ALGOL26 contract is:

> If your program compiles, the stated safety guarantees hold.

This is the same trust model as Rust:

> Safe Rust programs are memory safe.

The contract applies to **safe** ALGOL26 code. Explicit `unsafe` blocks are outside the contract.

## Verification Status

| Guarantee | Status | Evidence |
|-----------|--------|----------|
| Type safety | 🟢 Proven | conformance tests |
| Immutability | 🟢 Proven | mutation_of_val.gol rejected |
| Bounds (literal) | 🟢 Proven | out_of_bounds.gol rejected |
| Bounds (dynamic) | 🟢 Proven | runtime check |
| Use-after-move | 🟢 Proven | use_after_move.gol rejected |
| Option safety | 🟡 Partial | type system exists, pattern matching pending |
| Result safety | 🟡 Partial | type system exists, pattern matching pending |
| Race freedom | 🔴 Not proven | foundation only |
| No UB | 🔴 Not proven | requires formal proof |
# ALGOL26 Safety Roadmap

## Phase 1: Current (v0.1)

### Implemented
- ✅ Basic type checking
- ✅ Variable scope checking
- ✅ Function signature checking
- ✅ Indentation-based blocks
- ✅ Arithmetic type validation
- ✅ Duplicate variable detection
- ✅ Undefined variable detection
- ✅ Undefined function detection

### In Progress
- 🟨 String variable support
- 🟨 Boolean operations
- 🟨 More comparison operators

## Phase 2: Type System Enhancement (v0.2)

### Planned
- [ ] Algebraic data types
- [ ] Option type (no null)
- [ ] Result type for errors
- [ ] Pattern matching
- [ ] Generics
- [ ] Type aliases
- [ ] Type casting (explicit)

### Examples

```gol
// Option type
type Option<T> =
    Some(T)
    None

// Result type
type Result<T, E> =
    Ok(T)
    Error(E)

// Pattern matching
match value
    Some(x) -> process(x)
    None -> handle_missing()
```

## Phase 3: Memory Safety (v0.3)

### Planned
- [ ] Ownership model
- [ ] Borrow checking
- [ ] Bounds checking
- [ ] Deterministic cleanup
- [ ] Defer statements
- [ ] Move semantics
- [ ] Reference types

### Examples

```gol
// Ownership
var buffer := allocate(1024)
// buffer owns the memory

// Transfer ownership
var new_owner := move(buffer)
// buffer is no longer valid

// Automatic cleanup
defer free(buffer)
// freed when scope exits
```

## Phase 4: Advanced Safety (v0.4)

### Planned
- [ ] Escape analysis
- [ ] Region-based memory
- [ ] Lifetime inference
- [ ] Immutability by default
- [ ] Effect system
- [ ] Capability-based security

### Examples

```gol
// Immutability
val x := 42  // immutable
var y := 42  // mutable

// Region-based memory
region r do
    var temp := allocate(100)
    // temp freed when region exits
end region
```

## Phase 5: Concurrency Safety (v0.5)

### Planned
- [ ] Message passing
- [ ] Structured concurrency
- [ ] Data race prevention
- [ ] Deadlock detection
- [ ] Async/await
- [ ] Channels

### Examples

```gol
// Message passing
channel ch
spawn do
    send(ch, 42)
end spawn

var value := receive(ch)

// Structured concurrency
parallel do
    task1()
    task2()
end parallel
```

## Phase 6: Unsafe Escape Hatch (v0.6)

### Planned
- [ ] Unsafe blocks
- [ ] Raw pointers
- [ ] Manual memory management
- [ ] FFI to C libraries
- [ ] Inline assembly

### Examples

```gol
// Unsafe block
unsafe do
    var ptr := raw_allocate(100)
    // dangerous operations
    raw_free(ptr)
end unsafe

// FFI
extern "C" function malloc(size: int) -> pointer
```

## Safety Guarantees Matrix

| Guarantee | Phase | Status |
|-----------|-------|--------|
| Type safety | 1 | ✅ |
| Scope checking | 1 | ✅ |
| Function checking | 1 | ✅ |
| Pattern exhaustiveness | 2 | 🔲 |
| No null (Option) | 2 | 🔲 |
| Ownership | 3 | 🔲 |
| Borrow checking | 3 | 🔲 |
| Bounds checking | 3 | 🔲 |
| Deterministic cleanup | 3 | 🔲 |
| Escape analysis | 4 | 🔲 |
| Region memory | 4 | 🔲 |
| Data race prevention | 5 | 🔲 |
| Deadlock detection | 5 | 🔲 |
| Unsafe blocks | 6 | 🔲 |

## Milestones

### M1: Basic Safety (Current)
- Type checking works
- Scope checking works
- Basic error messages

### M2: Type Safety (Next)
- ADTs implemented
- Option/Result types
- Pattern matching

### M3: Memory Safety
- Ownership model
- Borrow checking
- Bounds checking

### M4: Advanced Safety
- Escape analysis
- Region memory
- Immutability

### M5: Concurrency Safety
- Message passing
- Data race prevention

### M6: Full Safety
- Unsafe blocks
- FFI support
- Complete safety model

## Design Principles

1. **Safety by default** - Safe operations are the norm
2. **Explicit unsafety** - Dangerous operations require opt-in
3. **Compile-time when possible** - Catch errors early
4. **Runtime when necessary** - Bounds checking fallback
5. **No undefined behavior** - All operations defined
6. **Deterministic** - No garbage collection pauses
7. **Zero-cost abstractions** - Safety without performance loss
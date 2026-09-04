# ALGOL26 Design Principles

## Language Layers

### Layer 1: ALGOL Heritage (Preserved)
- Procedures as primary abstraction
- Structured programming constructs
- Mathematical expression syntax
- Block-oriented thinking
- Algorithmic focus
- Clear control flow

### Layer 2: ALGOL26 Core (Modernized)
- Significant indentation instead of begin/end
- Static type checking with inference
- Memory safety without garbage collection overhead
- First-class error handling
- Module system for code organization
- Algebraic data types
- Pattern matching

### Layer 3: ALGOL26 Extensions (Future)
- Generic programming
- Structured concurrency
- Message passing
- Regions for memory management
- Escape analysis

## Safety Model

### Memory Safety Levels

#### Level 1: Safe (Default)
```gol
var x := 42          // Automatic stack allocation
var list := [1,2,3]  // Bounds checked
var s := "hello"     // Immutable string
```

#### Level 2: Controlled (Explicit)
```gol
var buffer := allocate(1024)  // Explicit heap allocation
defer free(buffer)             // Deterministic cleanup
```

#### Level 3: Raw (Unsafe Block)
```gol
unsafe do
    // Raw pointer operations
    // Explicitly dangerous
end unsafe
```

### Ownership Rules

1. **Single owner** - Each value has one owner
2. **Scope-based cleanup** - Values freed when scope exits
3. **Transfer ownership** - Explicit move semantics
4. **Borrowing** - Temporary references with lifetime checking

### Type System Goals

1. **Static typing** - Errors caught at compile time
2. **Type inference** - Reduce boilerplate
3. **No null** - Option<T> for optional values
4. **Algebraic types** - Sum and product types
5. **Generics** - Parametric polymorphism

## Design Decisions

### D001: Significant Indentation
**Historical**: ALGOL 58 used `begin`/`end` blocks
**Problem**: Excessive syntax noise, visual clutter
**Solution**: Significant indentation (Python/ISWIM style)
**Trade-off**: Requires editor support for indentation
**Status**: ✅ Implemented

### D002: Type System
**Historical**: ALGOL 58 had minimal typing
**Problem**: Runtime type errors, lack of safety
**Solution**: Static typing with inference
**Trade-off**: Some flexibility lost
**Status**: 🟨 Partial (basic types, inference)

### D003: Memory Safety
**Historical**: Manual memory management
**Problem**: Buffer overflows, use-after-free, leaks
**Solution**: Managed memory with ownership concepts
**Trade-off**: Some performance overhead
**Status**: 🔲 Planned
- Ownership model
- Borrow checking
- Bounds checking
- Deterministic cleanup

### D004: Error Handling
**Historical**: Error codes, no standard mechanism
**Problem**: Inconsistent error handling
**Solution**: Result<T, E> pattern, no exceptions
**Trade-off**: More verbose for happy path
**Status**: 🔲 Planned
- Result<T, E> type
- No exceptions
- Pattern matching for errors

### D005: Algebraic Data Types
**Historical**: Not in ALGOL 58
**Problem**: Representing complex data structures
**Solution**: Sum types with pattern matching
**Trade-off**: Learning curve for new concepts
**Status**: 🔲 Planned

```gol
type Option<T> =
    Some(T)
    None

type Result<T, E> =
    Ok(T)
    Error(E)
```

### D006: Pattern Matching
**Historical**: Not in ALGOL 58
**Problem**: Complex conditional logic
**Solution**: Exhaustive pattern matching
**Trade-off**: More syntax to learn
**Status**: 🔲 Planned

```gol
match value
    Some(x) -> process(x)
    None -> handle_missing()
```

## Feature Evaluation Framework

For every proposed feature, apply this test:

1. **Historical Connection**: Does it have ALGOL heritage?
2. **Problem Solving**: What specific problem does it solve?
3. **Simplicity**: Does it make the language simpler?
4. **Coherence**: Does it still feel like ALGOL26?
5. **Safety**: Does it maintain or improve safety?

Score 1-3 for each criterion. Accept if total ≥ 12.

## Compiler Architecture

```
Source (.gol)
    ↓
[Lexer] → Tokens
    ↓
[Parser] → AST
    ↓
[Semantic Analyzer]
    ├── Type checking
    ├── Name resolution
    ├── Ownership analysis (future)
    ├── Borrow checking (future)
    ├── Bounds analysis (future)
    └── Exhaustiveness (future)
    ↓
[ALGOL26 IR] (future)
    ↓
[Optimization]
    ↓
[LLVM Backend]
    ↓
Native Machine Code
```

## Error Message Philosophy

Errors should be:

1. **Specific** - Exact location and problem
2. **Actionable** - Suggest how to fix
3. **Educational** - Explain why it's wrong
4. **Consistent** - Same format everywhere

Example:

```
error[E0421]: undefined variable 'unknown_value'
  --> examples/test.gol:4:14
   |
 4 |     total := unknown_value + 10.0
   |              ^^^^^^^^^^^^^
   |
   = note: variable 'unknown_value' was not declared
   = help: declare it with 'var unknown_value := ...'
```

## Performance Philosophy

1. **Zero-cost abstractions** - High-level features compile to efficient code
2. **No hidden allocations** - Memory usage is explicit
3. **Deterministic cleanup** - No GC pauses
4. **Optimization friendly** - LLVM optimizations

## Safety Guarantees

### Compile-Time
- Type safety
- Ownership rules
- Borrow checking
- Pattern exhaustiveness

### Runtime (When Compile-Time Can't Prove)
- Bounds checking
- Integer overflow checking (optional)
- Null safety (via Option)

### Never
- Use-after-free
- Data races
- Buffer overflows
- Undefined behavior

## Syntax Examples

### Variables and Types
```gol
var x := 42          // Integer inference
var y := 3.14        // Float inference
var name := "ALGOL"  // String inference
var z: float := 5.0  // Explicit type
```

### Control Flow
```gol
if temperature > 30.0 then
    Terminal.print("Hot")
elif temperature > 20.0 then
    Terminal.print("Warm")
else
    Terminal.print("Cool")
```

### Loops
```gol
for item in collection do
    process(item)

while condition do
    update()
```

### Procedures
```gol
procedure calculate_stats(data)
    var total := 0.0
    for val in data do
        total := total + val
    return total
```

## Standard Library Modules

### Terminal
- `Terminal.print(value)` - Print a value
- `Terminal.input()` - Read user input

### Math
- `Math.sqrt(x)` - Square root
- `Math.pow(x, y)` - Power
- `Math.sin(x)` - Sine
- `Math.cos(x)` - Cosine

### List
- `List.length(list)` - Get length
- `List.append(list, item)` - Add item
- `List.sort(list)` - Sort list

## Project Identity

ALGOL26 is not:
- 'Another Python clone'
- 'Rust with different syntax'
- 'A historical reenactment'

ALGOL26 is:
- ALGOL's ideas, modernized
- A language for clear algorithmic expression
- A bridge between historical wisdom and modern safety

## Summary

ALGOL26: the clarity and algorithmic spirit of ALGOL 58, the readability of indentation-based languages, the safety lessons of Rust, the control and performance of C/C++, and the compiler technology of 2026.
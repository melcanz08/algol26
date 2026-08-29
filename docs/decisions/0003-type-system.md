# D003: Type System

## Status
🟨 Partial (basic types implemented, full checking in progress)

## Context
ALGOL 58 had minimal typing. Modern languages have shown that static typing with inference can prevent many runtime errors while keeping code concise.

## Decision
ALGOL26 uses static typing with type inference.

## Rationale

1. Prevents runtime type errors
2. Type inference reduces boilerplate
3. Better IDE support
4. Enables compiler optimizations
5. Safer refactoring

## Type System Goals

### Current
- Basic types: int, float, string, bool
- Type inference from literals
- Type checking in semantic analyzer
- No implicit conversions

### Planned
- Algebraic data types
- Option type (no null)
- Result type for errors
- Generics
- Pattern matching

## Examples

### Type Inference
```gol
var x := 42          // int
var y := 3.14        // float
var z := "text"      // string
var b := true        // bool
```

### Explicit Types
```gol
var x: int := 42
var y: float := 3.14
var z: string := "text"
```

### Type Errors (Caught at Compile Time)
```gol
var x := 42
// x := "hello"  // Error: Type mismatch
```

## Trade-offs

### Advantages
- Early error detection
- Better documentation
- Optimization opportunities
- Refactoring safety

### Disadvantages
- More verbose (mitigated by inference)
- Learning curve for type concepts
- Some flexibility lost

## Implementation Notes

- Semantic analyzer performs type checking
- Types are inferred from literals
- Type annotations are optional
- No implicit conversions
- Arithmetic requires matching types

## Related Decisions
- D001: Significant Indentation
- D002: File Extension
- D004: Memory Safety (planned)

## References
- Rust type system
- Haskell type inference
- ALGOL 58 type model
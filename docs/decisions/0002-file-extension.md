# D002: File Extension .gol

## Status
✅ Accepted

## Context
Need a file extension for ALGOL26 source files.

## Decision
Use `.gol` extension.

## Rationale

1. Short and memorable
2. Distinctive to ALGOL26
3. Easy to type
4. No conflicts with existing languages
5. Gives ALGOL26 its own identity

## Alternatives Considered

| Extension | Reason Rejected |
|-----------|-----------------|
| `.alg` | Too generic, conflicts with ALGOL |
| `.agl` | Used by other projects |
| `.a26` | Less intuitive |
| `.algo` | Too long |

## Examples

```
hello.gol
fibonacci.gol
statistics.gol
data_processing.gol
```

## Implementation Notes

- Compiler accepts `.gol` files
- LLVM IR output uses `.ll` extension
- Binary output uses no extension (or platform-specific)
- Syntax highlighting for `.gol` in editors

## Related Decisions
- D001: Significant Indentation
- D003: Type System

## References
- Rust uses `.rs`
- Go uses `.go`
- Python uses `.py`
- ALGOL 58 used `.alg`
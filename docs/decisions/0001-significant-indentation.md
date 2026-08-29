# D001: Significant Indentation

## Status
✅ Accepted

## Context
ALGOL 58 used `begin`/`end` block delimiters. Modern languages like Python have shown that significant indentation can be more readable and less noisy.

## Decision
ALGOL26 uses significant indentation instead of `begin`/`end`.

## Rationale

1. Reduces syntax noise
2. Forces consistent formatting
3. Python/ISWIM/Haskell influence
4. Better visual structure
5. Eliminates need for `end` delimiters

## Trade-offs

### Advantages
- Less visual clutter
- Cleaner code structure
- Easier to read nested blocks
- Forces proper code formatting
- Modern language feel

### Disadvantages
- Requires editor support for indentation
- Mixing tabs/spaces can cause issues
- Less explicit about block boundaries
- Copy/paste can break indentation
- Some developers prefer explicit delimiters

## Examples

### Traditional ALGOL
```algol
if x > 5 then
    begin
        print(x)
    end
```

### ALGOL26
```gol
if x > 5 then
    Terminal.print(x)
```

## Implementation Notes

- Lexer tracks indentation levels
- Indent token: increase in indentation
- Dedent token: decrease in indentation
- Tabs should be converted to spaces
- Recommended: 4 spaces per indent level

## Related Decisions
- D002: File Extension (.gol)
- D003: Type System
- D004: Memory Safety

## References
- Python PEP 8
- Haskell layout rule
- ISWIM (Peter Landin)
- ALGOL 58 Report
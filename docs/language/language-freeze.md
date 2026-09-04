# ALGOL26 v0.5 Language Freeze

**Declared**: 2026-09-03
**Effective**: Immediately

## The Language Is FROZEN

The following are **FROZEN** and will NOT change:

### Syntax
- All keywords (function, procedure, var, val, if, else, for, while, etc.)
- All operators (+, -, *, /, <, >, <=, >=, ==, !=, and, or)
- Assignment operator (`:=`)
- Significant indentation
- Type annotation syntax (`x: Type`)

### Types
- Int, Float, String, Bool, Void
- List<T>
- Option<T>, Result<T, E>
- Ptr (raw pointer)
- TypeVar (generics)
- Generic types

### Ownership/Borrowing
- Copy semantics (Int, Float, Bool)
- Move semantics (String, List)
- Borrow rules (immutable, mutable)
- Scope-based lifetime

### Memory
- Region-based memory model
- Unsafe blocks
- FFI boundaries

### Traits/Generics
- `trait` declarations
- `impl` blocks
- Method resolution
- Generic type parameters

### Modules
- `import` statements
- Module loading

### Error Handling
- Option types
- Result types
- Try/catch/finally

### Control Flow
- If/else expressions
- For loops (over lists)
- While loops
- Break/continue
- Return statements

## What IS Allowed to Change

| Category | Allowed? |
|----------|----------|
| Bug fixes | ✅ Yes |
| Compiler performance | ✅ Yes |
| Diagnostics quality | ✅ Yes |
| Architecture refactoring | ✅ Yes |
| Test improvements | ✅ Yes |
| Documentation | ✅ Yes |
| Backend correctness | ✅ Yes |
| Internal IR improvements | ✅ Yes (with IR version bump) |

## What is NOT Allowed

| Category | Allowed? |
|----------|----------|
| New syntax | ❌ No |
| New operators | ❌ No |
| New type features | ❌ No |
| New ownership rules | ❌ No |
| New semantic behavior | ❌ No |
| New keywords | ❌ No |

## Exception Process

If an actual **correctness defect** is found in Language 0.5:
1. Document the defect
2. Propose a fix
3. Bump language version to 0.6
4. Update all documentation
5. Update all examples
6. Update all tests

This is NOT for convenience — only for genuine bugs.

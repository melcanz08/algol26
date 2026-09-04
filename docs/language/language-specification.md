# ALGOL26 Language Specification v0.1.0

## 1. Introduction

ALGOL26 is a historically inspired reconstruction and hypothetical continuation of ALGOL 58, preserving ALGOL's emphasis on clear algorithms, mathematical notation, structured control flow, and procedural abstraction while replacing explicit block delimiters with significant indentation and incorporating lessons learned from nearly seven decades of programming-language research.

It aims to combine high-level clarity with systems-level control: memory safety, ownership, deterministic resource management, concurrency safety, and static typing are enforced by default, while an explicit unsafe boundary permits low-level programming when necessary.

## 2. Lexical Structure

### 2.1 File Extension
ALGOL26 source files use the `.gol` extension.

### 2.2 Comments
```gol
// Single line comment
-- Also a comment
```

### 2.3 Identifiers
- Start with letter or underscore
- Can contain letters, digits, underscores
- Case sensitive

### 2.4 Keywords
```
procedure    function    return
var          val         if
else         for         while
in           do          true
false        and         or
not          spawn       parallel
channel      send        receive
unsafe       defer
```

### 2.5 Operators
```
:=    Assignment
+     Addition
-     Subtraction
*     Multiplication
/     Division
>     Greater than
<     Less than
>=    Greater or equal
<=    Less or equal
==    Equal
!=    Not equal
and   Logical AND
or    Logical OR
not   Logical NOT
```

## 3. Type System

### 3.1 Basic Types
| Type | Description | Default |
|------|-------------|---------|
| Int | 64-bit integer | 0 |
| Float | 64-bit float | 0.0 |
| String | String literal | "" |
| Bool | Boolean | false |
| List | Compile-time list | [] |

### 3.2 Type Inference
```gol
var x := 42          // Int
var y := 3.14        // Float
var z := "text"      // String
var b := true        // Bool
var l := [1, 2, 3]   // List
```

### 3.3 Mutability
```gol
val immutable := 42  // Cannot be reassigned
var mutable := 42    // Can be reassigned
```

## 4. Ownership Model

### 4.1 Ownership States
- **Owned**: Variable owns its value
- **Borrowed**: Temporary reference (future)
- **Moved**: Ownership transferred

### 4.2 Move Semantics
```gol
var x := 42.0
var y := x    // Moves ownership from x to y
// x is now invalid
```

### 4.3 Scoping Rules
- Variables are scoped to their indentation block
- Variables are freed when scope exits
- Deferred statements execute in reverse order

## 5. Control Flow

### 5.1 If Statement
```gol
if condition then
    // body
elif other_condition then
    // body
else
    // body
```

### 5.2 For Loop
```gol
for item in collection do
    // body
```

### 5.3 While Loop
```gol
while condition do
    // body
```

## 6. Memory Safety

### 6.1 Bounds Checking
- Compile-time checking for literal indices
- Runtime checking for dynamic indices
- Error: "Array index out of bounds"

### 6.2 Regions (Future)
```gol
region r do
    // allocations freed when region exits
end region
```

## 7. Concurrency

### 7.1 Spawn
```gol
spawn do
    // concurrent block
```

### 7.2 Channels
```gol
channel ch
send ch, value
receive ch
```

### 7.3 Safety Rules
- No shared mutable state
- Ownership transfer via channels
- Immutable data can be shared

## 8. Unsafe Boundary

```gol
unsafe do
    // Explicitly outside safety guarantees
    // Localized and auditable
end unsafe
```

## 9. Standard Library

### Terminal
- `print value`

### Math (planned)
- `Math.sqrt(x)`
- `Math.pow(x, y)`

## 10. Compiler Pipeline

```
Source (.gol)
    ↓
Lexer
    ↓
Parser
    ↓
AST
    ↓
Semantic Analysis
    ↓
ALGOL26 IR (planned)
    ↓
LLVM IR
    ↓
Native Code
```
# ALGOL26 Formal Language Specification

**Version**: 2.5.0 (Frozen)

## 1. Introduction

ALGOL26 is a statically-typed, compiled programming language inspired by ALGOL 58, ISWIM, and Python (indentation-based blocks).

## 2. Lexical Grammar

### 2.1 Tokens

| Token Type | Description |
|------------|-------------|
| Identifier | `[a-zA-Z_][a-zA-Z0-9_]*` |
| IntLit | `[0-9]+` |
| FloatLit | `[0-9]+\.[0-9]+` |
| StringLit | `"..."` |
| Keyword | `procedure`, `function`, `return`, `var`, `val`, `if`, `else`, `for`, `while`, `in`, `do`, `true`, `false`, `and`, `or`, `not`, `import`, `try`, `catch`, `finally`, `break`, `continue`, `defer`, `spawn`, `parallel` |
| Operator | `:=`, `=`, `+`, `-`, `*`, `/`, `>`, `<`, `>=`, `<=`, `==`, `!=`, `(`, `)`, `[`, `]`, `,`, `:` |

### 2.2 Indentation

- Blocks are delimited by indentation
- Spaces (not tabs) define nesting
- Consistent indentation required

## 3. Types

### 3.1 Primitive Types

| Type | Description | Default |
|------|-------------|---------|
| `Int` | 64-bit signed integer | `0` |
| `Float` | 64-bit floating point | `0.0` |
| `String` | UTF-8 string | `""` |
| `Bool` | Boolean | `false` |

### 3.2 Composite Types

| Type | Syntax |
|------|--------|
| List | `[elem1, elem2, ...]` |
| Option | `Some(value)` / `None` |
| Result | `Ok(value)` / `Error(value)` |

### 3.3 Type Promotion

| Operation | Result |
|-----------|--------|
| `Int + Int` | `Int` |
| `Int + Float` | `Float` |
| `Float + Int` | `Float` |
| `Float + Float` | `Float` |
| `String + String` | `String` |

## 4. Declarations

### 4.1 Variables

```gol
val x := 10.0    // Immutable
var y := 20.0    // Mutable
```

### 4.2 Functions

```gol
function name(param1: type1, param2: type2) -> return_type
    // body
    return value

procedure main
    // entry point
```

### 4.3 Imports

```gol
import "filename.gol"
```

## 5. Statements

| Statement | Syntax |
|-----------|--------|
| Assignment | `x := value` |
| Declaration | `val x := value` / `var x := value` |
| Return | `return value` |
| Print | `Terminal.print(value)` |
| If | `if cond then ... else ...` |
| For | `for item in list do ...` |
| While | `while cond do ...` |
| Break | `break` |
| Continue | `continue` |
| Defer | `defer statement` |
| Spawn | `spawn ...` |
| Try/Catch/Finally | `try ... catch err ... finally ...` |

## 6. Expressions

### 6.1 Operators (by precedence)

| Precedence | Operators |
|------------|-----------|
| 1 (highest) | `()`, `[]`, function calls |
| 2 | `*`, `/` |
| 3 | `+`, `-` |
| 4 | `>`, `<`, `>=`, `<=` |
| 5 | `==`, `!=` |
| 6 | `and` |
| 7 (lowest) | `or` |

## 7. Standard Library (22 functions)

### 7.1 Math (10 functions)

| Function | Signature |
|----------|-----------|
| sqrt | `Math.sqrt(x: Float) -> Float` |
| pow | `Math.pow(x: Float, y: Float) -> Float` |
| sin | `Math.sin(x: Float) -> Float` |
| cos | `Math.cos(x: Float) -> Float` |
| abs | `Math.abs(x: Float) -> Float` |
| floor | `Math.floor(x: Float) -> Float` |
| ceil | `Math.ceil(x: Float) -> Float` |
| exp | `Math.exp(x: Float) -> Float` |
| log | `Math.log(x: Float) -> Float` |
| tan | `Math.tan(x: Float) -> Float` |

### 7.2 String (5 functions)

| Function | Signature |
|----------|-----------|
| length | `String.length(s: String) -> Int` |
| concat | `String.concat(s1: String, s2: String) -> String` |
| substring | `String.substring(s: String, start: Int, len: Int) -> String` |
| to_upper | `String.to_upper(s: String) -> String` |
| to_lower | `String.to_lower(s: String) -> String` |

### 7.3 File (3 functions)

| Function | Signature |
|----------|-----------|
| read | `File.read(path: String) -> String` |
| write | `File.write(path: String, content: String) -> Int` |
| append | `File.append(path: String, content: String) -> Int` |

### 7.4 List (4 functions)

| Function | Signature |
|----------|-----------|
| length | `List.length(arr: List) -> Int` |
| sum | `List.sum(arr: List) -> Float` |
| max | `List.max(arr: List) -> Float` |
| min | `List.min(arr: List) -> Float` |

## 8. Safety Guarantees

| Guarantee | Status |
|-----------|--------|
| Type safety | Compile-time |
| Immutability enforcement | Compile-time |
| Move semantics | Compile-time |
| Bounds checking (literal) | Compile-time |
| Bounds checking (dynamic) | Runtime |
| Race detection (write-write) | Compile-time |

## 9. Backend Architecture

| Backend | Output | Status |
|---------|--------|--------|
| LLVM | Native executable | Stable |
| Interpreter | Direct execution | Stable |
| WASM | `.wasm` module | Working |

## 10. Language Freeze

This specification defines ALGOL26 v2.5.0.
The language is frozen as of this version.
Future changes require a new version number.
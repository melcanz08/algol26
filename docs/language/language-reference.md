# ALGOL26 Language Reference

## Overview

ALGOL26 is a programming language inspired by ALGOL 58 with modern features from ISWIM and Python (indentation-based blocks).

## Version: v0.1.0

## Syntax

### Comments

```gol
// Single line comment
-- Alternative single line comment
```

### Variables

```gol
val x := 10.0    // Immutable variable
var y := 20.0    // Mutable variable
```

### Data Types

| Type | Example | Description |
|------|---------|-------------|
| Int | `5` | 64-bit integer |
| Float | `5.0` | 64-bit floating point |
| String | `"Hello"` | String literal |
| Bool | `true` / `false` | Boolean |
| List | `[1.0, 2.0, 3.0]` | Array of values |

### Type Promotion

- `Int + Int = Int`
- `Int + Float = Float` (automatic promotion)
- `Float + Float = Float`
- `String + String = String` (concatenation)

### Control Flow

```gol
if condition then
    // then block
else
    // else block

for item in list do
    // loop body

while condition do
    // loop body

match value
    Some(v) -> // handle Some
    None -> // handle None
```

### Functions

```gol
function add(x: float, y: float) -> float
    return x + y

procedure main
    val result := add(5.0, 3.0)
    Terminal.print(result)
```

### Error Handling

```gol
try
    // risky code
catch err
    // handle error
finally
    // always executes
```

### Concurrency

```gol
spawn
    // concurrent block

parallel do
    // parallel block 1
    // parallel block 2
```

### Modules

```gol
import "utils.gol"
import "math/advanced.gol"
```

## Standard Library (22 functions)

### Math Module (10)

| Function | Description |
|----------|-------------|
| `Math.sqrt(x)` | Square root |
| `Math.pow(x, y)` | Power |
| `Math.sin(x)` | Sine |
| `Math.cos(x)` | Cosine |
| `Math.abs(x)` | Absolute value |
| `Math.floor(x)` | Floor |
| `Math.ceil(x)` | Ceiling |
| `Math.exp(x)` | Exponential |
| `Math.log(x)` | Natural logarithm |
| `Math.tan(x)` | Tangent |

### String Module (5)

| Function | Description |
|----------|-------------|
| `String.length(s)` | String length |
| `String.concat(s1, s2)` | Concatenate |
| `String.substring(s, start, len)` | Substring |
| `String.to_upper(s)` | Uppercase |
| `String.to_lower(s)` | Lowercase |

### File Module (3)

| Function | Description |
|----------|-------------|
| `File.read(path)` | Read file |
| `File.write(path, content)` | Write file |
| `File.append(path, content)` | Append to file |

### List Module (4)

| Function | Description |
|----------|-------------|
| `List.length(arr)` | Array length |
| `List.sum(arr)` | Sum of elements |
| `List.max(arr)` | Maximum element |
| `List.min(arr)` | Minimum element |

## CLI Commands

| Command | Description |
|---------|-------------|
| `algol26 check file.gol` | Type-check only |
| `algol26 build file.gol` | Compile to executable |
| `algol26 run file.gol` | Compile and run |
| `algol26 wasm file.gol` | Compile to WASM |
| `algol26 --version` | Show version |
| `algol26 --help` | Show help |

## Architecture

```
Source (.gol)
  -> Lexer -> Parser -> AST
  -> Semantic Analysis
  -> Semantic IR
  -> Backend (LLVM / Interpreter / WASM)
```

## Safety Guarantees

| Guarantee | Status |
|-----------|--------|
| Type safety | Proven |
| Immutability | Proven |
| Bounds checking | Proven |
| Use-after-move | Proven |
| Race detection | Write-write conflicts |
# Union Types

**Status:** Current. Implemented on the interpreter; LLVM and WASM
refuse union programs at the capability boundary (`Feature::Unions`).

**ADR:** [ADR 0055](../decisions/0055-sum-and-recursive-types.md)

## Overview

A union is a named type with a fixed set of variants, each
optionally carrying a payload. It is ALGOL26's sum type:

```gol
union Json
    Null
    Bool(Bool)
    Num(Int)
    Str(String)
    Arr(List<Json>)
    Obj(Map<String, Json>)
```

## Construction

```gol
val n := Json.Num(42)
val s := Json.Str("hello")
val null := Json.Null
```

The variant name follows a dot, matching enum variant syntax.
Payload arity is checked at compile time.

## Pattern matching

```gol
match j
    case Null then "null"
    case Num(n) then Int.to_string(n)
    case Str(s) then s
    case Arr(xs) then "array"
    case Obj(m) then "object"
```

Every variant must be covered, or a `case _` fallback must be
present. Guards are supported:

```gol
match n
    case Num(x) if x < 0 then "negative"
    case Num(x) then "non-negative"
```

## Recursive unions

A union variant may refer to the union itself through an
indirection (`List`, `Map`, `Option`, `Borrow`, `Pointer`).
Direct self-reference is rejected as infinitely sized.

## Cross-file use

```gol
# json.gol
pub union Json
    Null
    Num(Int)

# main.gol
import "json.gol"

proc main
    val n := Json.Num(42)
```

The union and its variants must be `pub` to be used across files.

## Backend support

| Backend | Support |
|---|---|
| Interpreter | Full |
| LLVM | Refused (`E0002`) |
| WASM | Refused (`E0002`) |

See [ADR 0055](../decisions/0055-sum-and-recursive-types.md) for the
full design and the deferred LLVM/WASM codegen.

## See also

- [Record types](record.md)
- [Enum types](enum_types.md)
- [ADR 0055](../decisions/0055-sum-and-recursive-types.md)
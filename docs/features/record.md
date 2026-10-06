# `rec` — Structured Data Types

**Status:** Interpreter complete. LLVM and WASM refuse programs
that use records; use `--interpreter` to run them.

**ADRs:** `docs/decisions/0024-record.md` (records),
`docs/decisions/0026-structural-copy.md` (structural `Copy`)

## What it is

A `rec` declaration names a bundle of fields. Values of that type
are constructed with a `Name { field: value, ... }` literal and
read with `.field`. Records are the language's way to model
"a thing with parts" — a point, a person, a config — instead of
passing parallel lists or long argument lists.

```
rec Point
    x: Int
    y: Int
```

## Syntax

### Declaration

```
rec Name
    field1: Type1
    field2: Type2
```

### Construction

```
val p := Point { x: 1, y: 2 }
```

Field order in the literal does not have to match declaration
order. Every field must be provided exactly once.

### Field read

```
print(p.x)         // 1
```

### Field write

Requires the record binding to be `var`:

```
var p := Point { x: 1, y: 2 }
p.x := 99
print(p.x)         // 99
```

Assigning to a field of a `val` binding is an error:

```
val p := Point { x: 1, y: 2 }
p.x := 99          // E0007: cannot assign to field of immutable variable
```

### Field names

Field names must not be reserved words. `rec Segment` with
fields `start` and `end` will fail to parse because `end` is a
keyword. Rename to avoid the collision (`tail`, `finish`, `stop`).
A future enhancement could allow contextual or escaped keywords,
but v1 requires distinct names.

### Pattern matching

```
match p
    case Point { x, y }
        print(x + y)
    case _
        print("not a point")
```

Bindings name fields by name — `Point { x, y }` binds `x` to
the field named `x` and `y` to the field named `y`. The order in
the pattern does not have to match the declaration order.

### Methods

Records participate in the existing trait system. Method calls
use the `x.method()` form; the receiver is inserted by the
frontend, so the `impl` body does not declare `self` explicitly.

```
impl Show for Point
    function show() -> String
        return "(" + Int.to_string(self.x) + ", " + Int.to_string(self.y) + ")"

proc main
    val p := Point { x: 1, y: 2 }
    print(p.show())    // (1, 2)
```

`self` is available inside the method body, bound to the receiver.
`impl` methods are renamed to `<Record>_<method>` during the
frontend normalization step (`expand_impl_methods`), and the IR
builder resolves `p.show()` to that mangled name.

### Records in collections

```
val pts := [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]
print(pts[1].x)        // 3
```

### Chained method calls on field accesses

`p.x.method()` does not parse — the parser rejects method calls
on complex receiver expressions. Bind the field to a local first:

```
val tmp := p.x
tmp.method()
```

This is a language-level limitation, not a record-specific one;
it applies to any field access followed by a method call.

### Records inside regions

A record declared inside a `region` block belongs to that region.
Since records do not allocate heap memory on their own, this only
matters for records that contain heap-typed fields:

```
region r
    val p := Point { x: 1, y: 2 }
    print(p.x)
```

## Semantics

### Value semantics with structural `Copy`

A record is a value, not a reference. Passing a record to a
function and mutating the parameter does not affect the caller's
copy.

A record is `Copy` iff every field is `Copy`. The rule is
structural and recursive:

- `Point { x: Int, y: Int }` is `Copy`. `val q := p` copies and
  `p` remains usable.
- `Person { name: String, age: Int }` is not `Copy`, because
  `String` is not. `val q := p` moves `p`; any later use of `p`
  is rejected with the existing `E-MOVE-001` diagnostic.
- `Line { start: Point, end_point: Point }` is `Copy` iff `Point`
  is.
- A record whose field type resolves to `Unknown` is
  conservatively treated as move-only.

**Mutability is orthogonal to `Copy`.** A `Copy` record bound
with `val` is still immutable: `val p := Point { x: 1, y: 2 };
p.x := 5` produces the existing `E0007` immutability diagnostic.
`Copy` affects move semantics, not assignment-through-the-binding
semantics.

**Borrowing is unchanged.** `&p` and `&mut p` work on records of
any `Copy`-ness. `Copy` decides whether a value is duplicated on
move, not whether it can be borrowed.

### No `Drop`

A record has no destructor. A record containing a heap-allocated
value (`String`, `List`, another non-`Copy` record) leaks under
the LLVM backend, exactly as a bare `String` does today. The
interpreter does not leak because its runtime values are
Rust-owned.

### Field access requires a record receiver

`p.x` where `p` is not a record is a compile error. `p` must be
a `Type::Record` (or `Unknown`, if the analyzer could not infer
it — the field then has type `Unknown`).

## Backend support

| Backend     | Support                                                |
|-------------|--------------------------------------------------------|
| Interpreter | Yes                                                    |
| LLVM        | Refused — pass `--interpreter` to run                  |
| WASM        | Refused — pass `--interpreter` to run                  |

Running a record program through LLVM produces:

```
error[E0002]: The LLVM backend does not support: records ...
Run through the interpreter instead:
    algol26 run --interpreter <file.gol>
```

The refusal is enforced by the capability check
(`Feature::Records`), not by codegen. See
`docs/STATUS.md` for the current feature × backend
matrix.

## What is not in v1

- **Generic record construction.** `rec Pair<T>` parses (the
  declaration accepts type parameters), but the explicit
  construction form `Pair<Int> { first: 1, second: 2 }` is not
  yet supported by the parser, and type-argument inference from
  field values is not implemented. Generic record support is a
  follow-up.
- **Sum types** (variants, tagged unions). Separate ADR.
- **Field-level borrows** (`&p.x`). Requires `Place` to become
  more than a variable name.
- **Auto-derived traits** (`Show` synthesized from fields).
  Needs a derive subsystem.
- **Nested destructuring** in `match` beyond one level.
- **LLVM lowering.** The follow-up ADR covers struct layout,
  stack vs heap allocation, GEP-based field access, and the
  interaction of structural `Copy` with LLVM `memcpy`.

## Example

```
rec Point
    x: Int
    y: Int

impl Show for Point
    function show() -> String
        return "(" + Int.to_string(self.x) + ", " + Int.to_string(self.y) + ")"

function add(a: Point, b: Point) -> Point
    return Point { x: a.x + b.x, y: a.y + b.y }

proc main
    var p := Point { x: 1, y: 2 }
    p.x := 10
    val q := add(p, Point { x: 5, y: 5 })
    print(q.show())           // (15, 7)

    match q
        case Point { x, y }
            print(x + y)      // 22
```

`add(p, Point { ... })` passes `p` by value. Because `Point` is
`Copy`, `p` remains usable after the call.

## See also

- `docs/decisions/0024-record.md` — the record feature ADR
- `docs/decisions/0026-structural-copy.md` — the `Copy` rule
- `docs/features/trait.md` — how `impl` blocks work
- `docs/features/region.md` — region attribution
# `rec` — Structured Data Types

**Status:** Interpreter complete. LLVM and WASM refuse programs
that use records; use `--interpreter` to run them.

**ADR:** `docs/decisions/0024-record.md`

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

Generic form (type parameters accepted, type-argument inference
currently infers from field values):

```
rec Pair<T>
    first: T
    second: T
```

### Construction

```
val p := Point { x: 1, y: 2 }
```

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

Bindings name fields by position — `Point { x, y }` binds `x` to
the field named `x` and `y` to the field named `y`. The order in
the pattern does not have to match the declaration order.

### Methods

Records participate in the existing trait system:

```
impl Show for Point
    function show(self: &Point) -> String
        return "(" + to_string(self.x) + ", " + to_string(self.y) + ")"

procedure main
    val p := Point { x: 1, y: 2 }
    print(p.show())    // (1, 2)
```

### Records in collections

```
val pts := [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]
print(pts[1].x)        // 3
```

### Records inside regions

A record declared inside a `region` belongs to that region. Since
v1 records do not allocate heap memory on their own, this only
matters for records that contain heap-typed fields:

```
region r
    val p := Point { x: 1, y: 2 }
    print(p.x)
```

## Semantics

**Value semantics.** A record is a value, not a reference. Passing
a record to a function and mutating the parameter does not affect
the caller's copy.

**Move-only in v1.** Records behave like `String` and `List` —
they move on assignment. Structural `Copy` (making `Point`,
whose fields are all `Copy`, itself `Copy`) is a follow-up.

**No `Drop`.** A record has no destructor. A record containing a
heap-allocated value (`String`, `List`, another non-`Copy`
record) leaks under the LLVM backend, exactly as a bare `String`
does today. The interpreter does not leak.

**Field access requires a record receiver.** `p.x` where `p` is
not a record is a compile error. `p` must be a `Type::Record`
(or `Unknown`, if the analyzer could not infer it — the field
then has type `Unknown`).

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

## What is not in v1

- Sum types (variants, tagged unions).
- Field-level borrows (`&p.x`).
- Auto-derived traits (`Show` synthesized from fields).
- Nested destructuring beyond one level.
- LLVM lowering.
- Structural `Copy` for records.

## Example

```
rec Point
    x: Int
    y: Int

impl Show for Point
    function show(self: &Point) -> String
        return "(" + to_string(self.x) + ", " + to_string(self.y) + ")"

function add(a: Point, b: Point) -> Point
    return Point { x: a.x + b.x, y: a.y + b.y }

procedure main
    var p := Point { x: 1, y: 2 }
    p.x := 10
    val q := add(p, Point { x: 5, y: 5 })
    print(q.show())           // (15, 7)

    match q
        case Point { x, y }
            print(x + y)      // 22
```

## See also

- `docs/decisions/0024-record.md` — design and rationale
- `docs/features/trait.md` — how `impl` blocks work
- `docs/features/region.md` — region attribution

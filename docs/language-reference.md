# ALGOL26 Language Reference

**Version**: v0.8.0

This document describes ALGOL26 as it exists today. Every syntax
construct and behavior here has been exercised by the compiler.
Where a feature works only in certain configurations, that is
stated explicitly.

For the pipeline internals, see `docs/architecture-direction.md`.
For historical context and design lineage, see `docs/decisions/`.

---

## 1. Lexical Structure

### 1.1 Source Files

ALGOL26 source files use the `.gol` extension.

### 1.2 Comments

```
// Single-line comment
-- Also a single-line comment
```

### 1.3 Identifiers

Identifiers begin with a letter or underscore and may contain
letters, digits, and underscores. They are case-sensitive. Dots are
not part of identifiers — `Math.sqrt` lexes as three tokens
(`Math`, `.`, `sqrt`) and is disambiguated by the parser as a
qualified function name.

### 1.4 Indentation

Indentation defines block structure. A block is a sequence of
statements at a consistent indentation level deeper than its
introducing statement.

- Use **spaces only**. Mixing tabs and spaces within a single line's
  leading whitespace is a lexical error (`E0001`).
- A tab counts as four spaces for the purpose of computing indent
  depth.
- Inconsistent dedentation — a line whose indent level does not
  match any enclosing block — is a lexical error.

### 1.5 Keywords

```
procedure   function    return
var         val         if
else        for         while
in          do          true
false       and         or
not         break       continue
match       case        try
catch       finally     defer
import      spawn       parallel
channel     send        receive
region      unsafe      extern
from        as          static
dynamic     alloc       free
trait       impl        Self
where       end         null
Some        None        Ok
Error       print       rec
```

### 1.6 Operators and Punctuation

| Symbol  | Meaning                        |
|---------|--------------------------------|
| `:=`    | Assignment                     |
| `+` `-` `*` `/` | Arithmetic           |
| `>` `<` `>=` `<=` `==` `!=` | Comparison  |
| `and` `or` `not` | Logical (short-circuit for `and`/`or`) |
| `&`     | Immutable borrow               |
| `&mut`  | Mutable borrow                 |
| `*`     | Dereference (prefix)           |
| `->`    | Return type arrow              |
| `..`    | Exclusive range                |
| `..=`   | Inclusive range                |
| `.`     | Method / field access, qualified name |
| `::`    | Trait-method qualification     |
| `( )` `[ ]` `{ }` `,` `:` | Grouping, indexing, lists, record literals |

---

## 2. Types

### 2.1 Primitive Types

| Type     | Literals       | Copy? |
|----------|----------------|-------|
| `Int`    | `42`, `-7`, `1_000_000` | yes |
| `Float`  | `3.14`, `1e10`, `-0.5`   | yes |
| `Bool`   | `true`, `false`          | yes |
| `String` | `"hello"`, `"a\\nb"`      | no  |
| `Ptr`    | `null`                    | yes |
| `Void`   | --                        | --   |

`Int` is a 64-bit signed integer. `Float` is IEEE 754 double
precision. `String` is an immutable UTF-8 value; assigning a
`String` moves it.

### 2.2 Composite Types

| Type              | Description                          |
|-------------------|--------------------------------------|
| `List<T>`         | Homogeneous sequence, indexed by `Int` |
| `Map<K, V>`       | Key-value container; keys are `Int`, `String`, or `Bool` |
| `Option<T>`       | `Some(v)` or `None`                  |
| `Result<T, E>`    | `Ok(v)` or `Error(e)`                |
| `Record`          | A named bundle of fields declared with `rec` |
| `Channel<T>`      | Message-passing channel              |
| `Borrow<T>`       | Immutable reference                  |
| `MutBorrow<T>`    | Mutable reference                    |
| `Pointer<T>`      | Raw pointer                          |

### 2.3 Type Annotations

Annotations are optional and appear after a colon:

```
val x: Int    := 42
val y: Float  := 3.14
val z: List<Float> := [1.0, 2.0]
var opt: Option<Int> := Some(5)
var counts: Map<String, Int> := Map {}
```

Type names are case-insensitive for the primitives (`Float` and
`float` are both valid).

### 2.4 Type Promotion

The analyzer unifies operand types via `common_supertype`:

| Expression                 | Result  |
|----------------------------|---------|
| `Int` op `Int`             | `Int`   |
| `Int` op `Float`           | `Float` |
| `Float` op `Int`           | `Float` |
| `Float` op `Float`         | `Float` |
| `String` `+` `String`      | `String` (concatenation) |

Arithmetic and comparison operators require **numeric** operands.
`"a" < "b"` is a type error, not a string comparison.

Boolean operators `and` / `or` require `Bool` operands on both sides.

---

## 3. Variables

### 3.1 Declaration

```
val x := 42          // immutable
var y := 0.0         // mutable
```

`val` bindings cannot be reassigned. `var` bindings can:

```
y := 3.14            // OK
x := 43              // error: cannot assign to immutable
```

### 3.2 Type Inference

If no annotation is given, the type is inferred from the initializer:

```
val a := 5           // Int
val b := 3.0         // Float
val c := "text"      // String
val d := true        // Bool
val e := [1.0, 2.0]  // List<Float>
val p := Point { x: 1, y: 2 }   // Point
val m := Map { "a": 1 }        // Map<String, Int>
```

Empty list and map literals need context to infer their element or
value types:

```
var xs: List<Int> := []
var m: Map<String, Int> := Map {}
```

### 3.3 Declaration Without Initializer

Every `val` and `var` requires an initializer. Uninitialized
declarations are not supported.

---

## 4. Ownership and Borrowing

ALGOL26 uses **lexical** ownership and borrow checking. A reference
lives until the enclosing scope is popped. Non-Lexical Lifetimes
(NLL) are not implemented; programs that rely on NLL may be rejected
conservatively.

### 4.1 Move Semantics

Non-`Copy` types (`String`, `List<T>`, `Map<K, V>`, and records
with a non-`Copy` field) are moved on assignment:

```
var s := "hello"
var t := s           // s is moved into t
print(s)             // error: use of moved variable 's'
```

`Int`, `Float`, `Bool`, and `Ptr` are `Copy` — assigning them
duplicates the value.

### 4.2 Structural `Copy` for Records

A record is `Copy` iff every field is `Copy`. The rule is
structural and recursive:

- `rec Point { x: Int, y: Int }` is `Copy`.
- `rec Person { name: String, age: Int }` is not `Copy`,
  because `String` is not.
- `rec Line { start: Point, end: Point }` is `Copy` iff `Point`
  is.

```
val p := Point { x: 1, y: 2 }
val q := p           // p is copied; both p and q remain usable
print(p.x)
print(q.x)
```

A record whose field type resolves to `Unknown` is conservatively
treated as move-only.

Mutability is orthogonal to `Copy`. A `Copy` record bound with
`val` is still immutable:

```
val p := Point { x: 1, y: 2 }
p.x := 5             // error: cannot assign to field of immutable variable
```

### 4.3 Immutable Borrows

```
val x := 5.0
val p := &x          // p : Borrow<Float>
val y := *p          // y : Float
```

Multiple immutable borrows of the same variable may coexist:

```
val a := &x
val b := &x          // OK
```

### 4.4 Mutable Borrows

```
var x := 5.0
val p := &mut x      // p : MutBorrow<Float>
*p := 10.0           // write through the reference
```

Only one mutable borrow may be active at a time. A mutable borrow
excludes all other borrows of the same variable:

```
var x := 5.0
val p := &mut x
val q := &mut x      // error: cannot mutably borrow 'x' more than once
val r := &x          // error: cannot borrow 'x' while mutably borrowed
```

Reading the source directly while it is mutably borrowed is also a
compile-time error until the borrow's scope ends:

```
var x := 5.0
val p := &mut x
print(x)             // error: cannot read 'x' while it is mutably borrowed
```

### 4.5 Reference Escape

A reference that outlives its source is a compile-time error. The
current analysis catches direct cases (`return &x`, storing `&x` in
a longer-lived location); a full lifetime model is future work.

### 4.6 Copy, Move, and Mutability Are Separate Axes

- **`Copy`** decides whether a value is duplicated on move or
  transferred.
- **`var` / `val`** decides whether a binding can be reassigned.
- **Borrow** decides whether the value is currently referenced by
  a `Borrow<T>` or `MutBorrow<T>`.

A `Copy` record can be borrowed. A non-`Copy` record can be `var`.
The three rules compose independently.

---

## 5. Control Flow

### 5.1 `if` / `else`

```
if x > 5.0 then
    print("large")
else
    print("small")
```

`then` is optional. `else if` chains are supported:

```
if x > 10.0
    print("very large")
else if x > 5.0
    print("large")
else
    print("small")
```

### 5.2 `for`

```
for item in [1.0, 2.0, 3.0] do
    print(item)
```

`do` is optional. The iterable must be a `List<T>`; the loop variable
has type `T`.

**Move-in-loop rule**: `for` bodies execute at least once on any
non-empty iterable, so moving a non-`Copy` variable inside the body
would trigger again on the next iteration. Such moves are rejected:

```
for item in list do
    var y := x       // error: cannot move 'x' in loop body
```

### 5.3 `while`

```
var n := 0
while n < 10 do
    n := n + 1
    print(n)
```

`do` is optional. A variable moved in a `while` body is marked as
"potentially moved" in the enclosing scope; subsequent uses error.
This is conservative because the loop may run zero times, but it
prevents unsafe use-after-move.

### 5.4 `break` / `continue`

`break` exits the innermost loop. `continue` jumps to the next
iteration.

### 5.5 `match`

```
match value
    case Some(v)
        print(v)
    case None
        print("empty")
```

Each arm begins with `case`. The pattern is one of:

- `Some(name)` / `None`
- `Ok(name)` / `Error(name)`
- `_` (wildcard)
- A variable binding: `case x`
- A literal: `case 42`, `case "hello"`
- A record destructure: `case Point { x, y }`
- Nested patterns: `case Some(Ok(v))`
- List destructuring: `case [head, ...]`
- A guarded pattern: `case Some(v) if v > 0`

### 5.6 Short-Circuit Evaluation

`and` and `or` short-circuit. The right operand is not evaluated if
the left operand determines the result:

```
false and side_effect()   // side_effect not called
true  or  side_effect()   // side_effect not called
```

This is implemented via an explicit two-branch CFG in the IR, so
runtime behavior matches the language semantics.

---

## 6. Functions

### 6.1 Declaration

```
function add(x: Float, y: Float) -> Float
    return x + y

procedure main
    print(add(1.0, 2.0))
```

`procedure` is sugar for `function name() -> Void`. The short form
`proc` is accepted as a lexer alias for `procedure`.

### 6.2 Optional Syntax

- Parameters: parentheses are required if any parameters exist.
  Empty parentheses are optional: `procedure main` and
  `procedure main()` are equivalent.
- Return type: `-> T` and `: T` are equivalent.
- Return value: `return` with no value in a `Void` function is
  allowed.

### 6.3 Expression-Bodied Functions

Any expression can be a function body:

```
function square(x: Float) -> Float
    x * x
```

The last expression is the return value.

### 6.4 Generics

```
function identity<T>(x: T) -> T
    return x

function first<T>(xs: List<T>) -> Option<T>
    if List.length(xs) == 0
        return None
    return Some(xs[0])
```

Type parameters are single uppercase letters by convention. The
analyzer binds them per call site, recursing into container types:
`first([1, 2, 3])` binds `T = Int` because `List<T>` against
`List<Int>` unifies the element.

Explicit call-site type arguments (`identity<Int>(42)`) are not
supported by the parser; inference from argument types is the
only form. See `docs/features/generic.md` for the full contract.

### 6.5 Trait Bounds

```
function max_of<T>(a: T, b: T) -> T where T: Comparable
    ...
```

The `where` clause restricts `T` to types implementing the named
trait. Bounds are name-resolved only: the analyzer checks that the
trait is declared, but does not verify that a concrete type
substituted for `T` at a call site actually implements it. See
ADR 0025.

### 6.6 `extern` (FFI)

```
extern "C" function puts(s: String) -> Int from "libc"
```

Extern declarations have no body and are lowered to foreign calls.

---

## 7. Option and Result

### 7.1 Option

```
val some_val : Option<Int> := Some(42)
val no_val   : Option<Int> := None
```

Pattern matching unwraps:

```
match some_val
    case Some(v)
        print(v)
    case None
        print("nothing")
```

### 7.2 Result

```
function safe_divide(a: Float, b: Float) -> Result<Float, String>
    if b == 0.0 then
        return Error("division by zero")
    return Ok(a / b)
```

The error type is unrestricted: `Result<Float, MyError>` where
`MyError` is a record works and is exercised in the CLI validation
programs.

### 7.3 `try` / `catch`

`try` is Result-based. The try body must evaluate to a
`Result<T, E>`. The `catch` branch handles the `Error` case and
must produce a value of type `T`.

```
val result := try
    safe_divide(10.0, 0.0)
catch err
    print(err)
    0.0
```

If the try body produces `Ok(v)`, `result` is `v`. If it produces
`Error(e)`, the catch block runs with `err` bound to `e` and
`result` is the catch block's value. The catch binding has the
try body's error type, so a record error type binds as a record
and its fields are accessible.

**Backend note**: `try/catch` works end-to-end through the
interpreter. The LLVM and WASM backends refuse programs that use
it with a clear message directing the user to
`algol26 run --interpreter`.

---

## 8. Null and Pointers

### 8.1 Null

`null` is a value of type `Ptr`:

```
val p := null
if p == null then
    print("no pointer")
```

### 8.2 Static Null-Deref Rule

Dereferencing a value statically known to be null is a compile-time
error (`E0007`):

```
val p := null
val x := *p          // error: cannot dereference 'p': it is
                     // statically known to be null
```

The analyzer tracks `val` bindings initialized to `null`. `var`
bindings are not tracked by value flow; a `var` holding null at
runtime is undefined behavior if dereferenced.

### 8.3 Borrowing and Dereference

```
var x := 5.0
val p := &x          // p: Borrow<Float>
val y := *p          // y: Float
```

`&x` creates a shared borrow of `x`. `&mut x` creates a mutable
borrow. `*p` reads through a pointer, borrow, or mut-borrow.

Borrows (`&x`, `&mut x`) produce tracked reference types
(`Borrow<T>`, `MutBorrow<T>`) that participate in the borrow
checker's lifetime model. This is distinct from a raw pointer
(`Pointer<T>`), which carries no lifetime information and is not
tracked by the analyzer.

The IR carries a distinct `AddrOf` operation for producing a raw
pointer. The surface parser currently does not produce it — no
`addr_of` syntax exists today. It is reserved for a future
raw-pointer feature and is not user-visible.

---

## 9. Defer

### 9.1 Semantics

`defer` registers a block to run when the enclosing function
returns. Defers run in LIFO order: the most recently registered
runs first.

```
function compute() -> Int
    defer
        print("first to run")
    defer
        print("second to run")
    return 42
```

Output:
```
second to run
first to run
```

### 9.2 Interaction with Return

The return value is preserved across defers. `return 42` with a
pending defer returns `42`, having run the defer block in between.

### 9.3 Current Limitations

- Only function `return` triggers defers. `break` / `continue` /
  fall-through do not.
- One defer stack per function; nested blocks do not create
  independent scopes.

---

## 10. Regions

A `region` scopes allocations:

```
region scratch
    var buf := alloc(1024)
    // use buf
// buf is deallocated here
```

Regions nest. Deallocating a region frees its children.

The region system provides parent/child discipline and double-free
protection at runtime. Compile-time proofs that a raw pointer does
not outlive its region are future work.

---

## 11. Concurrency

### 11.1 `spawn`

```
val x := 42
spawn
    print(x)
```

The spawned block runs concurrently with the enclosing block. The
interpreter runs the spawned block sequentially; the LLVM and WASM
backends refuse spawn at the capability check.

### 11.2 `parallel`

```
parallel
    print("A")
and
    print("B")
```

Blocks separated by `and` (or `,`) run concurrently. The parallel
construct joins before continuing. Same backend note as `spawn`.

### 11.3 Channels

```
channel ch

spawn
    send ch, 42

receive ch into result
print(result)
```

Channels carry values between spawned tasks. The `receive` binding
uses `into` or `as`.

**Backend note**: channels are refused by all three backends at the
capability check. The feature is specified but has no runtime on any
backend today. See ADR 0020.

### 11.4 Race Detection

The analyzer performs syntactic access-conflict detection:

- write + write = race
- write + read = race

`val` bindings are not counted as writes (they are written once
before any concurrent observer). `var` bindings shared across
spawns are flagged conservatively.

**Limitations**: the detector does not track aliases, channels as
synchronization, function-call boundaries, or thread lifetime. It
catches the common patterns but is not a proof of race freedom.

---

## 12. Traits

### 12.1 Declaration

```
trait Comparable
    function compare(other: Self) -> Int
```

The trait method list declares signatures. The receiver is not
written — `self` is bound at the `impl` site.

### 12.2 Implementation

```
impl Comparable for Int
    function compare(other: Int) -> Int
        if self < other then return -1
        if self > other then return 1
        return 0
```

Inside the method body, `self` is bound to the receiver. The
frontend step `expand_impl_methods` inserts the receiver as the
first parameter and renames the method to `<Type>_<method>`. The
`impl` body does not declare `self` explicitly.

Impl blocks for records work the same way:

```
rec Point
    x: Int
    y: Int

impl Show for Point
    function show() -> String
        return "(" + Int.to_string(self.x) + ", " + Int.to_string(self.y) + ")"
```

### 12.3 Method Call

```
val p := Point { x: 1, y: 2 }
print(p.show())
```

Dotted calls resolve through the trait registry at analysis time,
and through `resolve_method_call` in the IR builder at lowering
time. For records, the builder tries the impl-mangled form
(`Point_show`) before the builtin path.

**Chained method calls are not supported.** `x.field.method()`
does not parse — the parser rejects method calls on complex
receiver expressions. Bind the field to a local first:

```
val tmp := p.x
tmp.method()
```

### 12.4 Current Limitations

- Bounds are name-resolved only (ADR 0025).
- Supertrait syntax is not implemented.
- Associated types are not implemented.
- Default methods (methods with bodies in a `trait` declaration)
  are not implemented. The `TraitMethod` AST node has no body
  field.
- Trait objects (`Box<dyn Trait>` or equivalent) are not
  implemented.

---

## 13. Unsafe

```
unsafe
    var p := alloc(8)
    // raw pointer manipulation
    free(p)
```

Code inside an `unsafe` block is exempt from certain safety checks:
raw pointer dereference, `alloc`, and `free` are permitted only
within an unsafe block (ADR 0015). The block is a lexical scope,
not a keyword-delimited region.

---

## 14. Modules

Imports may be declared at file top level or inside a `procedure`:

```
import "utils.gol"
import "data/parser.gol"
import "analysis/stats.gol"

procedure main
    import "config.gol"
    ...
```

Both forms work. Imports are resolved relative to the importing
file's directory, then along configured search paths. Imported
files' own imports are followed recursively, so a module does not
need to name every transitively required file.

Circular imports are detected and reported. A file reached through
two different relative routes (`data/model.gol` and
`../data/model.gol`) is canonicalized to one entry and loaded once.

All declarations (functions, records, traits, impls) from the
imported file become available in the importing file's scope.

**Known limitation**: diagnostic spans in imported files are
attributed to the importing file's path. A type error inside
`data/parser.gol` reports a location in the file that imported it.

---

## 15. Standard Library

### 15.1 Math

| Function             | Signature              |
|----------------------|------------------------|
| `Math.sqrt(x)`       | `Float -> Float`       |
| `Math.pow(x, y)`     | `Float x Float -> Float` |
| `Math.sin(x)`        | `Float -> Float`       |
| `Math.cos(x)`        | `Float -> Float`       |
| `Math.tan(x)`        | `Float -> Float`       |
| `Math.abs(x)`        | `Float -> Float`       |
| `Math.floor(x)`      | `Float -> Float`       |
| `Math.ceil(x)`       | `Float -> Float`       |
| `Math.exp(x)`        | `Float -> Float`       |
| `Math.log(x)`        | `Float -> Float`       |

### 15.2 String

| Function                              | Signature                       |
|---------------------------------------|---------------------------------|
| `String.length(s)`                    | `String -> Int`                 |
| `String.concat(s1, s2)`               | `String x String -> String`     |
| `String.substring(s, start, len)`     | `String x Int x Int -> String`  |
| `String.to_upper(s)`                  | `String -> String`              |
| `String.to_lower(s)`                  | `String -> String`              |
| `String.trim(s)`                      | `String -> String`              |
| `String.split(s, sep)`                | `String x String -> List<String>` |
| `String.join(xs, sep)`                | `List<String> x String -> String` |

### 15.3 Conversions

| Function             | Signature                       |
|----------------------|---------------------------------|
| `Int.to_string(n)`   | `Int -> String`                 |
| `String.to_int(s)`   | `String -> Option<Int>`         |

`String.to_int` returns `Option<Int>` rather than `Int` because
not every string is a valid integer. Use `match` to distinguish.

### 15.4 File

| Function                          | Signature                       |
|-----------------------------------|---------------------------------|
| `File.read(path)`                 | `String -> String`              |
| `File.write(path, content)`       | `String x String -> Int`        |
| `File.append(path, content)`      | `String x String -> Int`        |

### 15.5 List

| Function             | Signature               |
|----------------------|-------------------------|
| `List.length(arr)`   | `List<T> -> Int`        |
| `List.sum(arr)`      | `List<Float> -> Float`  |
| `List.max(arr)`      | `List<Float> -> Float`  |
| `List.min(arr)`      | `List<Float> -> Float`  |
| `list.append(x)`     | mutating; `var` receiver required |

### 15.6 Map

| Function             | Signature               |
|----------------------|-------------------------|
| `m.insert(k, v)`     | mutating; `var` receiver required |
| `m.get(k)`           | `-> Option<V>`          |
| `m.contains(k)`      | `-> Bool`               |
| `m.keys()`           | `-> List<K>`            |
| `m.values()`         | `-> List<V>`            |
| `m.length()`         | `-> Int`                |

Keys are restricted to `Int`, `String`, and `Bool`.

### 15.7 Memory

| Function      | Signature                        |
|---------------|----------------------------------|
| `alloc(size)` | `Int -> Pointer<Unknown>`        |
| `free(ptr)`   | `Pointer<Unknown> -> Void`       |

`alloc` and `free` require an `unsafe` block.

### 15.8 Method Syntax

Built-in functions with dotted names may be called as methods on a
variable:

```
val list := [1.0, 2.0]
list.length()          // equivalent to List.length(list)

val s := "hello"
s.to_upper()           // equivalent to String.to_upper(s)
```

The receiver must be a bare identifier, not a general expression.
`"hello".to_upper()` does not parse — the parser rejects method
calls on complex receivers. Bind to a variable first:

```
val s := "hello"
print(s.to_upper())    // OK
```

Zero-argument methods may be called without parentheses:
`list.length` and `list.length()` are equivalent. Methods that
take arguments (`m.get(k)`, `m.insert(k, v)`) require parentheses.

---

## 16. Command-Line Interface

| Command                                | Behavior                          |
|----------------------------------------|-----------------------------------|
| `algol26 check <file.gol>`             | Type-check only                   |
| `algol26 build <file.gol>`             | Compile to native executable      |
| `algol26 run <file.gol>`               | Compile and execute               |
| `algol26 wasm <file.gol>`              | Compile to WebAssembly            |
| `algol26 run --interpreter <file.gol>` | Run through the interpreter only  |
| `algol26 inspect --ir <file.gol>`      | Dump semantic IR                  |
| `algol26 inspect --capabilities`       | Print feature × backend matrix    |

Flags:

- `--interpreter` — skip LLVM codegen; run through the tree-walking
  interpreter. Required for programs that use records, maps,
  `List.append`, `Option`, `Result`, `try/catch`, or any other
  feature the LLVM backend refuses.
- `--emit-llvm` — write the LLVM IR and exit without linking.
- `--run` — after `build`, execute the compiled binary.
- `--output NAME` / `-o NAME` — set the output name.
- `--timing` — print per-phase compile durations.
- `--version` / `-v`, `--help` / `-h`.

Program arguments follow a `--` separator:

```
algol26 run --interpreter main.gol -- --config=file.conf verbose
```

Inside the program, `args()` returns the list of arguments after
`--`.

---

## 17. Compiler Architecture

```
Source (.gol)
  |
  v
Lexer --> Parser --> AST
  |
  v
Module resolution      (imports inlined, recursively)
  |
  v
Loop desugaring        (unrolling + expansion)
  |
  v
Impl-method expansion  (renames impl methods to <Type>_<method>)
  |
  v
ExprId assignment      (stable identity for every expression)
  |
  v
Semantic analysis      (types, ownership, borrows, traits)
  |                     produces a type table keyed by ExprId
  v
InstantiationPlan      (transitive closure over generic calls)
  |
  v
SemanticIRBuilder      (CFG construction, consumes the type table
  |                     and the plan; emits one SemanticFunction
  |                     per concrete generic specialization)
  v
CFG verification       (structural checks)
  |
  v
Semantic verification  (instruction-level type checks)
  |
  v
Optimizer              (folding, DCE, branch simplification)
  |
  v
Re-verification        (post-optimize invariant check)
  |
  v
Capability scan        (refuses unsupported features per backend)
  |
  v
VerifiedIR             (gate: only verified IR reaches backends)
  |
  +--> LLVM backend
  +--> Interpreter backend
  +--> WASM backend
```

Every stage after the frontend runs through the compiler pass
pipeline. Passes declare a contract (input level, output level,
kind) enforced by the scheduler — see `docs/pass-contracts.md`.

---

## 18. Safety Guarantees

The following table states what the compiler **actually enforces**,
based on the current test suite. "Enforced" means there is a test
that would fail if the guarantee were broken.

| Guarantee                        | Status        | Enforced by             |
|----------------------------------|---------------|-------------------------|
| Type safety                      | Enforced      | Analyzer + verifier     |
| Immutability (`val` reassign)    | Enforced      | Analyzer                |
| Use-after-move                   | Enforced      | Analyzer                |
| Structural `Copy` for records    | Enforced      | Analyzer                |
| Borrow conflicts (double-mut, mut-while-immut, read-while-mut) | Enforced | Analyzer |
| Reference escape (direct cases)  | Enforced      | Analyzer                |
| Static array bounds (literals)   | Enforced      | Analyzer                |
| Runtime array bounds             | Enforced      | LLVM codegen + interp   |
| Null-deref of statically-null    | Enforced      | Analyzer                |
| Defer LIFO ordering              | Enforced      | IR builder              |
| Short-circuit `and`/`or`         | Enforced      | IR builder              |
| Trait declaration validity       | Enforced      | Trait registry          |
| Trait bound enforcement          | Not enforced  | Name-resolved only (ADR 0025) |
| Race detection (basic patterns)  | Partial       | Race detector           |
| Region allocation safety         | Partial       | Runtime allocator       |
| Region pointer lifetime          | Not enforced  | Compile-time work TODO  |
| Alias-aware race proof           | Not enforced  | Future work             |
| NLL borrow lifetimes             | Not implemented | Lexical model only    |

---

## 19. Known Limitations

### Language-level

- **Chained method calls on field accesses.** `x.field.method()`
  does not parse; bind the field to a local first.
- **Method calls on complex receivers.** `"hello".to_upper()` and
  `m.values().length()` do not parse; the parser requires a bare
  identifier as the receiver.
- **Explicit call-site type arguments.** `identity<Int>(42)` does
  not parse; inference from argument types is the only form.
- **Generic records.** `rec Pair<T>` parses, but the construction
  form `Pair<Int> { first: 1 }` is not supported by the parser.
- **Sum types.** The language has no variants or tagged unions;
  recursive structured data (a JSON tree, an AST) is not
  expressible. No ADR yet.
- **Reserved-word collisions.** `end`, `from`, `in`, `do`, `as`,
  and the rest of the keyword set cannot be used as field names.
  `rec Segment { start, end }` fails to parse.

### Backend-level

- **Records, `Map<K, V>`, and `List.append` are interpreter-only.**
  LLVM and WASM refuse programs that use them via the capability
  check. See `docs/STATUS.md` for the full matrix.
- **`Option`, `Result`, `try/catch`, and `String.*` conversions**
  are interpreter-only for the same reason.
- **Channels have no runtime on any backend.** See ADR 0020.
- **Regions that allocate** are refused by WASM (RawMemory is
  refused). The interpreter and LLVM support them.

### Analysis-level

- **Alias analysis for races.** The race detector does not track
  references.
- **Defer.** Only `return` triggers defers; `break`/`continue`
  and fall-through do not.
- **Value-flow analysis.** `var` bindings holding `null` at
  runtime are not tracked.
- **Trait bounds.** Name-resolved only (ADR 0025).
- **Diagnostic file provenance.** Errors inside imported files
  are attributed to the importing file.
- **Common subexpression elimination.** Not implemented.

---

## 20. Versioning

The `VERSION` file at the repository root and the git tags are the
authoritative source of the language version. This document tracks
the latest released version.
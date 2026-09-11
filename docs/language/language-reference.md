# ALGOL26 Language Reference

**Version**: v0.8.0

This document describes ALGOL26 as it exists today. Every syntax
construct and behavior here has been exercised by the compiler. Where
a feature works only in certain configurations, that is stated
explicitly.

For the pipeline internals, see `docs/architecture/`. For historical
context and design lineage, see `docs/decisions/`.

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
not part of identifiers -- `Math.sqrt` lexes as three tokens
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
- Inconsistent dedentation -- a line whose indent level does not
  match any enclosing block -- is a lexical error.

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
Error       print
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
| `( )` `[ ]` `,` `:` | Grouping, indexing, lists |

---

## 2. Types

### 2.1 Primitive Types

| Type     | Literals       | Copy? |
|----------|----------------|-------|
| `Int`    | `42`, `-7`, `1_000_000` | yes |
| `Float`  | `3.14`, `1e10`, `-0.5`   | yes |
| `Bool`   | `true`, `false`          | yes |
| `String` | `"hello"`, `"a\nb"`      | no  |
| `Ptr`    | `null`                    | yes |
| `Void`   | --                        | --   |

`Int` is a 64-bit signed integer. `Float` is IEEE 754 double
precision. `String` is an immutable UTF-8 value; assigning a
`String` moves it.

### 2.2 Composite Types

| Type              | Description                          |
|-------------------|--------------------------------------|
| `List<T>`         | Homogeneous sequence, indexed by `Int` |
| `Option<T>`       | `Some(v)` or `None`                  |
| `Result<T, E>`    | `Ok(v)` or `Error(e)`                |
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

Non-`Copy` types (`String`, `List`, and user composites) are moved
on assignment:

```
var s := "hello"
var t := s           // s is moved into t
print(s)             // error: use of moved variable 's'
```

`Int`, `Float`, `Bool`, and `Ptr` are `Copy` -- assigning them
duplicates the value.

### 4.2 Immutable Borrows

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

### 4.3 Mutable Borrows

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

### 4.4 Reference Escape

A reference that outlives its source is a compile-time error. The
current analysis catches direct cases (`return &x`, storing `&x` in
a longer-lived location); a full lifetime model is future work.

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
```

Type parameters are single uppercase letters by convention. The
analyzer binds them per call site.

### 6.5 Trait Bounds

```
function max_of<T>(a: T, b: T) -> T where T: Comparable
    ...
```

The `where` clause restricts `T` to types implementing the named
trait.

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
`result` is the catch block's value.

**Backend note**: `try/catch` works end-to-end through the
interpreter. The LLVM backend refuses programs that use it with a
clear message directing the user to `algol26 run --interpreter`.

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

### 8.3 Address-Of and Deref

```
var x := 5.0
val p := &x          // address of x
val y := *p          // load from p
```

The `AddrOf` operator produces a pointer; `Deref` loads from a
pointer, borrow, or mut-borrow.

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

The spawned block runs concurrently with the enclosing block.

### 11.2 `parallel`

```
parallel
    print("A")
and
    print("B")
```

Blocks separated by `and` (or `,`) run concurrently. The parallel
construct joins before continuing.

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
    function compare(self: Self, other: Self) -> Int
```

### 12.2 Implementation

```
impl Comparable for Int
    function compare(self: Int, other: Int) -> Int
        if self < other then return -1
        if self > other then return 1
        return 0
```

### 12.3 Method Call

```
val n := 5.compare(3)
```

Dotted calls resolve through the trait registry. Type errors report
which method on which type failed.

### 12.4 Current Limitations

- Supertrait syntax is not implemented.
- Associated types are not implemented.
- Generic impls accept single-uppercase-letter type parameters as
  wildcards; strict generic constraint checking is not yet
  performed.

---

## 13. Unsafe

```
unsafe
    var p := alloc(8)
    // raw pointer manipulation
    free(p)
```

Code inside an `unsafe` block is exempt from certain safety checks.
The block is a lexical scope, not a keyword-delimited region.

---

## 14. Modules

```
import "utils.gol"
import math/advanced
```

Imports are resolved relative to the importing file, then along
configured search paths. Circular imports are detected and reported.

All public declarations from the imported file become available.

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

### 15.3 File

| Function                          | Signature                       |
|-----------------------------------|---------------------------------|
| `File.read(path)`                 | `String -> String`              |
| `File.write(path, content)`       | `String x String -> Int`        |
| `File.append(path, content)`      | `String x String -> Int`        |

### 15.4 List

| Function             | Signature               |
|----------------------|-------------------------|
| `List.length(arr)`   | `List<T> -> Int`        |
| `List.sum(arr)`      | `List<Float> -> Float`  |
| `List.max(arr)`      | `List<Float> -> Float`  |
| `List.min(arr)`      | `List<Float> -> Float`  |

### 15.5 Memory

| Function      | Signature                        |
|---------------|----------------------------------|
| `alloc(size)` | `Int -> Pointer<Unknown>`        |
| `free(ptr)`   | `Pointer<Unknown> -> Void`       |

### 15.6 Method Syntax

Built-in functions with dotted names may be called as methods:

```
list.length()          // equivalent to List.length(list)
"hello".to_upper()     // equivalent to String.to_upper("hello")
```

---

## 16. Command-Line Interface

| Command                                | Behavior                          |
|----------------------------------------|-----------------------------------|
| `algol26 check <file.gol>`             | Type-check only                   |
| `algol26 build <file.gol>`             | Compile to native executable      |
| `algol26 run <file.gol>`               | Compile and execute               |
| `algol26 wasm <file.gol>`              | Compile to WebAssembly            |
| `algol26 --interpreter <file.gol>`     | Run through the interpreter only  |

Flags:

- `--interpreter` -- skip LLVM codegen; run through the tree-walking
  interpreter. Required for programs that use `try/catch`.
- `--emit-llvm` -- write the LLVM IR and exit without linking.
- `--run` -- after `build`, execute the compiled binary.
- `--output NAME` / `-o NAME` -- set the output name.
- `--version` / `-v`, `--help` / `-h`.

Flags may appear in any position.

---

## 17. Compiler Architecture

```
Source (.gol)
  |
  v
Lexer --> Parser --> AST
  |
  v
Module resolution      (imports inlined)
  |
  v
Loop desugaring        (unrolling + expansion)
  |
  v
Impl-method expansion
  |
  v
Monomorphization       (generic specialization)
  |
  v
Semantic analysis      (types, ownership, borrows, traits)
  |                     produces a type table keyed by AST node
  v
SemanticIRBuilder      (CFG construction, consumes the type table)
  |
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
VerifiedIR             (gate: only verified IR reaches backends)
  |
  +--> LLVM backend
  +--> Interpreter backend
  +--> WASM backend
```

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
| Borrow conflicts (double-mut, mut-while-immut, read-while-mut) | Enforced | Analyzer |
| Reference escape (direct cases)  | Enforced      | Analyzer                |
| Static array bounds (literals)   | Enforced      | Analyzer                |
| Runtime array bounds             | Enforced      | LLVM codegen + interp   |
| Null-deref of statically-null    | Enforced      | Analyzer                |
| Defer LIFO ordering              | Enforced      | IR builder              |
| Short-circuit `and`/`or`         | Enforced      | IR builder              |
| Race detection (basic patterns)  | Partial       | Race detector           |
| Region allocation safety         | Partial       | Runtime allocator       |
| Region pointer lifetime          | Not enforced  | Compile-time work TODO  |
| Alias-aware race proof           | Not enforced  | Future work             |
| NLL borrow lifetimes             | Not implemented | Lexical model only    |

---

## 19. Known Limitations

- **`try/catch` and LLVM**: LLVM refuses programs using `try/catch`
  with a clear diagnostic. Use `--interpreter`.
- **Alias analysis**: the race detector does not track references.
  A write through a reference is not connected to a read of the
  source variable.
- **Defer**: only `return` triggers defers; `break`/`continue` and
  fall-through do not.
- **Value-flow analysis**: `var` bindings holding `null` at runtime
  are not tracked.
- **Generic constraints**: single-letter type parameters are
  accepted as wildcards; strict coherence checking is not yet
  performed.
- **Common subexpression elimination**: not implemented.

---

## 20. Versioning

The `VERSION` file at the repository root and the git tags are the
authoritative source of the language version. This document tracks
the latest released version.
# Blocks

ALGOL26 has two equivalent notations for block structure. Both
parse to the same AST and compile to the same program. You pick
the one that reads better for the code in front of you.

## The two forms

**Indentation form** — the block is delimited by indentation. This
is the canonical form; the language's examples, tutorials, and
fixtures use it.

```
proc main
    val x := 10
    if x > 0 then
        print("positive")
    else
        print("negative")
```

**Brace form** — the block is delimited by `{` and `}`. Indentation
inside the block is cosmetic.

```
proc main {
    val x := 10
    if x > 0 {
        print("positive")
    } else {
        print("negative")
    }
}
```

Both programs produce identical output.

## Rules

### `then` after `if`

In indentation form, `then` separates the condition from the body.
In brace form, `{` does that job, and `then` is omitted.

```
// indentation form
if x > 0 then
    print("positive")

// brace form
if x > 0 {
    print("positive")
}
```

### `then` in brace-form `match` arms

Match arms in brace form require `then` between the pattern and
the body. Without it, `case A { ... }` is ambiguous with a record
pattern `A { ... }`.

```
match d {
    case A then {
        1
    }
    case Point { x, y } then {
        x + y
    }
}
```

The indentation form is unchanged:

```
match d
    case A
        1
    case Point { x, y }
        x + y
```

### Semicolons

None. Statements end at the newline. There is no way to put two
statements on one line; write each statement on its own line.

### Trailing expressions

A block's value is its last expression, in both forms.

```
// indentation form
fn square(x: Int) -> Int
    x * x

// brace form
fn square(x: Int) -> Int {
    x * x
}
```

## When to use which

Both work everywhere. The choice is stylistic.

**Use indentation form when:**

- You are writing code for a repository that already uses it.
- You want one canonical visual shape with no brace-placement
  debates.
- You are coming from Python.

**Use brace form when:**

- You are writing code for a repository that already uses it.
- You want your editor's auto-format, copy-paste, and diff tools
  to handle block structure without special indentation handling.
- You are coming from Rust, Go, Swift, C, Java, JavaScript, or C#.
- You want to write a small block on one line:
  ```
  fn double(x: Int) -> Int { x * 2 }
  ```

## One form per file

A file may legally mix the two forms — the parser accepts it — but
it reads better not to. Pick a form for the file and use it
throughout.

There is currently no `algol26 fmt` to enforce this. A future
formatter, when it exists, will normalize blocks to whichever form
the file's first block uses.

## What stays the same in both forms

Everything except the block delimiters. In particular:

- `:=` for assignment, `=` for equality.
- `val` and `var` for immutability and mutability.
- `proc` for statement bodies, `fn` for expression bodies.
- `rec`, `enum`, `impl`, `trait` for type declarations.
- `&` and `&mut` for references.
- `*r` for dereference; `*r := ...` for writes through a mutable
  reference is not yet parseable.
- Method calls, operators, string handling, generics.

The two forms are two notations for the same language, not two
dialects. Any ALGOL26 program can be rewritten from one to the
other mechanically, and `tests/paired.rs` does exactly that for
seven constructs.

## Why both

Both notations have real advantages.

Indentation form is denser and has one canonical shape. There is
no question of where the `{` goes, no argument about brace style,
no "reformat the file to 4-space braces" pull request. The code
looks like what it is.

Brace form is robust to tooling. Indentation-significant code
loses its structure under copy-paste, under diffs that reflow
whitespace, and in editors that normalize tabs. Brace-form code
survives all of that unchanged: the delimiters are explicit, and
whitespace is decorative.

Other languages ship both. Scala 3 calls it "Optional Braces" and
has supported it since 2021. F# has had "verbose" and
"lightweight" syntaxes since its early days. Ruby has two block
forms for method arguments. ALGOL26's version is simpler than
Scala's — the two forms are fully interchangeable, and neither is
deprecated.

## Limitations

- **Brace-form `match` arms require `then`.** See the rule above.
- **Mixing forms in one file is legal but discouraged.** The
  formatter that would normalize it does not exist yet.
- **Indentation inside brace blocks is cosmetic.** It has no
  effect on parsing. You can indent it how you like, or not at
  all.

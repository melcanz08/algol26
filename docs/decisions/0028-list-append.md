# ADR 0028: `List.append` — dynamic list growth

Status: Accepted (design); implementation pending

## Context

ALGOL26's `List<T>` is fixed-length once constructed. Programs
that need to grow a list incrementally — filter a collection,
accumulate results, build a value of unknown length — must
pre-allocate a fixed-capacity buffer and track the count
separately:

    var buf := [0, 0, 0, 0, 0, 0, 0, 0]
    var n := 0
    for x in source
        if keep(x)
            buf[n] := x
            n := n + 1

That works but has three problems: a hard ceiling, three times
the code, and a bug surface (forgetting to increment `n`).

The outsider's proposal flags `analysis/filter.gol` as blocked
on this: 'produces variable-length lists. Needs pre-allocation
without List.append.' With `Map<K, V>` and structural `Copy`
landed, `List.append` is the last container primitive the
remaining proposal modules need.

## Decision

Add a mutating `List.append` method:

    var out := []
    for x in source
        if keep(x)
            out.append(x)

The method takes one argument, mutates the receiver in place,
and returns `Void`.

### Method form, not function form

`list.append(x)`, matching the shape of the other container
operations (`list.length()`, `map.insert(k, v)`). The receiver
is the implicit first argument, so the language has one
uniform rule for how container operations read.

### Mutating, requires `var`

`append` mutates its receiver. Calling it on a `val` binding is
an error:

    val out := []
    out.append(1)          // E0007: cannot call 'append' on immutable variable

The diagnostic is the same one `Map.insert` and `p.x := v` on
an immutable binding already produce. `var out := []` accepts
the call.

This matches the language's existing model: `var` and `val`
differ in whether the binding's value can be reassigned, and
mutation-through-a-method is a form of reassignment.

### Single argument

`append(x)` takes exactly one value. To append several at once,
iterate:

    for x in more
        out.append(x)

A variadic `append(a, b, c)` is deliberately not part of v1 —
the loop form is explicit and costs the same.

### Returns `Void`

No return value. Not `List<T>`, not `Int` (the new length),
not `Option`. Chains that want the length call `.length()`
separately:

    out.append(x)
    val n := out.length()

Returning the list would suggest it's a new value, which it
isn't. Returning the length would tempt `append` into expression
position, where it reads as a pure operation.

## Semantics

### In-place mutation

`out.append(x)` grows `out` by one element. `out.length()`
afterwards is one greater than before. Any subsequent
`out[i]` for `i` in the old range returns the same value as
before; the new last index returns `x`.

### Empty-list initializer

`var out := []` — an empty list literal — infers
`List<Unknown>` in v1. Appending an `Int` produces a
`List<Int>` at the type-table level (the analyzer unifies
the receiver's element type with the argument's), but the
binding's declared type is `List<Unknown>`.

The pragmatic workaround: annotate the empty list,

    var out: List<Int> := []
    out.append(1)

which gives `out` a concrete element type from the start.
The un-annotated `var out := []` form works in the interpreter
because the runtime value is just a `Vec<RuntimeValue>` that
grows; the type-level imprecision is a pre-existing property
of empty list literals, not something this ADR introduces.

### Static length tracking is invalidated

The analyzer currently tracks a list's length at each binding
so `list[10]` on a three-element list produces a compile-time
OOB error. After an `append`, the tracked length is stale.

The analyzer clears the tracked length for the receiver on
every `append`. This means the compile-time OOB check no
longer fires for that binding, which is the conservative
choice: keeping a stale length would produce spurious OOB
errors for indices that are now valid.

Concretely:

    var xs := [1, 2, 3]
    print(xs[2])           // OK — analyzer knows xs.len() == 3
    xs.append(4)
    print(xs[3])           // no compile-time check, runtime OK
    print(xs[10])          // no compile-time check, runtime OOB

The runtime bounds check that the interpreter and LLVM already
emit for `ArrayAccess` remains, so a program with a genuine
OOB still fails — it just fails at runtime rather than at
analysis time.

### Ownership

`append` does not move its receiver. The list binding remains
valid after the call. The appended value is moved into the
list if it's non-`Copy`, matching how list literals already
consume their elements.

    var names := ["a", "b"]
    val more := "c"
    names.append(more)     // `more` is moved into `names`
    print(more)            // E-MOVE-001

For `Copy` element types (Int, Float, Bool, `Copy` records),
the appended value is duplicated and the source binding
remains usable.

### Nested lists

`List<List<T>>` works without a special case:

    var outer := [[1, 2]]
    outer.append([3, 4])
    print(outer.length())   // 2

The element type is `List<Int>`, which is not `Copy`, so the
appended inner list is moved into the outer list. This is the
correct semantics and matches the existing list-literal
behavior.

## Type system

No new type variant. `List<T>` is unchanged.

## AST

No new AST form. `list.append(x)` parses to the existing
`ExprKind::FunctionCall { name: "list.append", args: [x] }`
that all dotted method calls produce.

## Analyzer

The analyzer's method-call dispatch already handles dotted
names of the form `receiver.method`. `List.append` slots in
there. Unlike the other `List.*` operations (which are
registered as builtins with no mutability check), `append`
needs to inspect the receiver's mutability flag — the same
shape the `Map.insert` dispatch uses.

Two checks:

1. The receiver must be a `var` binding. `val` produces the
   existing immutability diagnostic.
2. The argument's type must coerce to the receiver's element
   type. A mismatch is a type error.

After successful analysis of the call, the analyzer clears
`self.list_lengths[receiver]` to invalidate the compile-time
bounds check (see 'Static length tracking is invalidated').

## IR

New lowering: `list.append(x)` becomes

    Instruction::Call {
        func: "List.append",
        args: [Variable(receiver), translated_x],
        result: None,
    }

The receiver is prepended, matching the convention all Map
and List builtin calls already use. `result: None` because
`append` returns `Void` — it's a statement, not an expression.

The verifier needs an arm: on `Call { func: "List.append", .. }`,
check that the first arg is a variable of `List<T>` type and
the second arg's type coerces to `T`. This mirrors the
`Map.*` short-circuit already in place.

## Interpreter

New arm in `eval_builtin_call` for `"List.append"`.

The receiver is grabbed structurally (a `TypedIRValue::Variable`)
rather than evaluated, because evaluating would clone the list
and lose the mutation. Same shape as `Map.insert`:

    "List.append" => {
        let receiver_name = /* args[0] as Variable */;
        let val = self.eval_value(&args[1])?;
        let receiver = self.variables.get_mut(&receiver_name)?;
        match receiver {
            RuntimeValue::List(items) => {
                items.push(val);
                Ok(RuntimeValue::Void)
            }
            other => Err(/* TypeMismatch */),
        }
    }

The list is a `Vec<RuntimeValue>` in the interpreter, so append
is `Vec::push`. No bounds ceiling, no reallocation policy to
decide.

## Backends

| Backend     | Support                                                        |
|-------------|----------------------------------------------------------------|
| Interpreter | Yes — `Vec<RuntimeValue>::push`                                |
| LLVM        | Refused — capability check (`Feature::ListAppend`)             |
| WASM        | Refused — capability check (`Feature::ListAppend`)             |

The LLVM backend lowers `List<T>` as a fixed-size stack array
with a statically-known length (that's how `List.length` is
resolved without a runtime field). Append would require a
dynamic-allocation path, a growth policy, and a runtime length
field on every list value. That's a substantial redesign of
the LLVM list representation, worth its own ADR.

### Capability

New `Feature::ListAppend`. Interpreter claims it; LLVM and
WASM do not.

The scanner fires on any `Instruction::Call` whose callee name
is `List.append`.

## What is deliberately not in v1

- **`List.extend(other)`** — bulk append. A loop over `other`
  is the v1 workaround; the bulk form is a follow-up if a use
  case needs the performance.
- **`List.insert(i, x)`** — indexed insertion. Needs shifting,
  bounds reasoning, and an interaction with the static-length
  tracking that append sidesteps by only growing at the end.
- **`List.remove(i)`** / **`List.pop()`** — removal. Separate
  ADR, same reasons as `Map.remove`.
- **`List.clear()`** — trivial follow-up; `list := []` covers
  it.
- **Guaranteed amortized O(1) append.** The interpreter is
  `Vec::push`, which amortizes; the language does not specify
  complexity.

## Consequences

**Positive.**

- `analysis/filter.gol` becomes buildable without the
  pre-allocation workaround.
- Any 'produce a variable-length list' program — accumulate,
  filter, flatten, collect — becomes straightforward.
- The last container primitive the outsider's proposal needs
  is now in place. Eleven of twelve flagged modules unblock
  (only full recursive JSON still needs sum types).

**Negative.**

- LLVM support is deferred; `List.append` is interpreter-only
  until the LLVM list representation is redesigned.
- Static length tracking is weakened: any program that appends
  loses the analyzer's compile-time bounds check on that
  binding. Runtime bounds checks remain.

**Neutral.**

- The mutating-`var`-required pattern is now established across
  three features (`p.x := v`, `m.insert(k, v)`, `list.append(x)`).
  It's a coherent rule, not a set of one-offs.

## Tests

- `append_grows_list_by_one` — `[1, 2].append(3)` length is 3.
- `append_returns_void` — the call in expression position is
  a type error (or the value is discarded).
- `append_requires_var` — `val xs := []; xs.append(1)` errors
  with the immutability diagnostic.
- `append_moves_non_copy_value` — appending a `String` to a
  `List<String>` moves the source binding.
- `append_copies_copy_value` — appending an `Int` leaves the
  source binding usable.
- `append_invalidates_static_length` — a literal index that
  was OOB before append no longer produces a compile-time error
  after.
- `append_to_empty_annotated_list` — `var xs: List<Int> := [];
  xs.append(1)` produces a `List<Int>` of length 1.
- `append_to_nested_list` — `List<List<Int>>` grows correctly.
- `append_rejects_wrong_element_type` — appending a `String`
  to a `List<Int>` is a type error.
- `filter_via_append` — end-to-end: filter a list using append
  and verify the output.
- Capability: `interpreter_accepts_list_append`,
  `llvm_rejects_list_append`, `wasm_rejects_list_append`.

## See also

- `docs/decisions/0027-map.md` — the `Map.insert` pattern this
  follows
- `docs/decisions/0024-record.md` — `p.x := v`, the original
  mutating-operation design
- `docs/features/list.md` — the existing container
- The outsider's proposal — `analysis/filter.gol` is the
  immediate motivation

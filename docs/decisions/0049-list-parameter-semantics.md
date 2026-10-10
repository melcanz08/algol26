# ADR 0049: List parameters are by value

Status: Accepted

## Context

`List<T>` in a function-parameter position behaves differently
on the two backends today:

- **Interpreter** clones the caller's list when binding it to
  a parameter. Element writes are local; the caller's binding
  is unchanged.
- **LLVM** passes the descriptor struct by value but aliases
  the underlying buffer. `xs[i] := v` writes through to the
  caller's storage.

Probe:

    proc set(xs: List<Int>, i: Int)
        xs[i] := 99

    proc main
        val a := [10, 20, 30]
        set(a, 1)
        print(a[1])       // interp: 20, llvm: 99

Same program, different output, no diagnostic.

## Decision

Lists are passed by value. A `List<T>` parameter receives its
own copy of the caller's elements. Mutating the parameter does
not affect the caller's binding. Sharing requires an explicit
reference (`&List<T>` / `&mut List<T>`), matching how every
other type in the language behaves.

Aligns `List<T>` with the value semantics of records (ADR
0036) and the ownership model's no-implicit-aliasing rule
(ADR 0005). The interpreter's behavior is correct. LLVM
changes.

## Implementation

At list-parameter binding, LLVM codegen:

1. Reads `{buffer, length}` from the incoming descriptor.
2. `malloc(length * elem_size)`.
3. `memcpy(fresh, buffer, length * elem_size)`.
4. Stores `{fresh, length, length}` into the parameter's
   descriptor alloca. `capacity = length` marks it heap-owned
   so a subsequent `.append` goes through the realloc path.

## Known limitations

- **Leak.** The copy is never freed. Freeing on exit needs
  region / scope cleanup to interact with heap buffers (ADR
  0050, forthcoming).
- **O(N) per call.** Optimization to skip the copy when the
  callee provably does not mutate requires inter-procedural
  analysis. Deferred.

## Alternatives considered

### A. Keep by-reference, fix the interpreter

Change `RuntimeValue::List(Vec<RuntimeValue>)` to hold an
`Rc<RefCell<Vec<RuntimeValue>>>`.

Rejected. Introduces implicit aliasing into a language whose
ownership model forbids it, makes records and lists inconsistent
in parameter position, leaves no way to ask for a private copy.

### B. Reject list-typed parameters

Require `&List<T>` or `&mut List<T>` at every call site.

Rejected. Too restrictive; reading functions would force `&xs`
noise on every caller.

## See also

- ADR 0005 (ownership)
- ADR 0036 (records, by-value ABI)
- ADR 0042 (LLVM dynamic lists)

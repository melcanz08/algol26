# Dynamic dispatch (`&dyn Trait`)

## Summary

`&dyn Trait` and `&mut dyn Trait` are fat pointers that carry both
the concrete value and a virtual method table. A method call through
a `&dyn Trait` receiver dispatches at runtime through that table,
selecting the impl for the value's concrete type.

## Surface syntax

```algol26
trait Shape
    function area(self: &Self) -> Float

rec Circle
    r: Float

impl Shape for Circle
    function area(self: &Circle) -> Float
        return 3.14159 * self.r * self.r

proc main
    val c := Circle { r: 2.0 }
    val s: &dyn Shape := &c      // implicit coercion &Circle -> &dyn Shape
    print(s.area())              // dispatches at runtime
```

## Rules

- `&T` coerces to `&dyn Trait` when `T: Trait`. The coercion is
  implicit at a binding, argument, or return position; no cast
  syntax is needed.
- `&mut T` coerces to `&mut dyn Trait` symmetrically.
- `dyn Trait` outside a borrow (`dyn Shape` alone) is a type
  error. `dyn Trait` is only valid as the inner type of a
  `Borrow` / `MutBorrow`.
- The coercion runs the object-safety check on the trait at the
  first `dyn Trait` in a program. A non-object-safe trait is
  rejected before the coercion is attempted.

## Object safety

For v1, a trait is *object-safe* iff every method is declared with
a `&Self` or `&mut Self` receiver. There are no generic methods and
no associated types in v1, so those clauses of Rust's object-safety
rule are structurally vacuous here and are not checked.

When a `dyn Trait` reference resolves to a trait whose methods
violate the rule, the analyzer emits:

```
error[E0002]: trait `Consuming` is not object-safe: method `take`
receiver must be `&Self` or `&mut Self` - a `dyn Trait` cannot be
dispatched through a by-value or consuming receiver
```

## Representation

A `&dyn Trait` value is a fat pointer:

```
{ data: ptr, vtable: ptr }
```

`data` points at the concrete value's storage; `vtable` points at an
internal-linkage constant array `[N x ptr]` where slot `i` holds the
address of the impl method for the trait's `i`-th declared method.

Vtables are emitted once per `(trait, concrete)` pair that appears
in a `dyn Trait` coercion. The mangled name is
`__algol26_vtable_{trait_id}_{concrete_type_mangled}`, and the impl
methods the slots point at are mangled
`{Trait}_{Concrete}_{method}`.

## Dispatch

`receiver.method(args)` on a `&dyn Trait` receiver:

1. **Analyzer.** Resolves `method` through the trait declaration
   (`trait_method` + `trait_method_slot` on `TraitRegistry`), and
   records a `VirtualCallInfo { trait_name, method_name, slot,
   return_type }` keyed by the call's `ExprId`.
2. **IR builder.** Emits `TypedIRValue::VirtualCall` at the source
   call site; the two statement-level emission sites convert it to
   `Instruction::VirtualCall`.
3. **Codegen (LLVM/WASM).** Loads the vtable pointer from the fat
   pointer, GEPs to `slot`, loads the method pointer, emits an
   indirect call with `data` as the first argument.
4. **Interpreter.** Reconstructs the mangled impl name
   `{Trait}_{Concrete}_{method}` at runtime from the fat pointer's
   stored `trait_name` and the concrete value's runtime kind, and
   invokes that function directly.

## Diagnostics

| Code | When |
|------|------|
| `E0002` | `dyn Trait` reaches a non-object-safe trait; trait method signature mismatch; call arity/type mismatch. |
| `E0003` | `dyn UnknownName` - the trait name isn't registered. |
| `E0011` | `trait` has no method by that name. |
| `E0012` | `&T -> &dyn Trait` coercion where `T` does not implement the trait. |

## Known limitations (v1)

- **No owned dynamic dispatch.** `Box<dyn Trait>` is not
  supported - the language has no heap ownership model. Only
  borrowed `&dyn` / `&mut dyn` forms exist.
- **No trait-object upcasting.** `&dyn Sub` does not coerce to
  `&dyn Super` where `Sub: Super`. Deferred.
- **No runtime type identity.** No `Any`, no `TypeId`, no
  downcasting. The purpose of `dyn Trait` is to preserve the
  trait's interface, not erase it.
- **No generic trait objects.** `dyn Trait<T>` with type arguments
  is out of scope.
- **Vtable layout is ABI-fixed.** The slot ordering is the trait's
  declaration order. Adding a method to a trait changes the layout
  of every vtable for that trait, which is a link-compatibility
  break for pre-compiled objects.
- **`&mut Self` through `&mut dyn Trait`.** The interpreter's
  virtual dispatch path does not implement the write-back
  machinery `eval_call` uses for concrete `&mut self` receivers.
  `&Self` receivers (the common case) are unaffected.

## Capability

All three backends support dynamic dispatch as of ADR 0038 D6.

## References

- `docs/decisions/0038-dynamic-dispatch.md` - design rationale and
  scope boundary.
- `docs/decisions/0033-methods-and-self.md` - the method and trait
  machinery `dyn Trait` builds on.
- `docs/decisions/0005-ownership-model.md` - the reference
  semantics `&dyn Trait` reuses.
- `docs/features/trait.md` - static traits.

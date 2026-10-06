# ADR 0038 — Dynamic Dispatch (`dyn Trait`)

## Status

Proposed. Requires investigation before implementation.

> **Status note (2026-10-06).** Methods and traits work statically
> on all three backends. A `T: Trait` bound is resolved at compile
> time; every call site knows the concrete type. The gap this ADR
> addresses is heterogeneity: a `List<T>` where `T: Shape` holds
> values of one concrete type, so `List<Circle>` and `List<Square>`
> cannot be mixed. Dynamic dispatch closes that gap.
>
> The first implementation step is investigation. The LLVM codegen
> is now understood well enough to propose a concrete direction;
> the interpreter's value model and the ownership interaction are
> the open questions.

## Context

ALGOL26's trait system provides static polymorphism. A generic
function with a trait bound is monomorphized at the call site,
which is why the method feature (ADR 0033) and generic impls
(ADR 0034) work on all three backends today.

Static dispatch cannot express:

- **Heterogeneous collections.** `List<Shape>` holding circles,
  squares, and triangles. Each element is a different concrete
  type; the list has one element type.
- **Open-ended plugin-style extension.** A library exports
  `register_handler(h: SomeTrait)` and the consumer supplies an
  impl the library has never seen.
- **Runtime polymorphism.** Code that reads a config value and
  picks a strategy at runtime rather than at compile time.

The current workaround is a closed enum:

```gol
enum Shape
    CircleShape(Circle)
    SquareShape(Square)
```

This works and is what Rust's own `enum` guidance recommends for
closed sets. It fails when the set is open.

## Decision

ALGOL26 will support **borrowed** dynamic dispatch: `&dyn Trait`
and `&mut dyn Trait`. Owned dynamic dispatch (`Box<dyn Trait>` or
equivalent) is deferred to a future ADR; the language has no heap
ownership model yet and inventing one for this feature would
double its scope.

### Fat pointer representation

A `dyn Trait` value is a fat pointer:

```
{ data: ptr, vtable: ptr }
```

`&dyn Trait` and `&mut dyn Trait` are therefore `{data, vtable}`
pairs where `data` is itself a reference to the concrete value.
This is the same representation Rust and Swift use; it is the
minimal shape that makes dynamic dispatch work.

### Vtable layout

One vtable per `(trait, concrete type)` pair. Method slots appear
in the trait's declaration order. For:

```gol
trait Shape
    function area(self: &Self) -> Float
    function perimeter(self: &Self) -> Float
```

the vtable for `Circle` is:

```llvm
@Circle_as_Shape_vtable = internal constant {
    ptr,   ; Circle_area
    ptr,   ; Circle_perimeter
}
```

No type identity, no size, no drop function — those are additions
for a heap-owned form, which this ADR does not introduce.

### Construction

A `&T` coerces to `&dyn Trait` when `T` implements `Trait`:

```gol
val c := Circle { r: 2.0 }
val s: &dyn Shape := &c
```

The compiler emits the fat pointer `{ &c, &Circle_as_Shape_vtable }`.
The coercion is implicit at a `&dyn Trait` binding site; the
analyzer inserts it. No user-facing cast syntax in v1.

`&mut T` coerces to `&mut dyn Trait` symmetrically. `&dyn Trait`
and `&mut dyn Trait` are distinct types; the mutability is on the
data reference, not on the trait object.

### Dispatch

A call `s.area()` where `s: &dyn Shape` compiles to:

```llvm
%vtable_field = getelementptr { ptr, ptr }, ptr %s, i32 0, i32 1
%vtable = load ptr, ptr %vtable_field
%method_slot = getelementptr ptr, ptr %vtable, i32 0
%method = load ptr, ptr %method_slot
%data = load ptr, ptr %s     ; the data half
%result = call double %method(ptr %data)
```

One indirect call. No type check, no boxing beyond the fat
pointer itself.

### Coherence

The vtable for `(Trait, ConcreteType)` is emitted once per
compilation unit that references the coercion. Duplicate emission
across the linker's units is resolved by symbol unification —
each vtable is emitted with internal linkage and the linker
picks one.

The set of impls contributing to a vtable is: every `impl Trait
for ConcreteType` visible in the compilation unit. Adding a new
impl in a different unit does not retroactively change an existing
vtable; the vtable is fixed at the coercion site.

This is the same rule C++ uses for static-linked virtual dispatch.
It's a v1 simplification; a dynamic-loading story would need
rethinking.

## Open questions

**Q1. Interpreter representation.** The interpreter has no
pointer model. A `dyn Trait` value needs a runtime representation
that carries enough information for dispatch. Two options: (a) a
`RuntimeValue::DynTrait { data: Box<RuntimeValue>, trait_name: String,
concrete_type: String }` and a lookup in a trait-method registry;
(b) a fat-pointer-shaped `{ Rc<RefCell<...>>, VtableId }` where
the vtable is a Rust-side enum. Option (a) is more direct and
does not require interior mutability.

**Q2. Ownership and borrowing.** A `&dyn Trait` borrows the
underlying data. What happens when the data is a temporary
(`&Circle { r: 2.0 }`)? The temporary's lifetime is the enclosing
expression; the reference cannot outlive it. The borrow checker
already handles this for `&Circle`; extending to `&dyn Circle`
should be uniform, but verify.

**Q3. Mutability propagation.** If `s: &mut dyn Shape`, can
`s.area()` call a method declared `self: &Self`? Syntactically
yes (`&mut T` coerces to `&T` at a call site), but the vtable
must carry the method for `&Self` receivers, not a separate
`&mut Self` slot. Decide whether the vtable is keyed by receiver
mode or receiver-agnostic.

**Q4. Capability matrix.** A new `Feature::DynamicDispatch`
is needed. LLVM has the machinery (indirect calls, vtables).
WASM inherits from LLVM; `wasm-ld` must accept the emitted
vtables and indirect calls, likely trivially. Interpreter needs
the representation from Q1. The capability should default to
refused on all three until each is verified.

**Q5. Interaction with generic impls.** `impl<T> Trait for
Pair<T>` produces a vtable per `(Trait, Pair<Int>)`, `(Trait,
Pair<String>)`, etc. — one per concrete monomorphization. The
mangled name for each vtable should follow the same scheme as
the specialized methods.

**Q6. `Self` in trait method signatures.** The trait's declaration
`function area(self: &Self) -> Float` appears in the vtable as a
function taking `&ConcreteType`. The substitution that ADR 0033
added to `validate_method_signature` already does this at the
signature level; the vtable construction reuses it.

**Q7. Diagnostics.** Coercing `&c` to `&dyn Shape` where `Circle`
does not implement `Shape` is a type error. Its message should
name the missing impl and the trait. A dedicated error class is
warranted; existing call-type-mismatch codes may not fit.

## Scope boundary

**In scope for v1:**

- `&dyn Trait` and `&mut dyn Trait` types.
- Implicit coercion from `&T` / `&mut T` at binding sites.
- Dynamic dispatch through method call syntax.
- Vtables emitted for every `(Trait, ConcreteType)` coercion.
- All three backends (capability matrix governs which).

**Explicitly out of scope:**

- `Box<dyn Trait>`, `Rc<dyn Trait>`, or any owned form. The
  language has no heap ownership model; adding one is its own ADR.
- Trait object safety rules. In Rust, not every trait is
  object-safe (generic methods, associated types). In v1, the
  analyzer should reject traits with non-object-safe methods at
  the coercion site with a clear diagnostic, rather than trying
  to define what "object-safe" means for ALGOL26.
- Upcasting between trait objects (`&dyn TraitA` to `&dyn
  TraitB` where `TraitB: TraitA`). Deferred.
- Type identity at runtime (`Any`, `TypeId`, downcasting).
  Deferred.
- Dynamic library loading. Vtables are fixed at link time.

## Alternatives considered

### Closed enums as the recommended pattern

**Rejected as a replacement, kept as a complement.** For
closed sets, the existing enum + `match` pattern is more
efficient (no vtable, direct call) and clearer (the compiler
enforces exhaustive handling). Documentation should recommend
enums for closed sets and reserve `dyn Trait` for open ones.
The two coexist.

### `Any`-style type-erased values with downcasting

**Rejected.** This is a different feature — runtime type
inspection, which ALGOL26's design explicitly declines (ADR
0033 §Terminology). The purpose of `dyn Trait` is to preserve
the trait's interface; `Any` erases it and asks for a runtime
check to recover. Different mental model, different safety
story.

### `Box<dyn Trait>` as the v1 form

**Rejected for v1.** This is the shape Rust actually uses most
in practice. But it requires a heap model — ownership of heap
data, drop semantics, `Box` as a language primitive — none of
which exist yet. Introducing all of that in the same ADR would
make it a multi-ADR project. Borrowed dynamic dispatch delivers
the polymorphism without the heap commitment.

### Generics with trait bounds + enum return

**Rejected.** This is the current workaround: define an enum
whose variants are the concrete types, return the enum, dispatch
via `match`. It works but requires the enum's variants to be
known at the enum's definition site — which is exactly the
closed-set restriction `dyn Trait` removes.

### No dynamic dispatch

**Rejected.** The feature is standard in every systems language
designed after 2010 (Rust, Swift, Go's interfaces, C++ with
explicit opt-in). Its absence is a real expressiveness gap. The
question is when, not whether.

## Implementation order

```
D0. Investigation. Answer Q1 (interpreter representation) and
    Q4 (capability matrix impact). Confirm the fat-pointer
    representation maps cleanly to the existing `Type::Pointer`
    and `Type::Borrow` shapes, or requires a new `Type::DynTrait`.

D1. AST and types. Add `TypeSyntax::DynTrait` and the
    corresponding `Type::DynTrait` variant. Parser accepts
    `&dyn Trait` and `&mut dyn Trait` in type position.

D2. Analyzer. Coercion rule: `&T` → `&dyn Trait` when `T`
    implements `Trait`. Reject coercion when the trait has
    non-object-safe methods. Reject `&dyn Trait` in positions
    where the concrete type is required.

D3. IR. Add `TypedIRValue::DynTrait { data, vtable_id }` and a
    semantic pass that gathers the `(Trait, ConcreteType)` pairs
    used in the program.

D4. LLVM codegen. Emit vtables as constant structs; lower
    `TypedIRValue::DynTrait` to a `{ptr, ptr}` alloca; lower
    method calls on `dyn Trait` to indirect calls.

D5. Interpreter. Add the runtime representation from Q1;
    dispatch through a `HashMap<(trait, concrete), Vec<fn>>`.

D6. Capability. `Feature::DynamicDispatch` refused by all
    backends until each D4/D5 lands; then enabled per backend.

D7. Fixtures, docs, matrix. Three valid fixtures (basic,
    heterogeneous list, generic impl) and three invalid
    (non-object-safe trait, missing impl, lifetime escape).
```

Each step compiles and is revertable. D0 is the gating step;
D1–D2 do not depend on LLVM specifics and can land together.

## Consequences

### Positive

- Heterogeneous collections work. `List<&dyn Shape>` holds
  circles, squares, and triangles.
- Plugin-style APIs become expressible.
- The language gains runtime polymorphism with compile-time
  type safety at the interface boundary.
- Static dispatch remains the default; `dyn Trait` is opt-in.
  Programs that don't use it pay nothing.

### Negative

- Indirect call overhead. Every `s.area()` on a `dyn Shape` is
  a vtable lookup and an indirect call — not free, but standard.
- Trait object safety rules must be defined well enough to reject
  the unsupported cases. This is a real design surface even in
  v1's minimal form.
- Vtable layout becomes a stable ABI concern across compiler
  versions and across compilation units.
- The interpreter gains a new runtime value kind, which is a
  semantic-layer change.

### Neutral

- The IR gains one new `TypedIRValue` variant; the semantic
  pipeline is otherwise unchanged.
- The existing trait + generic machinery handles most of the
  analysis; `dyn Trait` is a new type form, not a new resolution
  tier.

## References

- ADR 0005 (Ownership Model) — the reference semantics
  `&dyn Trait` reuses.
- ADR 0010 (Canonical IR) — the discipline that keeps the new
  value shape isolated.
- ADR 0033 (Methods) — the trait machinery this ADR builds on.
- ADR 0034 (Generic Impls) — the monomorphization this ADR
  parallels for vtable per concrete instantiation.
- ADR 0036 (LLVM Records) — the codegen pattern the LLVM-side
  implementation follows.
- Rust's `dyn Trait` documentation — the mental model and
  object-safety guidance this ADR mirrors.

## See also

- `docs/features/methods.md` — the static-dispatch feature this
  ADR extends.
- `docs/features/trait.md` — the trait mechanism.
- `docs/decisions/README.md` — the ADR index and convention.
# ADR 0033 — Methods, Receivers, and the OOP Direction

## Status

Accepted. Not yet implemented.

> **Status note (2026-10-04).** This ADR records a decision, not a
> completed implementation. It commits ALGOL26 to method syntax on
> records with explicit-typed `self` receivers, and explicitly
> declines classical inheritance, dynamic dispatch, and the
> `this` / `obj` / `own` alternatives.
>
> **Status note (2026-10-04, post-review).** Amended following an
> outside review. Six points that were listed as open questions
> are now frozen decisions: method-resolution precedence, field/
> method namespace, the exact set of types accepting inherent
> impls, method mangling, `Self` substitution for trait methods,
> and the surface-desugaring invariant. See §Review record at the
> end of this ADR for the audit trail. The implementation order
> (B1–B5) is unchanged; none of it has shipped yet.

> **Status note (2026-10-05, post-implementation).** B1–B4 are
> implemented for non-generic user types (records, enums, nominal
> types). Eleven of thirteen conformance fixtures pass on the
> interpreter. Two are deferred:
>
> - `generic_method_impl.gol` — generic impls (`impl<T> Trait for
>   Pair<T>`) are not parseable today; `parse_impl` does not accept
>   `impl<T>`, `ImplBlock` has no `type_params` field, and
>   `expand_impl_methods`'s mangling scheme does not accommodate
>   generic arguments in the type name. This is a follow-up ADR.
>
> - `method_mut_receiver.gol` and `method_matches_free_function.gol`
>   — write-through-`&mut self` requires either the interpreter to
>   model aliasing or LLVM to support records. Backend-blocked;
>   see the parity-gap work.
>
> The invariants the ADR commits to are implemented and verified:
> receiver modes through the ordinary ownership machinery, method
> resolution precedence (inherent > trait), field/method collision
> rejection, `self` outside `impl` rejection, `Self` substitution in
> trait method validation, and the desugaring-to-`Call` invariant.

## Context

An outside review asked whether ALGOL26 can accommodate
object-oriented programming. The review's answer was yes, but not
by importing a Java/C++ class system. It proposed a decomposition:

```
record        -> data
methods       -> behavior attached to a type
traits        -> interfaces / capabilities
impl          -> method implementation
nominal types -> identity
ownership     -> object lifetime
```

That decomposition matches ALGOL26's existing architecture almost
exactly. The language already has:

- `rec` for structured data (ADR 0024)
- `trait` + `impl Trait for T` for interface-based polymorphism
- `type X distinct Y` for nominal identity (ADR 0029)
- Ownership, borrowing, and regions for lifetime (ADRs 0005, 0007)

What is missing is the connective tissue: the ability to declare a
method on a type without wrapping it in a trait, and a receiver
concept that the borrow checker understands.

This ADR decides that direction, decides the receiver keyword, and
freezes the semantic details required to begin implementation.

## Decision

### Direction

ALGOL26 will support **methods on user-defined types**, via two
mechanisms:

1. **Inherent impls** — `impl Type` without a trait. Methods declared
   here are part of the type's own surface, not part of any trait.
2. **Trait impls** — `impl Trait for Type`, unchanged from today.

The following are **explicitly out of scope for v1**, and each would
require its own ADR:

- Classical inheritance (`class Dog extends Animal`)
- Dynamic dispatch (`dyn Trait` values, vtables)
- Visibility modifiers (`private`, `protected`)
- Reflection (runtime type inspection)
- Inherent impls on builtin types, generic types, or foreign types

### The receiver keyword is `self`

`self` (lowercase) is the receiver value. `Self` (uppercase) is the
enclosing type — already present in the parser as `Token::SelfType`
and mapped to `Type::TypeVar("Self")`. The two spellings pair:
`self` is the value, `Self` is its type.

### The receiver form is a typed first parameter

A method's receiver is its **first parameter**, named `self`. The
receiver's ownership mode is the parameter's type:

```gol
impl User
    function name_of(self: &User) -> String
        return self.name
    end

    function rename(self: &mut User, new_name: String)
        self.name := new_name
    end

    function into_id(self: User) -> UserId
        return self.id
    end
end
```

Three modes:

| Receiver | Mode | Call desugars to |
|---|---|---|
| `self: T` | consumed | `T_method(x, args)` |
| `self: &T` | shared borrow | `T_method(&x, args)` |
| `self: &mut T` | exclusive borrow | `T_method(&mut x, args)` |

Inside an `impl Type` block, a method whose first parameter is named
`self` may be called with dot syntax on a value of the enclosing
type: `u.rename("Alice")` desugars to `rename(&mut u, "Alice")`.

`self` is not a special reference. It is an ordinary binding whose
type and ownership mode the borrow checker reasons about exactly as
it does for any other binding. There is no object header, no hidden
`this` pointer, and no separate ownership theory for methods.

### `self` outside an `impl` block is an error

A parameter named `self` outside an `impl` block is a compile error.
`self` is not a general parameter name.

### No `mutating` keyword

The receiver's mutability is carried by its type, not by a modifier
on the method. `self: &mut User` is already a mutable borrow; no
additional keyword is needed. This avoids a second semantic axis
(receiver type x method modifier) that the analyzer would otherwise
have to reason about.

### Which types may receive inherent impls

In v1, inherent impls may target **monomorphic user-defined types**:

- user-defined records (`rec Foo`)
- user-defined enums (`enum Foo`)
- user-defined nominal types (`type Foo distinct Y`)

Deferred to a future ADR:

- **Builtin types** (`Int`, `Float`, `String`, `Bool`, `List<T>`,
  `Map<K,V>`, `Option<T>`, `Result<T,E>`, `Set<T>`, `Array`, `Tuple`,
  `Channel<T>`, `Ptr`, `Void`). Extending a builtin is a global
  coherence question — two modules could both `impl Int`, and the
  language has no orphan rule today to arbitrate.
- **User-defined generic types** (`rec Pair<T>`, `enum Tree<T>`).
  Receiver type substitution inside an inherent impl needs its own
  design pass; the existing trait machinery handles this case,
  so deferring costs nothing.
- **Foreign types.** No FFI-imported nominal types exist yet.

Trait impls continue to work for all of the above, exactly as today.

### Method resolution precedence

When a call site `u.name(args)` could resolve to more than one
declaration, the following order applies:

1. **Inherent methods** on `u`'s type
2. **Trait methods** provided by any trait in scope that `u`'s type
   implements, when exactly one such trait provides the name
3. Otherwise, a compile error

If two traits in scope both provide a method with the same name, and
no inherent method shadows it, the call is an ambiguity error. Call-
site disambiguation syntax (qualified `Trait::method(u, args)`) is
out of scope for v1 and would be its own ADR.

Within a single type's inherent methods, a duplicate name is an
error. Across the same type's multiple trait impls, the ambiguity
rule above applies at call time.

### Field / method namespace

Field names and inherent method names share one member namespace
per type. A collision is a compile error:

```gol
rec User
    name: String
end

impl User
    function name(self: &User) -> String    // ERROR: collides with field `name`
        ...
    end
end
```

This prevents the property/method ambiguity that would otherwise
arise if the syntax `u.name` (field) and `u.name()` (method)
diverged into two ways to reach the same member.

### Method calls are surface syntax

This is the load-bearing invariant of the whole ADR:

> A method call is surface syntax. `u.rename("Alice")` desugars to
> an ordinary `TypedIRValue::Call` with the receiver as the first
> argument: `User_rename(&mut u, "Alice")`. There is no method-
> specific IR value, no method-specific `SemanticFunction` shape,
> and no runtime dispatch unless the method is a trait method
> resolved through existing dispatch machinery.

This means method syntax does not create an OOP runtime model. The
canonical pipeline is unchanged:

```
source: u.rename("Alice")
    -> semantic resolution
    -> User_rename(&mut u, "Alice")
    -> SemanticIR Call
    -> VerifiedIR
    -> LLVM / WASM / interpreter
```

The backend does not need to know "this was a method". It only needs
to know "this is a verified function call".

### Method mangling

Mangled method names must use **canonical type identity**, not the
display name. Two `rec User` declared in different modules, each
with an inherent `save`, must mangle to distinct symbols.

The canonical scheme is provided by `mangled_type_name` in
`src/ir/instantiation_plan.rs`; the method mangler composes this
with the method name:

```
method_symbol(T, m) = format!("{}_{}", mangled_type_name(T), m)
```

**Known pre-existing issue (must be resolved in B4):**
`mangled_type_name` currently mangles `Type::Record(name, args)` by
the *display name*, not by an identity token. Two same-named records
in different modules therefore already collide in the mangled
namespace today. Whether to give records their own identity token
(analogous to `NominalTypeId`, `EnumTypeId`, `SubrangeTypeId`) is a
separate decision that B4 must resolve — either by adding identity,
or by fully qualifying the module path in the mangled name.

### `Self` substitution in trait methods

Trait method signatures may use `Self` to refer to the implementing
type:

```gol
trait Persistable
    function save(self: &Self)
end

impl Persistable for User
    function save(self: &User)
        ...
    end
end
```

At impl resolution, `Self` is substituted for the concrete
implementing type. The substitution **must reuse the existing
`InstantiationPlan` / `substitute` machinery**, not introduce a
parallel method-specific substitution system. Concretely:

```
trait method declaration:   self: &Self
impl resolved against:      User
resolved signature:         self: &User
```

The same must work for generic trait implementations, where the
existing specialization plan already produces concrete types at
monomorphization time.

### No new backend semantic operation

Because method calls lower to ordinary `Call` nodes, **no new backend
semantic operation is required** on LLVM, the interpreter, or WASM.
Backends may still require symbol-resolution adjustments for the new
mangled names, and backend tests for the new shapes.

The distinction that matters:

```
no new backend semantics        ✓
no backend changes whatsoever   ✗
```

### AST representation: FunctionDecl, not MethodDecl

`FunctionDecl` gains an `Option<ReceiverMode>` field:

```rust
pub struct FunctionDecl {
    ...
    pub receiver: Option<ReceiverMode>,
}

pub enum ReceiverMode {
    Consume,   // self: T
    Shared,    // self: &T
    Exclusive, // self: &mut T
}
```

No parallel `MethodDecl` type is introduced. The semantic reality is
`method = function + receiver parameter + owner/type context`, and the
IR already treats a method as a function. A parallel declaration
hierarchy would risk the same divergence the canonical-IR work
(ADRs 0010, 0013, 0018) was designed to prevent.

Analyzer-side metadata (a `MethodInfo` record describing the resolved
method, its owner type, and its receiver mode) is a separate concern
and belongs in the analyzer's tables, not the AST.

## What remains open

The review closed six points. The following are still open and must
be resolved during B1–B4 as noted:

1. **Method-call AST shape (B2).** Does `ExprKind::FunctionCall`
   grow a `receiver: Option<Box<Expr>>` field, or does a new
   `ExprKind::MethodCall` variant exist? The parser currently
   produces `FunctionCall { name: "u.rename", args: [...] }` and
   the analyzer splits the string. A dedicated variant would
   remove that string-splitting; the receiver field is less
   disruptive. Decide in B2.
2. **`self` in the trait registry (B3).** `Self` in a trait method
   signature must resolve through the trait registry at impl time.
   Confirm the existing registry carries enough information, or
   extend it.
3. **Semantic IR shape for the receiver (B4).** `u.rename("Alice")`
   lowers to `Call { function: "User_rename", args: [Variable("u",
   User), ...] }`. Confirm the receiver is emitted as the first
   argument, not a special `Call.self` field.

## Consequences

### Data model

- `FunctionDecl` gains `receiver: Option<ReceiverMode>`.
- New `ReceiverMode` enum: `Consume`, `Shared`, `Exclusive`.
- Analyzer's `records` table gains a `methods: HashMap<String,
  MethodInfo>` field, or a parallel `impls: HashMap<String,
  Vec<MethodInfo>>` table. Decide in B3.

### Frontend

- Parser: recognize `impl Type` without a `for` clause.
- Parser: recognize `self` as a reserved parameter name; the name
  is stored in the AST as an ordinary parameter name, and the
  analyzer attaches meaning to it.
- No new precedence rules: dot-method calls use the same postfix
  grammar as `u.to_base` does today.

### Analyzer

- Method resolution: on `u.method(args)`, apply the frozen
  precedence rule: inherent, then trait, then error.
- Declare `self` in the method body's scope with the receiver's
  resolved type (after `Self` substitution for trait methods).
- Reject `self` outside an `impl` block.
- Reject a field/inherent-method name collision.
- Reject duplicate names within a type's inherent methods.
- Reject an inherent impl on a builtin, generic, or foreign type.
- Reject an ambiguous call when two traits in scope both provide
  the name and no inherent method shadows it.

### IR

- Inherent methods lower to `SemanticFunction` entries with mangled
  names, exactly as trait-impl methods do today.
- `u.rename("Alice")` lowers to `TypedIRValue::Call { function:
  "User_rename", args: [Variable("u", User), String("Alice")] }`.
- The mangled name uses canonical type identity, not display name.
  Resolving the pre-existing `Type::Record` mangling ambiguity is
  part of B4.

### Backends

- **No new backend semantic operation.** Methods are functions with
  a mangled name and a leading argument. LLVM, interpreter, and WASM
  already handle this shape.
- Backends may require symbol-resolution adjustments for the new
  mangled names, and new backend tests.

### Diagnostics

- `self` outside an `impl` block.
- Duplicate method name within the same inherent impl.
- Field/inherent-method name collision.
- Ambiguous trait method call (two traits, same name).
- Inherent impl on a builtin, generic, or foreign type.
- Calling `u.method()` where `u`'s type has no such method and no
  trait provides one.

### Tests

Conformance fixtures, `tests/conformance/valid/methods/`:

- `method_shared_borrow.gol` — `self: &T`, returns a field
- `method_mut_receiver.gol` — `self: &mut T`, mutates a field
- `method_consuming_receiver.gol` — `self: T`, moves and returns
- `method_consuming_then_use_rejected.gol` — proves the receiver
  moves: after `u.into_id()`, a later `u.name_of()` is a use-after-
  move error, not a method-specific loophole
- `method_mut_conflict_rejected.gol` — proves the receiver obeys the
  same exclusive-borrow rules as `rename(&mut u, ...)`
- `method_on_nominal_type.gol` — `impl UserId`
- `method_on_enum.gol` — `impl Day`
- `method_resolution_inherent_precedence.gol` — inherent shadows
  trait method of the same name
- `method_field_collision_rejected.gol` — `impl User { fn name }`
  where `User` has field `name`
- `method_ambiguous_trait_rejected.gol` — two traits in scope both
  provide `save`, no inherent shadows
- `method_self_outside_impl_rejected.gol`
- `trait_self_substitution.gol` — `impl Persistable for User`,
  `Self := User` in the trait method signature
- `generic_method_impl.gol` — trait method on a generic impl
- `method_matches_free_function.gol` — the load-bearing invariant:
  `u.rename("Alice")` produces identical observable behavior to
  `rename(&mut u, "Alice")`

The last one is the most important. It is the conformance test that
proves method syntax is sugar, not a second execution model.

## Implementation order

```
B1. AST: receiver: Option<ReceiverMode> on FunctionDecl;
    ReceiverMode enum. No MethodDecl.
B2. Parser: `impl Type` without trait; `self` in parameter
    position. Decide method-call AST shape (open question 1).
B3. Analyzer: declare self; resolve dot-call to method with the
    frozen precedence; reject self outside impl; reject field/
    method collision; reject inherent impls on builtin/generic/
    foreign types; Self substitution for trait methods.
B4. IR: mangled names using canonical type identity. Resolve the
    pre-existing Type::Record mangling ambiguity (see §Method
    mangling). Lower method calls to ordinary Call nodes.
    B4 as implemented (2026-10-05). Mangling was already handled by
    expand_impl_methods; the IR builder's resolve_method_call already
    looked up Type_method. B4's actual work was extending that lookup
    from Type::Record to all four user-defined type forms. The
    Type::Record identity question remains open — it's not blocking
    methods because expand_impl_methods doesn't currently collide on
    same-named records in different modules, but it's still worth
    resolving.
B5. Conformance fixtures, feature doc, STATUS.md row, coverage
    matrix row + EXPECTED_MATURITY entry.
```

Each step compiles, tests, and is revertable.

## Alternatives considered

### `this` as the receiver keyword

**Rejected.** `this` is a reference-semantics word: C++ `this` is a
pointer, Java/JS `this` is a reference. ALGOL26 receivers can be
by-value, shared-borrow, or exclusive-borrow; none of these is
"a reference". `this` would misrepresent two of the three modes.
Additionally, `this` pairs with no existing type token — `function
clone(this: &Self) -> Self` reads broken against the parser's
existing `Token::SelfType`.

### `obj` as the receiver keyword

**Rejected.** `obj` is a noun, not a pronoun. A receiver keyword's
job is to point at one specific thing (the receiver), which is a
pronoun's job. `obj` names a category, and the reader has to scan
upward to find which object is meant. It collides with the common
local variable name `let obj = ...`; it pairs with no type token;
and it signals a root-object philosophy that ADR 0029 explicitly
rejected by making every nominal type distinct.

### `own` as the receiver keyword

**Rejected.** `own` already means ownership in ALGOL26 (ADR 0005).
`own: &User` would be self-contradictory — the type says borrow,
the keyword says own. In Rust, `own` denotes the consuming side of
the ownership model; using it for a borrow receiver inverts that
precedent.

### Rust-style compressed receiver syntax (`&self`, `&mut self`)

**Rejected for v1.** The compressed form requires parser
special-casing and does not match ALGOL26's style of full type
annotations everywhere. `self: &mut User` is more in keeping with
the rest of the language and costs the reader nothing.

### Swift's `mutating` method modifier

**Rejected.** `mutating` introduces a second semantic axis the
analyzer would have to reason about. The parameter form preserves
the single-axis rule. `mutating` also cannot express consuming
receivers, which the parameter form handles for free.

### Universal Function Call Syntax (UFCS)

**Rejected for v1.** Disambiguation is hard: two modules can both
define `length(String)` and neither is obviously preferred. The
`impl`-block-scoped form gives almost all of UFCS's ergonomics
without the ambiguity. UFCS is an additive change if ever wanted;
removing it would be breaking.

### `MethodDecl` as a parallel AST type

**Rejected.** The semantic reality is `method = function + receiver
+ owner context`. The IR already treats a method as a function. A
parallel declaration hierarchy would risk the divergence the
canonical-IR work (ADRs 0010, 0013, 0018) was designed to prevent.

### Classical inheritance

**Rejected.** The cost list, preserved so a future ADR reconsidering
inheritance starts from the list rather than a blank page:

- What owns the base object?
- How is the object laid out in memory?
- Are subclasses substitutable for their base?
- How does borrowing work across the hierarchy?
- Can a borrowed `Dog` become an `&Animal`?
- How does mutation interact with dynamic dispatch?
- What happens to the base sub-object when the derived value moves?
- How are virtual methods represented in Semantic IR?
- How do LLVM, WASM, and the interpreter agree on dispatch?

Each is a distinct design problem. Together they are a multi-month
project with its own ADR, not a syntactic addition.

The intended replacement for inheritance is composition plus traits:
shared behavior lives in traits, shared state lives in fields, and a
type that needs both is a record whose fields are the composed
types. This pattern should be documented in `docs/vision.md` or
`docs/features/trait.md`.

### Dynamic dispatch (`dyn Trait`)

**Rejected for v1.** A `dyn Trait` value requires a heap model, a
vtable layout decision, and indirect-call codegen on all three
backends. None of these exist today; ALGOL26 has `alloc`/`free` but
no GC, and the ownership model is deliberately not reference-counted.
Static trait bounds cover the vast majority of what OOP-shaped code
reaches for. If a real use case appears that static bounds cannot
express, dynamic dispatch can be its own ADR.

## Terminology

In user-facing documentation (`docs/features/`), this feature is
described as **methods** or **record methods**, not as "OOP".
What ALGOL26 gains is:

- encapsulated state through records
- methods through inherent impls
- nominal identity through `distinct`
- interfaces through traits
- polymorphism through traits and generics
- receiver-based dispatch through `self`

What it does not gain, deliberately:

- mandatory classes
- inheritance hierarchies
- virtual methods by default
- implicit reference semantics
- garbage-collected objects
- runtime reflection
- vtables in v1

That is the design identity of the language, not a shortcoming.

## Review record (2026-10-04)

An outside review of the initial draft identified the following.
All were addressed in this amendment:

1. **Precedence was internally undecided.** The draft listed it as
   open while the consequences section already stated a rule. Now
   frozen in §Method resolution precedence.
2. **Field/method collision was internally undecided.** Same shape.
   Now frozen in §Field / method namespace.
3. **"records and other types" was imprecise.** The draft did not
   say whether `impl Int`, `impl String`, or `impl List<Int>` are
   legal. Now frozen in §Which types may receive inherent impls.
4. **Mangling by display name is unsafe.** Two `rec User` in
   different modules would collide. Now frozen in §Method mangling,
   with a note that the pre-existing `Type::Record` mangling
   ambiguity must be resolved in B4.
5. **`Self` substitution mechanism was unspecified.** Now frozen in
   §`Self` substitution in trait methods, reusing the existing
   `InstantiationPlan` machinery.
6. **"No new backend work" needed qualification.** Now stated as
   "no new backend semantic operation", with the backend-tests
   caveat.

Additional points addressed from the same review:

- AST representation: `FunctionDecl` with a receiver field, not a
  parallel `MethodDecl`. See §AST representation.
- `self: T` consumption semantics: the receiver goes through the
  ordinary move machinery; a subsequent use is a use-after-move
  error. New test `method_consuming_then_use_rejected.gol`.
- Test plan expanded to cover shared-borrow lifetime, mut-borrow
  conflict, consuming move, inherent-vs-trait precedence, field
  collision, cross-module name collision, `Self` substitution,
  generic impls, methods on nominal types and enums, and the
  method-equals-free-function invariant.
- Terminology: "OOP" is not used in user-facing docs; the feature
  is called "methods". See §Terminology.

One point neither the reviewer nor the original draft addressed,
now decided: **user-defined generic types** (`rec Pair<T>`) are
also deferred from v1 inherent impls, for the same reason as other
generic cases — receiver type substitution in a generic inherent
impl needs its own design pass.

## References

- ADR 0005 (Ownership Model) — the three receiver modes
- ADR 0024 (Records) — the data types methods attach to
- ADR 0029 (Nominal Types) — distinct identity for user types
- ADR 0030 (Enum Types) — closed-world variants
- ADR 0032 (Set Types) — the most recent feature added under this
  architectural discipline
- `src/ir/instantiation_plan.rs` — the canonical mangling and
  specialization machinery that method mangling and `Self`
  substitution must reuse

## See also

- `docs/features/methods.md` — the feature contract for
  receiver-bearing declarations, resolution precedence, and
  receiver-mode semantics.
- `docs/features/trait.md` — the trait mechanism
- `docs/decisions/0034-generic-impls.md` — the follow-up ADR that
  lifts the generic-impl limitation ADR 0033 deferred.
- `docs/decisions/0035-interpreter-aliasing.md` — the follow-up ADR that
  makes `&mut self` write-through runnable on the interpreter.
- `docs/decisions/README.md` — the ADR index and convention

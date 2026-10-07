# Feature: Methods (`x.method(args)`, `x.method`)

> Per-feature contract, following the pattern described in `docs/architecture-direction.md`.
> This file is the authoritative answer to "what is a method in ALGOL26, and where does it live?"
> See ADR 0033 for the design rationale and decisions.

## Summary

A **method** attaches behavior to a user-defined type. It is declared inside
an `impl` block, has an explicit-typed receiver as its first parameter, and
is called with dot syntax on a value of that type:

```gol
rec User
    name: String

impl User
    function label(self: &User) -> String
        return self.name

proc main
    val u := User { name: "Alice" }
    print(u.label())          // "Alice"
```

Three receiver forms correspond to three ownership modes:

| Receiver | Mode | Meaning |
|---|---|---|
| `self: T` | Consume | The receiver is moved into the method. |
| `self: &T` | Shared | The receiver is borrowed immutably. |
| `self: &mut T` | Exclusive | The receiver is borrowed mutably. |

There is no `this`, no `obj`, no `own` — `self` is an ordinary binding with
an ordinary type, and the borrow checker reasons about it exactly as it does
for any other parameter. See ADR 0033 §"The receiver keyword is `self`".

Method syntax is **surface sugar**: by the time the IR is built, `u.label()`
is an ordinary `Call` to `User_label` with the receiver as the first
argument. There is no method-specific IR value, no method-specific backend
operation, and no runtime dispatch unless the method is a trait method
resolved through the existing trait machinery.

## Syntax

Declaration inside an `impl` block:

```gol
impl User
    function label(self: &User) -> String
        return self.name

    function rename(self: &mut User, new_name: String)
        self.name := new_name

    function into_name(self: User) -> String
        return self.name
```

Call site:

```gol
u.label()              // parenthesized form, any arity
u.into_name()          // consumes u
v.rename("Alicia")     // mutable receiver; v must be var
```

The bare form `u.length` (no parens) is the zero-argument shorthand,
mirroring the existing builtin method-call syntax:

```gol
val n := s.length      // bare form, zero-arg only
val n := s.length()    // parenthesized form, equivalent
```

`u.method(args)` (parenthesized) parses as `ExprKind::FunctionCall { name:
"u.method", args }`. `u.method` (bare) parses as `ExprKind::FieldAccess
{ object: u, field: "method" }`. Both surface forms go through the same
analyzer dispatch.

The parenthesized form works on any expression: `f().method()`,
`arr[0].method()`, `a.b.c.method()`. The `&mut self` receiver mode is
restricted to named bindings — the mutability check needs a `val`/`var`
declaration to consult. To call a mutating method on a temporary, bind
it to a `var` first.

## Which types may receive inherent impls

In v1, inherent impls (`impl Type`, without a `for` clause) may target:

- user-defined records (`rec Foo`)
- user-defined enums (`enum Foo`)
- user-defined nominal types (`type Foo distinct Y`)

Trait impls (`impl Trait for Type`) already existed and are unchanged.

**Not yet supported** (each would need its own ADR):

- **Builtin types** (`Int`, `Float`, `String`, `List<T>`, `Map<K,V>`,
  `Option<T>`, `Result<T,E>`, `Set<T>`, `Channel<T>`, `Ptr`, `Void`).
  Extending a builtin is a global coherence question with no orphan rule.
- **User-defined generic types** (`rec Pair<T>`, `enum Tree<T>`).
  Receiver type substitution inside a generic inherent impl needs its
  own design pass. See §Known limitations.
- **Foreign types.** No FFI-imported nominal types exist yet.

## Receiver semantics

The receiver's declared type determines how the call is checked:

**`self: &T`** — the receiver is shared-borrowed for the duration of the
call. The method body can read fields through `self` but cannot mutate
them. The caller's binding is not consumed.

**`self: &mut T`** — the receiver is exclusively borrowed. The caller's
binding must be declared `var`; a `val` binding is rejected with E0007.
Only one mutable borrow may be active at a time; `check_borrow_rules`
enforces this the same way it does for `&mut x`.

**`self: T`** — the receiver is moved into the method (if non-Copy). After
the call, the caller's binding is moved: any subsequent use is a use-after-
move error. This is the same machinery as `let y := x; use(x)`. See
ADR 0005.

`self` is not a special reference. Inside the method body it is a normal
binding whose ownership mode the analyzer reasons about.

## Method resolution precedence

When a call site `u.method(args)` could resolve to more than one declaration,
the following order applies:

1. **Inherent methods** on `u`'s type.
2. **Trait methods** provided by a trait in scope that `u`'s type
   implements, when exactly one such trait provides the name.
3. Otherwise, a compile error.

An inherent method shadows a trait method of the same name. Two traits in
scope both providing the same name with no inherent method shadowing it is
an ambiguity error. Call-site disambiguation syntax
(`Trait::method(u, args)`) is deferred to a future ADR.

## Field / method namespace

Field names and inherent method names share one member namespace per type.
A collision is a compile error:

```gol
rec User
    name: String

impl User
    function name(self: &User) -> String    // ERROR: collides with field `name`
```

This prevents the property/method ambiguity that would arise if `u.name`
(field) and `u.name()` (method) diverged into two ways to reach the same
member.

## `Self` in trait method declarations

Trait method signatures may use `Self` to refer to the implementing type:

```gol
trait Persistable
    function save(self: &Self)

impl Persistable for User
    function save(self: &User)
        print("saved")
```

At impl validation, `Self` in the trait's declared signature is substituted
for the impl's target type before comparing. The substitution is recursive —
`Borrow<Self>`, `MutBorrow<Self>`, and any nested form works uniformly.

## Where resolution happens

Resolution is layered across three sites:

**Parser.** `u.method(args)` becomes `ExprKind::FunctionCall { name:
"u.method", args }`. The parser does not know what a method is; it just
emits a dotted function name. Bare `u.method` becomes `FieldAccess`.

**`expand_impl_methods`** (in `src/compiler.rs`). Before the analyzer runs,
every method in every `impl` block is flattened into a top-level
`FunctionDecl` whose name is `Type_method` and whose first parameter is
`self`. Inherent impls are processed first so that inherent method names
claim their mangled slots before trait impls; a trait method whose mangled
name collides with an already-claimed inherent name is skipped. Two trait
impls providing the same mangled name still collide, which is what surfaces
ambiguity errors.

**Analyzer** (`src/semantics/analyzer/expr.rs`). In the `FunctionCall` arm,
when `clean_name` contains a `.`, the analyzer looks up the receiver's type
and dispatches through the tiers in §Method resolution precedence. The
inherent tier builds the mangled name `Type_method` and looks it up in
`self.functions`. The trait tier uses `resolve_trait_method`. The builtin
tier looks up `Type.method`.

**IR builder** (`src/semantics/builder/values.rs::resolve_method_call`,
`src/semantics/builder/expr.rs`). The builder's `resolve_method_call`
returns the mangled name; the caller wraps the receiver in `BorrowShared`
or `BorrowMutable` to match the declared self mode, then emits
`Call { function: <mangled>, args: [receiver, ...args] }`.

## IR representation

**None specific to methods.** After the builder's dispatch, the IR contains
only `TypedIRValue::Call`, `TypedIRValue::BorrowShared`,
`TypedIRValue::BorrowMutable`, and `TypedIRValue::Variable`. There is no
method-call variant in `TypedIRValue` or `Instruction`.

The critical test is
`tests/ir/borrow_deref_addrof_test.rs::test_method_call_desugars_to_function_call`,
which asserts that a method call produces an `Instruction::Call` with the
mangled callee name and the receiver as the first argument.

## Backends

| Backend | Support | Notes |
|---|---|---|
| Interpreter | Yes | All three receiver modes run end-to-end. `&mut self` write-through uses copy-in/copy-out — see ADR 0035. |
| LLVM | Not yet | Records are refused by the capability check. Method IR is correct, but the backend cannot lower record literals or field access. |
| WASM | Not yet | Same as LLVM, plus the WASM backend additionally refuses references. |

For read-only methods on the interpreter, the reference wrap is treated as
a pass-through: `BorrowShared { expr }` evaluates to `expr`'s value. This
is the correct semantics for a memory-model-free interpreter. See the
interpreter's capability matrix, which now declares `Feature::References`.

**Write-through-`&mut self`** requires either:
- teaching the interpreter a `Ref(name)` runtime value that `FieldAssign`
  writes through, or
- adding LLVM record support so mutating methods lower to real memory.

Neither is in scope for ADR 0033. See §Known limitations.

## Diagnostics

Currently emitted codes for methods:

| Code | Trigger |
|---|---|
| E0002 | Method arity mismatch, argument type mismatch, method on forbidden type. |
| E0007 | `&mut self` on a `val` binding, or use-after-consume of a `self: T` receiver. |
| E0010 | Ambiguous method: two or more traits provide a method with the same name for the same target type, and no inherent impl shadows them. The message names the traits. |
| E0011 | No such method: the receiver's type has no method with that name. The message lists the methods the type does have, capped at six with an `(and N more)` suffix. |

E0010 fires before `register_user_functions`, using the
`(target_type, method_name, trait_name)` triple from the AST. This
replaces the previous behavior, where two traits with the same method
name surfaced as `Duplicate function name 'User_save'` — the mangled
symbol, not the source-level collision.

E0011 covers the FunctionCall `x.method()`, MethodCall (complex
receiver), and FieldAccess `x.method` (zero-arg) forms. The candidate
list is derived from the `{Type}_{method}` mangling convention, so
inherent and trait methods both appear.

## Safety

Method syntax has no safety-relevant behavior of its own. All ownership
and borrow rules apply to the receiver exactly as they apply to any other
binding:

- `self: T` moves the receiver; a subsequent use is a use-after-move error.
- `self: &T` creates a shared borrow; the borrow checker enforces
  exclusivity against other borrows.
- `self: &mut T` creates an exclusive borrow; the receiver must be `var`,
  and no other borrow may be live.

The one invariant worth naming: the analyzer does not special-case
methods in its ownership model. `u.rename("Alicia")` and
`User_rename(&mut u, "Alicia")` produce identical analyzer behavior.
The `method_matches_free_function.gol` fixture (deferred, backend-blocked)
is the load-bearing test for this.

## Test coverage

Conformance fixtures, `tests/conformance/valid/methods/`:

- `method_shared_borrow.gol` — `self: &T` returns a field. Prints `Alice`.
- `method_consuming_receiver.gol` — `self: T` returns a field. Prints `Alice`.
- `method_on_enum.gol` — `impl Day`. Prints `true`.
- `method_on_nominal_type.gol` — `impl UserId`. Prints `user`.
- `method_resolution_inherent_precedence.gol` — inherent shadows trait.
  Prints `inherent`.
- `trait_self_substitution.gol` — `self: &Self` in trait, `self: &User`
  in impl. Prints `saved`.

Conformance fixtures, `tests/conformance/invalid/`:

- `method_field_collision_rejected.gol` — E0002.
- `method_self_outside_impl_rejected.gol` — E0002.
- `method_consuming_then_use_rejected.gol` — E0007.
- `method_mut_conflict_rejected.gol` — E0007.
- `method_ambiguous_trait_rejected.gol` — E0002.
- `method_on_wrong_type.gol` — E0004.

Unit and integration coverage:

- `tests/ir/borrow_deref_addrof_test.rs::test_method_call_desugars_to_function_call`
- `tests/differential/differential_true.rs::test_differential_method_syntax_no_parens`
- `tests/differential/differential_true.rs::test_differential_method_syntax_parens_matches_bare`
- `src/semantics/trait_registry/tests.rs` — trait validation with `Self`
  substitution.

### Gaps

- **No test for a method call on a complex receiver** (`f().method()`,
  `arr[0].method()`). The parser currently rejects non-`Var` receivers
  in `parse_postfix` with an explicit error. See §Known limitations.
- **No test for chained method calls** (`s.to_upper().length`). The same
  parser restriction applies.
- **No differential test for a user-defined inherent method.** The
  differential suite tests builtin method syntax only.
- **No test that `u.rename("Alice")` and `rename(&mut u, "Alice")`
  produce identical output.** Blocked on write-through-`&mut self`.

## Maturity

Following the stages in `docs/architecture-direction.md`:

```
Methods on non-generic user types
    semantics:   Stable
    parsed:      yes
    typed:       yes (as a call)
    validated:   yes (receiver ownership, precedence, ambiguity, collision)
    IR:          N/A (desugared to Call with a wrapped receiver)
    verified:    yes (as a normal call)
    interpreter: partial (read-only receivers; write-through &mut self
                 is not modeled)
    LLVM:        blocked on record support
    WASM:        blocked on record support
    optimized:   N/A
```

## Known limitations

One item remains deferred as of ADR 0033:

**1. Methods on non-interpreter backends.** Records and references are
refused by LLVM and WASM at the capability boundary, so no method call
can compile to native or WASM code today. Interpreter support is
complete, including write-through `&mut self` (ADR 0035) and generic
impls (ADR 0034). Closing the backend gap is ADR 0036 (LLVM records)
and ADR 0037 (WASM records and references).

## Checklist for related features

If you are adding a feature *like* methods (a new kind of
receiver-bearing declaration), you need to touch:

1. `src/frontend/ast.rs` — `FunctionDecl.receiver` field, and the
   `ReceiverMode` enum if you need new modes.
2. `src/frontend/parser/items.rs` — parsing for the new declaration form.
3. `src/compiler.rs::expand_impl_methods` — flattening into top-level
   `FunctionDecl`s with the mangled name.
4. `src/semantics/analyzer/mod.rs::analyze_with_spans` — registration
   checks (field/method collision, forbidden owners, ambiguity).
5. `src/semantics/analyzer/expr.rs` — resolution tier in the
   `FunctionCall` arm.
6. `src/semantics/builder/values.rs::resolve_method_call` — the owner-type
   match must stay in sync with the analyzer's `owner_name` match.
7. `src/semantics/builder/expr.rs` — receiver wrapping by declared mode.
8. Backend capability scans — the interpreter and LLVM capability matrices
   may need `Feature::References`, `Feature::Records`, etc.
9. Conformance fixtures under `tests/conformance/valid/` and `invalid/`.
10. `docs/features/<feature>.md` — this file.

The two matches in step 6 are the load-bearing invariant: the analyzer's
`owner_name` and the builder's `user_type_name` must agree on which type
forms can own methods. There is a cross-reference comment at each site.

## Open questions

Resolved in the current implementation, but worth recording for
posterity:

- Bare method syntax (`x.foo` without parens) is valid only for
  zero-argument methods. `x.foo(y)` requires the parens. This is a
  deliberate convention, not an oversight.
- Field access and method calls are distinguished by the receiver's
  type. Records with a field of the given name resolve to
  `FieldAccess`; everything else falls through to the method
  dispatch path.

Still open, deferred to future work:

- **Method calls reorderable?** `foo(x, y)` and `x.foo(y)` produce
  the same IR. Whether the language should prefer one form is a
  style question, not a semantic one.
- **Dedicated "method not found" error.** Currently an unknown method
  produces the generic "Type X does not have method 'Y'" from the
  inherent tier, or a plain call error elsewhere. A message naming
  the receiver type's available methods would be more helpful. Tier
  2 diagnostic-quality item.
- **Method syntax for operators.** `x.+(y)`. Currently no — operators
  are a separate parsing tier. Unifying them is a design question.
- **Dotted module paths.** If the language adds `module.function()`
  syntax for qualified imports, the parser must disambiguate between
  "method call on a module value" and "qualified lookup". No modules
  exist yet; when they do, this needs resolving.

## Checklist for method-like syntax features

If you are adding a syntax feature that desugars to an existing
construct (like method calls do to function calls), the touch points
are:

1. `src/frontend/lexer/` — new tokens, if any (method call reuses `.`).
2. `src/frontend/parser/` — the surface-form parsing rule.
3. `src/frontend/ast.rs` — an AST node for the surface form, if the
   existing `FunctionCall` shape doesn't suffice.
4. `src/semantics/analyzer/` — type checking against the desugared
   construct.
5. `src/semantics/builder/` — the desugaring site.
6. `tests/ir/` — a test asserting the IR shape produced.
7. `tests/differential/` — a test asserting equivalent surface forms
   produce identical output.
8. `docs/features/<feature>.md` — the contract file.

This is a narrower checklist than the section above, which covers
receiver-bearing declarations. Pick the one that matches your new
feature.

## See also

- `docs/decisions/0033-methods-and-self.md` — the ADR for this feature.
- `docs/features/trait.md` — trait method dispatch.
- `docs/features/generic.md` — generic function monomorphization, which
  generic impls would extend.
- `docs/features/record.md` — the data type methods attach to.
- `docs/features/nominal_types.md` — `Type::Distinct` as a method owner.
- `docs/features/enum_types.md` — `Type::Enum` as a method owner.
- `docs/decisions/0005-ownership-model.md` — the three receiver modes.
- `docs/decisions/0029-nominal-types.md` — nominal identity, relevant to
  mangling.
- `src/compiler.rs` — `expand_impl_methods`, the mangling site.
- `src/semantics/analyzer/expr.rs` — the resolution tiers.
- `src/semantics/builder/values.rs::resolve_method_call` — the IR-side
  lookup.
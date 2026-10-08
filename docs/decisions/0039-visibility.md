# ADR 0039 — Visibility Modifiers

## Status

Accepted. Implemented on all three backends (interpreter, LLVM, WASM).

> **Status note (2026-10-06).** Every declaration in ALGOL26 is
> currently visible everywhere. This ADR proposes adding visibility
> modifiers with a default of private and a scope rule tied to the
> module system. The implementation touches every declaration form
> and the whole test suite, so it needs a design pass before code.

## Context

ALGOL26 has a module system but no visibility on any declaration.
Any function, record, trait, enum, or nominal type is accessible
from anywhere once its module is imported.

This works while the language is small. It stops working when:

- **A library wants internal helpers.** Utility functions that
  structure the implementation but are not part of the public
  API become part of the API anyway.
- **Records need invariants.** A record's fields can be mutated
  freely by any code that can construct the record.
- **Two modules want to share a name.** The consumer has no way
  to hide one.

The language has no answer to what should be private. Every
systems language added visibility modifiers eventually. The
question is not whether but what shape.

## Decision

ALGOL26 will support **item-level visibility** with a default
of private and a `pub` keyword for opt-in public API.

### The keyword

`pub` is the modifier. It is placed before the declaration
keyword:

```gol
pub function exported_helper() -> Int
    return 42

function internal_helper() -> Int
    return 0
```

### Default is private

A declaration without `pub` is private to its module. This is
the Rust, Swift, and Kotlin default. It makes the safe case
(internal helper) the syntactic default and forces the developer
to think about what part of the API is exported.

### Scope is the file

For v1, privacy is scoped to the file. A `.gol` file is a module.
Everything in it can see everything else. `import "foo.gol"`
makes foo's public items visible to the importer; private items
stay invisible.

Declared modules (grouping multiple files under one module) are
deferred to a future ADR.

### What can be public

Every top-level declaration can carry `pub`:

- `function f` — free function visibility
- `procedure p` — same
- `rec R` — the record type is public; field visibility is separate
- `enum E` — all variants public if the enum is
- `type T distinct Base` — public nominal type
- `type T Base in Lo..Hi` — public subrange
- `trait Tr` — all trait methods are public
- `impl` block — not applicable; impls attach to types
- record field — per-field `pub`
- trait method — always public
- impl method — `pub` or default-private

### Record fields

A record's fields are **private by default**, even when the
record is public:

```gol
pub rec Wallet
    pub balance: Int
    owner_id: UserId          // private
```

External code can read/write `balance` but cannot see or touch
`owner_id`. The record's own methods in its `impl` can access
both.

### Trait methods are always public

A trait method is part of the trait's interface by definition.
It cannot carry `pub` or a private modifier. A trait impl's
method that satisfies a trait method inherits the trait's
public visibility.

### Inherent impl methods can be private

An inherent method with no trait is a normal item:

```gol
impl Wallet
    pub function balance(self: &Wallet) -> Int
        return self.balance

    function recompute_owner(self: &mut Wallet)
        // private; only callable within this module
```

The rule: an impl method is private unless declared `pub`,
matching the default-private rule for declarations.

### Enforcement point

The analyzer. The parser accepts `pub` syntactically and stores
it on the AST. Resolution-time checks reject cross-module access.
The analyzer already tracks which module each item belongs to;
the visibility check augments the existing name-resolution path.

This keeps the parser free of semantic knowledge and centralizes
the rule in one place.
## Open questions

**Q1. Which scoped-visibility forms are needed in v1?** Rust
has `pub(crate)`, `pub(super)`, `pub(in path)`. For v1, only
`pub` (public to anything that imports the module) and the
absence of `pub` (private to the file). Scoped forms are
deferred.

**Q2. Interaction with the module loader.** If two files both
import c, do they share c's private items? Under the v1 rule,
no — private means private to c.

**Q3. Diagnostic design.** An access to a private item should
produce a message naming the item, its defining module, and the
accessing module. A dedicated error code is warranted.

**Q4. `pub` on methods inside `impl Trait for T`.** These
satisfy trait methods and are public by definition. v1 rejects
a redundant `pub` there to keep the model clean.

**Q5. Interaction with distinct and subrange declarations.**
A `pub type UserId distinct Int` exports the type name and also
the `from_base` and `to_base` intrinsics, which are part of
the type's surface.

**Q6. AST ripple.** Every declaration AST node gains a
visibility field: `FunctionDecl`, `RecordDecl`, `TraitDecl`,
`EnumDecl`, `DistinctDecl`, `SubrangeDecl`, and the record
field tuple. Every construction site updates; the same
mechanical wave as `receiver: Option<ReceiverMode>` from
ADR 0033.

**Q7. Test suite impact.** The existing test suite mostly has
single-file programs, so most declarations can stay private
without breaking tests.

## Scope boundary

**In scope for v1:**

- `pub` keyword on top-level declarations and record fields.
- Default private, scoped to the file.
- Trait methods always public; inherent impl methods respect
  the default-private rule.
- Analyzer enforces; parser records.
- Cross-module access rejection with a dedicated error.

**Explicitly out of scope:**

- Scoped visibility (`pub(crate)`, `pub(super)`, `pub(in path)`).
- `private` / `protected` / `internal` synonyms.
- Package-level visibility.
- Friend declarations.
- Declared modules grouping multiple files.
- Reflection-based access bypass. There is no reflection.
## Alternatives considered

### Default public (Go, Python, JavaScript)

**Rejected.** Default-public makes the *unsafe* case (leaking
internal implementation) the syntactic default. Systems
languages have converged on default-private.

### `export` instead of `pub`

**Rejected.** `export` is a verb; it reads like an action rather
than a modifier. `pub` reads as an adjective, which is what a
modifier should be.

### `public` (Java, C#, C++)

**Rejected.** Four characters vs. three; the same meaning.
ALGOL26 has favored short keywords. `pub` matches that aesthetic.

### `open` (Swift)

**Rejected.** `open` in Swift means subclassable and overridable
outside the module — a distinction that collapses when the
language has no inheritance.

### Per-field visibility with a separate `private` keyword

**Rejected.** A separate `private` keyword doubles the modifier
vocabulary for no benefit. Default-private plus `pub` for the
exceptions is simpler.

### Enforce at parse time instead of analysis time

**Rejected.** The parser does not know which module an item is
being accessed from; that is a resolution-time question.

### File-level vs. declared modules

**v1 chooses file-level.** Declared modules are more expressive
but require the module loader to grow a declaration syntax,
which is a separate design question.

## Implementation order

```
V1. AST. Add visibility field to every declaration node and to
    record fields. Every construction site defaults to Private.

V2. Parser. Accept `pub` before declaration keywords. Parse
    per-field `pub` inside record bodies. Reject `pub` on trait
    methods and impl methods that satisfy trait methods.

V3. Analyzer. Track each item's defining module. At name
    resolution, reject cross-module access to private items.

V4. Fixtures. Valid: exported function, exported record with
    private field, private method used within module. Invalid:
    access private function from another module, access private
    field from outside impl, attempt `pub` on trait method.

V5. Docs. Feature doc at docs/features/visibility.md.

V6. Every existing test that constructs a declaration AST
    literal gets the field. Mechanical.
```

V3 is the interesting step. V1 and V2 are a mechanical wave;
V4 and V5 are fixtures and documentation; V6 is the
compiler-driven update across the test suite.

## Consequences

### Positive

- Encapsulation becomes expressible. A record's invariants can
  be enforced by making fields private and exposing only
  constructor and accessor methods.
- Libraries can hide implementation details.
- The default (private) matches systems-language convention.
- The analyzer's existing module-tracking infrastructure does
  most of the work.

### Negative

- Every existing declaration gains an implicit visibility, and
  every AST construction site updates. Broad but mechanical.
- Programs that were public-by-default now need `pub` on their
  exported surface. Source-breaking but early enough to be
  cheap.
- Trait method visibility rules have edge cases.
- Diagnostics need new error codes.

### Neutral

- The IR and backend codegen are unchanged. Visibility is a
  compile-time concept; nothing about it reaches the runtime.
- The trait registry is unchanged.
- Generic machinery is unchanged.

## References

- Rust's `pub` keyword and module system — the mental model.
- Swift's `public` / `internal` / `fileprivate` — the tiered
  approach this ADR simplifies to two tiers.
- Go's exported-via-capitalization — the alternative default
  this ADR rejects.
- src/frontend/module_loader.rs — the module resolution
  infrastructure the enforcement step builds on.
- src/semantics/analyzer/scopes.rs — the name-resolution
  path the visibility check augments.

## See also

- docs/features/methods.md — the method feature; inherent
  method visibility is affected.
- docs/features/record.md — the record feature; field
  visibility is added by this ADR.
- docs/decisions/README.md — the ADR index and convention.

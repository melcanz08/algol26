# ADR 0024: `rec` — structured data types

Status: Accepted (interpreter complete; LLVM/WASM refused)

## Context

ALGOL26 has no structured data type. `ExprKind::FieldAccess` was
previously rejected at analysis:

    Field access '.{field}' is not supported yet

Programs that want to group related values must use parallel
lists (`names: List<String>`, `ages: List<Int>`) or pass many
arguments to functions. Both are workarounds, not data modeling.

## Decision

Add `rec` declarations:

    rec Point
        x: Int
        y: Int

The keyword is `rec` (lowercase in source), named after ALGOL W's
`RECORD`. ALGOL W's record was the first structured-data feature
in the ALGOL family; ALGOL26's version extends it with traits,
generics, pattern matching, and region attribution.

### Syntax

Declaration:

    rec Point
        x: Int
        y: Int

    rec Person
        name: String
        age: Int

Generic:

    rec Pair<T>
        first: T
        second: T

Construction:

    val origin := Point { x: 0, y: 0 }
    val pr := Pair<Int> { first: 1, second: 2 }

Field access:

    print(p.x)
    p.x := 5         // requires `var p`

Pattern matching (extends existing `match`):

    match p
        case Point { x, y }
            print(x + y)

Traits and methods through the existing `impl` form:

    impl Show for Point
        function show(self: &Point) -> String
            return "(" + to_string(self.x) + ", " + to_string(self.y) + ")"

    procedure main
        val p := Point { x: 1, y: 2 }
        print(p.show())

### Semantics

**Value semantics, structural `Copy` (deferred).** A record is
`Copy` iff every field is `Copy`. `Point { x: Int, y: Int }` should
be `Copy`; `Person { name: String, age: Int }` should not. The v1
interpreter ships with records treated as move-only — `is_copy`
does not yet consult the record field table. A follow-up wires the
analyzer's record table into the ownership decision.

**Direct field mutation with `var`.** `p.x := 5` is legal when
`p` is a `var`. `val p` rejects it with the existing "cannot
assign to immutable" diagnostic.

**No `Drop`.** Records have no cleanup hook. A record containing
heap-allocated data leaks under the LLVM backend, exactly as a
bare `String` does today. The interpreter does not leak because
its runtime values are Rust-owned. A future ADR can add `Drop`
for the language as a whole; records are not the place to
introduce it.

**Region attribution is automatic.** A record declared inside a
`region` block belongs to that region, via the existing
`SemanticState::declare` mechanism. No new code needed.

### Backends

| Backend     | Support                                                        |
|-------------|----------------------------------------------------------------|
| Interpreter | Yes — `RuntimeValue::Record { name, fields }`                  |
| LLVM        | Refused — capability check (`Feature::Records`)                |
| WASM        | Refused — capability check (`Feature::Records`)                |

LLVM support is a follow-up ADR. It requires a struct layout
decision, stack vs heap allocation, GEP-based field access, and
the interaction of structural `Copy` with LLVM's `memcpy`.
That is a week of work on its own.

### Type representation

`Type::Record(String, Vec<Type>)`. Distinct from
`Type::Generic`, which stays as-is. `Point` becomes
`Type::Record("Point", [])`; `Pair<Int>` becomes
`Type::Record("Pair", [Int])`.

Mangling: `mangled_type_name` gains a `Record` arm; injective
by construction (`Record_<name>_<arity>_<args...>`).

### AST representation

New `RecordDecl` alongside `FunctionDecl`, `TraitDecl`,
`ImplBlock`:

    pub struct RecordDecl {
        pub name: String,
        pub type_params: Vec<String>,
        pub fields: Vec<(String, TypeSyntax)>,
        pub span: Span,
    }

New expression form:

    ExprKind::RecordLiteral {
        name: String,
        type_args: Vec<TypeSyntax>,
        fields: Vec<(String, Expr)>,
        span: Span,
    }

New statement form:

    Stmt::FieldAssign {
        target: String,
        field: String,
        value: Expr,
        span: Span,
    }

New pattern form:

    Pattern::Record { name: String, bindings: Vec<String> }

### IR representation

New `TypedIRValue::Record { name, fields, record_type }`.

Field reads reuse `TypedIRValue::FieldAccess { object, field, field_type }`,
which already existed in the IR.

New `Instruction::FieldAssign { target, field, value }` for
`p.x := v` in statement position.

New `SemanticPattern::Record { name, bindings }` for `match`
lowering.

### Capability

New `Feature::Records`. Interpreter claims it; LLVM and WASM do
not. The scanner fires on any `TypedIRValue::Record`,
`TypedIRValue::FieldAccess`, or `Instruction::FieldAssign`.

### What is deliberately not in v1

- **Sum types.** A second data model. Deserves its own ADR.
- **Field-level borrows** (`&p.x`). Requires `Place` to become
  more than a variable name. Follow-up.
- **Auto-derived traits** (`impl Show for Point` synthesized from
  fields). Needs a derive subsystem. Follow-up.
- **Nested record destructuring in `match`** beyond one level.
  Extends a feature that already exists.
- **Runtime reflection.** Not a direction the language is going.
- **Structural `Copy` for records.** v1 treats records as
  move-only. The analyzer's record table has everything needed;
  wiring it into `is_copy` is a small follow-up.

## Consequences

**Positive.**

- Programs can define their own structured data. The parallel-list
  workaround disappears.
- Records integrate with everything ALGOL26 already has: traits,
  generics, pattern matching, regions, `Option`/`Result`.
- The type system's value semantics extend naturally. No new
  ownership model.

**Negative.**

- LLVM support is deferred. Records are interpreter-only until a
  follow-up lands.
- Records containing heap data leak under LLVM. This is a
  pre-existing LLVM gap, not new, but records make it more
  visible.
- Field-level borrows are not available; `&p` borrows the whole
  record. Conservative but restrictive.
- Structural `Copy` is not yet implemented; all records move.

**Neutral.**

- `Type::Generic` and `Type::Record` coexist. `Generic` is
  currently used in a few builtin signatures; `Record` is new
  territory.

## Tests

Interpreter v1 ships with:

- `record_declaration_parses` — syntax
- `record_literal_construction` — `Point { x: 1, y: 2 }`
- `field_access_reads_value` — `p.x`
- `field_access_assigns_when_var` — `p.x := 5`
- `field_access_rejects_immutable` — `val p; p.x := 5` errors
- `record_pattern_match_destructures` — `case Point { x, y }`
- `record_in_list` — `List<Point>`
- `record_inside_region` — free via region exit
- Capability: `interpreter_accepts_records`, `llvm_rejects_records`,
  `wasm_rejects_records`

Structural `Copy` tests (`record_is_copy_when_all_fields_copy`,
`record_is_not_copy_when_field_is_heap`) are deferred along with
the feature itself.

## See also

- `docs/decisions/0003-type-system.md`
- `docs/decisions/0005-ownership-model.md`
- `docs/decisions/0007-region-memory.md`
- ALGOL W `RECORD` design (Niklaus Wirth, 1966)
- Rust `struct` semantics for the `Copy`/move model

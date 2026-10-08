# Visibility (`pub`)

## Summary

ALGOL26 declarations are private to their file by default. A
leading `pub` keyword exports the declaration. Privacy is
enforced by the analyzer at every name-resolution site that
crosses a module boundary.

## Surface syntax

```algol26
pub function exported() -> Int
    return 42

function internal() -> Int        // private to this file
    return 0

pub rec Wallet
    pub balance: Int              // visible to importers
    owner_id: Int                 // private, even though
                                  // the record is public
```

## Rules

- Default is private. A `.gol` file is a module; every
  declaration in it is visible to every other declaration in the
  same file.
- `pub` on a top-level declaration makes it visible to any file
  that imports the declaring file.
- `pub` is allowed on: `function`, `procedure`, `rec`, `enum`,
  `type` (both `distinct` and `subrange` forms), `trait`, and
  record fields.
- Trait methods are public by definition; a redundant `pub` is a
  parse error.
- A trait impl method is public by definition; a redundant `pub`
  is a parse error.
- An inherent impl method respects the default-private rule and
  may carry `pub`.
- `pub` on `impl` and `import` is a parse error: visibility is a
  property of the item an impl attaches to, and imports are not
  exported.

## Enforcement point

The analyzer. Every declaration carries a `Visibility` and a
canonical `module` path. The parser records both; the analyzer
compares the current function's module against the item's module
at every cross-module access:

- **Free function call.** `ExprKind::FunctionCall` after the
  `self.functions` lookup.
- **Record construction.** `ExprKind::RecordLiteral`, both for
  the record type and for each field.
- **Record field read.** `ExprKind::FieldAccess` on a
  `Type::Record` receiver, both for the type and for the field.

The check reads from `FunctionInfo.visibility` /
`FunctionInfo.module` and `RecordInfo.visibility` /
`RecordInfo.module` / `RecordInfo.field_visibilities`.

## Diagnostics

A cross-module access to a private item produces:

```
error[E0013]: function `internal` is private to module `helpers.gol`
  --> main.gol:5:11
   |
 5 |     print(internal())
   |           ^^^^^^^^^^
   |
   = help: Add `pub` to the declaration, or move the access into
           the declaring module
```

Record fields use the same diagnostic with `field` in place of
`function` and `Record::field` as the name.

## Representation

- `Visibility` is an enum with `Public` and `Private` (default).
  It lives on every top-level declaration node and, via
  `RecordInfo::field_visibilities`, on each record field.
- `module: Option<String>` carries the canonical path of the
  file the declaration was parsed from. `None` means the module
  is unknown (builtins, or a program not loaded from a file);
  the check defers when either side is `None`.

## Parser-level rejections

Three placements of `pub` are rejected at parse time, before
any analysis runs:

| Position | Reason |
|----------|--------|
| `pub` on a trait method | Trait methods are public by definition |
| `pub` on a trait impl method | Inherits the trait's public visibility |
| `pub` on `impl` or `import` | Visibility is a property of an item, not an impl; imports are not exported |

## Known limitations (v1)

- **No scoped visibility.** `pub(crate)`, `pub(super)`, and
  `pub(in path)` are not supported. Only two tiers exist: private
  to the file, or public to any importer.
- **No declared modules.** Files are modules. Grouping multiple
  files under a single module is a separate ADR.
- **No `private` keyword.** The default is private; there is no
  explicit modifier for it.
- **Imports and traits do not cross modules in v1.**
  `process_imports` merges functions, records, distincts, enums,
  and subranges; `traits` and `impls` are not yet forwarded.
  This is a pre-existing gap that ADR 0039 does not address.

## Capability

All three backends support visibility identically. It is a
compile-time concept; nothing about it reaches the runtime.

## References

- `docs/decisions/0039-visibility.md` — design rationale and
  scope boundary.
- `docs/features/methods.md` — inherent method visibility is
  affected by this ADR.
- `docs/features/record.md` — field visibility is added by this
  ADR.

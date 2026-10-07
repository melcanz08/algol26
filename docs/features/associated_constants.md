# Associated constants

A **trait** may declare a constant that each impl supplies:

```
trait Sizable
    const SIZE: Int

impl Sizable for Int
    const SIZE: Int := 8
```

Use sites reference the constant as `Type::NAME`. The parser folds
the `::` into a compound identifier (`Var("Int::SIZE")`), the
analyzer resolves it to the declared type, and the IR builder
inlines the impl's value expression at each use.

## Syntax

- **Trait declaration:** `const NAME: Type` on its own line inside
  a trait body. No value - the impl supplies it.
- **Impl definition:** `const NAME: Type := expr` inside an impl
  body. The `:=` operator matches the language's val/var
  binding style; a bare `=` is not a token.
- **Use site:** `Type::NAME`, usable anywhere an expression is
  legal.

## Semantics

- **Required definition.** If the trait declares a constant, every
  impl of that trait must define it. Omission is `E0002`.
- **No extras in trait impls.** An impl of a trait may not define a
  constant the trait did not declare. `E0002`.
- **Inherent impls** (no trait name) may declare constants freely.
- **Type check.** The impl's value expression must coerce to the
  declared type. `E0002` on mismatch.
- **Inlining.** Constants are not stored as runtime values. The
  analyzer records `(declared type, value expression)` keyed by
  `"Type::NAME"`; the IR builder substitutes the expression at
  each use site.

## Not yet supported

- Generic constants, or constants whose value depends on a type
  parameter.
- Default values in the trait declaration.
- Cycle detection; a self-referential constant overflows the stack
  rather than diagnosing.

## Diagnostics

| Code | Trigger |
|------|---------|
| E0002 | Trait declares a constant the impl does not define. |
| E0002 | Impl defines a constant the trait did not declare. |
| E0002 | Impl's value expression type does not match the declared type. |
| E0003 | `Type::NAME` reference where no constant is registered. |

## Tests

- `tests/fixtures/associated_constants.gol` - interpreter, LLVM,
  and WASM produce identical output.
- `tests/conformance/valid/associated_constants/associated_constants.gol`
- `tests/conformance/invalid/associated_constants_missing.gol`

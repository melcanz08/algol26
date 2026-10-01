# ALGOL26 Versioning Policy

**Effective**: v0.8.0

## Three Independent Versions

| Component | Current Version | Meaning |
|-----------|----------------|---------|
| **Language** | 0.5 | Syntax, semantics, type system |
| **Compiler** | 0.8.0 | Implementation of the language |
| **Semantic IR** | IR-1 | Canonical intermediate representation |

## Language Version (0.5)

The language version defines what programs are valid ALGOL26.
Changes to the language version indicate changes to:
- Syntax
- Type system
- Ownership/borrowing rules
- Memory semantics
- Control flow semantics

**Language 0.5 is FROZEN** — see language-freeze.md.

## Compiler Version (0.8.x)

The compiler version tracks implementation changes:
- Bug fixes
- Performance improvements
- Architecture refactoring
- Diagnostics improvements
- Backend improvements

Compiler changes do NOT change what programs mean.

## Semantic IR Version (IR-1)

The IR version tracks changes to the canonical representation.
Backends must target a specific IR version.
When IR changes incompatibly:
- IR version increments
- All backends must be updated
- Old IR is no longer supported

## Version Bumping Rules

| Change Type | Bump |
|-------------|------|
| Bug fix (behavior preserved) | Patch (0.8.x) |
| Architecture improvement | Minor (0.x.0) |
| Language semantics change | Language version |

## Compatibility Guarantee

- A program valid in Language 0.5 MUST compile in all Compiler 0.8.x versions
- A verified SemanticProgram (IR-1) MUST compile in all backends targeting IR-1
- Backend output MUST match Interpreter output for the same program

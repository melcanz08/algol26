# ALGOL26 No-Panic Policy

**Effective**: v0.8.0

## Rule

**User source code must NEVER cause a Rust panic.**

```
Invalid ALGOL26 program
    ↓
CompileError diagnostic
    ↓
Clean error message
```

NOT:

```
Invalid ALGOL26 program
    ↓
Rust panic
    ↓
Stack trace
```

## What IS Allowed to Panic

| Situation | Panic? | Reason |
|-----------|--------|--------|
| Internal invariant violation | Yes | "This should never happen" |
| Invalid IR after verification | Yes | Indicates compiler bug |
| Defensive `unreachable!()` on paths the capability check prevents | Yes | Unreachable by construction |
| Test assertions | Yes | `#[cfg(test)]` only |

## What MUST NOT Panic

| Situation | Must NOT Panic |
|-----------|---------------|
| Invalid syntax from user | Return `CompileError` |
| Type mismatch from user | Return `CompileError` |
| Undefined variable | Return `CompileError` |
| Out of bounds access | Return `CompileError` |
| Missing file | Return `CompileError` |
| Invalid module import | Return `CompileError` |
| Unresolved generic type argument | Return `CompileError` |
| Unknown record name in a signature | Return `CompileError` |
| Capability refusal (unsupported feature on a backend) | Return `CompileError` |

## Enforcement mechanisms

**Fuzz tests** (`tests/fuzz_tests_runner.rs`):

- `test_fuzz_compiler_no_panic` — random byte sequences through the
  full frontend
- `test_fuzz_parser_no_panic` — random token sequences through the
  parser
- `test_fuzz_lexer_no_panic` — random input through the lexer
- `test_fuzz_type_system_no_panic` — random type operations

**Property tests** (`tests/property_tests_runner.rs`):

- `test_compile_no_panic_malformed_source` — structurally malformed
  source produces errors, never panics
- `test_parser_no_panic_random_strings`
- `test_lexer_no_panic_random_strings`
- `test_type_from_str_no_panic`
- `test_type_system_no_panic`

**CI enforcement**:

- `cargo clippy --all-targets -- -D warnings` (in `.github/workflows/ci.yml`)
- The `clippy::unwrap_used` lint is available but not enabled
  crate-wide. Files that use `unwrap()` on infallible operations
  (e.g. `write!` into a `String`) can carry a file-level
  `#![allow(clippy::unwrap_used)]` with a justifying comment.
  See `src/ir/semantic_ir/display.rs` for the pattern.

## Current State

### Fuzz coverage

The fuzz and property tests above run in CI. They exercise the
frontend (lexer, parser, analyzer) with random input and assert
that the result is always a `CompileError`, never a panic.

**The CLI validation programs (2026-09-27 through 2026-10-01)**
exercised the compiler with four real multi-module programs and
produced thirteen latent bugs. Every failure surfaced as a clean
`CompileError` with an error code — no panics. The policy held
across every path exercised.

### Known `panic!` / `unreachable!()` sites

| Location | Kind | Reachability |
|---|---|---|
| `src/ir/cfg_verifier.rs` | internal invariant | Not user-reachable |
| `src/backends/llvm_codegen/instruction.rs` | `unreachable!()` on `Instruction::FieldAssign` | Not user-reachable (records refused by capability check) |
| `src/backends/llvm_codegen/value.rs` | `unreachable!()` on `TypedIRValue::Map` | Not user-reachable (maps refused) |
| `src/backends/llvm_codegen/types.rs` | `unreachable!()` on `Type::Record` / `Type::Map` | Not user-reachable |
| `src/semantics/builder/expr.rs::translate_short_circuit` | `unreachable!()` on non-And/Or | Guarded by match arm |
| `src/semantics/analyzer/mod.rs::declare_var` | `assert!` on duplicate variable | Possible if a user program shadows a name in a scope the analyzer does not model |
| `src/semantics/analyzer/mod.rs::pop_scope` | `assert!` on scope underflow | Internal invariant |
| `src/ir/instantiation_plan.rs::Specialization::new` | `assert_eq!` on arity | Internal invariant; callers construct from matched pairs |
| `src/ir/instantiation_plan.rs::from_instantiations` | `expect()` on non-empty plan | Internal invariant |
| `src/ir/semantic_ir/display.rs` | `unwrap()` on `write!` | Infallible by construction (`String` `Write` impl) |

The defensive `unreachable!()` sites in `llvm_codegen/` are
**intentionally** unreachable: the capability check refuses programs
using records, maps, and `List.append` before codegen runs, so those
arms should never fire. They are a fail-loud safety net, not a
user-facing path. If one of them ever fires, the capability check
has a bug and the panic is the correct response.

### Open items

- **`unwrap()` and `expect()` audit.** No systematic sweep has been
  done. Individual sites are justified by context (see `display.rs`),
  but there is no crate-wide invariant. Turning on the
  `clippy::unwrap_used` lint one module at a time and justifying
  each remaining site would close this.
- **`assert!` in `declare_var`.** The duplicate-variable check uses
  `assert!` rather than pushing a diagnostic. If a user program can
  reach it, the policy requires converting it to a `CompileError`.
  Whether it is reachable depends on whether the analyzer's scope
  model can miss a shadowing case. Not currently tested.
- **Panic sites in the parser.** The parser uses `expect_identifier`
  and `expect_token` which return `Result`, so most paths are
  covered. Any direct indexing of the token stream (`tokens[i]`) is
  a candidate for a bounds panic on truncated input. Not audited.

## Adding a new panic site

A panic added to non-test code should fall into one of two categories:

1. **Internal invariant.** The panic documents a state the compiler
   believes is impossible. The commit message should name the
   invariant and, if relevant, the check that guarantees it.

2. **Defensive refusal.** The panic catches a path the capability
   check or a prior validation pass should have prevented. The panic
   is a fail-loud safety net; the real defense is the earlier check.

If a panic does not fall into one of these, it is a bug: user source
reached a code path the compiler did not anticipate. Replace the
panic with a `CompileError` and add a test that exercises the input.

## See also

- `docs/STATUS.md` — the safety-guarantee table
- `tests/fuzz_tests_runner.rs` — the fuzz suite
- `tests/property_tests_runner.rs` — the property suite
- `.github/workflows/ci.yml` — CI enforcement
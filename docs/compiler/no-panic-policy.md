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
| Internal invariant violation | ✅ Yes | "This should never happen" |
| Invalid IR after verification | ✅ Yes | Indicates compiler bug |
| Test assertions | ✅ Yes | `#[cfg(test)]` only |

## What MUST NOT Panic

| Situation | Must NOT Panic |
|-----------|---------------|
| Invalid syntax from user | Return `CompileError` |
| Type mismatch from user | Return `CompileError` |
| Undefined variable | Return `CompileError` |
| Out of bounds access | Return `CompileError` |
| Missing file | Return `CompileError` |
| Invalid module import | Return `CompileError` |

## Current State

| Category | Status |
|----------|--------|
| Non-test panics | 1 found (cfg_verifier.rs — internal invariant) |
| Test panics | Acceptable |
| Unwrap/expect in non-test | Need audit |

## Enforcement

- Add fuzzing to verify no panics on invalid input
- Add test that feeds random invalid source
- Review every `unwrap()` in non-test code
- Review every `expect()` in non-test code
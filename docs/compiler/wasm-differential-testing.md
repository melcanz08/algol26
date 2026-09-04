# WASM Differential Testing — Status

**Date**: 2026-09-04
**Version**: v0.8.0

## Current State: COMPILATION TESTING ONLY

The WASM backend currently supports **compilation testing**, not **execution testing**.

### What We Test

| Test | Description | Status |
|------|-------------|--------|
| WASM compiles same programs as LLVM | 3 programs (basic, arithmetic, control flow) compile successfully | ✅ |
| WASM backend isolation | WASM backend doesn't import from frontend | ✅ |

### What We DON'T Test (Yet)

| Test | Why Not |
|------|---------|
| WASM execution output | No WASM runtime integration |
| WASM vs LLVM output comparison | `can_execute()` returns `false` |
| WASM vs Interpreter comparison | Interpreter is oracle, but WASM can't run |

---

## Why WASM Execution Testing Is Hard

1. **No WASM runtime installed** — Node.js is available but not integrated
2. **WASM backend is thin** — It wraps LLVM's WASM target but doesn't execute
3. **No `.wasm` file loading** — The compiler generates the file but doesn't run it

---

## What Full WASM Differential Testing Would Look Like

```
                    Test Program
                         │
                ┌────────┼────────┐
                ▼        ▼        ▼
          Interpreter   LLVM     WASM
                │        │        │
                │        │        ├──→ Node.js / wasmtime
                │        │        │
                └────────┼────────┘
                         ▼
                    Compare
                         │
                  ┌──────┴──────┐
                  ▼             ▼
                SAME         DIFFERENT
                  │             │
                 PASS          BUG
```

---

## Requirements for Full WASM Differential Testing

| Requirement | Current Status |
|-------------|---------------|
| WASM backend generates valid `.wasm` | ✅ Works |
| WASM runtime available | ✅ Node.js installed |
| WASM backend can execute | ❌ `can_execute()` returns false |
| Execution produces stdout | ❌ Not implemented |
| Test harness compares outputs | ❌ Not implemented |

---

## Next Steps for WASM Testing

1. **Add WASM execution to backend** — Use Node.js to run `.wasm` files
2. **Capture WASM stdout** — Read output from Node.js process
3. **Extend differential tests** — Compare Interpreter vs LLVM vs WASM
4. **Handle WASM-specific issues** — WASM has no filesystem, no stdin/stdout by default

---

## Current Test Coverage

| Test File | What It Tests |
|-----------|---------------|
| `tests/backends/wasm_backend_test.rs` | WASM backend trait contract (4 tests) |
| `tests/differential/wasm_differential_test.rs` | WASM compiles same programs as LLVM (2 tests) |
| `tests/backends/backend_independence.rs` | Semantic IR is backend-independent (4 tests) |

---

## Honest Assessment

**WASM differential testing is ~30% complete.**

- ✅ Compilation testing: Works
- ✅ Backend isolation: Verified
- ❌ Execution testing: Not implemented
- ❌ Output comparison: Not implemented

This is acceptable for v0.8.0. Full WASM execution testing is future work.
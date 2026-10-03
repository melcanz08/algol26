#!/usr/bin/env python3
"""
A6b: LLVM Cast arm for Type::Subrange.

Subranges lower to their base (Int or enum), both of which are i64
at the LLVM level. The wrap/unwrap is a no-op.
"""

from pathlib import Path

PATH = Path("src/backends/llvm_codegen/value.rs")
src = PATH.read_text()

old = """                    // ADR 0030: enum wrap. The value's LLVM type is
                    // i64 (matching `map_type(Type::Enum)`), so the
                    // cast is a no-op. No runtime bounds check in v1;
                    // the analyzer rejects out-of-range literals.
                    (BasicValueEnum::IntValue(_), Type::Enum { .. }) => v,"""

new = """                    // ADR 0030: enum wrap. The value's LLVM type is
                    // i64 (matching `map_type(Type::Enum)`), so the
                    // cast is a no-op. No runtime bounds check in v1;
                    // the analyzer rejects out-of-range literals.
                    (BasicValueEnum::IntValue(_), Type::Enum { .. }) => v,
                    // ADR 0031: subrange wrap/unwrap. `map_type`
                    // unwraps Subrange to its base, so the LLVM value
                    // is always an i64 for Int and enum bases. Both
                    // directions are no-ops at the LLVM level. The
                    // runtime bounds check is a separate instruction
                    // (BoundsCheck); it fires before this cast on the
                    // construct path.
                    (BasicValueEnum::IntValue(_), Type::Subrange { .. }) => v,"""

n = src.count(old)
if n != 1:
    print(f"FAIL: matched {n} times; expected 1")
    print("-" * 60)
    print(old[:300])
    print("-" * 60)
    raise SystemExit(1)

PATH.write_text(src.replace(old, new, 1))
print("OK: patched src/backends/llvm_codegen/value.rs")

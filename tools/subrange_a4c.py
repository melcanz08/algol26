#!/usr/bin/env python3
"""
A4c: IR-builder intercept for `T(v)` subrange construction.

Same shape as the analyzer's try_subrange_construct and the existing
nominal/enum conversion intrinsics: analyze the argument, wrap in a
Cast that carries the subrange type.
"""

from pathlib import Path

PATH = Path("src/semantics/builder/expr.rs")
src = PATH.read_text()

old = """                // ADR 0029/0030: conversion intrinsics. Each lowers
                // to a no-op Cast that carries the target type.
                if let Some(dot) = clean_name.find('.') {"""

new = """                // ADR 0031: subrange construction. `Percentage(75)`.
                // The callee is a bare identifier, not a dotted name.
                // Emit a Cast that carries the subrange type; the
                // runtime bounds check (A5) will be inserted
                // separately for non-literal arguments.
                if !clean_name.contains('.') {
                    if let Some(subrange) = self.subrange_types.get(clean_name).cloned() {
                        let inner = if let Some(arg) = args.first() {
                            self.translate_expr(program, func, current_block, arg)
                        } else {
                            TypedIRValue::Void
                        };
                        return TypedIRValue::Cast {
                            value: Box::new(inner),
                            target_type: subrange,
                        };
                    }
                }

                // ADR 0029/0030: conversion intrinsics. Each lowers
                // to a no-op Cast that carries the target type.
                if let Some(dot) = clean_name.find('.') {"""

n = src.count(old)
if n != 1:
    print(f"FAIL: matched {n} times; expected 1")
    print("-" * 60)
    print(old[:300])
    print("-" * 60)
    raise SystemExit(1)

PATH.write_text(src.replace(old, new, 1))
print("OK: patched src/semantics/builder/expr.rs")

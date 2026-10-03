#!/usr/bin/env python3
"""
A4b: wrap Int literals in a Cast when the analyzer recorded a
subrange type. Mirrors the explicit Cast that nominal `from_base`
and subrange `T(v)` construction already produce.
"""

from pathlib import Path

PATH = Path("src/semantics/builder/expr.rs")
src = PATH.read_text()

old = "            ExprKind::Int(i, _) => TypedIRValue::Int(*i),"
new = """            ExprKind::Int(i, _) => {
                // ADR 0031: literal coercion into a subrange. The
                // analyzer recorded the subrange type for this
                // expression when the surrounding context demanded
                // it (`val q: Percentage := 50`). Wrap the literal
                // in a Cast so the IR sees the correct type — same
                // shape as the nominal from_base and subrange T(v)
                // intercepts produce.
                match self.type_of_expr(expr) {
                    Some(ty @ Type::Subrange { .. }) => TypedIRValue::Cast {
                        value: Box::new(TypedIRValue::Int(*i)),
                        target_type: ty,
                    },
                    _ => TypedIRValue::Int(*i),
                }
            }"""

n = src.count(old)
if n != 1:
    print(f"FAIL: matched {n} times; expected 1")
    print("-" * 60)
    print(old)
    print("-" * 60)
    raise SystemExit(1)

PATH.write_text(src.replace(old, new, 1))
print("OK: patched src/semantics/builder/expr.rs")

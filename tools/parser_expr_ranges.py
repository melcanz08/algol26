#!/usr/bin/env python3
"""
Widen compound expression spans in src/frontend/parser/expr.rs.

Every `span: start_span,` and `span: ident_span,` in a constructed
ExprKind is replaced with `span: self.span_from(...)`, extending the
range to cover the last consumed token.

Skips:
  - ExprKind::Block   (parse_block_expr — trailing Dedent/End)
  - ExprKind::If      (parse_if_expr — same reason)
  - ExprKind::For     (parse_for_expr — same reason)
  - ExprKind::While   (parse_while_expr — same reason)
  - ExprKind::TryCatch (parse_try_catch_expr — same reason)

Those take their span from the opening keyword only. Extending them
requires tracking the last real (non-dummy) token before consuming
the trailing Dedent; deferred.
"""

import re
from pathlib import Path

PATH = Path("src/frontend/parser/expr.rs")

# Order matters: patterns applied top to bottom.
REPLACEMENTS = [
    # ─── 5 binary-op parsers (BinOp::Or/And/Greater/etc/Add/Mul) ──
    (
        re.compile(
            r"left = Expr::new\(ExprKind::Binary \{\n"
            r"(\s*left: Box::new\(left\),\n"
            r"\s*op: BinOp::\w+,\n"
            r"\s*right: Box::new\(right\),\n"
            r"\s*)span: start_span,\n"
        ),
        r"left = Expr::new(ExprKind::Binary {\n\1span: self.span_from(start_span),\n",
    ),

    # ─── parse_unary: Negate, Not, Deref, MutBorrow, Borrow ──────
    (
        re.compile(
            r"(Ok\(Expr::new\(ExprKind::(?:Unary \{[\s\S]*?op: UnaryOp::\w+,\n\s*expr: Box::new\([^)]+\),\n|Deref \{\n|Borrow \{\n|MutBorrow \{\n)"
            r"\s*expr: Box::new\([^)]+\),\n"
            r"\s*)span: start_span,\n"
        ),
        r"\1span: self.span_from(start_span),\n",
    ),

    # ─── parse_postfix: ArrayAccess, FieldAccess, FunctionCall ───
    (
        re.compile(
            r"expr = Expr::new\(ExprKind::(?:ArrayAccess|FieldAccess|FunctionCall) \{[\s\S]*?\n\s*span,\n\s*\}\);"
        ),
        lambda m: m.group(0).replace("span,\n            });", "span: self.span_from(span),\n            });", 1),
    ),

    # ─── parse_identifier_expr: FunctionCall / ArrayAccess / FieldAccess ─
    (
        re.compile(r"span: ident_span,"),
        r"span: self.span_from(ident_span),",
    ),

    # ─── parse_primary: alloc, free FunctionCall ────────────────
    (
        re.compile(
            r"(ExprKind::FunctionCall \{\n"
            r"\s*name: \"(?:alloc|free)\"\.to_string\(\),\n"
            r"\s*args: vec!\[[^\]]+\],\n"
            r"\s*)span: start_span,\n"
        ),
        r"\1span: self.span_from(start_span),\n",
    ),

    # ─── parse_primary: Some, Ok, Error ─────────────────────────
    (
        re.compile(
            r"(ExprKind::(?:Some|Ok|Error) \{\n"
            r"\s*value: Box::new\(value\),\n"
            r"\s*)span: start_span,\n"
        ),
        r"\1span: self.span_from(start_span),\n",
    ),

    # ─── parse_primary: List literal ────────────────────────────
    (
        re.compile(
            r"Ok\(Expr::new\(ExprKind::List\(elements, start_span\)\)\)"
        ),
        r"Ok(Expr::new(ExprKind::List(elements, self.span_from(start_span))))",
    ),

    # ─── parse_primary: Range (two variants) ────────────────────
    (
        re.compile(
            r"(Ok\(Expr::new\(ExprKind::Range \{\n"
            r"\s*start: Some\(Expr::boxed\(ExprKind::(?:Int|Number)\([^)]+\)\)\),\n"
            r"\s*end,\n"
            r"\s*inclusive: (?:true|false),\n"
            r"\s*)span: start_span,\n"
        ),
        r"\1span: self.span_from(start_span),\n",
    ),

    # ─── parse_record_literal ──────────────────────────────────
    (
        re.compile(
            r"(Ok\(Expr::new\(ExprKind::RecordLiteral \{\n"
            r"\s*name,\n"
            r"\s*type_args,\n"
            r"\s*fields,\n"
            r"\s*)span: start_span,\n"
        ),
        r"\1span: self.span_from(start_span),\n",
    ),

    # ─── parse_map_literal ─────────────────────────────────────
    (
        re.compile(
            r"(Ok\(Expr::new\(ExprKind::MapLiteral \{\n"
            r"\s*key_type,\n"
            r"\s*value_type,\n"
            r"\s*entries,\n"
            r"\s*)span: start_span,\n"
        ),
        r"\1span: self.span_from(start_span),\n",
    ),
]


def main():
    if not PATH.exists():
        print(f"ERROR: {PATH} not found. Run from the repo root.")
        raise SystemExit(1)

    src = PATH.read_text()
    original = src

    for i, (pattern, replacement) in enumerate(REPLACEMENTS, start=1):
        new_src, count = pattern.subn(replacement, src)
        if count == 0:
            print(f"WARN: replacement {i} matched 0 times (pattern may not apply)")
        else:
            print(f"replacement {i}: {count} site(s)")
        src = new_src

    if src == original:
        print("Nothing changed.")
        return

    # Sanity check: expected ~22+ replacements total. If fewer than
    # 15, something is off and the user should inspect the diff.
    total = original.count("span: start_span,") + original.count("span: ident_span,") - \
            src.count("span: start_span,") - src.count("span: ident_span,")
    print(f"total: {total} span field(s) rewritten")

    if total < 15:
        print()
        print("WARNING: fewer replacements than expected (~22).")
        print("Inspect the diff before committing.")

    PATH.write_text(src)
    print(f"wrote {PATH}")


if __name__ == "__main__":
    main()
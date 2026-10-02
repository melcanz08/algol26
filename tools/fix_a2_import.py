from pathlib import Path

PATH = Path("src/frontend/parser/mod.rs")

OLD = """use crate::frontend::ast::{
    BinOp, Expr, ExprKind, ExternDecl, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern, Program,
    RecordDecl, Stmt, TraitDecl, TraitMethod, TypeSyntax, UnaryOp, WhereClause,
};"""

NEW = """use crate::frontend::ast::{
    BinOp, DistinctDecl, Expr, ExprKind, ExternDecl, FunctionDecl, ImplBlock, MatchCaseExpr,
    Pattern, Program, RecordDecl, Stmt, TraitDecl, TraitMethod, TypeSyntax, UnaryOp, WhereClause,
};"""

src = PATH.read_text()
if src.count(OLD) != 1:
    print(f"FAIL: pattern matched {src.count(OLD)} times; expected 1")
    raise SystemExit(1)
PATH.write_text(src.replace(OLD, NEW, 1))
print(f"OK: patched {PATH}")

#!/usr/bin/env python3
"""
A2: parse `type Name Base in Low..High` and thread SubrangeDecl.

- ast.rs:     SubrangeDecl struct; Program.subrange_decls field
- items.rs:   dispatch on peek(pos+2) to split distinct vs subrange;
              new parse_subrange_decl and parse_subrange_bound
- mod.rs:     import SubrangeDecl
- program.rs: AstPayload.subranges field
- tests.rs:   two parser tests
- compiler.rs: threading done by subrange_a2_compiler.py
"""

from pathlib import Path

AST = Path("src/frontend/ast.rs")
ITEMS = Path("src/frontend/parser/items.rs")
MOD = Path("src/frontend/parser/mod.rs")
PROGRAM = Path("src/compiler/program.rs")
TESTS = Path("src/frontend/parser/tests.rs")


def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


# ─── ast.rs ────────────────────────────────────────────────────────
patch(AST, [
    (
        """/// An ordinal enumeration. `enum Day ...` with one variant per
/// indented line. Variants have ordinals 0..N in declaration order.
/// See ADR 0030.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}""",
        """/// An ordinal enumeration. `enum Day ...` with one variant per
/// indented line. Variants have ordinals 0..N in declaration order.
/// See ADR 0030.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}

/// A subrange type. `type Percentage Int in 0..100` or
/// `type WorkDay Day in Monday..Friday`. Bounds are written as
/// expressions so the analyzer can validate their shape (Int
/// literals for Int bases, variant-name `Var` nodes for enum
/// bases) and produce targeted diagnostics. See ADR 0031.
#[derive(Clone, Debug)]
pub struct SubrangeDecl {
    pub name: String,
    pub base: TypeSyntax,
    pub low: Expr,
    pub high: Expr,
    pub span: Span,
}""",
    ),
    (
        """#[derive(Clone, Debug, Default)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<FunctionDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<RecordDecl>,
    pub distinct_decls: Vec<DistinctDecl>,
    pub enum_decls: Vec<EnumDecl>,
}""",
        """#[derive(Clone, Debug, Default)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<FunctionDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<RecordDecl>,
    pub distinct_decls: Vec<DistinctDecl>,
    pub enum_decls: Vec<EnumDecl>,
    pub subrange_decls: Vec<SubrangeDecl>,
}""",
    ),
])

# ─── items.rs ──────────────────────────────────────────────────────
patch(ITEMS, [
    # 1. local vec
    (
        """        let mut records = Vec::new(); // NEW
        let mut distinct_decls = Vec::new();
        let mut enum_decls = Vec::new();

        while !matches!(self.peek(), Token::Eof) {""",
        """        let mut records = Vec::new(); // NEW
        let mut distinct_decls = Vec::new();
        let mut enum_decls = Vec::new();
        let mut subrange_decls = Vec::new();

        while !matches!(self.peek(), Token::Eof) {""",
    ),
    # 2. Dispatch on `type` -> peek(pos+2) to choose distinct vs subrange
    (
        """            } else if matches!(self.peek(), Token::Identifier(s) if s == "type") {
                distinct_decls.push(self.parse_distinct_decl()?);
            } else if matches!(self.peek(), Token::Identifier(s) if s == "enum") {""",
        """            } else if matches!(self.peek(), Token::Identifier(s) if s == "type") {
                // `type Name distinct Base` or `type Name Base in Low..High`.
                // Peek two tokens ahead (past `type` and the name) to
                // disambiguate.
                let after_name = self.tokens.get(self.pos + 2).map(|ti| &ti.token);
                if matches!(after_name, Some(Token::Identifier(s)) if s == "distinct") {
                    distinct_decls.push(self.parse_distinct_decl()?);
                } else {
                    subrange_decls.push(self.parse_subrange_decl()?);
                }
            } else if matches!(self.peek(), Token::Identifier(s) if s == "enum") {""",
    ),
    # 3. Return field + new methods
    (
        """        Ok(Program {
            imports: top_level_imports,
            functions,
            traits,
            impls,
            records, // NEW
            distinct_decls,
            enum_decls,
        })
    }""",
        """        Ok(Program {
            imports: top_level_imports,
            functions,
            traits,
            impls,
            records, // NEW
            distinct_decls,
            enum_decls,
            subrange_decls,
        })
    }

    /// Parse `type Name Base in Low..High`. Consumes the leading
    /// `type` identifier.
    ///
    /// `Base` is a type annotation (`Int` or an enum name). `Low`
    /// and `High` are single atoms — an Int literal or an
    /// identifier — because bounds are restricted to literals and
    /// variant names. Writing a full expression as a bound is
    /// rejected at parse time with a clear message.
    ///
    /// The `distinct` path is chosen by the caller
    /// (`parse_program`) via a two-token lookahead; this method
    /// assumes the distinct form did not match.
    pub(super) fn parse_subrange_decl(&mut self) -> Result<SubrangeDecl> {
        let start_span = self.current_span();

        // Consume `type` (an Identifier, not a reserved keyword).
        self.advance();

        let name = self.expect_identifier("type name")?;
        let base = self.parse_type_syntax()?;

        // `in` is a reserved token (`for x in list`).
        if !matches!(self.peek(), Token::In) {
            return Err(self.error(&format!(
                "expected `in` after base type in subrange declaration for '{}'",
                name
            )));
        }
        self.advance();

        let low = self.parse_subrange_bound()?;
        if !matches!(self.peek(), Token::DotDot) {
            return Err(self.error("expected `..` between subrange bounds"));
        }
        self.advance();
        let high = self.parse_subrange_bound()?;

        Ok(SubrangeDecl {
            name,
            base,
            low,
            high,
            span: self.span_from(start_span),
        })
    }

    /// A single subrange bound. Int literals and identifiers only;
    /// anything else is an error. See `parse_subrange_decl`.
    fn parse_subrange_bound(&mut self) -> Result<Expr> {
        let span = self.current_span();
        match self.advance() {
            Token::IntLit(n) => Ok(Expr::new(ExprKind::Int(n, span))),
            Token::Identifier(name) => Ok(Expr::new(ExprKind::Var(name, span))),
            other => Err(self.error(&format!(
                "subrange bound must be an integer literal or an enum variant name, found {:?}",
                other
            ))),
        }
    }""",
    ),
])

# ─── mod.rs ────────────────────────────────────────────────────────
patch(MOD, [
    (
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprKind, ExternDecl, FunctionDecl, ImplBlock,
    MatchCaseExpr, Pattern, Program, RecordDecl, Stmt, TraitDecl, TraitMethod, TypeSyntax,
    UnaryOp, WhereClause,
};""",
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprKind, ExternDecl, FunctionDecl, ImplBlock,
    MatchCaseExpr, Pattern, Program, RecordDecl, Stmt, SubrangeDecl, TraitDecl, TraitMethod,
    TypeSyntax, UnaryOp, WhereClause,
};""",
    ),
])

# ─── program.rs: AstPayload field ──────────────────────────────────
patch(PROGRAM, [
    (
        """    /// Enum declarations (`enum Name ...`). Consumed by the
    /// analyzer in A3. See ADR 0030.
    pub enums: Vec<crate::frontend::ast::EnumDecl>,
}""",
        """    /// Enum declarations (`enum Name ...`). Consumed by the
    /// analyzer in A3. See ADR 0030.
    pub enums: Vec<crate::frontend::ast::EnumDecl>,
    /// Subrange declarations (`type Name Base in Low..High`).
    /// See ADR 0031.
    pub subranges: Vec<crate::frontend::ast::SubrangeDecl>,
}""",
    ),
])

# ─── tests.rs: two parser tests ────────────────────────────────────
patch(TESTS, [
    (
        """#[test]
fn rejects_empty_enum() {
    let src = "enum Empty\\n";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let err = parser.parse_program().expect_err("should reject");
    assert!(
        err.message.contains("at least one variant"),
        "{}",
        err.message
    );
}""",
        """#[test]
fn rejects_empty_enum() {
    let src = "enum Empty\\n";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let err = parser.parse_program().expect_err("should reject");
    assert!(
        err.message.contains("at least one variant"),
        "{}",
        err.message
    );
}

// ─── Subrange types (ADR 0031 A2) ────────────────────────────────

#[test]
fn parses_subrange_declaration_with_int_bounds() {
    let src = "type Percentage Int in 0..100\\n";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect("parse failed");
    assert_eq!(program.subrange_decls.len(), 1);
    let decl = &program.subrange_decls[0];
    assert_eq!(decl.name, "Percentage");
    assert!(matches!(decl.base, TypeSyntax::Named(ref n) if n == "Int"));
    assert!(matches!(decl.low.kind, ExprKind::Int(0, _)));
    assert!(matches!(decl.high.kind, ExprKind::Int(100, _)));
}

#[test]
fn parses_subrange_declaration_with_enum_bounds() {
    let src = "enum Day\\n    Monday\\n    Sunday\\n\\ntype WorkDay Day in Monday..Sunday\\n";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect("parse failed");
    assert_eq!(program.subrange_decls.len(), 1);
    let decl = &program.subrange_decls[0];
    assert_eq!(decl.name, "WorkDay");
    assert!(matches!(decl.base, TypeSyntax::Named(ref n) if n == "Day"));
    assert!(matches!(&decl.low.kind, ExprKind::Var(n, _) if n == "Monday"));
    assert!(matches!(&decl.high.kind, ExprKind::Var(n, _) if n == "Sunday"));
}""",
    ),
])

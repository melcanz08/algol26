#!/usr/bin/env python3
"""
A2: parse `enum Name ...` and thread EnumDecl through the frontend.

Same shape as A2 for nominal types:
- ast.rs:     EnumDecl struct; Program.enum_decls field
- items.rs:   dispatch on Identifier("enum"); parse_enum_decl method
- mod.rs:     import EnumDecl
- program.rs: AstPayload.enums field
- compiler.rs: ParsedProgram.enums field; threaded through
               parse, process_imports, expand_impl_methods,
               desugar, prepare_frontend, and the AstPayload
               construction sites
- tests.rs:   two parser tests

`enum` is recognized as an identifier, not a keyword, so it stays
usable as a variable name. The analyzer does not consume enums yet —
that is A3.
"""

import re
from pathlib import Path

AST = Path("src/frontend/ast.rs")
ITEMS = Path("src/frontend/parser/items.rs")
MOD = Path("src/frontend/parser/mod.rs")
TESTS = Path("src/frontend/parser/tests.rs")
PROGRAM = Path("src/compiler/program.rs")
COMPILER = Path("src/compiler.rs")


AST_EDITS = [
    # 1. EnumDecl struct after DistinctDecl
    (
        """#[derive(Clone, Debug)]
pub struct DistinctDecl {
    pub name: String,
    pub base: TypeSyntax,
    pub span: Span,
}""",
        """#[derive(Clone, Debug)]
pub struct DistinctDecl {
    pub name: String,
    pub base: TypeSyntax,
    pub span: Span,
}

/// An ordinal enumeration. `enum Day ...` with one variant per
/// indented line. Variants have ordinals 0..N in declaration order.
/// See ADR 0030.
#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<String>,
    pub span: Span,
}""",
    ),
    # 2. Program.enum_decls field
    (
        """#[derive(Clone, Debug, Default)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<FunctionDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<RecordDecl>,
    pub distinct_decls: Vec<DistinctDecl>,
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
}""",
    ),
]


ITEMS_EDITS = [
    # 1. declare the vector in parse_program
    (
        """        let mut records = Vec::new(); // NEW
        let mut distinct_decls = Vec::new();

        while !matches!(self.peek(), Token::Eof) {""",
        """        let mut records = Vec::new(); // NEW
        let mut distinct_decls = Vec::new();
        let mut enum_decls = Vec::new();

        while !matches!(self.peek(), Token::Eof) {""",
    ),
    # 2. dispatch on Identifier("enum")
    (
        """            } else if matches!(self.peek(), Token::Identifier(s) if s == "type") {
                distinct_decls.push(self.parse_distinct_decl()?);
            } else if matches!(""",
        """            } else if matches!(self.peek(), Token::Identifier(s) if s == "type") {
                distinct_decls.push(self.parse_distinct_decl()?);
            } else if matches!(self.peek(), Token::Identifier(s) if s == "enum") {
                enum_decls.push(self.parse_enum_decl()?);
            } else if matches!(""",
    ),
    # 3. return the field
    (
        """        Ok(Program {
            imports: top_level_imports,
            functions,
            traits,
            impls,
            records, // NEW
            distinct_decls,
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
        })
    }

    /// Parse `enum Name` followed by one variant per indented line.
    /// Consumes the leading `enum` identifier. Requires at least one
    /// variant.
    ///
    /// `enum` is not a reserved keyword; it is matched as an
    /// `Identifier`. This mirrors `type` (ADR 0029) so existing
    /// programs that use `enum` as a variable name keep working.
    pub(super) fn parse_enum_decl(&mut self) -> Result<EnumDecl> {
        let start_span = self.current_span();

        // Consume `enum` (an Identifier, not a reserved keyword).
        self.advance();

        let name = self.expect_identifier("enum name")?;

        let mut variants = Vec::new();
        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                let variant = self.expect_identifier("variant name")?;
                variants.push(variant);
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }

        if variants.is_empty() {
            return Err(self.error(&format!(
                "enum '{}' must declare at least one variant",
                name
            )));
        }

        Ok(EnumDecl {
            name,
            variants,
            span: start_span,
        })
    }""",
    ),
]


MOD_EDITS = [
    (
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, Expr, ExprKind, ExternDecl, FunctionDecl, ImplBlock, MatchCaseExpr,
    Pattern, Program, RecordDecl, Stmt, TraitDecl, TraitMethod, TypeSyntax, UnaryOp, WhereClause,
};""",
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprKind, ExternDecl, FunctionDecl, ImplBlock,
    MatchCaseExpr, Pattern, Program, RecordDecl, Stmt, TraitDecl, TraitMethod, TypeSyntax,
    UnaryOp, WhereClause,
};""",
    ),
]


TESTS_EDITS = [
    (
        """#[test]
fn rejects_alias_without_distinct() {
    let src = "type UserId Int\\n";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let err = parser.parse_program().expect_err("should reject");
    assert!(err.message.contains("distinct"), "{}", err.message);
}""",
        """#[test]
fn rejects_alias_without_distinct() {
    let src = "type UserId Int\\n";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let err = parser.parse_program().expect_err("should reject");
    assert!(err.message.contains("distinct"), "{}", err.message);
}

// ─── Enum types (ADR 0030 A2) ────────────────────────────────────

#[test]
fn parses_enum_declaration() {
    let src = "enum Day\\n    Monday\\n    Tuesday\\n    Wednesday\\n";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect("parse failed");
    assert_eq!(program.enum_decls.len(), 1);
    assert_eq!(program.enum_decls[0].name, "Day");
    assert_eq!(
        program.enum_decls[0].variants,
        vec!["Monday", "Tuesday", "Wednesday"]
    );
}

#[test]
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
    ),
]


PROGRAM_EDITS = [
    (
        """pub struct AstPayload {
    pub functions: Rc<Vec<FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    /// Nominal type declarations (`type X distinct Y`).
    /// Carried for the analyzer's A3 wiring; ignored by A2.
    pub distincts: Vec<crate::frontend::ast::DistinctDecl>,
}""",
        """pub struct AstPayload {
    pub functions: Rc<Vec<FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    /// Nominal type declarations (`type X distinct Y`).
    pub distincts: Vec<crate::frontend::ast::DistinctDecl>,
    /// Enum declarations (`enum Name ...`). Consumed by the analyzer
    /// in A3. See ADR 0030.
    pub enums: Vec<crate::frontend::ast::EnumDecl>,
}""",
    ),
]


COMPILER_EDITS = [
    # 1. ParsedProgram gains the field
    (
        """pub struct ParsedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    pub distincts: Vec<crate::frontend::ast::DistinctDecl>,
    pub imports: Vec<String>,
}""",
        """pub struct ParsedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    pub distincts: Vec<crate::frontend::ast::DistinctDecl>,
    pub enums: Vec<crate::frontend::ast::EnumDecl>,
    pub imports: Vec<String>,
}""",
    ),
    # 2. prepare_frontend's final ParsedProgram
    (
        """        let parsed = ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits,
            impls: parsed.impls,
            records: parsed.records,
            distincts: parsed.distincts,
            imports: parsed.imports,
        };""",
        """        let parsed = ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits,
            impls: parsed.impls,
            records: parsed.records,
            distincts: parsed.distincts,
            enums: parsed.enums,
            imports: parsed.imports,
        };""",
    ),
    # 3. expand_impl_methods return
    (
        """        ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
            distincts: parsed.distincts.clone(),
            imports: parsed.imports.clone(),
        }
    }

    fn lex(&self, source: &str) -> Result<LexedProgram> {""",
        """        ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
            distincts: parsed.distincts.clone(),
            enums: parsed.enums.clone(),
            imports: parsed.imports.clone(),
        }
    }

    fn lex(&self, source: &str) -> Result<LexedProgram> {""",
    ),
    # 4. desugar return
    (
        """        crate::ir::loop_desugar::desugar_loops(&mut functions);
        ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
            distincts: parsed.distincts.clone(),
            imports: parsed.imports.clone(),
        }
    }

    fn parse(&self, lexed: LexedProgram) -> Result<ParsedProgram> {""",
        """        crate::ir::loop_desugar::desugar_loops(&mut functions);
        ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
            distincts: parsed.distincts.clone(),
            enums: parsed.enums.clone(),
            imports: parsed.imports.clone(),
        }
    }

    fn parse(&self, lexed: LexedProgram) -> Result<ParsedProgram> {""",
    ),
    # 5. parse return
    (
        """        let program = parser.parse_program()?;
        Ok(ParsedProgram {
            functions: Rc::new(program.functions),
            traits: program.traits,
            impls: program.impls,
            records: program.records,
            distincts: program.distinct_decls,
            imports: program.imports,
        })
    }""",
        """        let program = parser.parse_program()?;
        Ok(ParsedProgram {
            functions: Rc::new(program.functions),
            traits: program.traits,
            impls: program.impls,
            records: program.records,
            distincts: program.distinct_decls,
            enums: program.enum_decls,
            imports: program.imports,
        })
    }""",
    ),
    # 6. process_imports: declare all_enums, merge, and return
    (
        """        let mut all_functions = (*parsed.functions).clone();
        let mut all_records = parsed.records.clone();
        let mut all_distincts = parsed.distincts.clone();
        let mut visited: HashSet<PathBuf> = HashSet::new();""",
        """        let mut all_functions = (*parsed.functions).clone();
        let mut all_records = parsed.records.clone();
        let mut all_distincts = parsed.distincts.clone();
        let mut all_enums = parsed.enums.clone();
        let mut visited: HashSet<PathBuf> = HashSet::new();""",
    ),
    # 7. process_imports: pass all_enums through load_import_recursive + return
    (
        """                &mut all_functions,
                &mut all_records,
                &mut all_distincts,
                &mut visited,
            )?;
        }

        Ok(ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: all_records,
            distincts: all_distincts,
            imports: parsed.imports.clone(),
        })
    }""",
        """                &mut all_functions,
                &mut all_records,
                &mut all_distincts,
                &mut all_enums,
                &mut visited,
            )?;
        }

        Ok(ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: all_records,
            distincts: all_distincts,
            enums: all_enums,
            imports: parsed.imports.clone(),
        })
    }""",
    ),
    # 8. load_import_recursive signature
    (
        """        all_functions: &mut Vec<FunctionDecl>,
        all_records: &mut Vec<RecordDecl>,
        all_distincts: &mut Vec<crate::frontend::ast::DistinctDecl>,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<()> {""",
        """        all_functions: &mut Vec<FunctionDecl>,
        all_records: &mut Vec<RecordDecl>,
        all_distincts: &mut Vec<crate::frontend::ast::DistinctDecl>,
        all_enums: &mut Vec<crate::frontend::ast::EnumDecl>,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<()> {""",
    ),
    # 9. load_import_recursive: merge enums
    (
        """            for d in imported.distinct_decls {
                if !all_distincts.iter().any(|x| x.name == d.name) {
                    all_distincts.push(d);
                }
            }""",
        """            for d in imported.distinct_decls {
                if !all_distincts.iter().any(|x| x.name == d.name) {
                    all_distincts.push(d);
                }
            }
            for e in imported.enum_decls {
                if !all_enums.iter().any(|x| x.name == e.name) {
                    all_enums.push(e);
                }
            }""",
    ),
    # 10. load_import_recursive: pass all_enums into recursive call
    (
        """                    all_functions,
                    all_records,
                    all_distincts,
                    visited,
                )?;
            }
        }

        loader.end_import();""",
        """                    all_functions,
                    all_records,
                    all_distincts,
                    all_enums,
                    visited,
                )?;
            }
        }

        loader.end_import();""",
    ),
]


def patch(path, edits):
    if not path.exists():
        print(f"ERROR: {path} not found.")
        raise SystemExit(1)
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


def patch_astpayload_sites():
    """
    Add `enums: <expr>.enums.clone(),` after every
    `distincts: <expr>.distincts.clone(),` in compiler.rs.

    Uses a regex so it doesn't depend on the exact expression name
    (prep.parsed vs parsed).
    """
    src = COMPILER.read_text()
    pattern = re.compile(
        r"(?P<indent>[ \t]*)distincts: (?P<expr>\w+(?:\.\w+)*)\.distincts\.clone\(\),"
    )

    def repl(m):
        indent = m.group("indent")
        expr = m.group("expr")
        return (
            f"{indent}distincts: {expr}.distincts.clone(),\n"
            f"{indent}enums: {expr}.enums.clone(),"
        )

    new_src, n = pattern.subn(repl, src)
    if n == 0:
        print("FAIL: no distincts.clone() sites found in compiler.rs")
        raise SystemExit(1)
    COMPILER.write_text(new_src)
    print(f"OK: {COMPILER} ({n} AstPayload site(s) patched)")


def main():
    patch(AST, AST_EDITS)
    patch(ITEMS, ITEMS_EDITS)
    patch(MOD, MOD_EDITS)
    patch(TESTS, TESTS_EDITS)
    patch(PROGRAM, PROGRAM_EDITS)
    patch(COMPILER, COMPILER_EDITS)
    patch_astpayload_sites()
    print()
    print("NEXT: cargo build 2>&1 | head -30")
    print("      Expect: ast::EnumDecl not yet in scope, or one missed field")
    print("      Paste the first error if it fails.")


if __name__ == "__main__":
    main()

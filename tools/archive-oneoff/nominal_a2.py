#!/usr/bin/env python3
"""
A2: parse `type X = distinct Y` and carry DistinctDecl through the
frontend.

- ast.rs: new DistinctDecl struct, Program.distinct_decls field
- parser/items.rs: dispatch on `type` at top level, parse_distinct_decl
- compiler/program.rs: AstPayload gains a distincts field
- compiler.rs: ParsedProgram gains a distincts field; threaded
  through parse, process_imports, expand_impl_methods, desugar,
  prepare_frontend, and all five AstPayload construction sites

`type` is NOT reserved as a keyword — the parser matches
Identifier("type"). This avoids breaking any existing program that
uses `type` as a variable name. If it becomes a problem, promoting
it to a keyword is a one-line lexer change.

The analyzer does not consume distinct decls yet — that is A3.
"""

from pathlib import Path

AST = Path("src/frontend/ast.rs")
ITEMS = Path("src/frontend/parser/items.rs")
PROGRAM = Path("src/compiler/program.rs")
COMPILER = Path("src/compiler.rs")


EDITS = {
    AST: [
        # 1. DistinctDecl struct, placed after RecordDecl
        (
            """#[derive(Clone, Debug)]
pub struct RecordDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, TypeSyntax)>,
    pub span: Span,
}""",
            """#[derive(Clone, Debug)]
pub struct RecordDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, TypeSyntax)>,
    pub span: Span,
}

/// A nominal type declaration. `type UserId = distinct Int`.
/// See ADR 0029. The `NominalTypeId` is assigned by the analyzer
/// when it registers this declaration; the AST carries only the
/// syntactic form.
#[derive(Clone, Debug)]
pub struct DistinctDecl {
    pub name: String,
    pub base: TypeSyntax,
    pub span: Span,
}""",
        ),
        # 2. Program.distinct_decls field
        (
            """#[derive(Clone, Debug, Default)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<FunctionDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<RecordDecl>,
}""",
            """#[derive(Clone, Debug, Default)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<FunctionDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<RecordDecl>,
    pub distinct_decls: Vec<DistinctDecl>,
}""",
        ),
    ],

    ITEMS: [
        # 3. parse_program: declare the vector
        (
            """        let mut records = Vec::new(); // NEW

        while !matches!(self.peek(), Token::Eof) {""",
            """        let mut records = Vec::new(); // NEW
        let mut distinct_decls = Vec::new();

        while !matches!(self.peek(), Token::Eof) {""",
        ),
        # 4. parse_program: dispatch on `type`
        (
            """            } else if matches!(self.peek(), Token::Rec) {
                // NEW
                records.push(self.parse_record_decl()?); // NEW
            } else if matches!(""",
            """            } else if matches!(self.peek(), Token::Rec) {
                // NEW
                records.push(self.parse_record_decl()?); // NEW
            } else if matches!(self.peek(), Token::Identifier(s) if s == "type") {
                distinct_decls.push(self.parse_distinct_decl()?);
            } else if matches!(""",
        ),
        # 5. parse_program: add to the returned struct
        (
            """        Ok(Program {
            imports: top_level_imports,
            functions,
            traits,
            impls,
            records, // NEW
        })
    }""",
            """        Ok(Program {
            imports: top_level_imports,
            functions,
            traits,
            impls,
            records, // NEW
            distinct_decls,
        })
    }

    /// Parse `type Name = distinct BaseType`. Consumes the leading
    /// `type` identifier.
    ///
    /// The `distinct` modifier is required. `type X = Y` (a type
    /// alias) is reserved for a future ADR and is rejected here with
    /// a message that names the currently supported form.
    pub(super) fn parse_distinct_decl(&mut self) -> Result<DistinctDecl> {
        let start_span = self.current_span();

        // Consume `type` (an Identifier, not a reserved keyword).
        self.advance();

        let name = self.expect_identifier("type name")?;
        self.expect_token(Token::Assign, "'='")?;

        // `distinct` is required. `type X = Int` is a type alias,
        // not implemented in this ADR.
        match self.peek().clone() {
            Token::Identifier(s) if s == "distinct" => {
                self.advance();
            }
            other => {
                return Err(self.error(&format!(
                    "Expected `distinct` after `=` in type declaration, found {:?}. \
                     Type aliases (`type X = Y`) are not yet supported; \
                     use `type X = distinct Y` (see ADR 0029).",
                    other
                )));
            }
        }

        let base = self.parse_type_syntax()?;

        Ok(DistinctDecl {
            name,
            base,
            span: self.span_from(start_span),
        })
    }""",
        ),
    ],

    PROGRAM: [
        # 6. AstPayload gains a distincts field
        (
            """pub struct AstPayload {
    pub functions: Rc<Vec<FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
}""",
            """pub struct AstPayload {
    pub functions: Rc<Vec<FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    /// Nominal type declarations (`type X = distinct Y`).
    /// Carried for the analyzer's A3 wiring; ignored by A2.
    pub distincts: Vec<crate::frontend::ast::DistinctDecl>,
}""",
        ),
    ],

    COMPILER: [
        # 7. ParsedProgram gains the field
        (
            """pub struct ParsedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    pub imports: Vec<String>,
}""",
            """pub struct ParsedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    pub distincts: Vec<crate::frontend::ast::DistinctDecl>,
    pub imports: Vec<String>,
}""",
        ),
        # 8. run_pipeline_for AstPayload
        (
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;
        Ok(verified)""",
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
            distincts: prep.parsed.distincts.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;
        Ok(verified)""",
        ),
        # 9. prepare_frontend's final ParsedProgram
        (
            """        let parsed = ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits,
            impls: parsed.impls,
            records: parsed.records,
            imports: parsed.imports,
        };""",
            """        let parsed = ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits,
            impls: parsed.impls,
            records: parsed.records,
            distincts: parsed.distincts,
            imports: parsed.imports,
        };""",
        ),
        # 10. type_check_source_for AstPayload
        (
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
        });
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        self.run_type_check_pass(&mut program, &mut ctx)?;""",
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
            distincts: parsed.distincts.clone(),
        });
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        self.run_type_check_pass(&mut program, &mut ctx)?;""",
        ),
        # 11. run_interpreter_with_args AstPayload
        (
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::interpreter(),
        )?;""",
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
            distincts: prep.parsed.distincts.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::interpreter(),
        )?;""",
        ),
        # 12. compile AstPayload
        (
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
        });

        // ADR 0018: one canonical pipeline. All passes run to
        // completion; the target-specific work is below.""",
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
            distincts: prep.parsed.distincts.clone(),
        });

        // ADR 0018: one canonical pipeline. All passes run to
        // completion; the target-specific work is below.""",
        ),
        # 13. compile_to_wasm AstPayload
        (
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::wasm(),
        )?;""",
            """        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
            records: prep.parsed.records.clone(),
            distincts: prep.parsed.distincts.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::wasm(),
        )?;""",
        ),
        # 14. expand_impl_methods return
        (
            """        ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
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
            imports: parsed.imports.clone(),
        }
    }

    fn lex(&self, source: &str) -> Result<LexedProgram> {""",
        ),
        # 15. desugar return
        (
            """        crate::ir::loop_desugar::desugar_loops(&mut functions);
        ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
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
            imports: parsed.imports.clone(),
        }
    }

    fn parse(&self, lexed: LexedProgram) -> Result<ParsedProgram> {""",
        ),
        # 16. parse return
        (
            """        let program = parser.parse_program()?;
        Ok(ParsedProgram {
            functions: Rc::new(program.functions),
            traits: program.traits,
            impls: program.impls,
            records: program.records,
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
            imports: program.imports,
        })
    }""",
        ),
        # 17. process_imports: declare all_distincts, merge, and return
        (
            """        let mut all_functions = (*parsed.functions).clone();
        let mut all_records = parsed.records.clone();
        let mut visited: HashSet<PathBuf> = HashSet::new();""",
            """        let mut all_functions = (*parsed.functions).clone();
        let mut all_records = parsed.records.clone();
        let mut all_distincts = parsed.distincts.clone();
        let mut visited: HashSet<PathBuf> = HashSet::new();""",
        ),
        # 18. process_imports: pass all_distincts through load_import_recursive call
        (
            """                &mut all_functions,
                &mut all_records,
                &mut visited,
            )?;
        }

        Ok(ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: all_records,
            imports: parsed.imports.clone(),
        })
    }""",
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
        ),
        # 19. load_import_recursive signature
        (
            """        all_functions: &mut Vec<FunctionDecl>,
        all_records: &mut Vec<RecordDecl>,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<()> {""",
            """        all_functions: &mut Vec<FunctionDecl>,
        all_records: &mut Vec<RecordDecl>,
        all_distincts: &mut Vec<crate::frontend::ast::DistinctDecl>,
        visited: &mut HashSet<PathBuf>,
    ) -> Result<()> {""",
        ),
        # 20. load_import_recursive: merge distincts
        (
            """            for r in imported.records {
                if !all_records.iter().any(|x| x.name == r.name) {
                    all_records.push(r);
                }
            }""",
            """            for r in imported.records {
                if !all_records.iter().any(|x| x.name == r.name) {
                    all_records.push(r);
                }
            }
            for d in imported.distinct_decls {
                if !all_distincts.iter().any(|x| x.name == d.name) {
                    all_distincts.push(d);
                }
            }""",
        ),
        # 21. load_import_recursive: pass all_distincts into recursive call
        (
            """                    all_functions,
                    all_records,
                    visited,
                )?;
            }
        }

        loader.end_import();""",
            """                    all_functions,
                    all_records,
                    all_distincts,
                    visited,
                )?;
            }
        }

        loader.end_import();""",
        ),
    ],
}


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


def main():
    for p in EDITS:
        if not p.exists():
            print(f"ERROR: {p} not found. Run from the repo root.")
            raise SystemExit(1)

    for path, edits in EDITS.items():
        patch(path, edits)

    print()
    print("NEXT: cargo build (expect clean)")
    print("      cargo test --release")


if __name__ == "__main__":
    main()

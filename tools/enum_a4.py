#!/usr/bin/env python3
"""
A4: enum variant patterns.

- ast.rs:         Pattern::Variant(String) added
- pattern.rs:     uppercase-first identifier -> Pattern::Variant;
                  lowercase-first stays Pattern::Binding
- patterns.rs:    SemanticPattern::Variant { name, ordinal } added
- analyzer:       check_pattern_type handles Variant
- IR builder:     Match translation resolves the ordinal from the
                  matched type's Type::Enum
"""

from pathlib import Path

AST = Path("src/frontend/ast.rs")
PATTERN_PARSER = Path("src/frontend/parser/pattern.rs")
SEM_PATTERNS = Path("src/ir/semantic_ir/patterns.rs")
ANALYZER_EXPR = Path("src/semantics/analyzer/expr.rs")
BUILDER_EXPR = Path("src/semantics/builder/expr.rs")


EDITS = {
    AST: [
        (
            """#[derive(Clone, Debug)]
pub enum Pattern {
    Some(String),
    None,
    Ok(String),
    Error(String),
    Wildcard,
    Binding(String),
    Literal(Expr),
    SomeNested(Box<Pattern>),
    OkNested(Box<Pattern>),
    ErrorNested(Box<Pattern>),
    Guarded {
        pattern: Box<Pattern>,
        condition: Box<Expr>,
    },
    ListDestructure {
        first: Option<Box<Pattern>>,
        rest: Option<Box<Pattern>>,
    },
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Record {
        name: String,
        bindings: Vec<String>,
    },
}""",
            """#[derive(Clone, Debug)]
pub enum Pattern {
    Some(String),
    None,
    Ok(String),
    Error(String),
    Wildcard,
    Binding(String),
    Literal(Expr),
    SomeNested(Box<Pattern>),
    OkNested(Box<Pattern>),
    ErrorNested(Box<Pattern>),
    Guarded {
        pattern: Box<Pattern>,
        condition: Box<Expr>,
    },
    ListDestructure {
        first: Option<Box<Pattern>>,
        rest: Option<Box<Pattern>>,
    },
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Record {
        name: String,
        bindings: Vec<String>,
    },
    /// A bare enum variant match. `case Monday` where the matched
    /// type is `Type::Enum`. No payload, no binding. See ADR 0030.
    Variant(String),
}""",
        ),
    ],

    PATTERN_PARSER: [
        (
            """            Token::Identifier(name) => {
                self.advance();
                // `Name { a, b, c }` → record destructure pattern.
                if matches!(self.peek(), Token::LBrace) {
                    self.advance();
                    let mut bindings = Vec::new();
                    while !matches!(self.peek(), Token::RBrace | Token::Eof) {
                        bindings.push(self.expect_identifier("field binding")?);
                        if matches!(self.peek(), Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect_token(Token::RBrace, "'}'")?;
                    return Ok(Pattern::Record { name, bindings });
                }
                Ok(Pattern::Binding(name))
            }""",
            """            Token::Identifier(name) => {
                self.advance();
                // `Name { a, b, c }` → record destructure pattern.
                if matches!(self.peek(), Token::LBrace) {
                    self.advance();
                    let mut bindings = Vec::new();
                    while !matches!(self.peek(), Token::RBrace | Token::Eof) {
                        bindings.push(self.expect_identifier("field binding")?);
                        if matches!(self.peek(), Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect_token(Token::RBrace, "'}'")?;
                    return Ok(Pattern::Record { name, bindings });
                }
                // ADR 0030: an identifier starting with an uppercase
                // letter in pattern position is an enum variant pattern.
                // Lowercase-first identifiers remain variable bindings.
                // `Some`, `None`, `Ok`, `Error` have their own token
                // kinds and never reach this arm.
                //
                // This is a documented side effect: `case X` for a
                // single-uppercase-letter binding no longer binds a
                // variable. Users write lowercase bindings.
                if name
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
                {
                    return Ok(Pattern::Variant(name));
                }
                Ok(Pattern::Binding(name))
            }""",
        ),
    ],

    SEM_PATTERNS: [
        (
            """#[derive(Debug, Clone, PartialEq)]
pub enum SemanticPattern {
    Some { binding: String },
    None,
    Ok { binding: String },
    Error { binding: String },
    Wildcard,
    Literal(TypedIRValue),
    Record { name: String, bindings: Vec<String> },
}""",
            """#[derive(Debug, Clone, PartialEq)]
pub enum SemanticPattern {
    Some { binding: String },
    None,
    Ok { binding: String },
    Error { binding: String },
    Wildcard,
    Literal(TypedIRValue),
    Record { name: String, bindings: Vec<String> },
    /// Enum variant match. `ordinal` is the variant's index within
    /// the enum, resolved at IR-build time from the matched
    /// expression's `Type::Enum`. The interpreter and codegen
    /// compare `value == ordinal`; the name is carried for display
    /// and diagnostics only. See ADR 0030.
    Variant { name: String, ordinal: i64 },
}""",
        ),
    ],

    ANALYZER_EXPR: [
        (
            """            Pattern::Record { name, .. } => {
                if let Type::Record(n, _) = value_type {
                    if n == name {
                        Ok(())
                    } else {
                        Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "Cannot match pattern '{}' against value of type {}",
                                name, value_type
                            ),
                            ErrorCode::E0002,
                        ))
                    }
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot match record pattern against {}", value_type),
                        ErrorCode::E0002,
                    ))
                }
            }
            _ => Ok(()),
        }
    }""",
            """            Pattern::Record { name, .. } => {
                if let Type::Record(n, _) = value_type {
                    if n == name {
                        Ok(())
                    } else {
                        Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "Cannot match pattern '{}' against value of type {}",
                                name, value_type
                            ),
                            ErrorCode::E0002,
                        ))
                    }
                } else {
                    Err(CompileError::at(
                        self.current_span,
                        &format!("Cannot match record pattern against {}", value_type),
                        ErrorCode::E0002,
                    ))
                }
            }
            Pattern::Variant(name) => match value_type {
                Type::Enum { name: enum_name, variants, .. } => {
                    if variants.iter().any(|v| v == name) {
                        Ok(())
                    } else {
                        Err(CompileError::at(
                            self.current_span,
                            &format!(
                                "no variant '{}' on enum '{}'",
                                name, enum_name
                            ),
                            ErrorCode::E0004,
                        )
                        .with_suggestion(&format!(
                            "Valid variants of '{}': {}",
                            enum_name,
                            variants.join(", ")
                        )))
                    }
                }
                other => Err(CompileError::at(
                    self.current_span,
                    &format!(
                        "variant pattern requires an enum type; matched type is '{}'",
                        other
                    ),
                    ErrorCode::E0002,
                )),
            },
            _ => Ok(()),
        }
    }""",
        ),
    ],

    BUILDER_EXPR: [
        (
            """                        crate::frontend::ast::Pattern::Wildcard => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Binding(_) => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Literal(e) => SemanticPattern::Literal(
                            self.translate_expr(program, func, current_block, e),
                        ),
                        _ => SemanticPattern::Wildcard,
                    };""",
            """                        crate::frontend::ast::Pattern::Wildcard => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Binding(_) => SemanticPattern::Wildcard,
                        crate::frontend::ast::Pattern::Literal(e) => SemanticPattern::Literal(
                            self.translate_expr(program, func, current_block, e),
                        ),
                        crate::frontend::ast::Pattern::Variant(variant_name) => {
                            // ADR 0030: the ordinal is resolved at
                            // IR-build time from the matched type. The
                            // analyzer has already validated that the
                            // name is a variant of the enum, so a miss
                            // here would be an internal error; fall
                            // back to -1 so a would-be bug becomes a
                            // never-matching case rather than a panic.
                            let ordinal = match &matched_type {
                                Type::Enum { variants, .. } => {
                                    variants
                                        .iter()
                                        .position(|v| v == variant_name)
                                        .map(|i| i as i64)
                                        .unwrap_or(-1)
                                }
                                _ => -1,
                            };
                            SemanticPattern::Variant {
                                name: variant_name.clone(),
                                ordinal,
                            }
                        }
                        _ => SemanticPattern::Wildcard,
                    };""",
        ),
    ],
}


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


def main():
    for path, edits in EDITS.items():
        patch(path, edits)
    print()
    print("NEXT: cargo build 2>&1 | head -40")
    print("      Expect: exhaustive-match errors in backend/verifier/display")
    print("      that pattern over SemanticPattern. Paste them.")


if __name__ == "__main__":
    main()

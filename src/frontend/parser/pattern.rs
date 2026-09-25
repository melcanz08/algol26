// src/frontend/parser/pattern.rs

use super::*;

impl Parser {
    pub(super) fn parse_pattern(&mut self) -> Result<Pattern> {
        match self.peek().clone() {
            Token::Some => {
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    if matches!(
                        self.peek(),
                        Token::Some
                            | Token::Ok
                            | Token::Error
                            | Token::None
                            | Token::LBracket
                            | Token::IntLit(_)
                            | Token::FloatLit(_)
                    ) {
                        let nested = self.parse_pattern()?;
                        if matches!(self.peek(), Token::RParen) {
                            self.advance();
                        }
                        return Ok(Pattern::SomeNested(Box::new(nested)));
                    }
                    let var = self.expect_identifier("pattern variable")?;
                    if matches!(self.peek(), Token::RParen) {
                        self.advance();
                    }
                    Ok(Pattern::Some(var))
                } else {
                    let var = self.expect_identifier("pattern variable")?;
                    Ok(Pattern::Some(var))
                }
            }
            Token::None => {
                self.advance();
                Ok(Pattern::None)
            }
            Token::Ok => {
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    if matches!(
                        self.peek(),
                        Token::Some
                            | Token::Ok
                            | Token::Error
                            | Token::None
                            | Token::LBracket
                            | Token::IntLit(_)
                            | Token::FloatLit(_)
                    ) {
                        let nested = self.parse_pattern()?;
                        if matches!(self.peek(), Token::RParen) {
                            self.advance();
                        }
                        return Ok(Pattern::OkNested(Box::new(nested)));
                    }
                    let var = self.expect_identifier("pattern variable")?;
                    if matches!(self.peek(), Token::RParen) {
                        self.advance();
                    }
                    Ok(Pattern::Ok(var))
                } else {
                    let var = self.expect_identifier("pattern variable")?;
                    Ok(Pattern::Ok(var))
                }
            }
            Token::Error => {
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    if matches!(
                        self.peek(),
                        Token::Some
                            | Token::Ok
                            | Token::Error
                            | Token::None
                            | Token::LBracket
                            | Token::IntLit(_)
                            | Token::FloatLit(_)
                    ) {
                        let nested = self.parse_pattern()?;
                        if matches!(self.peek(), Token::RParen) {
                            self.advance();
                        }
                        return Ok(Pattern::ErrorNested(Box::new(nested)));
                    }
                    let var = self.expect_identifier("pattern variable")?;
                    if matches!(self.peek(), Token::RParen) {
                        self.advance();
                    }
                    Ok(Pattern::Error(var))
                } else {
                    let var = self.expect_identifier("pattern variable")?;
                    Ok(Pattern::Error(var))
                }
            }
            Token::LBracket => {
                self.advance();
                if matches!(self.peek(), Token::RBracket) {
                    self.advance();
                    return Ok(Pattern::ListDestructure {
                        first: None,
                        rest: None,
                    });
                }
                let first = self.parse_pattern()?;
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    if matches!(self.peek(), Token::RBracket) {
                        self.advance();
                        return Ok(Pattern::ListDestructure {
                            first: Some(Box::new(first)),
                            rest: None,
                        });
                    }
                    let rest = self.parse_pattern()?;
                    self.expect_token(Token::RBracket, "']'")?;
                    return Ok(Pattern::ListDestructure {
                        first: Some(Box::new(first)),
                        rest: Some(Box::new(rest)),
                    });
                }
                self.expect_token(Token::RBracket, "']'")?;
                Ok(Pattern::ListDestructure {
                    first: Some(Box::new(first)),
                    rest: None,
                })
            }
            Token::IntLit(n) => {
                self.advance();
                let lit_span = self.last_span();
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let start = Some(Expr::boxed(ExprKind::Int(n, lit_span)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    return Ok(Pattern::Range { start, end });
                }
                if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let start = Some(Expr::boxed(ExprKind::Int(n, lit_span)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    // Inclusive range pattern? We'll treat as exclusive for now.
                    return Ok(Pattern::Range { start, end });
                }
                Ok(Pattern::Literal(Expr::new(ExprKind::Int(n, lit_span))))
            }
            Token::FloatLit(f) => {
                self.advance();
                let lit_span = self.last_span();
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let start = Some(Expr::boxed(ExprKind::Number(f, lit_span)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    return Ok(Pattern::Range { start, end });
                }
                if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let start = Some(Expr::boxed(ExprKind::Number(f, lit_span)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    return Ok(Pattern::Range { start, end });
                }
                Ok(Pattern::Literal(Expr::new(ExprKind::Number(f, lit_span))))
            }
            Token::StringLit(s) => {
                self.advance();
                let lit_span = self.last_span();
                Ok(Pattern::Literal(Expr::new(ExprKind::String(s, lit_span))))
            }
            Token::True => {
                self.advance();
                let lit_span = self.last_span();
                Ok(Pattern::Literal(Expr::new(ExprKind::Bool(true, lit_span))))
            }
            Token::False => {
                self.advance();
                let lit_span = self.last_span();
                Ok(Pattern::Literal(Expr::new(ExprKind::Bool(false, lit_span))))
            }
            Token::Identifier(name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Token::Identifier(name) => {
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
            }
            other => Err(self.error(&format!("Unexpected token in pattern: {:?}", other))),
        }
    }
}

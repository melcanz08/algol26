// src/frontend/parser/expr.rs

use super::*;

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_logical_or()
    }

    pub(super) fn parse_logical_or(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_logical_and()?;
        while matches!(self.peek(), Token::Or) {
            self.advance();
            let right = self.parse_logical_and()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
                span: start_span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_logical_and(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), Token::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
                span: start_span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_comparison(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_additive()?;
        while matches!(
            self.peek(),
            Token::Gt
                | Token::Lt
                | Token::GreaterEqual
                | Token::LessEqual
                | Token::Equal
                | Token::NotEqual
        ) {
            let op = self.advance();
            let binop = match op {
                Token::Gt => BinOp::Greater,
                Token::GreaterEqual => BinOp::GreaterEqual,
                Token::LessEqual => BinOp::LessEqual,
                Token::Lt => BinOp::Less,
                Token::Equal => BinOp::Equal,
                Token::NotEqual => BinOp::NotEqual,
                other => {
                    return Err(self.error(&format!("Unexpected comparison operator: {:?}", other)))
                }
            };
            let right = self.parse_additive()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
                span: start_span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_additive(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_multiplicative()?;
        while matches!(self.peek(), Token::Plus | Token::Minus) {
            let op = self.advance();
            let binop = match op {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Subtract,
                other => {
                    return Err(self.error(&format!("Unexpected additive operator: {:?}", other)))
                }
            };
            let right = self.parse_multiplicative()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
                span: start_span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_multiplicative(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Token::Star | Token::Slash) {
            let op = self.advance();
            let binop = match op {
                Token::Star => BinOp::Multiply,
                Token::Slash => BinOp::Divide,
                other => {
                    return Err(
                        self.error(&format!("Unexpected multiplicative operator: {:?}", other))
                    )
                }
            };
            let right = self.parse_unary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
                span: start_span,
            };
        }
        Ok(left)
    }

    pub(super) fn parse_unary(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        match self.peek().clone() {
            Token::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Negate,
                    expr: Box::new(operand),
                    span: start_span,
                })
            }
            Token::Not => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(operand),
                    span: start_span,
                })
            }
            Token::Star => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Deref {
                    expr: Box::new(expr),
                    span: start_span,
                })
            }
            Token::Ampersand => {
                self.advance();
                if matches!(self.peek(), Token::Mut) {
                    self.advance();
                    let expr = self.parse_unary()?;
                    return Ok(Expr::MutBorrow {
                        expr: Box::new(expr),
                        span: start_span,
                    });
                }
                let expr = self.parse_unary()?;
                Ok(Expr::Borrow {
                    expr: Box::new(expr),
                    span: start_span,
                })
            }
            _ => self.parse_primary(),
        }
    }

    pub(super) fn parse_primary(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        match self.advance() {
            Token::If => self.parse_if_expr(),
            Token::For => self.parse_for_expr(),
            Token::While => self.parse_while_expr(),
            Token::Try => self.parse_try_catch_expr(),
            Token::Match => self.parse_match_expr(),
            Token::IntLit(start) => {
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::Range {
                        start: Some(Box::new(Expr::Int(start, start_span))),
                        end,
                        inclusive: false,
                        span: start_span,
                    })
                } else if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::Range {
                        start: Some(Box::new(Expr::Int(start, start_span))),
                        end,
                        inclusive: true,
                        span: start_span,
                    })
                } else {
                    Ok(Expr::Int(start, start_span))
                }
            }
            Token::FloatLit(v) => {
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::Range {
                        start: Some(Box::new(Expr::Number(v, start_span))),
                        end,
                        inclusive: false,
                        span: start_span,
                    })
                } else if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::Range {
                        start: Some(Box::new(Expr::Number(v, start_span))),
                        end,
                        inclusive: true,
                        span: start_span,
                    })
                } else {
                    Ok(Expr::Number(v, start_span))
                }
            }
            Token::StringLit(s) => Ok(Expr::String(s, start_span)),
            Token::True => Ok(Expr::Bool(true, start_span)),
            Token::False => Ok(Expr::Bool(false, start_span)),
            Token::NullPtr => Ok(Expr::NullPtr(start_span)),
            Token::Identifier(name) => self.parse_identifier_expr(name, start_span),
            Token::Alloc => {
                // `self.advance()` above already consumed `alloc`, so we
                // start at the `(`.
                self.expect_token(Token::LParen, "'('")?;
                let size = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                Ok(Expr::FunctionCall {
                    name: "alloc".to_string(),
                    args: vec![size],
                    span: start_span,
                })
            }
            Token::Free => {
                self.expect_token(Token::LParen, "'('")?;
                let ptr = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                Ok(Expr::FunctionCall {
                    name: "free".to_string(),
                    args: vec![ptr],
                    span: start_span,
                })
            }
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                Ok(expr)
            }
            Token::Some => {
                let parenthesized = matches!(self.peek(), Token::LParen);
                if parenthesized {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if parenthesized {
                    self.expect_token(Token::RParen, "')'")?;
                }
                Ok(Expr::Some {
                    value: Box::new(value),
                    span: start_span,
                })
            }
            Token::None => Ok(Expr::None(start_span)),
            Token::Ok => {
                let parenthesized = matches!(self.peek(), Token::LParen);
                if parenthesized {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if parenthesized {
                    self.expect_token(Token::RParen, "')'")?;
                }
                Ok(Expr::Ok {
                    value: Box::new(value),
                    span: start_span,
                })
            }
            Token::Error => {
                let parenthesized = matches!(self.peek(), Token::LParen);
                if parenthesized {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if parenthesized {
                    self.expect_token(Token::RParen, "')'")?;
                }
                Ok(Expr::Error {
                    value: Box::new(value),
                    span: start_span,
                })
            }
            Token::LBracket => {
                let mut elements = Vec::new();
                while !matches!(self.peek(), Token::RBracket | Token::Eof) {
                    elements.push(self.parse_expr()?);
                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }
                self.expect_token(Token::RBracket, "']'")?;
                Ok(Expr::List(elements, start_span))
            }
            other => Err(self.error(&format!("Unexpected expression: {:?}", other))),
        }
    }

    pub(super) fn parse_identifier_expr(&mut self, name: String, ident_span: Span) -> Result<Expr> {
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            let mut args = Vec::new();
            while !matches!(self.peek(), Token::RParen | Token::Eof) {
                args.push(self.parse_expr()?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::RParen, "')'")?;
            Ok(Expr::FunctionCall {
                name,
                args,
                span: ident_span,
            })
        } else if matches!(self.peek(), Token::LBracket) {
            self.advance();
            let index = self.parse_expr()?;
            self.expect_token(Token::RBracket, "']'")?;
            Ok(Expr::ArrayAccess {
                array: Box::new(Expr::Var(name, ident_span)),
                index: Box::new(index),
                span: ident_span,
            })
        } else if matches!(self.peek(), Token::Dot) {
            self.advance(); // consume dot
            let method_name = self.expect_identifier("method name")?;

            let args = if matches!(self.peek(), Token::LParen) {
                self.advance();
                let mut args = Vec::new();
                while !matches!(self.peek(), Token::RParen | Token::Eof) {
                    args.push(self.parse_expr()?);
                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }
                self.expect_token(Token::RParen, "')'")?;
                args
            } else {
                // Bare method syntax (`s.length`) — no parens. Desugar to a
                // zero-arg dotted call so it behaves like `s.length()`.
                //
                // NOTE: when struct support lands, this needs a discriminator
                // to tell `s.length` (method) from `point.x` (field). Today
                // nothing produces a valid FieldAccess, so no case is lost.
                Vec::new()
            };

            Ok(Expr::FunctionCall {
                name: format!("{}.{}", name, method_name),
                args,
                span: ident_span,
            })
        } else {
            Ok(Expr::Var(name, ident_span))
        }
    }

    pub(super) fn parse_if_expr(&mut self) -> Result<Expr> {
        let start_span = self.last_span();
        let condition = Box::new(self.parse_expr()?);

        self.skip_keyword("then");

        let then_branch = Box::new(self.parse_block_expr()?);

        let else_branch = if matches!(self.peek(), Token::Else) {
            self.advance();
            if matches!(self.peek(), Token::If) {
                self.advance(); // consume 'if' — parse_if_expr assumes it's gone
                let else_if_span = self.last_span();
                let else_if_expr = self.parse_if_expr()?;
                // Wrap the nested `if` as a *statement* inside the
                // synthetic block, not as a trailing expression. The
                // IR builder extracts an else branch's statements and
                // ignores trailing expressions; putting the nested
                // `if` as `Stmt::Expression(...)` makes it flow
                // through the normal statement path, which handles
                // `if` recursively.
                Some(Box::new(Expr::Block {
                    statements: vec![Stmt::Expression(else_if_expr)],
                    trailing_expr: None,
                    span: else_if_span,
                }))
            } else {
                Some(Box::new(self.parse_block_expr()?))
            }
        } else {
            None
        };

        Ok(Expr::If {
            condition,
            then_branch,
            else_branch,
            span: start_span,
        })
    }

    pub(super) fn parse_if_expr_from_stmt(&mut self) -> Result<Expr> {
        self.advance(); // consume 'if'
        self.parse_if_expr()
    }

    pub(super) fn parse_for_expr(&mut self) -> Result<Expr> {
        let start_span = self.last_span();
        let var = self.expect_identifier("iterator variable")?;
        self.expect_token(Token::In, "'in'")?;
        let iterable = Box::new(self.parse_expr()?);
        self.skip_optional_do();
        let (body, trailing_expr) = self.parse_loop_body()?;
        Ok(Expr::For {
            var,
            iterable,
            body,
            trailing_expr,
            span: start_span,
        })
    }

    pub(super) fn parse_while_expr(&mut self) -> Result<Expr> {
        let start_span = self.last_span();
        let condition = Box::new(self.parse_expr()?);
        self.skip_optional_do();
        let (body, trailing_expr) = self.parse_loop_body()?;
        Ok(Expr::While {
            condition,
            body,
            trailing_expr,
            span: start_span,
        })
    }

    pub(super) fn parse_try_catch_expr(&mut self) -> Result<Expr> {
        // Called from parse_primary (which already advanced past 'try')
        // and from parse_try_catch (which advanced just above).
        // No advance here — the caller has consumed the keyword.
        let start_span = self.last_span();

        let try_branch = Box::new(self.parse_block_expr()?);

        let mut catch_var = None;
        let catch_branch = if matches!(self.peek(), Token::Catch) {
            self.advance();
            if let Token::Identifier(var) = self.peek().clone() {
                self.advance();
                catch_var = Some(var);
            }
            Box::new(self.parse_block_expr()?)
        } else {
            Box::new(Expr::Block {
                statements: vec![],
                trailing_expr: None,
                span: start_span,
            })
        };

        let finally_body = if matches!(self.peek(), Token::Finally) {
            self.advance();
            Some(self.parse_block()?)
        } else {
            None
        };

        Ok(Expr::TryCatch {
            try_branch,
            catch_var,
            catch_branch,
            finally_body,
            span: start_span,
        })
    }

    pub(super) fn parse_block_expr(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut statements = Vec::new();
        let mut trailing_expr = None;

        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof | Token::End) {
                statements.push(self.parse_stmt()?);
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
            if let Token::End = self.peek() {
                self.advance();
            }
        } else if let Token::End = self.peek() {
            self.advance();
        } else {
            if !matches!(self.peek(), Token::Eof | Token::Dedent | Token::End) {
                statements.push(self.parse_stmt()?);
            }
        }

        if let Some(last) = statements.last() {
            let can_be_trailing = match last {
                Stmt::Expression(expr) => !matches!(
                    expr,
                    Expr::Block { .. }
                        | Expr::If { .. }
                        | Expr::Match { .. }
                        | Expr::For { .. }
                        | Expr::While { .. }
                        | Expr::TryCatch { .. }
                ),
                _ => false,
            };
            if can_be_trailing {
                if let Some(Stmt::Expression(expr)) = statements.pop() {
                    trailing_expr = Some(Box::new(expr));
                }
            }
        }

        Ok(Expr::Block {
            statements,
            trailing_expr,
            span: start_span,
        })
    }
}

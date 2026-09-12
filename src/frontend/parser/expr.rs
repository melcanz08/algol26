// src/frontend/parser/expr.rs

use super::*;

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_logical_or()
    }
    pub(super) fn parse_logical_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_logical_and()?;
        while matches!(self.peek(), Token::Or) {
            self.advance();
            let right = self.parse_logical_and()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    pub(super) fn parse_logical_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), Token::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    pub(super) fn parse_comparison(&mut self) -> Result<Expr> {
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
            };
        }
        Ok(left)
    }
    pub(super) fn parse_additive(&mut self) -> Result<Expr> {
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
            };
        }
        Ok(left)
    }
    pub(super) fn parse_multiplicative(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Token::Star | Token::Slash) {
            let op = self.advance();
            let binop = match op {
                Token::Star => BinOp::Multiply,
                Token::Slash => BinOp::Divide,
                other => {
                    return Err(self.error(&format!("Unexpected multiplicative operator: {:?}", other)))
                }
            };
            let right = self.parse_unary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    pub(super) fn parse_unary(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Token::Minus => {
                self.advance();
                if let Token::IntLit(n) = self.peek().clone() {
                    self.advance();
                    return Ok(Expr::Int(-n));
                }
                if let Token::FloatLit(f) = self.peek().clone() {
                    self.advance();
                    return Ok(Expr::Number(-f));
                }
                let operand = self.parse_unary()?;
                Ok(Expr::Binary {
                    left: Box::new(Expr::Number(0.0)),
                    op: BinOp::Subtract,
                    right: Box::new(operand),
                })
            }
            Token::Not => {
                let start_info = self.peek_info().clone();
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(operand),
                    span: Span::point(start_info.line, start_info.column),
                })
            }
            Token::Star => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::Deref {
                    expr: Box::new(expr),
                })
            }
            Token::Ampersand => {
                self.advance();
                if let Token::Identifier(ref s) = self.peek().clone() {
                    if s == "mut" {
                        self.advance();
                        let expr = self.parse_unary()?;
                        return Ok(Expr::MutBorrow {
                            expr: Box::new(expr),
                        });
                    }
                }
                let expr = self.parse_unary()?;
                Ok(Expr::Borrow {
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_primary(),
        }
    }
    pub(super) fn parse_primary(&mut self) -> Result<Expr> {
        //let start_info = self.peek_info().clone();
        match self.advance() {
            Token::If => self.parse_if_expr(),
            Token::For => self.parse_for_expr(),
            Token::While => self.parse_while_expr(),
            Token::Try => self.parse_try_catch_expr(),
            Token::IntLit(start) => {
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::Range {
                        start: Some(Box::new(Expr::Int(start))),
                        end,
                        inclusive: false,
                    })
                } else if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::Range {
                        start: Some(Box::new(Expr::Int(start))),
                        end,
                        inclusive: true,
                    })
                } else {
                    Ok(Expr::Int(start))
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
                        start: Some(Box::new(Expr::Number(v))),
                        end,
                        inclusive: false,
                    })
                } else if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::Range {
                        start: Some(Box::new(Expr::Number(v))),
                        end,
                        inclusive: true,
                    })
                } else {
                    Ok(Expr::Number(v))
                }
            }
            Token::StringLit(s) => Ok(Expr::String(s)),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::NullPtr => Ok(Expr::NullPtr), 
            Token::Identifier(name) => self.parse_identifier_expr(name),
            Token::Alloc => {
                self.advance();
                self.expect_token(Token::LParen, "'('")?;
                let size = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                let span = self.peek_info().clone();
                Ok(Expr::FunctionCall {
                    name: "alloc".to_string(),
                    args: vec![size],
                    span: Span::point(span.line, span.column),
                })
            }
            Token::Free => {
                self.advance();
                self.expect_token(Token::LParen, "'('")?;
                let ptr = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                let span = self.peek_info().clone();
                Ok(Expr::FunctionCall {
                    name: "free".to_string(),
                    args: vec![ptr],
                    span: Span::point(span.line, span.column),
                })
            }
            Token::LParen => {
                let expr = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                Ok(expr)
            }
            Token::Some => {
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if matches!(self.peek(), Token::RParen) {
                    self.advance();
                }
                Ok(Expr::Some {
                    value: Box::new(value),
                })
            }
            Token::None => Ok(Expr::None),
            Token::Ok => {
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if matches!(self.peek(), Token::RParen) {
                    self.advance();
                }
                Ok(Expr::Ok {
                    value: Box::new(value),
                })
            }
            Token::Error => {
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if matches!(self.peek(), Token::RParen) {
                    self.advance();
                }
                Ok(Expr::Error {
                    value: Box::new(value),
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
                Ok(Expr::List(elements))
            }
            other => Err(self.error(&format!("Unexpected expression: {:?}", other))),
        }
    }
    pub(super) fn parse_identifier_expr(&mut self, name: String) -> Result<Expr> {
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
            let span = self.peek_info().clone();
            Ok(Expr::FunctionCall {
                name,
                args,
                span: Span::point(span.line, span.column),
            })
        } else if matches!(self.peek(), Token::LBracket) {
            self.advance();
            let index = self.parse_expr()?;
            self.expect_token(Token::RBracket, "']'")?;
            let span = self.peek_info().clone();
            Ok(Expr::ArrayAccess {
                array: Box::new(Expr::Var(name, Span::point(span.line, span.column))),
                index: Box::new(index),
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

            let span = self.peek_info().clone();
            Ok(Expr::FunctionCall {
                name: format!("{}.{}", name, method_name),
                args,
                span: Span::point(span.line, span.column),
            })
        } else {
            let span = self.peek_info().clone();
            Ok(Expr::Var(name, Span::point(span.line, span.column)))
        }
    }
    pub(super) fn parse_if_expr(&mut self) -> Result<Expr> {
        let condition = Box::new(self.parse_expr()?);

        self.skip_keyword("then");

        let then_branch = Box::new(self.parse_block_expr()?);

        let else_branch = if matches!(self.peek(), Token::Else) {
            self.advance();
            if matches!(self.peek(), Token::If) {
                let else_if_expr = self.parse_if_expr()?;
                Some(Box::new(Expr::Block {
                    statements: vec![],
                    trailing_expr: Some(Box::new(else_if_expr)),
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
        })
    }
    pub(super) fn parse_if_expr_from_stmt(&mut self) -> Result<Expr> {
        self.advance(); // consume 'if'
        self.parse_if_expr()
    }
    pub(super) fn parse_for_expr(&mut self) -> Result<Expr> {
        let start_info = self.peek_info().clone();
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
            span: Span::point(start_info.line, start_info.column),
        })
    }
    pub(super) fn parse_while_expr(&mut self) -> Result<Expr> {
        let start_info = self.peek_info().clone();
        let condition = Box::new(self.parse_expr()?);
        self.skip_optional_do();
        let (body, trailing_expr) = self.parse_loop_body()?;
        Ok(Expr::While {
            condition,
            body,
            trailing_expr,
            span: Span::point(start_info.line, start_info.column),
        })
    }
    pub(super) fn parse_try_catch_expr(&mut self) -> Result<Expr> {
        // Called from parse_primary (which already advanced past 'try')
        // and from parse_try_catch (which advanced just above).
        // No advance here — the caller has consumed the keyword.
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
        })
    }
    pub(super) fn parse_block_expr(&mut self) -> Result<Expr> {
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
                Stmt::Expression(expr) => match expr {
                    Expr::Block { .. } => false,
                    Expr::If { .. } => false,
                    Expr::Match { .. } => false,
                    Expr::For { .. } => false,
                    Expr::While { .. } => false,
                    Expr::TryCatch { .. } => false,
                    _ => true,
                },
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
        })
    }
}
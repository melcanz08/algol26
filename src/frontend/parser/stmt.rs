// src/frontend/parser/stmt.rs

use super::*;

impl Parser {
    pub(super) fn parse_stmt(&mut self) -> Result<Stmt> {
        match self.peek().clone() {
            Token::Var | Token::Val => self.parse_var_decl(),
            Token::Print => self.parse_print(),
            Token::If => {
                let expr = self.parse_if_expr_from_stmt()?;
                Ok(Stmt::Expression(expr))
            }
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Spawn => self.parse_spawn(),
            Token::Parallel => self.parse_parallel(),
            Token::Channel => self.parse_channel_decl(),
            Token::Send => self.parse_send(),
            Token::Receive => self.parse_receive(),
            Token::Match => {
                self.advance();
                Ok(Stmt::Expression(self.parse_match_expr()?))
            }
            Token::Break => {
                let span = self.current_span();
                self.advance();
                Ok(Stmt::Break(span))
            }
            Token::Continue => {
                let span = self.current_span();
                self.advance();
                Ok(Stmt::Continue(span))
            }
            Token::Defer => self.parse_defer(),
            Token::Alloc => {
                let span = self.current_span();
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let size = self.parse_expr()?;
                    self.expect_token(Token::RParen, "')'")?;
                    Ok(Stmt::Expression(Expr::new(ExprKind::FunctionCall {
                        name: "alloc".to_string(),
                        args: vec![size],
                        span,
                    })))
                } else {
                    Err(self.error("Expected '(' after alloc"))
                }
            }
            Token::Free => {
                let span = self.current_span();
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let ptr = self.parse_expr()?;
                    self.expect_token(Token::RParen, "')'")?;
                    Ok(Stmt::Expression(Expr::new(ExprKind::FunctionCall {
                        name: "free".to_string(),
                        args: vec![ptr],
                        span,
                    })))
                } else {
                    Err(self.error("Expected '(' after free"))
                }
            }
            Token::Return => self.parse_return(),
            Token::Unsafe => self.parse_unsafe(),
            Token::Region => self.parse_region(),
            Token::Try => self.parse_try_catch(),
            Token::Import => self.parse_import(),
            Token::Identifier(name) => self.parse_identifier_stmt(name),
            _other => {
                let expr = self.parse_expr()?;
                Ok(Stmt::Expression(expr))
            }
        }
    }

    pub(super) fn parse_defer(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();

        if matches!(self.peek(), Token::Indent) {
            self.advance();
            let mut stmts = Vec::new();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                stmts.push(self.parse_stmt()?);
            }
            if matches!(self.peek(), Token::Dedent) {
                self.advance();
            }
            Ok(Stmt::Defer {
                stmt: Box::new(Stmt::Expression(Expr::new(ExprKind::Block {
                    statements: stmts,
                    trailing_expr: None,
                    span: start_span,
                }))),
                span: start_span,
            })
        } else {
            let stmt = self.parse_stmt()?;
            Ok(Stmt::Defer {
                stmt: Box::new(stmt),
                span: start_span,
            })
        }
    }

    pub(super) fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        let block_expr = self.parse_block_expr()?;
        if let ExprKind::Block {
            mut statements,
            trailing_expr,
            ..
        } = block_expr.kind
        {
            if let Some(expr) = trailing_expr {
                statements.push(Stmt::Expression(*expr));
            }
            Ok(statements)
        } else {
            Ok(vec![])
        }
    }

    pub(super) fn parse_loop_body(&mut self) -> Result<(Vec<Stmt>, Option<Box<Expr>>)> {
        if matches!(self.peek(), Token::Indent) {
            self.advance();
            let mut statements = Vec::new();
            while !matches!(self.peek(), Token::Dedent | Token::End | Token::Eof) {
                statements.push(self.parse_stmt()?);
            }

            let trailing_expr = match statements.last() {
                Some(Stmt::Expression(expr))
                    if !matches!(
                        &expr.kind,
                        ExprKind::Block { .. }
                            | ExprKind::If { .. }
                            | ExprKind::Match { .. }
                            | ExprKind::For { .. }
                            | ExprKind::While { .. }
                            | ExprKind::TryCatch { .. }
                    ) =>
                {
                    match statements.pop() {
                        Some(Stmt::Expression(expr)) => Some(Box::new(expr)),
                        _ => None,
                    }
                }
                _ => None,
            };

            if matches!(self.peek(), Token::Dedent) {
                self.advance();
            }
            if matches!(self.peek(), Token::End) {
                self.advance();
            }

            Ok((statements, trailing_expr))
        } else if matches!(self.peek(), Token::End) {
            self.advance();
            Ok((Vec::new(), None))
        } else {
            let expr = self.parse_expr()?;
            Ok((Vec::new(), Some(Box::new(expr))))
        }
    }

    pub(super) fn parse_var_decl(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        let is_mutable = matches!(self.peek(), Token::Var);
        self.advance();
        let name = self.expect_identifier("variable name")?;
        let type_annotation = self.parse_type_annotation()?;

        match self.advance() {
            Token::Assign => {}
            other => {
                return Err(self.error(&format!("Expected assignment operator, found {:?}", other)))
            }
        }

        let value = self.parse_expr()?;
        Ok(Stmt::VarDecl {
            span: start_span,
            name,
            value,
            type_annotation,
            mutable: is_mutable,
        })
    }

    pub(super) fn parse_print(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        if matches!(self.peek(), Token::LParen) {
            self.advance();
        }
        let expr = self.parse_expr()?;
        if matches!(self.peek(), Token::RParen) {
            self.advance();
        }
        Ok(Stmt::Print {
            expr,
            span: start_span,
        })
    }

    pub(super) fn parse_for(&mut self) -> Result<Stmt> {
        self.advance();
        Ok(Stmt::Expression(self.parse_for_expr()?))
    }

    pub(super) fn parse_while(&mut self) -> Result<Stmt> {
        self.advance();
        Ok(Stmt::Expression(self.parse_while_expr()?))
    }

    pub(super) fn parse_spawn(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        self.skip_optional_do();
        let body = self.parse_block()?;
        Ok(Stmt::Spawn {
            body,
            span: start_span,
        })
    }

    pub(super) fn parse_parallel(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        self.skip_optional_do();
        let mut blocks = Vec::new();
        blocks.push(self.parse_block()?);
        while matches!(self.peek(), Token::And) || matches!(self.peek(), Token::Comma) {
            self.advance();
            blocks.push(self.parse_block()?);
        }
        Ok(Stmt::Parallel {
            blocks,
            span: start_span,
        })
    }

    pub(super) fn parse_channel_decl(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        let name = self.expect_identifier("channel name")?;
        let _type_annotation = self.parse_type_annotation()?;
        Ok(Stmt::ChannelDecl {
            name,
            span: start_span,
        })
    }

    pub(super) fn parse_send(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        let channel = self.expect_identifier("channel name")?;
        if matches!(self.peek(), Token::Comma) {
            self.advance();
        }
        let value = self.parse_expr()?;
        Ok(Stmt::Send {
            channel,
            value,
            span: start_span,
        })
    }

    pub(super) fn parse_receive(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        let channel = self.expect_identifier("channel name")?;
        let target = if matches!(
            self.peek(),
            Token::Identifier(ref s) if s == "into" || s == "as"
        ) {
            self.advance();
            self.expect_identifier("receive target")?
        } else {
            String::new()
        };
        Ok(Stmt::Receive {
            channel,
            target,
            span: start_span,
        })
    }

    pub(super) fn parse_match_expr(&mut self) -> Result<Expr> {
        let start_span = self.last_span();
        let value = self.parse_expr()?;

        let mut cases = Vec::new();

        if let Token::Indent = self.peek() {
            self.advance();

            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                if !matches!(self.peek(), Token::Case) {
                    return Err(self.error("Expected 'case' in match arm"));
                }
                let case_span = self.current_span();
                self.advance();

                let mut pattern = self.parse_pattern()?;

                if matches!(self.peek(), Token::If) {
                    self.advance();
                    let condition = self.parse_expr()?;
                    pattern = Pattern::Guarded {
                        pattern: Box::new(pattern),
                        condition: Box::new(condition),
                    };
                }

                let body = self.parse_block_expr()?;

                let body = match body.kind {
                    ExprKind::Block {
                        statements,
                        trailing_expr,
                        span,
                    } => {
                        let span = if span == Span::default() {
                            case_span
                        } else {
                            span
                        };
                        Expr::new(ExprKind::Block {
                            statements,
                            trailing_expr,
                            span,
                        })
                    }
                    other => Expr::new(other),
                };

                cases.push(MatchCaseExpr { pattern, body });
            }

            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }

        Ok(Expr::new(ExprKind::Match {
            value: Box::new(value),
            cases,
            span: start_span,
        }))
    }

    pub(super) fn parse_return(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        let value = if matches!(self.peek(), Token::Eof | Token::Dedent) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Return {
            value,
            span: start_span,
        })
    }

    pub(super) fn parse_unsafe(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        let body = self.parse_block()?;
        Ok(Stmt::UnsafeBlock {
            body,
            span: start_span,
        })
    }

    pub(super) fn parse_region(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        let name = self.expect_identifier("region name")?;
        let body = self.parse_block()?;
        Ok(Stmt::RegionBlock {
            name,
            body,
            span: start_span,
        })
    }

    pub(super) fn parse_try_catch(&mut self) -> Result<Stmt> {
        self.advance();
        Ok(Stmt::Expression(self.parse_try_catch_expr()?))
    }

    pub(super) fn parse_identifier_stmt(&mut self, name: String) -> Result<Stmt> {
        let ident_span = self.current_span();
        self.advance();

        match self.peek().clone() {
            Token::LParen => {
                self.advance();
                let mut args = Vec::new();
                while !matches!(self.peek(), Token::RParen | Token::Eof) {
                    args.push(self.parse_expr()?);
                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }
                self.expect_token(Token::RParen, "')'")?;
                Ok(Stmt::Expression(Expr::new(ExprKind::FunctionCall {
                    name,
                    args,
                    span: ident_span,
                })))
            }
            Token::LBracket => {
                self.advance();
                let index = self.parse_expr()?;
                self.expect_token(Token::RBracket, "']'")?;

                if matches!(self.peek(), Token::Assign) {
                    self.advance();
                    let value = self.parse_expr()?;
                    Ok(Stmt::ArrayAssign {
                        array: name,
                        index,
                        value,
                        span: ident_span,
                    })
                } else {
                    Ok(Stmt::Expression(Expr::new(ExprKind::ArrayAccess {
                        array: Expr::boxed(ExprKind::Var(name, ident_span)),
                        index: Box::new(index),
                        span: ident_span,
                    })))
                }
            }
            Token::Assign => {
                self.advance();
                let value = self.parse_expr()?;
                Ok(Stmt::Assign {
                    name,
                    value,
                    span: ident_span,
                })
            }
            Token::Dot => {
                self.advance();
                let member_name = self.expect_identifier("method or field name")?;

                // `p.x(...)` → dotted method call (existing behavior).
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
                    return Ok(Stmt::Expression(Expr::new(ExprKind::FunctionCall {
                        name: format!("{}.{}", name, member_name),
                        args,
                        span: ident_span,
                    })));
                }

                // `p.x := v` → field assignment.
                if matches!(self.peek(), Token::Assign) {
                    self.advance();
                    let value = self.parse_expr()?;
                    return Ok(Stmt::FieldAssign {
                        target: name,
                        field: member_name,
                        value,
                        span: ident_span,
                    });
                }

                // `p.show` (bare, no parens, no assign) → bare method call.
                // Preserves the pre-record behavior for zero-arg method calls
                // written without parentheses.
                Ok(Stmt::Expression(Expr::new(ExprKind::FunctionCall {
                    name: format!("{}.{}", name, member_name),
                    args: Vec::new(),
                    span: ident_span,
                })))
            }
            Token::LBrace => {
                // Record literal in statement position, e.g.
                //     Point { x: 0, y: 0 }
                let lit = self.parse_record_literal(name, Vec::new(), ident_span)?;
                Ok(Stmt::Expression(lit))
            }
            _ => Ok(Stmt::Expression(Expr::new(ExprKind::Var(name, ident_span)))),
        }
    }
}

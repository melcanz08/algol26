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
            Token::Match => self.parse_match(),
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Token::Defer => {
                self.advance(); // consume 'defer'

                if matches!(self.peek(), Token::Indent) {
                    // Block form: `defer` followed by an indented body.
                    // Parse the indented statements and wrap them in an
                    // Expr::Block so the existing Stmt::Defer shape
                    // (single statement) still holds.
                    self.advance(); // consume Indent
                    let mut stmts = Vec::new();
                    while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                        stmts.push(self.parse_stmt()?);
                    }
                    if matches!(self.peek(), Token::Dedent) {
                        self.advance(); // consume Dedent
                    }
                    Ok(Stmt::Defer {
                        stmt: Box::new(Stmt::Expression(Expr::Block {
                            statements: stmts,
                            trailing_expr: None,
                        })),
                    })
                } else {
                    // Inline form: `defer print(...)` on one line.
                    let stmt = self.parse_stmt()?;
                    Ok(Stmt::Defer {
                        stmt: Box::new(stmt),
                    })
                }
            }
            Token::Alloc => {
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let size = self.parse_expr()?;
                    self.expect_token(Token::RParen, "')'")?;
                    let span = self.peek_info().clone();
                    Ok(Stmt::Expression(Expr::FunctionCall {
                        name: "alloc".to_string(),
                        args: vec![size],
                        span: Span::point(span.line, span.column),
                    }))
                } else {
                    Err(self.error("Expected '(' after alloc"))
                }
            }
            Token::Free => {
                self.advance();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let ptr = self.parse_expr()?;
                    self.expect_token(Token::RParen, "')'")?;
                    let span = self.peek_info().clone();
                    Ok(Stmt::Expression(Expr::FunctionCall {
                        name: "free".to_string(),
                        args: vec![ptr],
                        span: Span::point(span.line, span.column),
                    }))
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
            Token::Do | Token::In => {
                self.advance();
                self.parse_stmt()
            }
            Token::End => {
                self.advance();
                Ok(Stmt::Expression(Expr::Bool(true)))
            }
            _other => {
                let expr = self.parse_expr()?;
                Ok(Stmt::Expression(expr))
            }
        }
    }
    pub(super) fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        let block_expr = self.parse_block_expr()?;
        if let Expr::Block {
            mut statements,
            trailing_expr,
        } = block_expr
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
                        expr,
                        Expr::Block { .. }
                            | Expr::If { .. }
                            | Expr::Match { .. }
                            | Expr::For { .. }
                            | Expr::While { .. }
                            | Expr::TryCatch { .. }
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
        let span = self.peek_info().clone();
        Ok(Stmt::VarDecl {
            span: Span::point(span.line, span.column),
            name,
            value,
            type_annotation,
            mutable: is_mutable,
        })
    }
    pub(super) fn parse_print(&mut self) -> Result<Stmt> {
        self.advance();
        if matches!(self.peek(), Token::LParen) {
            self.advance();
        }
        let expr = self.parse_expr()?;
        if matches!(self.peek(), Token::RParen) {
            self.advance();
        }
        Ok(Stmt::Print { expr })
    }
    pub(super) fn parse_for(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'for'
        Ok(Stmt::Expression(self.parse_for_expr()?))
    }
    pub(super) fn parse_while(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'while'
        Ok(Stmt::Expression(self.parse_while_expr()?))
    }
    pub(super) fn parse_spawn(&mut self) -> Result<Stmt> {
        self.advance();
        self.skip_optional_do();
        let body = self.parse_block()?;
        Ok(Stmt::Spawn { body })
    }
    pub(super) fn parse_parallel(&mut self) -> Result<Stmt> {
        self.advance();
        self.skip_optional_do();
        let mut blocks = Vec::new();
        blocks.push(self.parse_block()?);
        while matches!(self.peek(), Token::And) || matches!(self.peek(), Token::Comma) {
            self.advance();
            blocks.push(self.parse_block()?);
        }
        Ok(Stmt::Parallel { blocks })
    }
    pub(super) fn parse_channel_decl(&mut self) -> Result<Stmt> {
        self.advance();
        let name = self.expect_identifier("channel name")?;
        let _type_annotation = self.parse_type_annotation()?;
        Ok(Stmt::ChannelDecl { name })
    }
    pub(super) fn parse_send(&mut self) -> Result<Stmt> {
        self.advance();
        let channel = self.expect_identifier("channel name")?;
        if matches!(self.peek(), Token::Comma) {
            self.advance();
        }
        let value = self.parse_expr()?;
        Ok(Stmt::Send { channel, value })
    }
    pub(super) fn parse_receive(&mut self) -> Result<Stmt> {
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
        Ok(Stmt::Receive { channel, target })
    }
    pub(super) fn parse_match(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'match'
        let value = self.parse_expr()?;

        let mut cases = Vec::new();

        if let Token::Indent = self.peek() {
            self.advance(); // indent to case level

            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                // Expect 'case' keyword
                if !matches!(self.peek(), Token::Case) {
                    // allow optional 'case'? but we'll require it now
                    return Err(self.error("Expected 'case' in match arm"));
                }
                self.advance(); // consume 'case'

                let mut pattern = self.parse_pattern()?;

                // pattern guard
                if matches!(self.peek(), Token::If) {
                    self.advance();
                    let condition = self.parse_expr()?;
                    pattern = Pattern::Guarded {
                        pattern: Box::new(pattern),
                        condition: Box::new(condition),
                    };
                }

                // optional 'then' or '=>'? We'll just parse body as block
                let mut body = Vec::new();
                if let Token::Indent = self.peek() {
                    self.advance();
                    while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                        body.push(self.parse_stmt()?);
                    }
                    if let Token::Dedent = self.peek() {
                        self.advance();
                    }
                } else {
                    // single expression?
                    let expr = self.parse_expr()?;
                    body.push(Stmt::Expression(expr));
                }

                cases.push(MatchCaseExpr {
                    pattern,
                    body: Expr::Block {
                        statements: body,
                        trailing_expr: None,
                    },
                });
            }

            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }

        Ok(Stmt::Expression(Expr::Match {
            value: Box::new(value),
            cases,
        }))
    }
    pub(super) fn parse_return(&mut self) -> Result<Stmt> {
        self.advance();
        let value = if matches!(self.peek(), Token::Eof | Token::Dedent) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Return { value })
    }
    pub(super) fn parse_unsafe(&mut self) -> Result<Stmt> {
        self.advance();
        let body = self.parse_block()?;
        Ok(Stmt::UnsafeBlock { body })
    }
    pub(super) fn parse_region(&mut self) -> Result<Stmt> {
        self.advance();
        let name = self.expect_identifier("region name")?;
        let body = self.parse_block()?;
        Ok(Stmt::RegionBlock { name, body })
    }
    pub(super) fn parse_try_catch(&mut self) -> Result<Stmt> {
        // Called from parse_stmt, which peeks — consume 'try' here.
        self.advance();
        Ok(Stmt::Expression(self.parse_try_catch_expr()?))
    }
    pub(super) fn parse_identifier_stmt(&mut self, name: String) -> Result<Stmt> {
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
                let span = self.peek_info().clone();
                Ok(Stmt::Expression(Expr::FunctionCall {
                    name,
                    args,
                    span: Span::point(span.line, span.column),
                }))
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
                    })
                } else {
                    let span = self.peek_info().clone();
                    Ok(Stmt::Expression(Expr::ArrayAccess {
                        array: Box::new(Expr::Var(name, Span::point(span.line, span.column))),
                        index: Box::new(index),
                    }))
                }
            }
            Token::Assign => {
                self.advance();
                let value = self.parse_expr()?;
                Ok(Stmt::Assign { name, value })
            }
            Token::Dot => {
                self.advance(); // consume dot
                let method_name = self.expect_identifier("method name")?;

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
                    Ok(Stmt::Expression(Expr::FunctionCall {
                        name: format!("{}.{}", name, method_name),
                        args,
                        span: Span::point(span.line, span.column),
                    }))
                } else {
                    // Bare method syntax (`s.length`) — no parens. Desugar to a
                    // zero-arg dotted call so it matches the parens form and the
                    // expression parser's handling.
                    //
                    // NOTE: when struct support lands, this needs a discriminator
                    // to tell `s.length` (method) from `point.x` (field). Today
                    // nothing produces a valid FieldAccess, so no case is lost.
                    let span = self.peek_info().clone();
                    Ok(Stmt::Expression(Expr::FunctionCall {
                        name: format!("{}.{}", name, method_name),
                        args: Vec::new(),
                        span: Span::point(span.line, span.column),
                    }))
                }
            }
            _ => {
                let span = self.peek_info().clone();
                Ok(Stmt::Expression(Expr::Var(
                    name,
                    Span::point(span.line, span.column),
                )))
            }
        }
    }
}
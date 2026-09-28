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
            left = Expr::new(ExprKind::Binary {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
                span: start_span,
            });
        }
        Ok(left)
    }

    pub(super) fn parse_logical_and(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), Token::And) {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::new(ExprKind::Binary {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
                span: start_span,
            });
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
            left = Expr::new(ExprKind::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
                span: start_span,
            });
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
            left = Expr::new(ExprKind::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
                span: start_span,
            });
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
            left = Expr::new(ExprKind::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
                span: start_span,
            });
        }
        Ok(left)
    }

    pub(super) fn parse_unary(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        match self.peek().clone() {
            Token::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::new(ExprKind::Unary {
                    op: UnaryOp::Negate,
                    expr: Box::new(operand),
                    span: start_span,
                }))
            }
            Token::Not => {
                self.advance();
                let operand = self.parse_unary()?;
                Ok(Expr::new(ExprKind::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(operand),
                    span: start_span,
                }))
            }
            Token::Star => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(Expr::new(ExprKind::Deref {
                    expr: Box::new(expr),
                    span: start_span,
                }))
            }
            Token::Ampersand => {
                self.advance();
                if matches!(self.peek(), Token::Mut) {
                    self.advance();
                    let expr = self.parse_unary()?;
                    return Ok(Expr::new(ExprKind::MutBorrow {
                        expr: Box::new(expr),
                        span: start_span,
                    }));
                }
                let expr = self.parse_unary()?;
                Ok(Expr::new(ExprKind::Borrow {
                    expr: Box::new(expr),
                    span: start_span,
                }))
            }
            _ => {
                let primary = self.parse_primary()?;
                self.parse_postfix(primary)
            }
        }
    }

    /// Apply postfix operators (`.member`, `[index]`) to an
    /// already-parsed primary expression, left to right.
    ///
    /// `parse_primary`'s identifier branch handles the leading
    /// identifier's `.` and `[` directly; this loop covers what
    /// follows any other primary (a function call, an array
    /// access, a parenthesized expression, a literal, etc.).
    ///
    /// Method calls (`x.method(...)`) are still restricted to bare
    /// identifier receivers — the AST has no form for a receiver
    /// expression in a call, and the analyzer resolves the receiver
    /// by name lookup. Complex receivers are rejected here rather
    /// than producing a misleading function name like
    /// `"pts[1].method"`.
    pub(super) fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr> {
        loop {
            match self.peek() {
                Token::LBracket => {
                    let span = expr.span();
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect_token(Token::RBracket, "']'")?;
                    expr = Expr::new(ExprKind::ArrayAccess {
                        array: Box::new(expr),
                        index: Box::new(index),
                        span,
                    });
                }
                Token::Dot => {
                    let span = expr.span();
                    self.advance();
                    let member = self.expect_identifier("member name")?;
                    if matches!(self.peek(), Token::LParen) {
                        // Method call. Only a bare identifier receiver is
                        // expressible in `ExprKind::FunctionCall { name }`.
                        let ExprKind::Var(name, _) = &expr.kind else {
                            return Err(self.error(
                                "method calls on complex receiver expressions are \
                                 not yet supported; bind the receiver to a variable first",
                            ));
                        };
                        let receiver = name.clone();
                        self.advance();
                        let mut args = Vec::new();
                        while !matches!(self.peek(), Token::RParen | Token::Eof) {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek(), Token::Comma) {
                                self.advance();
                            }
                        }
                        self.expect_token(Token::RParen, "')'")?;
                        expr = Expr::new(ExprKind::FunctionCall {
                            name: format!("{}.{}", receiver, member),
                            args,
                            span,
                        });
                    } else {
                        expr = Expr::new(ExprKind::FieldAccess {
                            object: Box::new(expr),
                            field: member,
                            span,
                        });
                    }
                }
                _ => break,
            }
        }
        Ok(expr)
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
                    Ok(Expr::new(ExprKind::Range {
                        start: Some(Expr::boxed(ExprKind::Int(start, start_span))),
                        end,
                        inclusive: false,
                        span: start_span,
                    }))
                } else if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::new(ExprKind::Range {
                        start: Some(Expr::boxed(ExprKind::Int(start, start_span))),
                        end,
                        inclusive: true,
                        span: start_span,
                    }))
                } else {
                    Ok(Expr::new(ExprKind::Int(start, start_span)))
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
                    Ok(Expr::new(ExprKind::Range {
                        start: Some(Expr::boxed(ExprKind::Number(v, start_span))),
                        end,
                        inclusive: false,
                        span: start_span,
                    }))
                } else if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    Ok(Expr::new(ExprKind::Range {
                        start: Some(Expr::boxed(ExprKind::Number(v, start_span))),
                        end,
                        inclusive: true,
                        span: start_span,
                    }))
                } else {
                    Ok(Expr::new(ExprKind::Number(v, start_span)))
                }
            }
            Token::StringLit(s) => Ok(Expr::new(ExprKind::String(s, start_span))),
            Token::True => Ok(Expr::new(ExprKind::Bool(true, start_span))),
            Token::False => Ok(Expr::new(ExprKind::Bool(false, start_span))),
            Token::NullPtr => Ok(Expr::new(ExprKind::NullPtr(start_span))),
            Token::Identifier(name) => self.parse_identifier_expr(name, start_span),
            Token::Alloc => {
                self.expect_token(Token::LParen, "'('")?;
                let size = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                Ok(Expr::new(ExprKind::FunctionCall {
                    name: "alloc".to_string(),
                    args: vec![size],
                    span: start_span,
                }))
            }
            Token::Free => {
                self.expect_token(Token::LParen, "'('")?;
                let ptr = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                Ok(Expr::new(ExprKind::FunctionCall {
                    name: "free".to_string(),
                    args: vec![ptr],
                    span: start_span,
                }))
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
                Ok(Expr::new(ExprKind::Some {
                    value: Box::new(value),
                    span: start_span,
                }))
            }
            Token::None => Ok(Expr::new(ExprKind::None(start_span))),
            Token::Ok => {
                let parenthesized = matches!(self.peek(), Token::LParen);
                if parenthesized {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if parenthesized {
                    self.expect_token(Token::RParen, "')'")?;
                }
                Ok(Expr::new(ExprKind::Ok {
                    value: Box::new(value),
                    span: start_span,
                }))
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
                Ok(Expr::new(ExprKind::Error {
                    value: Box::new(value),
                    span: start_span,
                }))
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
                Ok(Expr::new(ExprKind::List(elements, start_span)))
            }
            other => Err(self.error(&format!("Unexpected expression: {:?}", other))),
        }
    }

    pub(super) fn parse_identifier_expr(&mut self, name: String, ident_span: Span) -> Result<Expr> {
        if matches!(self.peek(), Token::LParen) {
            // Unchanged: function call `name(args)`.
            self.advance();
            let mut args = Vec::new();
            while !matches!(self.peek(), Token::RParen | Token::Eof) {
                args.push(self.parse_expr()?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::RParen, "')'")?;
            Ok(Expr::new(ExprKind::FunctionCall {
                name,
                args,
                span: ident_span,
            }))
        } else if matches!(self.peek(), Token::LBracket) {
            // Unchanged: `name[index]`.
            self.advance();
            let index = self.parse_expr()?;
            self.expect_token(Token::RBracket, "']'")?;
            Ok(Expr::new(ExprKind::ArrayAccess {
                array: Expr::boxed(ExprKind::Var(name, ident_span)),
                index: Box::new(index),
                span: ident_span,
            }))
        } else if name == "Map" && matches!(self.peek(), Token::LBrace) {
            // `Map { k: v, ... }` — key and value types inferred from entries.
            self.parse_map_literal(None, None, ident_span)
        } else if name == "Map"
            && matches!(self.peek(), Token::Lt)
            && self.looks_like_map_type_args()
        {
            // `Map<K, V> { ... }` — explicit type arguments. The empty
            // literal `Map<String, Int> {}` is only expressible this way.
            let (key_type, value_type) = self.parse_map_type_args()?;
            self.parse_map_literal(Some(key_type), Some(value_type), ident_span)
        } else if matches!(self.peek(), Token::LBrace) {
            // existing record-literal branch — unchanged
            self.parse_record_literal(name, Vec::new(), ident_span)
        } else if matches!(self.peek(), Token::Dot) {
            self.advance();
            let member_name = self.expect_identifier("member name")?;

            // `name.member(...)` → method call. Kept as the dotted
            // FunctionCall the existing trait/builtin dispatch expects.
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
                return Ok(Expr::new(ExprKind::FunctionCall {
                    name: format!("{}.{}", name, member_name),
                    args,
                    span: ident_span,
                }));
            }
            // `name.member` (no parens) → field access.
            Ok(Expr::new(ExprKind::FieldAccess {
                object: Expr::boxed(ExprKind::Var(name, ident_span)),
                field: member_name,
                span: ident_span,
            }))
        } else {
            Ok(Expr::new(ExprKind::Var(name, ident_span)))
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
                self.advance();
                let else_if_span = self.last_span();
                let else_if_expr = self.parse_if_expr()?;
                Some(Expr::boxed(ExprKind::Block {
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

        Ok(Expr::new(ExprKind::If {
            condition,
            then_branch,
            else_branch,
            span: start_span,
        }))
    }

    pub(super) fn parse_if_expr_from_stmt(&mut self) -> Result<Expr> {
        self.advance();
        self.parse_if_expr()
    }

    pub(super) fn parse_for_expr(&mut self) -> Result<Expr> {
        let start_span = self.last_span();
        let var = self.expect_identifier("iterator variable")?;
        self.expect_token(Token::In, "'in'")?;
        let iterable = Box::new(self.parse_expr()?);
        self.skip_optional_do();
        let (body, trailing_expr) = self.parse_loop_body()?;
        Ok(Expr::new(ExprKind::For {
            var,
            iterable,
            body,
            trailing_expr,
            span: start_span,
        }))
    }

    pub(super) fn parse_while_expr(&mut self) -> Result<Expr> {
        let start_span = self.last_span();
        let condition = Box::new(self.parse_expr()?);
        self.skip_optional_do();
        let (body, trailing_expr) = self.parse_loop_body()?;
        Ok(Expr::new(ExprKind::While {
            condition,
            body,
            trailing_expr,
            span: start_span,
        }))
    }

    pub(super) fn parse_try_catch_expr(&mut self) -> Result<Expr> {
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
            Expr::boxed(ExprKind::Block {
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

        Ok(Expr::new(ExprKind::TryCatch {
            try_branch,
            catch_var,
            catch_branch,
            finally_body,
            span: start_span,
        }))
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
                    &expr.kind,
                    ExprKind::Block { .. }
                        | ExprKind::If { .. }
                        | ExprKind::Match { .. }
                        | ExprKind::For { .. }
                        | ExprKind::While { .. }
                        | ExprKind::TryCatch { .. }
                ),
                _ => false,
            };
            if can_be_trailing {
                if let Some(Stmt::Expression(expr)) = statements.pop() {
                    trailing_expr = Some(Box::new(expr));
                }
            }
        }

        Ok(Expr::new(ExprKind::Block {
            statements,
            trailing_expr,
            span: start_span,
        }))
    }

    pub(super) fn parse_record_literal(
        &mut self,
        name: String,
        type_args: Vec<TypeSyntax>,
        start_span: Span,
    ) -> Result<Expr> {
        self.expect_token(Token::LBrace, "'{'")?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            let field_name = self.expect_identifier("field name")?;
            self.expect_token(Token::Colon, "':'")?;
            let value = self.parse_expr()?;
            fields.push((field_name, value));
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        self.expect_token(Token::RBrace, "'}'")?;

        Ok(Expr::new(ExprKind::RecordLiteral {
            name,
            type_args,
            fields,
            span: start_span,
        }))
    }
    /// Parse `<K, V>` after the `Map` identifier. Assumes the current
    /// token is `<`.
    fn parse_map_type_args(&mut self) -> Result<(TypeSyntax, TypeSyntax)> {
        self.expect_token(Token::Lt, "'<'")?;
        let key_type = self.parse_type_syntax()?;
        self.expect_token(Token::Comma, "','")?;
        let value_type = self.parse_type_syntax()?;
        self.expect_token(Token::Gt, "'>'")?;
        Ok((key_type, value_type))
    }

    /// Parse `{ k: v, ... }` after `Map` or `Map<K, V>`.
    ///
    /// Both empty (`{}`) and non-empty forms are accepted. An empty
    /// literal with no type args parses successfully here; the analyzer
    /// is what rejects it without an expected type from context.
    pub(super) fn parse_map_literal(
        &mut self,
        key_type: Option<TypeSyntax>,
        value_type: Option<TypeSyntax>,
        start_span: Span,
    ) -> Result<Expr> {
        self.expect_token(Token::LBrace, "'{'")?;
        let mut entries = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            let key = self.parse_expr()?;
            self.expect_token(Token::Colon, "':'")?;
            let value = self.parse_expr()?;
            entries.push((key, value));
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        self.expect_token(Token::RBrace, "'}'")?;
        Ok(Expr::new(ExprKind::MapLiteral {
            key_type,
            value_type,
            entries,
            span: start_span,
        }))
    }

    /// Lookahead: from the current position (which must be `<`), scan
    /// forward to the matching `>`. Return `true` iff every intervening
    /// token is legal in a type-argument list AND the token after the
    /// matching `>` is `{`.
    ///
    /// This is what disambiguates `Map<K, V> { ... }` from a comparison
    /// `Map < x`. The rule is conservative: any token that is not a
    /// type-syntax token causes an early `false`, at which point the
    /// caller falls through to treating `<` as the comparison operator.
    ///
    /// Nested type args (`Map<Int, Map<String, Bool>>`) push and pop on
    /// depth, so `>>` — which the lexer produces as two `Gt` tokens —
    /// closes both levels correctly.
    fn looks_like_map_type_args(&self) -> bool {
        debug_assert!(matches!(self.peek(), Token::Lt));
        let mut depth: i32 = 0;
        let mut i = self.pos;
        while i < self.tokens.len() {
            match &self.tokens[i].token {
                Token::Lt => depth += 1,
                Token::Gt => {
                    depth -= 1;
                    if depth == 0 {
                        return i + 1 < self.tokens.len()
                            && matches!(self.tokens[i + 1].token, Token::LBrace);
                    }
                    if depth < 0 {
                        return false;
                    }
                }
                Token::Identifier(_) | Token::Comma | Token::LBracket | Token::RBracket => {}
                _ => return false,
            }
            i += 1;
        }
        false
    }
}

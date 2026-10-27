// src/frontend/parser/expr.rs

use super::*;


// --- ADR 0061: namespace / type-name helpers -----------------------------
//
// A dotted call whose base is a builtin namespace (List, Map, String, ...)
// or a type-name-like identifier (uppercase first letter: Json, Day, Point)
// stays a `FunctionCall { name: "A.B", args }` per ADR 0061. Only a dotted
// call whose base is a *value* expression becomes `MethodCall`.

/// Fixed list of compiler-known builtin namespaces.
pub(crate) const BUILTIN_NAMESPACES: &[&str] = &[
    "List", "Map", "String", "Math", "File",
    "Int", "Float", "Bool", "Char",
    "Option", "Result", "Set", "Channel", "Ptr",
    "alloc", "free",
];

/// True if `name` is one of the fixed builtin namespaces.
pub(crate) fn is_builtin_namespace(name: &str) -> bool {
    BUILTIN_NAMESPACES.contains(&name)
}

/// Heuristic: a name that looks like a user-declared type.
pub(crate) fn is_type_name_like(name: &str) -> bool {
    name.chars().next().map_or(false, |c| c.is_ascii_uppercase())
}


impl Parser {
    pub(super) fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_logical_or()
    }

    pub(super) fn parse_logical_or(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_logical_and()?;
        while matches!(self.peek(), Token::Or) && !self.at_statement_boundary() {
            self.advance();
            let right = self.parse_logical_and()?;
            left = Expr::new(ExprKind::Binary {
                left: Box::new(left),
                op: BinOp::Or,
                right: Box::new(right),
                span: self.span_from(start_span),
            });
        }
        Ok(left)
    }

    pub(super) fn parse_logical_and(&mut self) -> Result<Expr> {
        let start_span = self.current_span();
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), Token::And) && !self.at_statement_boundary() {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::new(ExprKind::Binary {
                left: Box::new(left),
                op: BinOp::And,
                right: Box::new(right),
                span: self.span_from(start_span),
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
                | Token::In
        ) && !self.at_statement_boundary()
        {
            let op = self.advance();
            let binop = match op {
                Token::Gt => BinOp::Greater,
                Token::GreaterEqual => BinOp::GreaterEqual,
                Token::LessEqual => BinOp::LessEqual,
                Token::Lt => BinOp::Less,
                Token::Equal => BinOp::Equal,
                Token::NotEqual => BinOp::NotEqual,
                Token::In => BinOp::In,
                other => {
                    return Err(
                        self.error_at_last(&format!("Unexpected comparison operator: {:?}", other))
                    )
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
        while matches!(self.peek(), Token::Plus | Token::Minus) && !self.at_statement_boundary() {
            let op = self.advance();
            let binop = match op {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Subtract,
                other => {
                    return Err(
                        self.error_at_last(&format!("Unexpected additive operator: {:?}", other))
                    )
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
        while matches!(self.peek(), Token::Star | Token::Slash) && !self.at_statement_boundary() {
            let op = self.advance();
            let binop = match op {
                Token::Star => BinOp::Multiply,
                Token::Slash => BinOp::Divide,
                other => {
                    return Err(self.error_at_last(&format!(
                        "Unexpected multiplicative operator: {:?}",
                        other
                    )))
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
                    span: self.span_from(start_span),
                }))
            }
            Token::Ampersand => {
                self.advance();
                if matches!(self.peek(), Token::Mut) {
                    self.advance();
                    let expr = self.parse_unary()?;
                    return Ok(Expr::new(ExprKind::MutBorrow {
                        expr: Box::new(expr),
                        span: self.span_from(start_span),
                    }));
                }
                let expr = self.parse_unary()?;
                Ok(Expr::new(ExprKind::Borrow {
                    expr: Box::new(expr),
                    span: self.span_from(start_span),
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
                    // ADR 0057. Postfix index is inside `[ ... ]`.
                    let saved_col = self.statement_start_column;
                    self.statement_start_column = None;
                    let index = self.parse_expr()?;
                    self.statement_start_column = saved_col;
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
                        // Method call. Bare-Var receivers keep the
                        // existing FunctionCall form for backward
                        // compatibility; anything else produces a
                        // dedicated MethodCall node.
                        self.advance();
                        // ADR 0057. Method-call arguments are inside
                        // `( ... )`.
                        let saved_col = self.statement_start_column;
                        self.statement_start_column = None;
                        let mut args = Vec::new();
                        while !matches!(self.peek(), Token::RParen | Token::Eof) {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek(), Token::Comma) {
                                self.advance();
                            }
                        }
                        self.statement_start_column = saved_col;
                        self.expect_token(Token::RParen, "')'")?;

                        expr = Expr::new({
                            // ADR 0061: a dotted call whose base is a
                            // namespace or type name stays a FunctionCall;
                            // a dotted call on a value is a MethodCall.
                            match &expr.kind {
                                ExprKind::Var(base_name, _)
                                    if is_builtin_namespace(base_name)
                                        || is_type_name_like(base_name) =>
                                {
                                    ExprKind::FunctionCall {
                                        name: format!("{}.{}", base_name, member),
                                        args,
                                        span,
                                    }
                                }
                                _ => ExprKind::MethodCall {
                                    receiver: Box::new(expr),
                                    method: member,
                                    args,
                                    span,
                                },
                            }
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
        // ADR 0054: a bare `{ ... }` in expression position is a
        // brace-form block. Route to `parse_block_expr` before
        // consuming the `{`, so the block parser sees it and takes
        // the brace-form path.
        if matches!(self.peek(), Token::LBrace) {
            return self.parse_block_expr();
        }
        match self.advance() {
            Token::If => self.parse_if_expr(),
            Token::For => self.parse_for_expr(),
            Token::While => self.parse_while_expr(),
            Token::Try => self.parse_try_catch_expr(),
            Token::Match => self.parse_match_expr(),
            Token::CharLit(c) => Ok(Expr::new(ExprKind::Char(c, start_span))),
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
                        span: self.span_from(start_span),
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
                        span: self.span_from(start_span),
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
                        span: self.span_from(start_span),
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
                        span: self.span_from(start_span),
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
                    span: self.span_from(start_span),
                }))
            }
            Token::Free => {
                self.expect_token(Token::LParen, "'('")?;
                let ptr = self.parse_expr()?;
                self.expect_token(Token::RParen, "')'")?;
                Ok(Expr::new(ExprKind::FunctionCall {
                    name: "free".to_string(),
                    args: vec![ptr],
                    span: self.span_from(start_span),
                }))
            }
            Token::LParen => {
                // ADR 0054: inside parentheses, `X { ... }` is a
                // record literal regardless of the surrounding
                // block-openable position. Clear the suppression for
                // the inner expression and restore it after.
                let saved = self.suppress_brace_literal;
                self.suppress_brace_literal = false;
                // ADR 0057. Inside `( ... )`, line breaks are
                // continuations; the statement-boundary rule is
                // inactive.
                let saved_col = self.statement_start_column;
                self.statement_start_column = None;
                let result = self.parse_expr();
                self.suppress_brace_literal = saved;
                self.statement_start_column = saved_col;
                let expr = result?;
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
                    span: self.span_from(start_span),
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
                    span: self.span_from(start_span),
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
                    span: self.span_from(start_span),
                }))
            }
            Token::LBracket => {
                // ADR 0054: inside `[ ... ]`, a `{` after an
                // identifier is unambiguously a record literal.
                let saved = self.suppress_brace_literal;
                self.suppress_brace_literal = false;
                // ADR 0057. Inside `[ ... ]`, line breaks are
                // continuations.
                let saved_col = self.statement_start_column;
                self.statement_start_column = None;
                let mut elements = Vec::new();
                let result: Result<()> = (|| {
                    while !matches!(self.peek(), Token::RBracket | Token::Eof) {
                        elements.push(self.parse_expr()?);
                        if matches!(self.peek(), Token::Comma) {
                            self.advance();
                        }
                    }
                    Ok(())
                })();
                self.suppress_brace_literal = saved;
                self.statement_start_column = saved_col;
                result?;
                self.expect_token(Token::RBracket, "']'")?;
                Ok(Expr::new(ExprKind::List(
                    elements,
                    self.span_from(start_span),
                )))
            }
            other => Err(self.error_at_last(&format!("Unexpected expression: {:?}", other))),
        }
    }

    pub(super) fn parse_identifier_expr(
        &mut self,
        mut name: String,
        ident_span: Span,
    ) -> Result<Expr> {
        // UFCS: fold `::segment` suffixes into the identifier string.
        // `Trait::method(args)` becomes `FunctionCall { name:
        // "Trait::method", args }`, and every downstream branch (the
        // function-call form, `.member`, bare `Var`) works on the
        // compound name unchanged. The analyzer splits on `::` to
        // resolve the trait and receiver. Chains like `A::B::C` fold
        // repeatedly; only the first two segments are meaningful today,
        // but the loop costs nothing.
        while matches!(self.peek(), Token::DoubleColon) {
            self.advance();
            let segment = self.expect_identifier("identifier after '::'")?;
            name = format!("{}::{}", name, segment);
        }

        if matches!(self.peek(), Token::LParen) {
            // Unchanged: function call `name(args)`.
            self.advance();
            // ADR 0057. Call arguments are inside `( ... )`.
            let saved_col = self.statement_start_column;
            self.statement_start_column = None;
            let mut args = Vec::new();
            while !matches!(self.peek(), Token::RParen | Token::Eof) {
                args.push(self.parse_expr()?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.statement_start_column = saved_col;
            self.expect_token(Token::RParen, "')'")?;
            Ok(Expr::new(ExprKind::FunctionCall {
                name,
                args,
                span: self.span_from(ident_span),
            }))
        } else if matches!(self.peek(), Token::LBracket) {
            // Unchanged: `name[index]`.
            self.advance();
            // ADR 0057. Index expressions are inside `[ ... ]`.
            let saved_col = self.statement_start_column;
            self.statement_start_column = None;
            let index = self.parse_expr()?;
            self.statement_start_column = saved_col;
            self.expect_token(Token::RBracket, "']'")?;
            Ok(Expr::new(ExprKind::ArrayAccess {
                array: Expr::boxed(ExprKind::Var(name, ident_span)),
                index: Box::new(index),
                span: self.span_from(ident_span),
            }))
        } else if name == "Map" && matches!(self.peek(), Token::LBrace) {
            // `Map { k: v, ... }` — key and value types inferred from entries.
            self.parse_map_literal(None, None, ident_span)
        } else if name == "Map"
            && matches!(self.peek(), Token::Lt)
            && self.looks_like_type_args_followed_by_brace()
        {
            // `Map<K, V> { ... }` — explicit type arguments. The empty
            // literal `Map<String, Int> {}` is only expressible this way.
            let (key_type, value_type) = self.parse_map_type_args()?;
            self.parse_map_literal(Some(key_type), Some(value_type), ident_span)
        } else if name == "Set"
            && matches!(self.peek(), Token::Lt)
            && self.looks_like_type_args_followed_by_brace()
        {
            // `Set<T> { ... }` — element type required. Unlike Map,
            // there is no `Set { ... }` brace-only form: the inferred
            // form is not supported in v1 (ADR 0032 design question 5).
            let element_type = self.parse_set_type_arg()?;
            self.parse_set_literal(element_type, ident_span)
        } else if matches!(self.peek(), Token::Lt) && self.looks_like_type_args_followed_by_brace()
        {
            // ADR 0034: generic user-type record literal.
            //   `Pair<Int> { first: 1, second: 2 }`
            //
            // The Map and Set branches above handle those builtins
            // specifically because they need custom literal parsing
            // (Map takes key:value pairs, Set takes a single element
            // list). For every other name, this branch parses a
            // standard record literal with explicit type arguments.
            //
            // The `looks_like_type_args_followed_by_brace` guard
            // requires `{` immediately after the matching `>`, so
            // `Pair < Int` (comparison) never falls into this arm.
            let type_args = self.parse_type_args()?;
            self.parse_record_literal(name, type_args, ident_span)
        } else if matches!(self.peek(), Token::LBrace) && !self.suppress_brace_literal {
            // Non-generic record literal: `User { name: "Alice" }`.
            // Must come AFTER the Set and generic-literal branches:
            // `Set<Day> { ... }` and `Pair<Int> { ... }` are otherwise
            // claimed by this general brace-literal path.
            //
            // ADR 0054: suppressed in block-openable positions. In
            // `match d {`, the `{` opens the match arm list, not a
            // record literal of `d`.
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
                return Ok(Expr::new({
                    // ADR 0061: a dotted call whose base is a namespace
                    // or type name stays a FunctionCall; a dotted call
                    // on a value is a MethodCall.
                    if is_builtin_namespace(&name) || is_type_name_like(&name) {
                        ExprKind::FunctionCall {
                            name: format!("{}.{}", name, member_name),
                            args,
                            span: self.span_from(ident_span),
                        }
                    } else {
                        ExprKind::MethodCall {
                            receiver: Expr::boxed(ExprKind::Var(name, ident_span)),
                            method: member_name,
                            args,
                            span: self.span_from(ident_span),
                        }
                    }
                }));
            }
            // `name.member` (no parens) → field access.
            Ok(Expr::new(ExprKind::FieldAccess {
                object: Expr::boxed(ExprKind::Var(name, ident_span)),
                field: member_name,
                span: self.span_from(ident_span),
            }))
        } else {
            Ok(Expr::new(ExprKind::Var(name, ident_span)))
        }
    }

    pub(super) fn parse_if_expr(&mut self) -> Result<Expr> {
        let start_span = self.last_span();
        // ADR 0054: suppress record-literal interpretation of a
        // trailing `{` after the condition. `if x > 0 {` opens a
        // block, not a literal.
        let saved = self.suppress_brace_literal;
        self.suppress_brace_literal = true;
        let condition_result = self.parse_expr();
        self.suppress_brace_literal = saved;
        let condition = Box::new(condition_result?);

        self.skip_keyword("then");

        // ADR 0055 Session 3. A `then` body is either a block
        // (indented/braced) or an inline expression. Parse inline
        // bodies with `parse_expr` first so `if cond then a + b`
        // produces a trailing expression rather than a dangling
        // `+ b`. Fall back to `parse_stmt` for statement-only
        // forms like `print(...)`, mirroring `parse_match_case`'s
        // case-body logic.
        //
        // `parse_block_expr` for the block forms; the inline try-parse
        // otherwise. Both produce a `Block` node, so the type
        // inference in `ExprKind::If` sees a uniform shape.
        let then_branch = Box::new(if matches!(self.peek(), Token::Indent | Token::LBrace) {
            self.parse_block_expr()?
        } else {
            let snapshot = self.pos;
            match self.parse_expr() {
                Ok(expr) => {
                    let span = expr.span();
                    Expr::new(ExprKind::Block {
                        statements: vec![],
                        trailing_expr: Some(Box::new(expr)),
                        span,
                    })
                }
                Err(_) => {
                    self.pos = snapshot;
                    self.statement_start_column = self.current_column();
                    let stmt = self.parse_stmt()?;
                    let span = stmt.span();
                    Expr::new(ExprKind::Block {
                        statements: vec![stmt],
                        trailing_expr: None,
                        span,
                    })
                }
            }
        });

        // ADR 0054. In brace form, `if cond { a } else { b }` is
        // often written with the `else` on its own line:
        //
        //     if cond {
        //         a
        //     } else {
        //         b
        //     }
        //
        // The newline before `else` produces an Indent/Dedent
        // pair from the lexer. Skip past them before checking for
        // `Else`; if `Else` is not there, restore the position so
        // those tokens remain visible to the caller.
        let saved_pos = self.pos;
        while matches!(self.peek(), Token::Indent | Token::Dedent) {
            self.advance();
        }

        let else_branch = if matches!(self.peek(), Token::Else) {
            self.advance();
            if matches!(self.peek(), Token::If) {
                self.advance();
                let else_if_span = self.last_span();
                let else_if_expr = self.parse_if_expr()?;
                // The inner `if` goes in `trailing_expr`, not
                // `statements`. The analyzer's Block arm types a
                // block as Void when there is no trailing
                // expression — so `else if` in expression
                // position used to produce "if branches produce
                // inconsistent results" (one branch value, the
                // other Void). This shape is what the
                // `check_all_paths_return` comment in `items.rs`
                // already assumed.
                Some(Expr::boxed(ExprKind::Block {
                    statements: vec![],
                    trailing_expr: Some(Box::new(else_if_expr)),
                    span: else_if_span,
                }))
            } else if matches!(self.peek(), Token::Match) {
                // Same shape as `else if`: an expression-position
                // `match` in the else arm. Without this branch,
                // `else match n` falls through to
                // `parse_block_expr`, which types the block as
                // Void and produces "if branches produce
                // inconsistent results" on the analyzer side.
                //
                // `parse_match_expr` expects the leading `match`
                // token already consumed — same convention as the
                // call site in `parse_stmt`.
                let else_match_span = self.current_span();
                self.advance();
                let else_match_expr = self.parse_match_expr()?;
                Some(Expr::boxed(ExprKind::Block {
                    statements: vec![],
                    trailing_expr: Some(Box::new(else_match_expr)),
                    span: else_match_span,
                }))
            } else if matches!(self.peek(), Token::Try) {
                // Same shape; `try E catch e F` is expression
                // position. `parse_try_catch_expr` expects the
                // leading `try` token already consumed, matching
                // the call site in `parse_try_catch`.
                let else_try_span = self.current_span();
                self.advance();
                let else_try_expr = self.parse_try_catch_expr()?;
                Some(Expr::boxed(ExprKind::Block {
                    statements: vec![],
                    trailing_expr: Some(Box::new(else_try_expr)),
                    span: else_try_span,
                }))
            } else if matches!(self.peek(), Token::Indent | Token::LBrace) {
                // A real block (indented or braced).
                Some(Box::new(self.parse_block_expr()?))
            } else {
                // ADR 0058 session 6. A single-expression else
                // body. Parse a full expression so a trailing
                // binary operator is consumed here, not left for
                // the caller:
                //
                //     if cond then 0
                //     else self.total() / self.count()
                //
                // Without this branch, `parse_block_expr` parses
                // `self.total()` as a statement and the `/` is
                // parsed by the caller as `(if ...) / self.count()`
                // — a different program.
                let else_span = self.current_span();
                let else_expr = self.parse_expr()?;
                Some(Expr::boxed(ExprKind::Block {
                    statements: vec![],
                    trailing_expr: Some(Box::new(else_expr)),
                    span: else_span,
                }))
            }
        } else {
            // No `else` — restore position so the Indent/Dedent we
            // skipped are still there for the caller.
            self.pos = saved_pos;
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
        // ADR 0054: same suppression as parse_if_expr.
        let saved = self.suppress_brace_literal;
        self.suppress_brace_literal = true;
        let iterable_result = self.parse_expr();
        self.suppress_brace_literal = saved;
        let iterable = Box::new(iterable_result?);
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
        // ADR 0054: same suppression as parse_if_expr.
        let saved = self.suppress_brace_literal;
        self.suppress_brace_literal = true;
        let condition_result = self.parse_expr();
        self.suppress_brace_literal = saved;
        let condition = Box::new(condition_result?);
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
        let (statements, trailing_expr) = self.parse_body_statements(true)?;
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
            span: self.span_from(start_span),
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
            span: self.span_from(start_span),
        }))
    }

    /// Parse `<T>` after the `Set` identifier. Assumes the current
    /// token is `<`.
    fn parse_set_type_arg(&mut self) -> Result<TypeSyntax> {
        self.expect_token(Token::Lt, "'<'")?;
        let element_type = self.parse_type_syntax()?;
        self.expect_token(Token::Gt, "'>'")?;
        Ok(element_type)
    }

    /// Parse `{ e1, e2, ... }` after `Set<T>`. Both empty (`{}`) and
    /// non-empty forms are accepted. Elements are comma-separated
    /// expressions; the analyzer checks them against the declared
    /// element type in A3.
    pub(super) fn parse_set_literal(
        &mut self,
        element_type: TypeSyntax,
        start_span: Span,
    ) -> Result<Expr> {
        self.expect_token(Token::LBrace, "'{'")?;
        let mut elements = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            elements.push(self.parse_expr()?);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        self.expect_token(Token::RBrace, "'}'")?;
        Ok(Expr::new(ExprKind::SetLiteral {
            element_type,
            elements,
            span: self.span_from(start_span),
        }))
    }

    /// Lookahead: from the current position (which must be `<`), scan
    /// forward to the matching `>`. Return `true` iff every intervening
    /// token is legal in a type-argument list AND the token after the
    /// matching `>` is `{`.
    ///
    /// Disambiguates `Map<K, V> { ... }` and `Set<T> { ... }` from
    /// comparisons like `Map < x`. The rule is conservative: any token
    /// that is not a type-syntax token causes an early `false`, at
    /// which point the caller falls through to treating `<` as the
    /// comparison operator.
    ///
    /// Nested type args (`Map<Int, Map<String, Bool>>`) push and pop on
    /// depth, so `>>` — which the lexer produces as two `Gt` tokens —
    /// closes both levels correctly.
    fn looks_like_type_args_followed_by_brace(&self) -> bool {
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

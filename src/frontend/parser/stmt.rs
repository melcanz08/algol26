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
            Token::Star => self.parse_deref_assign_or_expr(),
            Token::Identifier(name) => self.parse_identifier_stmt(name),
            _other => {
                let expr = self.parse_expr()?;
                Ok(Stmt::Expression(expr))
            }
        }
    }

    /// ADR 0056. `*r := v` (or `*r.f := v`, `*r[i] := v`).
    ///
    /// `*expr` in expression position is already parsed by
    /// `parse_expr` as `ExprKind::Deref`. This entry point parses
    /// the deref expression, then checks whether it is followed by
    /// the assignment token. If so, the statement is a write
    /// through the reference; if not, the deref expression is a
    /// bare expression statement (`*r` alone, `*r + 1`, etc.).
    ///
    /// No backtracking is needed: `parse_expr` consumes exactly
    /// the deref production, and any binary operator that follows
    /// (`*r + 1`) yields an `ExprKind::Binary` whose `Assign`-check
    /// fails the same way `a + b := c` does.
    pub(super) fn parse_deref_assign_or_expr(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        let target = self.parse_expr()?;
        if matches!(self.peek(), Token::Assign) {
            self.advance();
            let value = self.parse_expr()?;
            Ok(Stmt::DerefAssign {
                target,
                value,
                span: start_span,
            })
        } else {
            Ok(Stmt::Expression(target))
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

    /// ADR 0054. Parse a block body in either notation:
    ///
    ///   * **Indentation form** — `Indent stmts... Dedent` (with an
    ///     optional trailing `End` for constructs that carry one).
    ///   * **Brace form** — `{ stmts... }`.
    ///   * **Single-statement form** — a lone statement, as in
    ///     `if cond then x` on one line.
    ///
    /// `allow_trailing` controls whether the last statement, if it
    /// is an expression of a form whose value can be the block's,
    /// is extracted as the trailing expression. `parse_block` (used
    /// by statement-position constructs like `for` and `unsafe`)
    /// passes `false`; `parse_block_expr` and `parse_loop_body`
    /// pass `true`.
    pub(super) fn parse_body_statements(
        &mut self,
        allow_trailing: bool,
    ) -> Result<(Vec<Stmt>, Option<Box<Expr>>)> {
        let mut statements = Vec::new();

        if matches!(self.peek(), Token::Indent) {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::End | Token::Eof) {
                self.statement_start_column = self.current_column();
                statements.push(self.parse_stmt()?);
            }
            if matches!(self.peek(), Token::Dedent) {
                self.advance();
            }
            if matches!(self.peek(), Token::End) {
                self.advance();
            }
            // ADR 0054: a brace-form block whose body is indented
            // emits `Indent ... Dedent }`. The Indent branch has
            // consumed up to and including Dedent; consume the
            // closing `}` too so the helper is symmetric with the
            // LBrace branch below.
            if matches!(self.peek(), Token::RBrace) {
                self.advance();
            }

        } else if matches!(self.peek(), Token::LBrace) {
            // ADR 0054: brace-form body. `{ stmts... }`.
            //
            // The lexer still emits `Indent`/`Dedent` for indented
            // lines inside braces, because it does not track brace
            // depth. Skip them here — inside a brace block, the
            // delimiters carry the structure and indentation is
            // cosmetic.
            self.advance();
            while matches!(self.peek(), Token::Indent | Token::Dedent) {
                self.advance();
            }

            // Try to parse the whole brace content as a single
            // expression. If `parse_expr` consumes everything up
            // to the closing `}`, the block is an expression whose
            // value is that expression — `{ x + 1 }`,
            // `{ self.total() / self.count() }`,
            // `{ if c { a } else { b } }`.
            //
            // Without this try-parse, `parse_stmt` on `self.total()
            // / self.count()` parses `self.total()` as one
            // statement, leaves `/` dangling, and `parse_stmt` on
            // the next iteration fails on `/`.
            //
            // Backtracking is safe here: `parse_expr` only fails
            // fast (on the first token if it is not a primary), or
            // succeeds and consumes to `}`. No side effects, no
            // state to undo beyond `self.pos`.
            // Try to parse the block's content as a single
            // trailing expression — but only when the caller wants
            // a trailing expression. `parse_block` (used by `proc`
            // and `fn` bodies where the result is discarded) passes
            // `allow_trailing = false` and must not have its whole
            // body silently collapsed into a discarded trailing
            // expression.
            if allow_trailing {
                let snapshot = self.pos;
                let try_result = self.parse_expr();
                if try_result.is_ok() {
                    while matches!(self.peek(), Token::Indent | Token::Dedent) {
                        self.advance();
                    }
                }
                match try_result {
                    Ok(expr) if matches!(self.peek(), Token::RBrace) => {
                        // Consumed everything up to the closing brace.
                        self.advance();
                        return Ok((vec![], Some(Box::new(expr))));
                    }
                    _ => {
                        self.pos = snapshot;
                    }
                }
            }

            // Fall back to statement-by-statement parsing.
            while !matches!(self.peek(), Token::RBrace | Token::Eof) {
                self.statement_start_column = self.current_column();
                statements.push(self.parse_stmt()?);
                while matches!(self.peek(), Token::Indent | Token::Dedent) {
                    self.advance();
                }
            }
            self.expect_token(Token::RBrace, "'}'")?;
        } else if matches!(self.peek(), Token::End) {
            self.advance();
        } else if !matches!(self.peek(), Token::Eof | Token::Dedent | Token::End) {
            // Single-statement body: `if cond then x`.
            self.statement_start_column = self.current_column();
            statements.push(self.parse_stmt()?);
        }

        // ADR 0054 follow-up. Any expression at the end of a
        // block in expression position (`allow_trailing == true`)
        // is the block's value — including `if`, `match`, `while`,
        // `for`, and `try` (they are expressions in ALGOL26).
        //
        // Previously the check was inverted: it excluded exactly
        // those expression-position constructs, so a nested `if`
        // at the end of an if-branch block was typed as `Void` and
        // the outer `if` reported "one branch yields a value, the
        // other does not."
        //
        // A `Void`-valued expression like `print(...)` at the end
        // of a block still ends up as the block's value, which the
        // analyzer's Block arm types as `Void` — the same result
        // as not promoting it. So the promotion is safe for every
        // expression kind.
        let mut trailing_expr: Option<Box<Expr>> = None;
        if allow_trailing {
            if let Some(Stmt::Expression(_)) = statements.last() {
                if let Some(Stmt::Expression(expr)) = statements.pop() {
                    trailing_expr = Some(Box::new(expr));
                }
            }
        }

        Ok((statements, trailing_expr))
    }

    pub(super) fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        let (statements, _) = self.parse_body_statements(false)?;
        Ok(statements)
    }

    pub(super) fn parse_loop_body(&mut self) -> Result<(Vec<Stmt>, Option<Box<Expr>>)> {
        // ADR 0054. Same block grammar as `parse_body_statements`,
        // but the "single-statement" branch parses an expression,
        // not a statement, so `while cond do i := i + 1` produces a
        // trailing expression.
        if matches!(self.peek(), Token::Indent) || matches!(self.peek(), Token::LBrace) {
            let (mut body, trailing) = self.parse_body_statements(true)?;
            // ADR 0054 follow-up. A loop body runs as a statement
            // list, one iteration at a time. The last expression of
            // the block is a *statement* that executes on each
            // iteration — not the loop's value (loops don't yield
            // values in ALGOL26; `for`/`while` are always Void). So
            // any trailing expression that
            // `parse_body_statements(true)` produced must be moved
            // back into the body before returning.
            //
            // This fixes the case where a `match` or `if` at the end
            // of a loop body was classified as a trailing value and
            // silently not executed by the loop desugar.
            if let Some(expr) = trailing {
                body.push(Stmt::Expression(*expr));
            }
            return Ok((body, None));
        }
        if matches!(self.peek(), Token::End) {
            self.advance();
            return Ok((Vec::new(), None));
        }
        let expr = self.parse_expr()?;
        Ok((Vec::new(), Some(Box::new(expr))))
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
        // ADR 0054: suppress record-literal interpretation of a
        // trailing `{` after the matched value. `match d {` opens
        // the arm list, not a literal.
        let saved = self.suppress_brace_literal;
        self.suppress_brace_literal = true;
        let value_result = self.parse_expr();
        self.suppress_brace_literal = saved;
        let value = value_result?;

        let mut cases = Vec::new();

        // ADR 0054. The arm list accepts either an indented block or
        // a `{ ... }` block. `closes_with_brace` tracks which one so
        // the closer is the right token.
        let closes_with_brace = matches!(self.peek(), Token::LBrace);

        if matches!(self.peek(), Token::Indent) || closes_with_brace {
            self.advance();

            let arm_terminator = if closes_with_brace {
                Token::RBrace
            } else {
                Token::Dedent
            };

            if closes_with_brace {
                while matches!(self.peek(), Token::Indent | Token::Dedent) {
                    self.advance();
                }
            }

            while !matches!(self.peek(), t if *t == arm_terminator)
                && !matches!(self.peek(), Token::Eof)
            {
                if !matches!(self.peek(), Token::Case) {
                    return Err(self.error("Expected 'case' in match arm"));
                }
                let case_span = self.current_span();
                self.advance();

                // ADR 0054. When the arm is brace-delimited, the
                // pattern is followed by `then` before the body's `{`:
                //
                //     match d {
                //         case A then { 1 }
                //         case Point { x, y } then { x + y }
                //     }
                //
                // Without `then`, `case A {` is ambiguous: the
                // pattern parser takes `A {` as a `Pattern::Record`.
                // With `then`, the pattern and the body are cleanly
                // separated, matching `if cond then { ... }`.
                //
                // In the indentation form, `then` is not required —
                // the body is on the following indented line, and
                // the pattern ends at the newline.
                //
                // `then` is currently lexed as an identifier (see
                // `skip_keyword`), not a reserved token, so we skip
                // it by name.
                let saved = self.suppress_brace_literal;
                self.suppress_brace_literal = true;
                let pattern_result = self.parse_pattern();
                self.suppress_brace_literal = saved;
                let mut pattern = pattern_result?;

                if matches!(self.peek(), Token::If) {
                    self.advance();
                    let condition = self.parse_expr()?;
                    pattern = Pattern::Guarded {
                        pattern: Box::new(pattern),
                        condition: Box::new(condition),
                    };
                }

                // Consume `then` if present. In brace form it is
                // required; in indentation form it is optional (the
                // lexer would have turned `then` at end-of-line into
                // an identifier, and `skip_keyword` looks it up by
                // name).
                self.skip_keyword("then");

                // ADR 0055 Session 3. An inline case body is
                // either an expression (`a + b`, `Int.to_string(n)`)
                // or a statement form (`print(...)`). Try the
                // expression parser first — it handles binary
                // operators — and fall back to the statement
                // parser for statement-only forms like `print`.
                // Backtracking is safe: `parse_expr` fails fast
                // on `Token::Print` because `parse_primary` doesn't
                // handle it as a leading token.
                let body = if matches!(self.peek(), Token::Indent | Token::LBrace) {
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
                };

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

                if closes_with_brace {
                    while matches!(self.peek(), Token::Indent | Token::Dedent) {
                        self.advance();
                    }
                }
            }

            if matches!(self.peek(), t if *t == arm_terminator) {
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
        // ADR 0054. A bare `return` is followed by `Dedent` in
        // indentation form and by `RBrace` in brace form. Both mean
        // "no return value". Before this check included `RBrace`,
        // the brace-form bare return tried to parse `}` as an
        // expression and errored at the closing brace.
        let value = if matches!(self.peek(), Token::Eof | Token::Dedent | Token::RBrace) {
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
                // Continue the postfix chain so `foo().bar()` and
                // `foo().field` parse as a single expression
                // statement.
                let call = Expr::new(ExprKind::FunctionCall {
                    name,
                    args,
                    span: ident_span,
                });
                let expr = self.parse_postfix(call)?;
                Ok(Stmt::Expression(expr))
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
                    // Continue the postfix chain so `arr[0].method()`
                    // and `arr[0].field` parse as a single expression
                    // statement.
                    let arr_expr = Expr::new(ExprKind::ArrayAccess {
                        array: Expr::boxed(ExprKind::Var(name, ident_span)),
                        index: Box::new(index),
                        span: ident_span,
                    });
                    let expr = self.parse_postfix(arr_expr)?;
                    Ok(Stmt::Expression(expr))
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
                //
                // Continue the postfix chain so `b.items.append(2)`,
                // `p.x(y).z()`, and `p.x(y).field` parse as a single
                // expression statement. The `LParen` and `LBracket`
                // branches above already do this; the Dot branch was
                // the one that returned early, stranding anything
                // after the first `member(...)`.
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
                    let call = Expr::new(ExprKind::FunctionCall {
                        name: format!("{}.{}", name, member_name),
                        args,
                        span: ident_span,
                    });
                    let expr = self.parse_postfix(call)?;
                    return Ok(Stmt::Expression(expr));
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

                // `p.x` (bare, no parens, no assign) in statement
                // position — a record field read *or* a zero-arg
                // method call written without parens. The parser
                // cannot tell which; the analyzer resolves by type.
                //
                // Build a `FieldAccess` node, which the analyzer's
                // `FieldAccess` arm already disambiguates (records
                // use field lookup; every other receiver falls
                // through to zero-arg method dispatch). The
                // alternative — a `FunctionCall { name: "p.x" }` —
                // forces the analyzer to guess "method" and fails
                // for fields: `b.items.append(2)` reports
                // "Type Bag does not have method 'items'".
                //
                // Then continue the postfix chain so `b.items.append(2)`
                // sees the `.append` that follows.
                let field_expr = Expr::new(ExprKind::FieldAccess {
                    object: Expr::boxed(ExprKind::Var(name, ident_span)),
                    field: member_name,
                    span: ident_span,
                });
                let expr = self.parse_postfix(field_expr)?;
                Ok(Stmt::Expression(expr))
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

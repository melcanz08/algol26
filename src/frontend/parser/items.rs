// src/frontend/parser/items.rs

use super::*;
use crate::frontend::ast::{ImplConst, ReceiverMode, TraitConst, Visibility};

impl Parser {
    pub fn parse_program(&mut self) -> Result<Program> {
        let mut functions = Vec::new();
        let mut traits = Vec::new();
        let mut impls = Vec::new();
        let mut top_level_imports = Vec::new();
        let mut records = Vec::new(); // NEW
        let mut distinct_decls = Vec::new();
        let mut enum_decls = Vec::new();
        let mut subrange_decls = Vec::new();

        while !matches!(self.peek(), Token::Eof) {
            // ADR 0039. Optional leading `pub` on any declaration.
            // Determine the declaration kind by looking past it; each
            // parser consumes the `pub` itself.
            let pub_offset = if matches!(self.peek(), Token::Identifier(s) if s == "pub") {
                1
            } else {
                0
            };
            let lookahead: &Token = self
                .tokens
                .get(self.pos + pub_offset)
                .map(|ti| &ti.token)
                .unwrap_or(&Token::Eof);

            if matches!(lookahead, Token::Trait) {
                traits.push(self.parse_trait()?);
            } else if matches!(lookahead, Token::Impl) {
                if pub_offset == 1 {
                    return Err(self.error(
                        "`pub` is not allowed on `impl` blocks; visibility \
                         is a property of the item an impl attaches to, \
                         not of the impl itself",
                    ));
                }
                impls.push(self.parse_impl()?);
            } else if matches!(lookahead, Token::Rec) {
                records.push(self.parse_record_decl()?);
            } else if matches!(lookahead, Token::Identifier(s) if s == "type") {
                // `type Name distinct Base` or `type Name Base in Low..High`.
                let after_name = self
                    .tokens
                    .get(self.pos + pub_offset + 2)
                    .map(|ti| &ti.token);
                if matches!(after_name, Some(Token::Identifier(s)) if s == "distinct") {
                    distinct_decls.push(self.parse_distinct_decl()?);
                } else {
                    subrange_decls.push(self.parse_subrange_decl()?);
                }
            } else if matches!(lookahead, Token::Identifier(s) if s == "enum") {
                enum_decls.push(self.parse_enum_decl()?);
            } else if matches!(lookahead, Token::Proc | Token::Function | Token::Extern) {
                functions.push(self.parse_function()?);
            } else if matches!(lookahead, Token::Import) {
                if pub_offset == 1 {
                    return Err(self.error("`pub` is not allowed on `import` statements"));
                }
                self.advance();
                let path = match self.advance() {
                    Token::StringLit(s) => s,
                    Token::Identifier(s) => s,
                    other => {
                        return Err(self.error(&format!("Expected import path, found {:?}", other)))
                    }
                };
                top_level_imports.push(path);
            } else {
                return Err(
                    self.error(&format!("Unexpected token at top level: {:?}", self.peek()))
                );
            }
        }

        Ok(Program {
            imports: top_level_imports,
            functions,
            traits,
            impls,
            records, // NEW
            distinct_decls,
            enum_decls,
            subrange_decls,
        })
    }

    /// Parse `type Name Base in Low..High`. Consumes the leading
    /// `type` identifier.
    ///
    /// `Base` is a type annotation (`Int` or an enum name). `Low`
    /// and `High` are single atoms — an Int literal or an
    /// identifier — because bounds are restricted to literals and
    /// variant names. Writing a full expression as a bound is
    /// rejected at parse time with a clear message.
    ///
    /// The `distinct` path is chosen by the caller
    /// (`parse_program`) via a two-token lookahead; this method
    /// assumes the distinct form did not match.
    pub(super) fn parse_subrange_decl(&mut self) -> Result<SubrangeDecl> {
        let visibility = self.try_parse_visibility();
        let start_span = self.current_span();

        // Consume `type` (an Identifier, not a reserved keyword).
        self.advance();

        let name = self.expect_identifier("type name")?;
        let base = self.parse_type_syntax()?;

        // `in` is a reserved token (`for x in list`). If we're not
        // looking at `in` here, the user either wrote `type X Int`
        // (an alias, not yet supported) or made a mistake. Name both
        // legal forms so the diagnostic is actionable regardless of
        // which one they intended.
        if !matches!(self.peek(), Token::In) {
            return Err(self.error(&format!(
                "expected `distinct` (for a nominal type) or `in` \
                 (for a subrange) after the base type of '{}'",
                name
            )));
        }
        self.advance();

        let low = self.parse_subrange_bound()?;
        if !matches!(self.peek(), Token::DotDot) {
            return Err(self.error("expected `..` between subrange bounds"));
        }
        self.advance();
        let high = self.parse_subrange_bound()?;

        Ok(SubrangeDecl {
            name,
            base,
            low,
            high,
            span: self.span_from(start_span),
            visibility,
            module: None,
        })
    }

    /// A single subrange bound. Int literals and identifiers only;
    /// anything else is an error. See `parse_subrange_decl`.
    fn parse_subrange_bound(&mut self) -> Result<Expr> {
        let span = self.current_span();
        match self.advance() {
            Token::IntLit(n) => Ok(Expr::new(ExprKind::Int(n, span))),
            Token::Identifier(name) => Ok(Expr::new(ExprKind::Var(name, span))),
            other => Err(self.error(&format!(
                "subrange bound must be an integer literal or an enum variant name, found {:?}",
                other
            ))),
        }
    }

    /// Parse `enum Name` followed by one variant per indented line.
    /// Consumes the leading `enum` identifier. Requires at least one
    /// variant.
    ///
    /// `enum` is not a reserved keyword; it is matched as an
    /// `Identifier`. This mirrors `type` (ADR 0029) so existing
    /// programs that use `enum` as a variable name keep working.
    pub(super) fn parse_enum_decl(&mut self) -> Result<EnumDecl> {
        let visibility = self.try_parse_visibility();
        let start_span = self.current_span();

        // Consume `enum` (an Identifier, not a reserved keyword).
        self.advance();

        let name = self.expect_identifier("enum name")?;

        let mut variants = Vec::new();
        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                let variant = self.expect_identifier("variant name")?;
                variants.push(variant);
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }

        if variants.is_empty() {
            return Err(self.error(&format!(
                "enum '{}' must declare at least one variant",
                name
            )));
        }

        Ok(EnumDecl {
            name,
            variants,
            span: start_span,
            visibility,
            module: None,
        })
    }

    /// Parse `type Name = distinct BaseType`. Consumes the leading
    /// `type` identifier.
    ///
    /// The `distinct` modifier is required. `type X = Y` (a type
    /// alias) is reserved for a future ADR and is rejected here with
    /// a message that names the currently supported form.
    pub(super) fn parse_distinct_decl(&mut self) -> Result<DistinctDecl> {
        let visibility = self.try_parse_visibility();
        let start_span = self.current_span();

        // Consume `type` (an Identifier, not a reserved keyword).
        self.advance();

        let name = self.expect_identifier("type name")?;

        // `distinct` is required. `type X = Int` (a type alias) is
        // not implemented in this ADR. There is no `=` separator:
        // the lexer reserves bare `=` for future use and rejects it
        // with a message pointing at `:=` and `==`.
        match self.peek().clone() {
            Token::Identifier(s) if s == "distinct" => {
                self.advance();
            }
            other => {
                return Err(self.error(&format!(
                    "Expected `distinct` after `=` in type declaration, found {:?}.                      Type aliases (`type X = Y`) are not yet supported;                      use `type X = distinct Y` (see ADR 0029).",
                    other
                )));
            }
        }

        let base = self.parse_type_syntax()?;

        Ok(DistinctDecl {
            name,
            base,
            span: self.span_from(start_span),
            visibility,
            module: None,
        })
    }

    pub(super) fn parse_function(&mut self) -> Result<FunctionDecl> {
        // ADR 0039. Top-level functions and impl methods may carry a
        // leading `pub`. Trait-method and trait-impl rejection is
        // enforced by the callers (parse_trait and parse_impl).
        let visibility = self.try_parse_visibility();

        let is_extern = matches!(self.peek(), Token::Extern);
        let mut ffi_info = None;

        if is_extern {
            self.advance();
            let mut info = ExternDecl::default();
            if let Token::StringLit(abi) = self.peek().clone() {
                self.advance();
                info.abi = Some(abi);
            }
            ffi_info = Some(info);
        }

        let is_function = matches!(self.peek(), Token::Function);
        if !is_function && !matches!(self.peek(), Token::Proc) {
            return Err(self.error("Expected 'function' or 'proc'"));
        }
        self.advance();

        let name = self.expect_identifier("function name")?;

        // generic type parameters
        let mut type_params = Vec::new();
        if matches!(self.peek(), Token::Lt) {
            self.advance();
            while !matches!(self.peek(), Token::Gt | Token::Eof) {
                let type_param = self.expect_identifier("type parameter")?;
                type_params.push(type_param);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::Gt, "'>'")?;
        }

        // parameters
        let mut params = Vec::new();
        let mut receiver: Option<ReceiverMode> = None;
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            while !matches!(self.peek(), Token::RParen | Token::Eof) {
                if matches!(self.peek(), Token::Ellipsis) {
                    self.advance();
                    if let Some(ref mut info) = ffi_info {
                        info.variadic = true;
                    }
                    break;
                }
                let param_name = self.expect_identifier("parameter name")?;
                let param_type = self.parse_type_annotation()?;

                // `self` in first position: derive ReceiverMode from the type.
                if param_name == "self" && params.is_empty() {
                    receiver = Some(match &param_type {
                        Some(TypeSyntax::Named(_)) => ReceiverMode::Consume,
                        Some(TypeSyntax::Generic { name, args })
                            if args.len() == 1 && name.eq_ignore_ascii_case("borrow") =>
                        {
                            ReceiverMode::Shared
                        }
                        Some(TypeSyntax::Generic { name, args })
                            if args.len() == 1
                                && (name.eq_ignore_ascii_case("mutborrow")
                                    || name.eq_ignore_ascii_case("mut_borrow")) =>
                        {
                            ReceiverMode::Exclusive
                        }
                        // Any other type — including a generic name like `Box<T>`
                        // — is a by-value receiver. ADR 0034.
                        Some(_) => ReceiverMode::Consume,
                        _ => {
                            return Err(
                                self.error("`self` must have a type annotation (T, &T, or &mut T)")
                            )
                        }
                    });
                }

                params.push((param_name, param_type));
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::RParen, "')'")?;
        }

        // return type
        let return_type = if is_function {
            if matches!(self.peek(), Token::Arrow | Token::Colon) {
                self.advance();
                Some(self.parse_type_syntax()?)
            } else {
                None
            }
        } else {
            None
        };

        // where clauses
        let mut where_clauses = Vec::new();
        if matches!(self.peek(), Token::Where) {
            self.advance();
            loop {
                let type_param = self.expect_identifier("type parameter")?;
                if matches!(self.peek(), Token::Colon) {
                    self.advance();
                    let trait_name = self.expect_identifier("trait name")?;
                    where_clauses.push(WhereClause {
                        type_param,
                        trait_name,
                    });
                }
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        // FFI clauses
        if let Some(mut info) = ffi_info {
            if matches!(self.peek(), Token::From) {
                self.advance();
                info.library = match self.advance() {
                    Token::StringLit(s) => Some(s),
                    Token::Identifier(s) => Some(s),
                    _ => None,
                };
            }
            if matches!(self.peek(), Token::As) {
                self.advance();
                info.symbol_name = match self.advance() {
                    Token::StringLit(s) => Some(s),
                    Token::Identifier(s) => Some(s),
                    _ => None,
                };
            }
            ffi_info = Some(info);
        }

        // body
        let body = if is_extern {
            Vec::new()
        } else {
            self.parse_block()?
        };

        Ok(FunctionDecl {
            name,
            params,
            return_type,
            body,
            is_extern,
            ffi_info,
            type_params,
            where_clauses,
            receiver,
            visibility,
            module: None,
        })
    }

    pub(super) fn parse_trait(&mut self) -> Result<TraitDecl> {
        let visibility = self.try_parse_visibility();
        self.advance();
        let name = self.expect_identifier("trait name")?;
        let mut methods = Vec::new();
        let mut constants = Vec::new();
        let mut associated_types: Vec<String> = Vec::new();
        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                // ADR 0039. Trait methods are public by definition.
                // A redundant `pub` is rejected so the source matches
                // the model.
                if matches!(self.peek(), Token::Identifier(s) if s == "pub") {
                    return Err(self.error(
                        "`pub` is not allowed on trait methods; a trait \
                         method is public by definition",
                    ));
                }
                // ADR 0041. Associated type declaration: `type Name`.
                // The concrete type is supplied by each impl.
                if matches!(self.peek(), Token::Identifier(s) if s == "type") {
                    self.advance();
                    let assoc_name = self.expect_identifier("associated type name")?;
                    associated_types.push(assoc_name);
                    continue;
                }
                // Associated constant declaration: `const NAME: Type`.
                // Value is supplied by each impl (step 1c).
                if matches!(self.peek(), Token::Const) {
                    self.advance();
                    let cname = self.expect_identifier("constant name")?;
                    self.expect_token(Token::Colon, "':'")?;
                    let ctype = self.parse_type_syntax()?;
                    constants.push(TraitConst {
                        name: cname,
                        type_: ctype,
                    });
                    continue;
                }
                let is_function = matches!(self.peek(), Token::Function);
                if !is_function && !matches!(self.peek(), Token::Proc) {
                    return Err(self.error("Expected 'function' or 'const' in trait body"));
                }
                self.advance();
                let method_name = self.expect_identifier("method name")?;
                let mut params = Vec::new();
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    while !matches!(self.peek(), Token::RParen | Token::Eof) {
                        let param_name = self.expect_identifier("parameter name")?;
                        let param_type = self.parse_type_annotation()?;
                        params.push((param_name, param_type));
                        if matches!(self.peek(), Token::Comma) {
                            self.advance();
                        }
                    }
                    self.expect_token(Token::RParen, "')'")?;
                }
                let return_type = if is_function {
                    if matches!(self.peek(), Token::Arrow | Token::Colon) {
                        self.advance();
                        Some(self.parse_type_syntax()?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                methods.push(TraitMethod {
                    name: method_name,
                    params,
                    return_type,
                });
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }
        Ok(TraitDecl {
            name,
            methods,
            constants,
            visibility,
            module: None,
            associated_types,
        })
    }

    /// Parse an optional `where T: Trait, U: Trait2` clause list.
    /// Returns an empty vector when the current token is not
    /// `where`. Used by `parse_impl`; `parse_function` has an
    /// equivalent inline block that will be replaced by a call to
    /// this helper in a follow-up cleanup.
    fn parse_where_clauses(&mut self) -> Result<Vec<WhereClause>> {
        let mut where_clauses = Vec::new();
        if matches!(self.peek(), Token::Where) {
            self.advance();
            loop {
                let type_param = self.expect_identifier("type parameter")?;
                if matches!(self.peek(), Token::Colon) {
                    self.advance();
                    let trait_name = self.expect_identifier("trait name")?;
                    where_clauses.push(WhereClause {
                        type_param,
                        trait_name,
                    });
                }
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        Ok(where_clauses)
    }

    pub(super) fn parse_impl(&mut self) -> Result<ImplBlock> {
        self.advance(); // consume `impl`

        // ADR 0034: optional impl-level type parameters.
        //   impl<T> Trait for Type<T>
        //   impl<T> Type<T>
        let mut type_params = Vec::new();
        if matches!(self.peek(), Token::Lt) {
            self.advance();
            while !matches!(self.peek(), Token::Gt | Token::Eof) {
                type_params.push(self.expect_identifier("type parameter")?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::Gt, "'>'")?;
        }

        let first_ident = self.expect_identifier("trait or type name")?;

        let (trait_name, target_type, target_type_args) = if matches!(self.peek(), Token::For) {
            self.advance();
            let target = self.expect_identifier("target type")?;
            let args = self.parse_type_args()?;
            (Some(first_ident), target, args)
        } else {
            let args = self.parse_type_args()?;
            (None, first_ident, args)
        };

        // Optional where clause: `impl<T> Trait for List<T> where T: Ord`.
        let where_clauses = self.parse_where_clauses()?;

        let mut methods = Vec::new();
        let mut constants = Vec::new();
        let mut associated_types: Vec<(String, TypeSyntax)> = Vec::new();
        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                // ADR 0041. Associated type definition:
                // `type Name := ConcreteType`. The ADR's syntax
                // summary writes `=`, but a bare `=` is not a
                // token in this language (see ADR 0040's rationale
                // for rejecting it). `:=` matches the impl body's
                // existing `const NAME: Type := expr` form.
                if matches!(self.peek(), Token::Identifier(s) if s == "type") {
                    self.advance();
                    let assoc_name = self.expect_identifier("associated type name")?;
                    self.expect_token(Token::Assign, "':='")?;
                    let concrete = self.parse_type_syntax()?;
                    associated_types.push((assoc_name, concrete));
                    continue;
                }
                // Associated constant definition:
                // `const NAME: Type := expr`. `:=` matches the
                // language's val/var binding style; a bare `=`
                // would need its own lexer token.
                if matches!(self.peek(), Token::Const) {
                    self.advance();
                    let cname = self.expect_identifier("constant name")?;
                    self.expect_token(Token::Colon, "':'")?;
                    let ctype = self.parse_type_syntax()?;
                    self.expect_token(Token::Assign, "':='")?;
                    let value = self.parse_expr()?;
                    constants.push(ImplConst {
                        name: cname,
                        type_: ctype,
                        value,
                    });
                    continue;
                }
                let method = self.parse_function()?;
                // ADR 0039. A method that satisfies a trait method
                // inherits the trait's public visibility; a redundant
                // `pub` is rejected. Inherent impl methods respect the
                // default-private rule and may carry `pub`.
                if trait_name.is_some() && method.visibility == Visibility::Public {
                    return Err(self.error(
                        "`pub` is not allowed on a trait impl method; \
                         the trait method is already public",
                    ));
                }
                methods.push(method);
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }

        Ok(ImplBlock {
            trait_name,
            type_params,
            target_type,
            target_type_args,
            methods,
            constants,
            where_clauses,
            module: None,
            associated_types,
        })
    }

    /// Parse an optional `<T1, T2, ...>` type-argument list. Returns an
    /// empty vector when the current token isn't `<`.
    pub(super) fn parse_type_args(&mut self) -> Result<Vec<TypeSyntax>> {
        let mut args = Vec::new();
        if matches!(self.peek(), Token::Lt) {
            self.advance();
            while !matches!(self.peek(), Token::Gt | Token::Eof) {
                args.push(self.parse_type_syntax()?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::Gt, "'>'")?;
        }
        Ok(args)
    }

    pub(super) fn parse_record_decl(&mut self) -> Result<RecordDecl> {
        let visibility = self.try_parse_visibility();
        let start_span = self.current_span();
        self.advance(); // consume `rec`

        let name = self.expect_identifier("record name")?;

        // Optional type parameters: rec Pair<T>
        let mut type_params = Vec::new();
        if matches!(self.peek(), Token::Lt) {
            self.advance();
            while !matches!(self.peek(), Token::Gt | Token::Eof) {
                type_params.push(self.expect_identifier("type parameter")?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::Gt, "'>'")?;
        }

        // Body: indented `field: Type` lines. Same shape as `parse_trait`.
        let mut fields = Vec::new();
        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                // ADR 0039. Per-field `pub`; default private.
                let field_vis = self.try_parse_visibility();
                let field_name = self.expect_identifier("field name")?;
                self.expect_token(Token::Colon, "':'")?;
                let field_type = self.parse_type_syntax()?;
                fields.push((field_name, field_type, field_vis));
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }

        Ok(RecordDecl {
            name,
            type_params,
            fields,
            span: start_span,
            visibility,
            module: None,
        })
    }

    pub(super) fn parse_import(&mut self) -> Result<Stmt> {
        let start_span = self.current_span();
        self.advance();
        let path = match self.advance() {
            Token::StringLit(s) => s,
            Token::Identifier(s) => s,
            other => return Err(self.error(&format!("Expected import path, found {:?}", other))),
        };
        Ok(Stmt::Import {
            path,
            span: start_span,
        })
    }
}

// algol26/src/frontend/parser.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::span::Span;
use crate::frontend::ast::{
    BinOp, Expr, ExternDecl, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern, Program, Stmt,
    TraitDecl, TraitMethod, TypeSyntax, UnaryOp, WhereClause,
};
use crate::frontend::lexer::Token;

#[derive(Clone, Debug)]
struct TokenInfo {
    token: Token,
    line: usize,
    column: usize,
}

pub struct Parser {
    tokens: Vec<TokenInfo>,
    pos: usize,
    span_map: std::collections::HashMap<usize, (usize, usize)>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self::new_with_positions(tokens, Vec::new())
    }

    pub fn new_with_positions(tokens: Vec<Token>, positions: Vec<(usize, usize)>) -> Self {
        let token_infos = tokens
            .into_iter()
            .enumerate()
            .map(|(i, token)| {
                let (line, column) = positions.get(i).copied().unwrap_or((0, 0));
                TokenInfo {
                    token,
                    line,
                    column,
                }
            })
            .collect();

        Parser {
            tokens: token_infos,
            pos: 0,
            span_map: std::collections::HashMap::new(),
        }
    }

    pub fn get_span_map(&self) -> &std::collections::HashMap<usize, (usize, usize)> {
        &self.span_map
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|ti| &ti.token)
            .unwrap_or(&Token::Eof)
    }

    fn peek_info(&self) -> &TokenInfo {
        self.tokens.get(self.pos).unwrap_or(&TokenInfo {
            token: Token::Eof,
            line: 0,
            column: 0,
        })
    }

    fn advance(&mut self) -> Token {
        let info = self.tokens.get(self.pos).cloned().unwrap_or(TokenInfo {
            token: Token::Eof,
            line: 0,
            column: 0,
        });
        self.pos += 1;
        info.token
    }

    fn error(&self, message: &str) -> CompileError {
        let info = self.peek_info();
        CompileError::simple(message, info.line, info.column, "", ErrorCode::E0001)
    }

    fn skip_keyword(&mut self, keyword: &str) {
        if let Token::Identifier(id) = self.peek() {
            if id == keyword {
                self.advance();
            }
        }
    }

    fn expect_identifier(&mut self, context: &str) -> Result<String> {
        match self.advance() {
            Token::Identifier(n) => Ok(n),
            other => Err(self.error(&format!("Expected {}, found {:?}", context, other))),
        }
    }

    fn expect_token(&mut self, expected: Token, context: &str) -> Result<()> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(())
        } else {
            Err(self.error(&format!("Expected {}, found {:?}", context, self.peek())))
        }
    }

    fn parse_type_syntax(&mut self) -> Result<TypeSyntax> {
        if matches!(self.peek(), Token::Ampersand) {
            self.advance(); // consume &

            if let Token::Identifier(ref s) = self.peek() {
                if s == "mut" {
                    self.advance(); // consume mut
                    let inner = self.parse_type_syntax()?;
                    return Ok(TypeSyntax::Generic {
                        name: "MutBorrow".to_string(),
                        args: vec![inner],
                    });
                }
            }

            let inner = self.parse_type_syntax()?;
            return Ok(TypeSyntax::Generic {
                name: "Borrow".to_string(),
                args: vec![inner],
            });
        }

        if matches!(self.peek(), Token::SelfType) {
            self.advance();
            return Ok(TypeSyntax::Named("Self".to_string()));
        }
        let base = self.expect_identifier("type name")?;
        if matches!(self.peek(), Token::LBracket | Token::Lt) {
            let is_bracket = matches!(self.peek(), Token::LBracket);
            self.advance();
            let mut args = Vec::new();
            while !matches!(
                self.peek(),
                Token::RBracket | Token::Gt | Token::Eof | Token::RParen
            ) {
                args.push(self.parse_type_syntax()?);
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            if is_bracket {
                self.expect_token(Token::RBracket, "']'")?;
            } else {
                self.expect_token(Token::Gt, "'>'")?;
            }
            Ok(TypeSyntax::Generic { name: base, args })
        } else {
            Ok(TypeSyntax::Named(base))
        }
    }

    fn parse_type_annotation(&mut self) -> Result<Option<TypeSyntax>> {
        if matches!(self.peek(), Token::Colon) {
            self.advance();
            Ok(Some(self.parse_type_syntax()?))
        } else {
            Ok(None)
        }
    }

    pub fn parse_program(&mut self) -> Result<Program> {
        let mut functions = Vec::new();
        let mut traits = Vec::new();
        let mut impls = Vec::new();
        let mut top_level_imports = Vec::new();

        while !matches!(self.peek(), Token::Eof) {
            if matches!(self.peek(), Token::Trait) {
                traits.push(self.parse_trait()?);
            } else if matches!(self.peek(), Token::Impl) {
                impls.push(self.parse_impl()?);
            } else if matches!(
                self.peek(),
                Token::Procedure | Token::Function | Token::Extern
            ) {
                functions.push(self.parse_function()?);
            } else if matches!(self.peek(), Token::Import) {
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
        })
    }

    fn parse_function(&mut self) -> Result<FunctionDecl> {
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
        if !is_function && !matches!(self.peek(), Token::Procedure) {
            return Err(self.error("Expected 'function' or 'procedure'"));
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
        })
    }

    fn parse_if_expr_from_stmt(&mut self) -> Result<Expr> {
        self.advance(); // consume 'if'
        self.parse_if_expr()
    }

    fn parse_loop_body(&mut self) -> Result<(Vec<Stmt>, Option<Box<Expr>>)> {
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

    fn parse_block_expr(&mut self) -> Result<Expr> {
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

    fn parse_block(&mut self) -> Result<Vec<Stmt>> {
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

    fn parse_stmt(&mut self) -> Result<Stmt> {
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

    fn parse_var_decl(&mut self) -> Result<Stmt> {
        let is_mutable = matches!(self.peek(), Token::Var);
        self.advance();
        let name = self.expect_identifier("variable name")?;
        let type_annotation = self.parse_type_annotation()?.map(|t| t.to_string_rep());

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

    fn parse_print(&mut self) -> Result<Stmt> {
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

    fn parse_if_expr(&mut self) -> Result<Expr> {
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

    fn parse_for_expr(&mut self) -> Result<Expr> {
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

    fn parse_while_expr(&mut self) -> Result<Expr> {
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

    fn parse_for(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'for'
        Ok(Stmt::Expression(self.parse_for_expr()?))
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        self.advance(); // consume 'while'
        Ok(Stmt::Expression(self.parse_while_expr()?))
    }

    fn skip_optional_do(&mut self) {
        if matches!(self.peek(), Token::Do) {
            self.advance();
        } else {
            self.skip_keyword("do");
        }
    }

    fn parse_spawn(&mut self) -> Result<Stmt> {
        self.advance();
        self.skip_optional_do();
        let body = self.parse_block()?;
        Ok(Stmt::Spawn { body })
    }

    fn parse_parallel(&mut self) -> Result<Stmt> {
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

    fn parse_channel_decl(&mut self) -> Result<Stmt> {
        self.advance();
        let name = self.expect_identifier("channel name")?;
        let _type_annotation = self.parse_type_annotation()?;
        Ok(Stmt::ChannelDecl { name })
    }

    fn parse_send(&mut self) -> Result<Stmt> {
        self.advance();
        let channel = self.expect_identifier("channel name")?;
        if matches!(self.peek(), Token::Comma) {
            self.advance();
        }
        let value = self.parse_expr()?;
        Ok(Stmt::Send { channel, value })
    }

    fn parse_receive(&mut self) -> Result<Stmt> {
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

    fn parse_match(&mut self) -> Result<Stmt> {
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

    fn parse_pattern(&mut self) -> Result<Pattern> {
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
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let start = Some(Box::new(Expr::Int(n)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    return Ok(Pattern::Range { start, end });
                }
                if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let start = Some(Box::new(Expr::Int(n)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    // Inclusive range pattern? We'll treat as exclusive for now.
                    return Ok(Pattern::Range { start, end });
                }
                Ok(Pattern::Literal(Expr::Int(n)))
            }
            Token::FloatLit(f) => {
                self.advance();
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let start = Some(Box::new(Expr::Number(f)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    return Ok(Pattern::Range { start, end });
                }
                if matches!(self.peek(), Token::DotDotEqual) {
                    self.advance();
                    let start = Some(Box::new(Expr::Number(f)));
                    let end = if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        Some(Box::new(self.parse_expr()?))
                    } else {
                        None
                    };
                    return Ok(Pattern::Range { start, end });
                }
                Ok(Pattern::Literal(Expr::Number(f)))
            }
            Token::StringLit(s) => {
                self.advance();
                Ok(Pattern::Literal(Expr::String(s)))
            }
            Token::True => {
                self.advance();
                Ok(Pattern::Literal(Expr::Bool(true)))
            }
            Token::False => {
                self.advance();
                Ok(Pattern::Literal(Expr::Bool(false)))
            }
            Token::Identifier(name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
            Token::Identifier(name) => {
                self.advance();
                Ok(Pattern::Binding(name))
            }
            _ => {
                self.advance();
                Ok(Pattern::Wildcard)
            }
        }
    }

    fn parse_return(&mut self) -> Result<Stmt> {
        self.advance();
        let value = if matches!(self.peek(), Token::Eof | Token::Dedent) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        Ok(Stmt::Return { value })
    }

    fn parse_unsafe(&mut self) -> Result<Stmt> {
        self.advance();
        let body = self.parse_block()?;
        Ok(Stmt::UnsafeBlock { body })
    }

    fn parse_region(&mut self) -> Result<Stmt> {
        self.advance();
        let name = self.expect_identifier("region name")?;
        let body = self.parse_block()?;
        Ok(Stmt::RegionBlock { name, body })
    }

    fn parse_try_catch(&mut self) -> Result<Stmt> {
        // Called from parse_stmt, which peeks — consume 'try' here.
        self.advance();
        Ok(Stmt::Expression(self.parse_try_catch_expr()?))
    }

    fn parse_try_catch_expr(&mut self) -> Result<Expr> {
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

    fn parse_import(&mut self) -> Result<Stmt> {
        self.advance();
        let path = match self.advance() {
            Token::StringLit(s) => s,
            Token::Identifier(s) => s,
            other => return Err(self.error(&format!("Expected import path, found {:?}", other))),
        };
        Ok(Stmt::Import { path })
    }

    fn parse_identifier_stmt(&mut self, name: String) -> Result<Stmt> {
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

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr> {
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

    fn parse_logical_and(&mut self) -> Result<Expr> {
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

    fn parse_comparison(&mut self) -> Result<Expr> {
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

    fn parse_additive(&mut self) -> Result<Expr> {
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

    fn parse_multiplicative(&mut self) -> Result<Expr> {
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

    fn parse_unary(&mut self) -> Result<Expr> {
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

    fn parse_primary(&mut self) -> Result<Expr> {
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

    fn parse_identifier_expr(&mut self, name: String) -> Result<Expr> {
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

    fn parse_trait(&mut self) -> Result<TraitDecl> {
        self.advance();
        let name = self.expect_identifier("trait name")?;
        let mut methods = Vec::new();
        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                let is_function = matches!(self.peek(), Token::Function);
                if !is_function && !matches!(self.peek(), Token::Procedure) {
                    return Err(self.error("Expected 'function' in trait method"));
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
        Ok(TraitDecl { name, methods })
    }

    fn parse_impl(&mut self) -> Result<ImplBlock> {
        self.advance();
        let trait_name = self.expect_identifier("trait name")?;
        if matches!(self.peek(), Token::For) {
            self.advance();
        } else {
            return Err(self.error("Expected 'for' in impl block"));
        }
        let target_type = self.expect_identifier("target type")?;
        let mut methods = Vec::new();
        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                let method = self.parse_function()?;
                methods.push(method);
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
        }
        Ok(ImplBlock {
            trait_name,
            target_type,
            methods,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;

    fn parse_source(source: &str) -> Result<Vec<FunctionDecl>> {
        let lexer = Lexer::new(source.to_string())?;
        let mut parser = Parser::new(lexer.tokens);
        let program = parser.parse_program()?;
        Ok(program.functions)
    }

    #[test]
    fn test_parse_simple_function() {
        let source = "function main() -> Float\n    return 42.0";
        let functions = parse_source(source).expect("parse error");
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "main");
        assert_eq!(
            functions[0].return_type,
            Some(TypeSyntax::Named("Float".to_string()))
        );
        assert_eq!(functions[0].body.len(), 1);
    }

    #[test]
    fn test_parse_var_decl() {
        let source = "function main()\n    var x := 5\n    val y := 10";
        let functions = parse_source(source).expect("parse error");
        assert_eq!(functions[0].body.len(), 2);
    }

    #[test]
    fn test_parse_if_else() {
        let source = "function main()\n    if x > 5\n        print x\n    else\n        print 0";
        let functions = parse_source(source).expect("parse error");
        assert_eq!(functions[0].body.len(), 1);
    }

    #[test]
    fn test_parse_array_access() {
        let source = "function main()\n    var x := arr[0]";
        let functions = parse_source(source).expect("parse error");
        match &functions[0].body[0] {
            Stmt::VarDecl { value, .. } => match value {
                Expr::ArrayAccess { .. } => {}
                _ => panic!("Expected ArrayAccess"),
            },
            _ => panic!("Expected VarDecl"),
        }
    }

    #[test]
    fn test_parse_function_call() {
        let source = "function main()\n    print add(1, 2)";
        let functions = parse_source(source).expect("parse error");
        match &functions[0].body[0] {
            Stmt::Print { expr } => match expr {
                Expr::FunctionCall { name, args, .. } => {
                    assert_eq!(name, "add");
                    assert_eq!(args.len(), 2);
                }
                _ => panic!("Expected FunctionCall"),
            },
            _ => panic!("Expected Print"),
        }
    }

    #[test]
    fn test_parse_for_as_expr() {
        let source = "function main()\n    val x := for i in [1,2,3] do i + 1";
        let functions = parse_source(source).expect("parse error");
        match &functions[0].body[0] {
            Stmt::VarDecl { value, .. } => match value {
                Expr::For {
                    var, trailing_expr, ..
                } => {
                    assert_eq!(var, "i");
                    assert!(trailing_expr.is_some());
                }
                _ => panic!("Expected For expr"),
            },
            _ => panic!("Expected VarDecl"),
        }
    }

    #[test]
    fn test_parse_method_call() {
        let source = "function main()\n    list.append(3)";
        let functions = parse_source(source).expect("parse error");
        match &functions[0].body[0] {
            Stmt::Expression(Expr::FunctionCall { name, args, .. }) => {
                assert_eq!(name, "list.append");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Expected FunctionCall with dotted name"),
        }
    }

    #[test]
    fn test_parse_range() {
        let source = "function main()\n    val r := 1..5";
        let functions = parse_source(source).expect("parse error");
        match &functions[0].body[0] {
            Stmt::VarDecl { value, .. } => match value {
                Expr::Range { start, end, inclusive } => {
                    assert!(!inclusive);
                    assert!(start.is_some());
                    assert!(end.is_some());
                }
                _ => panic!("Expected Range"),
            },
            _ => panic!("Expected VarDecl"),
        }
    }
}
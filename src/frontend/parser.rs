// algol26/src/frontend/parser.rs - 100% Orthogonal: for/while as expressions

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
    node_counter: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self::new_with_positions(tokens, Vec::new())
    }

    fn parse_block_with_trailing(&mut self) -> Result<(Vec<Stmt>, Option<Box<Expr>>)> {
        let block_expr = self.parse_block_expr()?;
        match block_expr {
            Expr::Block {
                statements,
                trailing_expr,
            } => Ok((statements, trailing_expr)),
            other => Err(self.error(&format!("Expected block, found: {:?}", other))),
        }
    }

    pub fn new_with_positions(tokens: Vec<Token>, positions: Vec<(usize, usize)>) -> Self {
        let token_infos = tokens
            .into_iter()
            .enumerate()
            .map(|(i, token)| {
                let (line, column) = positions.get(i).copied().unwrap_or((0, 0)); // TODO: Replace with proper Span::default()
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
            node_counter: 0,
        }
    }

    pub fn get_span_map(&self) -> &std::collections::HashMap<usize, (usize, usize)> {
        &self.span_map
    }

    fn record_span(&mut self) -> usize {
        let info = self.peek_info().clone();
        let id = self.node_counter;
        self.node_counter += 1;
        self.span_map.insert(id, (info.line, info.column));
        id
    }

    fn peek(&self) -> &Token {
        self
            .tokens
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
        CompileError::new(message, info.line, info.column, "", ErrorCode::E0001)
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

    fn parse_type_annotation(&mut self) -> Result<Option<TypeSyntax>> {
        if matches!(self.peek(), Token::Colon) {
            self.advance();
            // Check for Self type
            if matches!(self.peek(), Token::SelfType) {
                self.advance();
                Ok(Some(TypeSyntax::Named("Self".to_string())))
            } else {
                let type_name = self.expect_identifier("type name")?;
                Ok(Some(TypeSyntax::from_string(&type_name)))
            }
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
            self.advance(); // consume 'extern'

            // Create basic FFI info
            let mut info = ExternDecl::default();

            // Parse optional ABI string
            if let Token::StringLit(abi) = self.peek().clone() {
                self.advance();
                info.abi = Some(abi);
            }

            ffi_info = Some(info);
        }

        // Now continue with normal function parsing
        let is_function = matches!(self.peek(), Token::Function);
        if !is_function && !matches!(self.peek(), Token::Procedure) {
            return Err(self.error("Expected 'function' or 'procedure'"));
        }
        self.advance();

        let name = self.expect_identifier("function name")?;

        // Parse generic type parameters <T, U, V>
        let mut type_params = Vec::new();
        if matches!(self.peek(), Token::Lt) {
            self.advance(); // consume <
            while !matches!(self.peek(), Token::Gt | Token::Eof) {
                let type_param = self.expect_identifier("type parameter")?;
                type_params.push(type_param);

                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.expect_token(Token::Gt, "'>'")?;
        }

        // Parse parameters
        let mut params = Vec::new();
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            while !matches!(self.peek(), Token::RParen | Token::Eof) {
                // Handle variadic
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

        // Parse return type
        let return_type = if is_function {
            if matches!(self.peek(), Token::Arrow | Token::Colon) {
                self.advance();
                let type_name = self.expect_identifier("return type")?;
                Some(TypeSyntax::from_string(&type_name))
            } else {
                None
            }
        } else {
            None
        };

        // Parse where clauses (for type constraints)
        let mut where_clauses = Vec::new();
        if matches!(self.peek(), Token::Where) {
            self.advance();

            // Parse one or more constraints separated by commas
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

        // For extern functions, parse FFI-specific clauses
        if let Some(mut info) = ffi_info {
            // Parse "from" clause
            if matches!(self.peek(), Token::From) {
                self.advance();
                info.library = match self.advance() {
                    Token::StringLit(s) => Some(s),
                    Token::Identifier(s) => Some(s),
                    _ => None,
                };
            }

            // Parse "as" clause for symbol renaming
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

        // Parse body (empty for extern)
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
            type_params,   // NEW
            where_clauses, // NEW
        })
    }

    fn parse_if_expr_from_stmt(&mut self) -> Result<Expr> {
        self.advance();
        self.parse_if_expr()
    }

    fn parse_block_expr(&mut self) -> Result<Expr> {
        let mut statements = Vec::new();
        let mut trailing_expr = None;

        if let Token::Indent = self.peek() {
            self.advance();
            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                statements.push(self.parse_stmt()?);
            }
            if let Token::Dedent = self.peek() {
                self.advance();
            }
        } else {
            if !matches!(self.peek(), Token::Eof | Token::Dedent) {
                statements.push(self.parse_stmt()?);
            }
        }

        if let Some(Stmt::Expression(_)) = statements.last() {
            if let Some(Stmt::Expression(expr)) = statements.pop() {
                trailing_expr = Some(Box::new(expr));
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
                self.advance();
                let stmt = self.parse_stmt()?;
                Ok(Stmt::Defer {
                    stmt: Box::new(stmt),
                })
            }
            Token::Alloc => {
                // Handle alloc(size) as function call
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
                // Handle free(ptr) as function call
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

    fn parse_for(&mut self) -> Result<Stmt> {
        self.advance();
        let var = self.expect_identifier("iterator variable")?;
        self.expect_token(Token::In, "'in'")?;
        let iterable = self.parse_expr()?;

        self.skip_optional_do();

        // FIX: Use parse_block not parse_block_with_trailing to keep If as body, not trailing_expr
        // parse_block_with_trailing pops last Expression as trailing value, leaving body empty
        let body = self.parse_block()?;
        Ok(Stmt::Expression(Expr::For {
            var,
            iterable: Box::new(iterable),
            body,
            trailing_expr: None,
            span: Span::default(),
        }))
    }

    fn parse_while(&mut self) -> Result<Stmt> {
        self.advance();
        let condition = self.parse_expr()?;

        self.skip_optional_do();

        let body = self.parse_block()?;
        Ok(Stmt::Expression(Expr::While {
            condition: Box::new(condition),
            body,
            trailing_expr: None,
            span: Span::default(),
        }))
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
            self.advance(); // consume indent to case level

            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                // Consume 'case' keyword if present
                if let Token::Identifier(ref s) = self.peek() {
                    if s == "case" {
                        self.advance();
                    }
                }

                let mut pattern = self.parse_pattern()?;

                // Check for pattern guard: pattern if condition
                if matches!(self.peek(), Token::If) {
                    self.advance();
                    let condition = self.parse_expr()?;
                    pattern = Pattern::Guarded {
                        pattern: Box::new(pattern),
                        condition: Box::new(condition),
                    };
                }

                // Parse the case body manually (not using parse_block)
                // This allows us to properly handle multiple cases at the same indent level
                let mut body = Vec::new();
                if let Token::Indent = self.peek() {
                    self.advance(); // consume indent to body level

                    while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                        body.push(self.parse_stmt()?);
                    }

                    if let Token::Dedent = self.peek() {
                        self.advance(); // consume dedent back to case level
                    }
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
                self.advance(); // consume final dedent to end match block
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
                    // Check for nested pattern
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
                    // Check for nested pattern
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
                    // Check for nested pattern
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
                // List destructuring
                self.advance(); // consume [

                if matches!(self.peek(), Token::RBracket) {
                    self.advance();
                    return Ok(Pattern::ListDestructure {
                        first: None,
                        rest: None,
                    });
                }

                // Parse first pattern
                let first = self.parse_pattern()?;

                // Check for rest pattern with ..
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

                // Simple list destructuring without rest
                self.expect_token(Token::RBracket, "']'")?;
                Ok(Pattern::ListDestructure {
                    first: Some(Box::new(first)),
                    rest: None,
                })
            }
            Token::IntLit(n) => {
                self.advance();
                // Check for range pattern
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let start = Some(Box::new(Expr::Int(n)));
                    if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        let end = self.parse_expr()?;
                        return Ok(Pattern::Range {
                            start,
                            end: Some(Box::new(end)),
                        });
                    }
                    return Ok(Pattern::Range { start, end: None });
                }
                Ok(Pattern::Literal(Expr::Int(n)))
            }
            Token::FloatLit(f) => {
                self.advance();
                // Check for range pattern
                if matches!(self.peek(), Token::DotDot) {
                    self.advance();
                    let start = Some(Box::new(Expr::Number(f)));
                    if matches!(self.peek(), Token::IntLit(_) | Token::FloatLit(_)) {
                        let end = self.parse_expr()?;
                        return Ok(Pattern::Range {
                            start,
                            end: Some(Box::new(end)),
                        });
                    }
                    return Ok(Pattern::Range { start, end: None });
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
                // Variable binding pattern
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
        self.advance();

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

        Ok(Stmt::Expression(Expr::TryCatch {
            try_branch,
            catch_var,
            catch_branch,
            finally_body,
        }))
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
        // We are currently at the identifier token, consume it
        self.advance();

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
                name,
                args,
                span: Span::point(span.line, span.column),
            }))
        } else if matches!(self.peek(), Token::LBracket) {
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
                // Not an assignment, it's an array access as expression
                // Backtrack to re-parse as full expression
                self.pos -= 1; // back to identifier
                let expr = self.parse_expr()?;
                Ok(Stmt::Expression(expr))
            }
        } else if matches!(self.peek(), Token::Assign) {
            self.advance();
            let value = self.parse_expr()?;
            Ok(Stmt::Assign { name, value })
        } else {
            // Backtrack and parse as a full expression
            // This handles: bare variables, binary ops, comparisons, etc.
            if self.pos > 0 {
                self.pos -= 1;
            }
            let expr = self.parse_expr()?;
            Ok(Stmt::Expression(expr))
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
                    return Err(self.error(&format!("Unexpected comparison operator: {:?}", other)))
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
                    return Err(self.error(&format!("Unexpected comparison operator: {:?}", other)))
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
                // Check if next is an integer literal
                if let Token::IntLit(n) = self.peek().clone() {
                    self.advance();
                    return Ok(Expr::Int(-n)); // Negative integer literal
                }
                // Check if next is a float literal
                if let Token::FloatLit(f) = self.peek().clone() {
                    self.advance();
                    return Ok(Expr::Number(-f)); // Negative float literal
                }
                // Otherwise, negate the expression
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
        // Capture span before consuming token
        let start_info = self.peek_info().clone();
        match self.advance() {
            Token::If => self.parse_if_expr(),
            Token::For => {
                // for x in iterable do ... [trailing_expr]
                let var = self.expect_identifier("iterator variable")?;
                self.expect_token(Token::In, "'in'")?;
                let iterable = Box::new(self.parse_expr()?);
                self.skip_optional_do();
                let (body, trailing_expr) = self.parse_block_with_trailing()?;
                Ok(Expr::For {
                    var,
                    iterable,
                    body,
                    trailing_expr,
                    span: Span::point(start_info.line, start_info.column),
                })
            }
            Token::While => {
                let condition = Box::new(self.parse_expr()?);
                self.skip_optional_do();
                let (body, trailing_expr) = self.parse_block_with_trailing()?;
                Ok(Expr::While {
                    condition,
                    body,
                    trailing_expr,
                    span: Span::point(start_info.line, start_info.column),
                })
            }
            Token::FloatLit(v) => Ok(Expr::Number(v)),
            Token::IntLit(v) => Ok(Expr::Int(v)),
            Token::StringLit(s) => Ok(Expr::String(s)),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Identifier(name) => self.parse_identifier_expr(name),
            Token::Alloc => {
                // Handle alloc(size) as expression
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
                // Handle free(ptr) as expression
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
            Ok(Expr::ArrayAccess {
                array: Box::new(Expr::Var(name.clone(), Span::default())),
                index: Box::new(index),
            })
        } else {
            let _span_id = self.record_span();
            let span = self.peek_info().clone();
            Ok(Expr::Var(name, Span::point(span.line, span.column)))
        }
    }
    fn parse_trait(&mut self) -> Result<TraitDecl> {
        self.advance(); // consume 'trait'
        let name = self.expect_identifier("trait name")?;

        let mut methods = Vec::new();

        // Parse indented block of method signatures
        if let Token::Indent = self.peek() {
            self.advance(); // consume indent

            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                // Parse method signature: function name(params) -> ReturnType
                let is_function = matches!(self.peek(), Token::Function);
                if !is_function && !matches!(self.peek(), Token::Procedure) {
                    return Err(self.error("Expected 'function' in trait method"));
                }
                self.advance();

                let method_name = self.expect_identifier("method name")?;

                // Parse parameters
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

                // Parse return type
                let return_type = if matches!(self.peek(), Token::Arrow | Token::Colon) {
                    self.advance();
                    let type_name = self.expect_identifier("return type")?;
                    Some(TypeSyntax::from_string(&type_name))
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
        self.advance(); // consume 'impl'
        let trait_name = self.expect_identifier("trait name")?;

        // Consume 'for' keyword
        if matches!(self.peek(), Token::For) {
            self.advance();
        } else {
            return Err(self.error("Expected 'for' in impl block"));
        }

        let target_type = self.expect_identifier("target type")?;

        let mut methods = Vec::new();

        // Parse method bodies
        if let Token::Indent = self.peek() {
            self.advance(); // consume indent

            while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                // Parse each method as a function
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
        let functions = program.functions;
        Ok(functions)
    }

    #[test]
    fn test_parse_simple_function() {
        let source = "function main() -> Float\n    return 42.0";
        let functions = parse_source(source).unwrap();

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
        let functions = parse_source(source).unwrap();

        assert_eq!(functions[0].body.len(), 2);
        match &functions[0].body[0] {
            Stmt::VarDecl { name, mutable, .. } => {
                assert_eq!(name, "x");
                assert!(*mutable);
            }
            _ => assert!(false, "Expected VarDecl"),
        }
    }

    #[test]
    fn test_parse_if_else() {
        let source = "function main()\n    if x > 5\n        print x\n    else\n        print 0";
        let functions = parse_source(source).unwrap();

        assert_eq!(functions[0].body.len(), 1);
        match &functions[0].body[0] {
            Stmt::Expression(Expr::If { else_branch, .. }) => {
                assert!(else_branch.is_some());
            }
            _ => panic!("Expected If expression statement"),
        }
    }

    #[test]
    fn test_parse_array_access() {
        let source = "function main()\n    var x := arr[0]";
        let functions = parse_source(source).unwrap();

        match &functions[0].body[0] {
            Stmt::VarDecl { value, .. } => match value {
                Expr::ArrayAccess { .. } => {}
                _ => panic!("Expected ArrayAccess"),
            },
            _ => assert!(false, "Expected VarDecl"),
        }
    }

    #[test]
    fn test_parse_function_call() {
        let source = "function main()\n    print add(1, 2)";
        let functions = parse_source(source).unwrap();

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
        let functions = parse_source(source).unwrap();
        match &functions[0].body[0] {
            Stmt::VarDecl { value, .. } => match value {
                Expr::For {
                    var, trailing_expr, ..
                } => {
                    assert_eq!(var, "i");
                    assert!(trailing_expr.is_some());
                }
                _ => panic!("Expected For expr, got {:?}", value),
            },
            _ => assert!(false, "Expected VarDecl"),
        }
    }
}

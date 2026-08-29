// algol26/src/parser.rs

use crate::ast::{Expr, FunctionDecl, Stmt, BinOp, MatchCase, Pattern};
use crate::diagnostics::{CompileError, ErrorCode, Result};
use crate::lexer::Token;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        self.pos += 1;
        tok
    }

    fn skip_keyword(&mut self, keyword: &str) {
        if let Token::Identifier(id) = self.peek() {
            if id == keyword {
                self.advance();
            }
        }
    }

    pub fn parse_program(&mut self) -> Result<Vec<FunctionDecl>> {
        let mut functions = Vec::new();
        let mut top_level_imports = Vec::new();
        
        while !matches!(self.peek(), Token::Eof) {
            if matches!(self.peek(), Token::Procedure | Token::Function) {
                functions.push(self.parse_function()?);
            } else if matches!(self.peek(), Token::Import) {
                // Parse top-level import and add it as a statement in a synthetic function
                self.advance();
                let path = match self.advance() {
                    Token::StringLit(s) => s,
                    Token::Identifier(s) => s,
                    other => return Err(CompileError::new(
                        &format!("Expected import path, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                top_level_imports.push((path, functions.len()));
                // Store for later - we'll attach to the first function or main
            } else {
                self.advance();
            }
        }
        
        // If we have top-level imports, attach them to the first function (usually main)
        if !top_level_imports.is_empty() && !functions.is_empty() {
            for (path, _) in top_level_imports {
                functions[0].body.insert(0, Stmt::Import { path });
            }
        }
        
        Ok(functions)
    }

    fn parse_function(&mut self) -> Result<FunctionDecl> {
        let is_function = matches!(self.peek(), Token::Function);
        self.advance();

        let name = match self.advance() {
            Token::Identifier(n) => n,
            other => return Err(CompileError::new(
                &format!("Expected function name, found {:?}", other),
                    0, 0, "",
                    ErrorCode::E0001
                )),
        };

        let mut params = Vec::new();
        if matches!(self.peek(), Token::LParen) {
            self.advance();
            while !matches!(self.peek(), Token::RParen) {
                let param_name = match self.advance() {
                    Token::Identifier(n) => n,
                    other => return Err(CompileError::new(
                        &format!("Expected parameter name, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                
                let param_type = if matches!(self.peek(), Token::Colon) {
                    self.advance();
                    match self.advance() {
                        Token::Identifier(t) => t,
                        other => return Err(CompileError::new(
                            &format!("Expected type name, found {:?}", other),
                                0, 0, "",
                                ErrorCode::E0001
                            )),
                    }
                } else {
                    "float".to_string()
                };
                
                params.push((param_name, param_type));
                
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
            }
            self.advance();
        }

        let return_type = if is_function {
            if matches!(self.peek(), Token::Colon) {
                self.advance();
                Some(match self.advance() {
                    Token::Identifier(t) => t,
                    other => return Err(CompileError::new(
                        &format!("Expected return type, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                })
            } else {
                Some("float".to_string())
            }
        } else {
            None
        };

        let body = self.parse_block()?;
        Ok(FunctionDecl { name, params, return_type, body })
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>> {
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
            if !matches!(self.peek(), Token::Eof | Token::Dedent) {
                body.push(self.parse_stmt()?);
            }
        }
        
        Ok(body)
    }

    fn parse_stmt(&mut self) -> Result<Stmt> {
        match self.peek().clone() {
            Token::Var | Token::Val => {
                let is_mutable = matches!(self.peek(), Token::Var);
                self.advance();
                let name = match self.advance() {
                    Token::Identifier(n) => n,
                    other => return Err(CompileError::new(
                        &format!("Expected variable name, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                
                let type_annotation = if matches!(self.peek(), Token::Colon) {
                    self.advance();
                    Some(match self.advance() {
                        Token::Identifier(t) => t,
                        other => return Err(CompileError::new(
                            &format!("Expected type name, found {:?}", other),
                                0, 0, "",
                                ErrorCode::E0001
                            )),
                    })
                } else {
                    None
                };
                
                match self.advance() {
                    Token::Assign => {}
                    other => return Err(CompileError::new(
                        &format!("Expected ':=', found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                }
                
                let value = self.parse_expr()?;
                Ok(Stmt::VarDecl { name, value, type_annotation, mutable: is_mutable })
            }
            Token::Print => {
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
            Token::If => {
                self.advance();
                let condition = self.parse_expr()?;
                
                // Skip optional 'then'
                self.skip_keyword("then");
                
                let then_body = self.parse_block()?;
                
                let else_body = if matches!(self.peek(), Token::Else) {
                    self.advance();
                    Some(self.parse_block()?)
                } else {
                    None
                };
                
                Ok(Stmt::If { condition, then_body, else_body })
            }
            Token::For => {
                self.advance();
                let var = match self.advance() {
                    Token::Identifier(n) => n,
                    other => return Err(CompileError::new(
                        &format!("Expected iterator variable, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                
                match self.advance() {
                    Token::In => {}
                    other => return Err(CompileError::new(
                        &format!("Expected 'in', found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                }
                
                let iterable = self.parse_expr()?;
                
                // Skip optional 'do' - handle both Token::Do and Token::Identifier("do")
                if matches!(self.peek(), Token::Do) {
                    self.advance();
                } else {
                    self.skip_keyword("do");
                }
                
                let body = self.parse_block()?;
                
                Ok(Stmt::For { var, iterable, body })
            }
            Token::While => {
                self.advance();
                let condition = self.parse_expr()?;
                
                // Skip optional 'do' - handle both Token::Do and Token::Identifier("do")
                if matches!(self.peek(), Token::Do) {
                    self.advance();
                } else {
                    self.skip_keyword("do");
                }
                
                let body = self.parse_block()?;
                
                Ok(Stmt::While { condition, body })
            }
            Token::Spawn => {
                self.advance(); // consume 'spawn'
                // Consume optional 'do' keyword (handle both Token::Do and Identifier)
                if matches!(self.peek(), Token::Do) {
                    self.advance();
                } else if matches!(self.peek(), Token::Identifier(ref s) if s == "do") {
                    self.advance();
                }
                let body = self.parse_block()?;
                Ok(Stmt::Spawn { body })
            }
            Token::Parallel => {
                self.advance(); // consume 'parallel'
                // Parse 'do' if present (handle both Token::Do and Identifier)
                if matches!(self.peek(), Token::Do) {
                    self.advance();
                } else if matches!(self.peek(), Token::Identifier(ref s) if s == "do") {
                    self.advance();
                }
                // Parse the block
                let body = self.parse_block()?;
                Ok(Stmt::Parallel { blocks: vec![body] })
            }
            Token::Channel => {
                self.advance(); // consume 'channel'
                let name = match self.advance() {
                    Token::Identifier(n) => n,
                    other => return Err(CompileError::new(
                        &format!("Expected channel name, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                Ok(Stmt::ChannelDecl { name })
            }
            Token::Send => {
                self.advance(); // consume 'send'
                let channel = match self.advance() {
                    Token::Identifier(n) => n,
                    other => return Err(CompileError::new(
                        &format!("Expected channel name, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                // Optional comma
                if matches!(self.peek(), Token::Comma) {
                    self.advance();
                }
                let value = self.parse_expr()?;
                Ok(Stmt::Send { channel, value })
            }
            Token::Receive => {
                self.advance(); // consume 'receive'
                let channel = match self.advance() {
                    Token::Identifier(n) => n,
                    other => return Err(CompileError::new(
                        &format!("Expected channel name, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                Ok(Stmt::Receive { channel, target: String::new() })
            }
            Token::Match => {
                self.advance(); // consume 'match'
                let value = self.parse_expr()?;
                
                let mut cases = Vec::new();
                
                if let Token::Indent = self.peek() {
                    self.advance(); // consume Indent
                    
                    while !matches!(self.peek(), Token::Dedent | Token::Eof) {
                        // Parse pattern
                        let pattern = match self.advance() {
                            Token::Some => {
                                self.advance(); // consume 'Some'
                                if matches!(self.peek(), Token::LParen) {
                                    self.advance();
                                }
                                let var = match self.advance() {
                                    Token::Identifier(n) => n,
                                    _ => "_".to_string(),
                                };
                                if matches!(self.peek(), Token::RParen) {
                                    self.advance();
                                }
                                Pattern::Some(var)
                            }
                            Token::None => {
                                self.advance();
                                Pattern::None
                            }
                            Token::Ok => {
                                self.advance();
                                if matches!(self.peek(), Token::LParen) {
                                    self.advance();
                                }
                                let var = match self.advance() {
                                    Token::Identifier(n) => n,
                                    _ => "_".to_string(),
                                };
                                if matches!(self.peek(), Token::RParen) {
                                    self.advance();
                                }
                                Pattern::Ok(var)
                            }
                            Token::Error => {
                                self.advance();
                                if matches!(self.peek(), Token::LParen) {
                                    self.advance();
                                }
                                let var = match self.advance() {
                                    Token::Identifier(n) => n,
                                    _ => "_".to_string(),
                                };
                                if matches!(self.peek(), Token::RParen) {
                                    self.advance();
                                }
                                Pattern::Error(var)
                            }
                            _ => {
                                Pattern::Wildcard
                            }
                        };
                        
                        let body = self.parse_block()?;
                        cases.push(MatchCase { pattern, body });
                    }
                    
                    if let Token::Dedent = self.peek() {
                        self.advance();
                    }
                }
                
                Ok(Stmt::Match { value, cases })
            }
            Token::Break => {
                self.advance();
                Ok(Stmt::Break)
            }
            Token::Continue => {
                self.advance();
                Ok(Stmt::Continue)
            }
            Token::Identifier(name) if name == "defer" => {
                self.advance(); // consume 'defer'
                let stmt = self.parse_stmt()?;
                Ok(Stmt::Defer { stmt: Box::new(stmt) })
            }
            Token::Return => {
                self.advance();
                let value = if matches!(self.peek(), Token::Eof | Token::Dedent) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                Ok(Stmt::Return { value })
            }
            Token::Identifier(name) => {
                self.advance();
                
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Token::RParen) {
                        args.push(self.parse_expr()?);
                        if matches!(self.peek(), Token::Comma) {
                            self.advance();
                        }
                    }
                    self.advance();
                    Ok(Stmt::FunctionCall { name, args })
                } else {
                    match self.advance() {
                        Token::Assign => {}
                        other => return Err(CompileError::new(
                            &format!("Expected assignment or function call, found {:?}", other),
                                0, 0, "",
                                ErrorCode::E0001
                            )),
                    }
                    let value = self.parse_expr()?;
                    Ok(Stmt::Assign { name, value })
                }
            }
            // Handle stray keywords gracefully
            Token::Do | Token::In => {
                // Skip these keywords if they appear as statements
                self.advance();
                self.parse_stmt()
            }
            Token::Unsafe => {
                self.advance();
                let body = self.parse_block()?;
                Ok(Stmt::UnsafeBlock { body })
            }
            Token::Region => {
                self.advance();
                let name = match self.advance() {
                    Token::Identifier(n) => n,
                    other => return Err(CompileError::new(
                        &format!("Expected region name, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                let body = self.parse_block()?;
                Ok(Stmt::RegionBlock { name, body })
            }
            Token::Try => {
                self.advance();
                
                // Parse try body
                let try_body = self.parse_block()?;
                
                // Check for catch
                let mut catch_var = None;
                let mut catch_body = Vec::new();
                if matches!(self.peek(), Token::Catch) {
                    self.advance();
                    
                    // Optional catch variable
                    if let Token::Identifier(var) = self.peek().clone() {
                        self.advance();
                        catch_var = Some(var);
                    }
                    
                    catch_body = self.parse_block()?;
                }
                
                // Check for finally
                let mut finally_body = None;
                if matches!(self.peek(), Token::Finally) {
                    self.advance();
                    finally_body = Some(self.parse_block()?);
                }
                
                Ok(Stmt::TryCatch {
                    try_body,
                    catch_var,
                    catch_body,
                    finally_body,
                })
            }
            Token::Import => {
                self.advance();
                let path = match self.advance() {
                    Token::StringLit(s) => s,
                    Token::Identifier(s) => s,
                    other => return Err(CompileError::new(
                        &format!("Expected import path, found {:?}", other),
                            0, 0, "",
                            ErrorCode::E0001
                        )),
                };
                Ok(Stmt::Import { path })
            }
            other => Err(CompileError::new(
                &format!("Unexpected statement token: {:?}", other),
                    0, 0, "",
                    ErrorCode::E0001
                )),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_logical_and()?;

        while matches!(self.peek(), Token::Or) {
            let _ = self.advance();
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
            let _ = self.advance();
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

        while matches!(self.peek(), Token::Greater | Token::Less | Token::GreaterEqual | Token::LessEqual | Token::Equal | Token::NotEqual) {
            let op = self.advance();
            let binop = match op {
                Token::Greater => BinOp::Greater,
                Token::GreaterEqual => BinOp::GreaterEqual,
                Token::LessEqual => BinOp::LessEqual,
                Token::Less => BinOp::Less,
                Token::Equal => BinOp::Equal,
                Token::NotEqual => BinOp::NotEqual,
                _ => unreachable!(),
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
                Token::Or => BinOp::Or,
                _ => unreachable!(),
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
        let mut left = self.parse_primary()?;

        while matches!(self.peek(), Token::Star | Token::Slash) {
            let op = self.advance();
            let binop = match op {
                Token::Star => BinOp::Multiply,
                Token::Slash => BinOp::Divide,
                _ => unreachable!(),
            };
            let right = self.parse_primary()?;
            left = Expr::Binary {
                left: Box::new(left),
                op: binop,
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Token::Star => {
                self.advance(); // consume *
                let expr = self.parse_primary()?;
                return Ok(Expr::Deref { expr: Box::new(expr) });
            }
            Token::Ampersand => {
                self.advance(); // consume &
                // Check for "mut" keyword
                if let Token::Identifier(ref s) = self.peek().clone() {
                    if s == "mut" {
                        self.advance(); // consume "mut"
                        let expr = self.parse_primary()?;
                        return Ok(Expr::MutBorrow { expr: Box::new(expr) });
                    }
                }
                let expr = self.parse_primary()?;
                return Ok(Expr::Borrow { expr: Box::new(expr) });
            }
            _ => {}
        }
        
        match self.advance() {
            Token::FloatLit(v) => Ok(Expr::Number(v)),
            Token::IntLit(v) => Ok(Expr::Int(v)),
            Token::StringLit(s) => Ok(Expr::String(s)),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Identifier(n) => {
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Token::RParen) {
                        args.push(self.parse_expr()?);
                        if matches!(self.peek(), Token::Comma) {
                            self.advance();
                        }
                    }
                    self.advance();
                    Ok(Expr::FunctionCall { name: n, args })
                } else if matches!(self.peek(), Token::LBracket) {
                    // Array access
                    self.advance();
                    let index = self.parse_expr()?;
                    if matches!(self.peek(), Token::RBracket) {
                        self.advance();
                    }
                    Ok(Expr::ArrayAccess {
                        array: Box::new(Expr::Var(n)),
                        index: Box::new(index),
                    })
                } else {
                    Ok(Expr::Var(n))
                }
            }
            Token::Minus => {
                self.advance(); // consume '-'
                let operand = self.parse_primary()?;
                // Wrap in Binary with 0 - operand
                Ok(Expr::Binary {
                    left: Box::new(Expr::Number(0.0)),
                    op: BinOp::Subtract,
                    right: Box::new(operand),
                })
            }
            Token::LParen => {
                let expr = self.parse_expr()?;
                if matches!(self.peek(), Token::RParen) {
                    self.advance();
                }
                Ok(expr)
            }
            Token::Some => {
                self.advance(); // consume 'Some'
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if matches!(self.peek(), Token::RParen) {
                    self.advance();
                }
                Ok(Expr::Some { value: Box::new(value) })
            }
            Token::None => {
                self.advance(); // consume 'None'
                Ok(Expr::None)
            }
            Token::Ok => {
                self.advance(); // consume 'Ok'
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if matches!(self.peek(), Token::RParen) {
                    self.advance();
                }
                Ok(Expr::Ok { value: Box::new(value) })
            }
            Token::Error => {
                self.advance(); // consume 'Error'
                if matches!(self.peek(), Token::LParen) {
                    self.advance();
                }
                let value = self.parse_expr()?;
                if matches!(self.peek(), Token::RParen) {
                    self.advance();
                }
                Ok(Expr::Error { value: Box::new(value) })
            }
            Token::LBracket => {
                let mut elements = Vec::new();
                while !matches!(self.peek(), Token::RBracket | Token::Eof) {
                    elements.push(self.parse_expr()?);
                    if matches!(self.peek(), Token::Comma) {
                        self.advance();
                    }
                }
                if matches!(self.peek(), Token::RBracket) {
                    self.advance();
                }
                Ok(Expr::List(elements))
            }
            other => Err(CompileError::new(
                &format!("Unexpected expression primary: {:?}", other),
                    0, 0, "",
                    ErrorCode::E0001
                )),
        }
    }
}

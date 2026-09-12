// src/frontend/parser/items.rs

use super::*;

impl Parser {
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
    pub(super) fn parse_function(&mut self) -> Result<FunctionDecl> {
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
    pub(super) fn parse_trait(&mut self) -> Result<TraitDecl> {
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
    pub(super) fn parse_impl(&mut self) -> Result<ImplBlock> {
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
    pub(super) fn parse_import(&mut self) -> Result<Stmt> {
        self.advance();
        let path = match self.advance() {
            Token::StringLit(s) => s,
            Token::Identifier(s) => s,
            other => return Err(self.error(&format!("Expected import path, found {:?}", other))),
        };
        Ok(Stmt::Import { path })
    }
}
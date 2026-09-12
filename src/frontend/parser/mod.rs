// algol26/src/frontend/parser/mod.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::span::Span;
use crate::frontend::ast::{
    BinOp, Expr, ExternDecl, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern, Program, Stmt,
    TraitDecl, TraitMethod, TypeSyntax, UnaryOp, WhereClause,
};
use crate::frontend::lexer::Token;

mod types;
mod pattern;
mod items;
mod stmt;
mod expr;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
pub(super) struct TokenInfo {
    token: Token,
    line: usize,
    column: usize,
}

pub struct Parser {
    pub(super) tokens: Vec<TokenInfo>,
    pub(super) pos: usize,
    pub(super) span_map: std::collections::HashMap<usize, (usize, usize)>,
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

    fn skip_optional_do(&mut self) {
        if matches!(self.peek(), Token::Do) {
            self.advance();
        } else {
            self.skip_keyword("do");
        }
    }
}
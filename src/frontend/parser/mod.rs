// algol26/src/frontend/parser/mod.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::span::Span;
use crate::frontend::ast::{
    BinOp, Expr, ExternDecl, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern, Program, Stmt,
    TraitDecl, TraitMethod, TypeSyntax, UnaryOp, WhereClause,
};
use crate::frontend::lexer::{SpannedToken, Token};

mod expr;
mod items;
mod pattern;
mod stmt;
#[cfg(test)]
mod tests;
mod types;

#[derive(Clone, Debug)]
pub(super) struct TokenInfo {
    token: Token,
    span: Span,
}

pub struct Parser {
    pub(super) tokens: Vec<TokenInfo>,
    pub(super) pos: usize,
    /// Span of the most recently consumed token, updated by `advance()`.
    /// Used by PR-4 to construct compound node spans. Defaults to
    /// `Span::default()` before any token is consumed.
    last_span: Span,
}

impl Parser {
    pub fn new(tokens: Vec<SpannedToken>) -> Self {
        let token_infos = tokens
            .into_iter()
            .map(|st| TokenInfo {
                token: st.token,
                span: st.span,
            })
            .collect();

        Parser {
            tokens: token_infos,
            pos: 0,
            last_span: Span::default(),
        }
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .map(|ti| &ti.token)
            .unwrap_or(&Token::Eof)
    }

    fn peek_info(&self) -> TokenInfo {
        self.tokens.get(self.pos).cloned().unwrap_or(TokenInfo {
            token: Token::Eof,
            span: Span::default(),
        })
    }

    /// Span of the token at the current position (the one `peek` would return).
    /// Returns `Span::default()` at EOF.
    #[allow(dead_code)] // TODO(pr-4): remove once AST nodes carry spans
    pub(super) fn current_span(&self) -> Span {
        self.tokens
            .get(self.pos)
            .map(|ti| ti.span)
            .unwrap_or_default()
    }

    /// Span of the most recently consumed token, as recorded by `advance()`.
    #[allow(dead_code)] // TODO(pr-4): remove once AST nodes carry spans
    pub(super) fn last_span(&self) -> Span {
        self.last_span
    }

    fn advance(&mut self) -> Token {
        let info = self.tokens.get(self.pos).cloned().unwrap_or(TokenInfo {
            token: Token::Eof,
            span: Span::default(),
        });
        self.last_span = info.span;
        self.pos += 1;
        info.token
    }

    fn error(&self, message: &str) -> CompileError {
        let info = self.peek_info();
        CompileError::simple(
            message,
            info.span.start_line,
            info.span.start_column,
            "",
            ErrorCode::E0001,
        )
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

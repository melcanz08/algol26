// src/frontend/lexer/decl.rs

use super::*;

impl Lexer {
    pub(super) fn parse_declaration(
        keyword: Token,
        rest: &str,
        tokens: &mut Vec<Token>,
        positions: &mut Vec<usize>,
    ) {
        tokens.push(keyword);
        if !rest.is_empty() {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();

            if !name.is_empty() {
                let name_len = name.len();
                positions.push(name_len);
                tokens.push(Token::Identifier(name));

                let after_name = &rest[name_len..];
                Lexer::parse_signature(after_name, tokens, positions);
            }
        }
    }

    pub(super) fn parse_signature(signature: &str, tokens: &mut Vec<Token>, positions: &mut Vec<usize>) {
        let mut chars = signature.chars().peekable();
        let mut pos = 0usize;
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                pos += 1;
            } else if c == '(' {
                positions.push(pos);
                chars.next();
                tokens.push(Token::LParen);
            } else if c == ')' {
                chars.next();
                tokens.push(Token::RParen);
            } else if c == ':' {
                chars.next();
                tokens.push(Token::Colon);
            } else if c == ',' {
                chars.next();
                tokens.push(Token::Comma);
            } else if c == '<' {
                chars.next();
                tokens.push(Token::Lt);
            } else if c == '>' {
                chars.next();
                tokens.push(Token::Gt);
            } else if c == '&' {
                chars.next();
                pos += 1;
                tokens.push(Token::Ampersand);
            } else if c == '-' {
                chars.next();
                if let Some(&'>') = chars.peek() {
                    chars.next();
                    tokens.push(Token::Arrow);
                }
            } else if c.is_alphabetic() || c == '_' {
                let ident = Lexer::read_identifier(&mut chars);
                if ident == "where" {
                    tokens.push(Token::Where);
                } else {
                    tokens.push(Token::Identifier(ident));
                }
            } else {
                chars.next();
            }
        }
    }
}
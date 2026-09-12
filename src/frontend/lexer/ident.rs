// src/frontend/lexer/ident.rs

use super::*;

impl Lexer {
    pub(super) fn read_identifier(chars: &mut Peekable<Chars>) -> String {
        let mut ident = String::new();
        while let Some(&ch) = chars.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                chars.next();
            } else {
                break;
            }
        }
        ident
    }

    pub(super) fn classify_identifier(ident: String, tokens: &mut Vec<Token>) {
        match ident.as_str() {
            "print" => tokens.push(Token::Print),
            "defer" => tokens.push(Token::Defer),
            "alloc" => tokens.push(Token::Alloc),
            "free" => tokens.push(Token::Free),
            _ => {
                if let Some(token) = KEYWORDS.get(ident.as_str()) {
                    tokens.push(token.clone());
                } else {
                    tokens.push(Token::Identifier(ident));
                }
            }
        }
    }
}
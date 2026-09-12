// src/frontend/lexer/operator.rs

use super::*;

impl Lexer {
    pub(super) fn handle_operator(
        chars: &mut Peekable<Chars>,
        position: &mut usize,
        line_number: usize,
        line: &str,
        tokens: &mut Vec<Token>,
    ) -> Result<()> {
        let Some(c) = chars.next() else {
            return Ok(());
        };
        *position += 1;

        match c {
            ':' => {
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::Assign);
                } else {
                    tokens.push(Token::Colon);
                }
            }
            '&' => tokens.push(Token::Ampersand),
            '+' => tokens.push(Token::Plus),
            '-' => {
                if let Some(&'>') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::Arrow);
                } else {
                    tokens.push(Token::Minus);
                }
            }
            '*' => tokens.push(Token::Star),
            '/' => tokens.push(Token::Slash),
            '<' => {
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::LessEqual);
                } else {
                    tokens.push(Token::Lt);
                }
            }
            '>' => {
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::GreaterEqual);
                } else {
                    tokens.push(Token::Gt);
                }
            }
            '=' => {
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::Equal);
                } else {
                    return Err(CompileError::simple(
                        "Unexpected '='; use ':=' for assignment or '==' for equality",
                        line_number,
                        *position,
                        line,
                        ErrorCode::E0001,
                    ));
                }
            }
            '!' => {
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::NotEqual);
                } else {
                    return Err(CompileError::simple(
                        "Unexpected '!'; use '!=' for not-equal or 'not' for logical negation",
                        line_number,
                        *position,
                        line,
                        ErrorCode::E0001,
                    ));
                }
            }
            '[' => tokens.push(Token::LBracket),
            ']' => tokens.push(Token::RBracket),
            ',' => tokens.push(Token::Comma),
            '(' => tokens.push(Token::LParen),
            ')' => tokens.push(Token::RParen),
            '.' => {
                // Need to look ahead to distinguish between Dot, DotDot, DotDotEqual, Ellipsis
                if let Some(&next) = chars.peek() {
                    match next {
                        '.' => {
                            chars.next();
                            *position += 1;
                            if let Some(&third) = chars.peek() {
                                if third == '=' {
                                    chars.next();
                                    *position += 1;
                                    tokens.push(Token::DotDotEqual);
                                } else if third == '.' {
                                    chars.next();
                                    *position += 1;
                                    tokens.push(Token::Ellipsis);
                                } else {
                                    tokens.push(Token::DotDot);
                                }
                            } else {
                                tokens.push(Token::DotDot);
                            }
                        }
                        _ => {
                            // Single dot
                            tokens.push(Token::Dot);
                        }
                    }
                } else {
                    tokens.push(Token::Dot);
                }
            }
            _ => {
                return Err(CompileError::simple(
                    &format!("Unexpected character: '{}'", c),
                    line_number,
                    *position,
                    line,
                    ErrorCode::E0001,
                ));
            }
        }
        Ok(())
    }
}
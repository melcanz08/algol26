// src/frontend/lexer/literal.rs

use super::*;

impl Lexer {
    pub(super) fn read_string(
        chars: &mut Peekable<Chars>,
        position: &mut usize,
        line_number: usize,
        line: &str,
    ) -> Result<String> {
        let mut string_content = String::new();

        while let Some(&ch) = chars.peek() {
            if ch == '"' {
                chars.next();
                *position += 1;
                return Ok(string_content);
            } else if ch == '\\' {
                chars.next();
                *position += 1;

                if let Some(&escaped) = chars.peek() {
                    chars.next();
                    *position += 1;

                    match escaped {
                        'n' => string_content.push('\n'),
                        't' => string_content.push('\t'),
                        'r' => string_content.push('\r'),
                        '"' => string_content.push('"'),
                        '\\' => string_content.push('\\'),
                        '0' => string_content.push('\0'),
                        _ => {
                            return Err(CompileError::simple(
                                &format!("Invalid escape sequence: \\{}", escaped),
                                line_number,
                                *position,
                                line,
                                ErrorCode::E0001,
                            ));
                        }
                    }
                } else {
                    return Err(CompileError::simple(
                        "Unterminated escape sequence",
                        line_number,
                        *position,
                        line,
                        ErrorCode::E0001,
                    ));
                }
            } else {
                string_content.push(ch);
                chars.next();
                *position += 1;
            }
        }

        Err(CompileError::simple(
            "Unterminated string literal",
            line_number,
            *position,
            line,
            ErrorCode::E0001,
        ))
    }

    pub(super) fn read_number(chars: &mut Peekable<Chars>) -> Result<(Token, usize)> {
        let mut num_str = String::new();
        let mut is_float = false;
        let mut length = 0;

        // Read integer part
        while let Some(&ch) = chars.peek() {
            if ch.is_numeric() || ch == '_' {
                num_str.push(ch);
                chars.next();
                length += 1;
            } else {
                break;
            }
        }

        // Check for decimal point. Only treat `.` as a decimal
        // point if it is followed by a digit (or an exponent
        // marker `e`/`E`, matching Rust's `5.e10` float form).
        // This distinguishes:
        //
        //   5.0      -> Float(5.0)     (digit after dot)
        //   5.e3     -> Float(5000.0)  (exponent after dot)
        //   5.       -> Float(5.0)     (EOF/whitespace after dot —
        //                               preserves the current
        //                               behavior)
        //   5.inc    -> Int(5) Dot Ident(inc)  (identifier after
        //                               dot — the dot is postfix)
        //   1..5     -> Int(1) DotDot Int(5)   (range, not a
        //                               decimal point)
        //
        // Prior to this check, `5.inc()` lexed as `Float(5.0)
        // Ident(inc)`, which made the parser see two arguments to
        // the enclosing call instead of a method call on `5`.
        if let Some(&c) = chars.peek() {
            if c == '.' {
                // Peek at the char after the dot without consuming.
                let mut lookahead = chars.clone();
                lookahead.next();
                let after_dot = lookahead.peek().copied();
                drop(lookahead);

                let next_is_dot = after_dot == Some('.');
                let next_is_numberish = after_dot.map_or(true, |ch| {
                    // End-of-input after the dot: treat as a float
                    // (`5.`) — preserves the pre-fix behavior.
                    // Digit or exponent marker: a float.
                    // Anything else (identifier, operator, etc.):
                    // a postfix dot, not a decimal point.
                    ch.is_ascii_digit() || ch == 'e' || ch == 'E'
                });

                if !next_is_dot && next_is_numberish {
                    is_float = true;
                    num_str.push('.');
                    chars.next();
                    length += 1;
                    // Read fractional part
                    while let Some(&ch) = chars.peek() {
                        if ch.is_numeric() || ch == '_' {
                            num_str.push(ch);
                            chars.next();
                            length += 1;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        // Check for exponent
        if let Some(&c) = chars.peek() {
            if c == 'e' || c == 'E' {
                is_float = true;
                num_str.push(c);
                chars.next();
                length += 1;
                if let Some(&sign) = chars.peek() {
                    if sign == '+' || sign == '-' {
                        num_str.push(sign);
                        chars.next();
                        length += 1;
                    }
                }
                while let Some(&ch) = chars.peek() {
                    if ch.is_numeric() {
                        num_str.push(ch);
                        chars.next();
                        length += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        let cleaned: String = num_str.chars().filter(|&c| c != '_').collect();

        if is_float {
            match cleaned.parse::<f64>() {
                Ok(val) => Ok((Token::FloatLit(val), length)),
                Err(_) => Err(CompileError::simple(
                    &format!("Invalid float literal: {}", cleaned),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )),
            }
        } else {
            match cleaned.parse::<i64>() {
                Ok(val) => Ok((Token::IntLit(val), length)),
                Err(_) => Err(CompileError::simple(
                    &format!("Invalid integer literal: {}", cleaned),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                )),
            }
        }
    }
}

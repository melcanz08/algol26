// src/frontend/lexer/decl.rs

use super::*;

impl Lexer {
    /// Tokenize a `procedure NAME ...` or `function NAME ...` declaration line.
    ///
    /// `trimmed` is the entire trimmed source line, e.g.
    /// `"procedure main(a: Int, b: Int)"`. `keyword_len` is the length of the
    /// leading keyword so we can skip past it. Offsets passed to
    /// `char_offsets` are 0-based positions from the start of `trimmed`,
    /// matching the convention used by `tokenize_expression`.
    ///
    /// Every token pushed also pushes exactly one offset. That invariant is
    /// what PR-1a relies on to zip tokens and positions together into
    /// `SpannedToken`s.
    pub(super) fn parse_declaration(
        keyword: Token,
        keyword_len: usize,
        trimmed: &str,
        tokens: &mut Vec<Token>,
        char_offsets: &mut Vec<usize>,
    ) {
        debug_assert!(
            trimmed.len() >= keyword_len,
            "keyword_len larger than source line"
        );

        // The keyword itself starts at offset 0.
        tokens.push(keyword);
        char_offsets.push(0);

        let after_keyword = &trimmed[keyword_len..];
        let trimmed_after = after_keyword.trim_start();
        let leading_ws = after_keyword.len() - trimmed_after.len();
        let name_start = keyword_len + leading_ws;

        let name: String = trimmed_after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if name.is_empty() {
            return;
        }

        let name_len = name.chars().count();
        tokens.push(Token::Identifier(name));
        char_offsets.push(name_start);

        // Everything after the name (whitespace, then the signature).
        // parse_signature skips whitespace itself, so we preserve offsets.
        let after_name_byte = trimmed_after
            .char_indices()
            .nth(name_len)
            .map(|(i, _)| i)
            .unwrap_or(trimmed_after.len());
        let after_name = &trimmed_after[after_name_byte..];
        let after_name_start = name_start + name_len;

        Lexer::parse_signature(after_name, after_name_start, tokens, char_offsets);
    }

    /// Tokenize the parameter list and return-type portion of a declaration.
    ///
    /// `signature` is the text following the function name (may start with
    /// whitespace). `base_offset` is the offset of `signature[0]` within the
    /// original trimmed line. Every pushed token also pushes exactly one
    /// offset into `char_offsets`.
    ///
    /// Unknown characters are silently skipped for now. PR-5 will turn those
    /// into diagnostics.
    pub(super) fn parse_signature(
        signature: &str,
        base_offset: usize,
        tokens: &mut Vec<Token>,
        char_offsets: &mut Vec<usize>,
    ) {
        let mut chars = signature.char_indices().peekable();

        while let Some((i, c)) = chars.next() {
            let offset = base_offset + i;

            if c.is_whitespace() {
                continue;
            }

            match c {
                '(' => {
                    tokens.push(Token::LParen);
                    char_offsets.push(offset);
                }
                ')' => {
                    tokens.push(Token::RParen);
                    char_offsets.push(offset);
                }
                ':' => {
                    tokens.push(Token::Colon);
                    char_offsets.push(offset);
                }
                ',' => {
                    tokens.push(Token::Comma);
                    char_offsets.push(offset);
                }
                '<' => {
                    tokens.push(Token::Lt);
                    char_offsets.push(offset);
                }
                '>' => {
                    tokens.push(Token::Gt);
                    char_offsets.push(offset);
                }
                '&' => {
                    tokens.push(Token::Ampersand);
                    char_offsets.push(offset);
                }
                '-' => {
                    if matches!(chars.peek(), Some(&(_, '>'))) {
                        chars.next(); // consume '>'
                        tokens.push(Token::Arrow);
                        char_offsets.push(offset);
                    }
                    // A lone '-' in a signature is skipped for now.
                }
                c if c.is_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    ident.push(c);
                    while let Some(&(_, nc)) = chars.peek() {
                        if nc.is_alphanumeric() || nc == '_' {
                            ident.push(nc);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    if ident == "where" {
                        tokens.push(Token::Where);
                    } else {
                        tokens.push(Token::Identifier(ident));
                    }
                    char_offsets.push(offset);
                }
                _ => {
                    // Unknown character in signature. Silently skipped;
                    // will become a diagnostic in PR-5.
                }
            }
        }
    }
}
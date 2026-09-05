// algol26/src/frontend/lexer.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use std::collections::HashMap;
use std::iter::Peekable;
use std::str::Chars;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // Keywords
    Procedure,
    Function,
    Return,
    Var,
    Val,
    If,
    Else,
    For,
    While,
    In,
    Do,
    Print,
    True,
    False,

    // Literals
    Identifier(String),
    FloatLit(f64),
    IntLit(i64),
    StringLit(String),

    // Operators
    Ampersand,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    GreaterEqual,
    LessEqual,
    Equal,
    NotEqual,
    And,
    Or,
    Not,

    // Delimiters
    LBracket,
    RBracket,
    Comma,
    LParen,
    RParen,
    Colon,
    Arrow,

    // Concurrency
    Spawn,
    Channel,
    Send,
    Receive,
    Parallel,

    // Option/Result
    Some,
    None,
    Ok,
    Error,
    Match,

    // Structure
    Indent,
    Dedent,
    Eof,
    Break,
    Continue,
    Defer,
    Alloc,
    Free,

    // Modules
    Import,

    // Error Handling
    Try,
    Catch,
    Finally,

    // Memory
    Region,
    Unsafe,
    Extern,

    // FFI keywords
    From,
    As,
    Static,
    Dynamic,

    // C types
    CType(CTypeName),
    Ellipsis,

    Lt,          // < for generics and comparison
    Gt,          // > for generics and comparison
    DoubleColon, // :: for trait methods
    Where,       // where clause

    DotDot,

    Trait,
    Impl,
    SelfType,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CTypeName {
    Void,
    Bool,
    Char,
    UChar,
    Short,
    UShort,
    Int,
    UInt,
    Long,
    ULong,
    LongLong,
    ULongLong,
    Float,
    Double,
    CString,
    Pointer,
    ConstPointer,
    SizeT,
    SSizeT,
    IntPtrT,
    UIntPtrT,
}

pub struct Lexer {
    pub tokens: Vec<Token>,
    pub positions: Vec<(usize, usize)>, // (line, column) for each token
}

// MODULE_FUNCTIONS REMOVED:
// The lexer should not know about stdlib functions.
// read_identifier already includes dots, so "Math.sqrt" is just an identifier.
// Semantic resolution (not lexical analysis) determines if it's a valid function.

// Define keywords lookup table
lazy_static::lazy_static! {
    static ref KEYWORDS: HashMap<&'static str, Token> = {
        let mut m = HashMap::new();
        m.insert("procedure", Token::Procedure);
        m.insert("function", Token::Function);
        m.insert("var", Token::Var);
        m.insert("val", Token::Val);
        m.insert("if", Token::If);
        m.insert("else", Token::Else);
        m.insert("for", Token::For);
        m.insert("while", Token::While);
        m.insert("in", Token::In);
        m.insert("do", Token::Do);
        m.insert("true", Token::True);
        m.insert("false", Token::False);
        m.insert("and", Token::And);
        m.insert("or", Token::Or);
        m.insert("not", Token::Not);
        m.insert("return", Token::Return);
        m.insert("spawn", Token::Spawn);
        m.insert("channel", Token::Channel);
        m.insert("send", Token::Send);
        m.insert("receive", Token::Receive);
        m.insert("parallel", Token::Parallel);
        m.insert("Some", Token::Some);
        m.insert("None", Token::None);
        m.insert("Ok", Token::Ok);
        m.insert("Error", Token::Error);
        m.insert("break", Token::Break);
        m.insert("continue", Token::Continue);
        m.insert("match", Token::Match);
        m.insert("import", Token::Import);
        m.insert("try", Token::Try);
        m.insert("catch", Token::Catch);
        m.insert("finally", Token::Finally);
        m.insert("region", Token::Region);
        m.insert("unsafe", Token::Unsafe);
        m.insert("extern", Token::Extern);
        m.insert("from", Token::From);

        m.insert("as", Token::As);
        m.insert("static", Token::Static);
        m.insert("dynamic", Token::Dynamic);
        m.insert("where", Token::Where);

        m.insert("trait", Token::Trait);
        m.insert("impl", Token::Impl);
        m.insert("Self", Token::SelfType);
        m
    };
}

// Helper: Strip comments but not inside string literals
fn strip_comment_not_in_string(line: &str) -> String {
    let mut result = String::new();
    let mut in_string = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '"' {
            in_string = !in_string;
            result.push(c);
        } else if c == '/' && !in_string {
            if let Some(&'/') = chars.peek() {
                break;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
}

impl Lexer {
    pub fn new(source: String) -> Result<Self> {
        let mut tokens = Vec::new();
        let mut indent_stack = vec![0];
        let mut pending_dedents = 0;

        let lines: Vec<&str> = source.lines().collect();
        let mut line_idx = 0;
        let mut token_positions: Vec<(usize, usize)> = Vec::new();
        let mut current_line = 1usize;

        while line_idx < lines.len() || pending_dedents > 0 {
            if pending_dedents > 0 {
                pending_dedents -= 1;
                tokens.push(Token::Dedent);
                continue;
            }

            if line_idx >= lines.len() {
                if indent_stack.len() > 1 {
                    indent_stack.pop();
                    tokens.push(Token::Dedent);
                }
                break;
            }

            let line = lines[line_idx];
            let line_number = line_idx + 1;

            if line.trim().is_empty()
                || line.trim().starts_with("//")
                || line.trim().starts_with("--")
            {
                line_idx += 1;
                continue;
            }

            // --- bulletproof indent: tabs = 4 spaces, mixed = error ---
            let raw_indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
            let has_space = raw_indent.contains(' ');
            let has_tab = raw_indent.contains('\t');

            if has_space && has_tab {
                return Err(CompileError::new(
                    "TabError: Mixed tabs and spaces - use spaces only (4 spaces per indent)",
                    line_number,
                    0,
                    line,
                    ErrorCode::E0001,
                ));
            }

            let indent = raw_indent
                .chars()
                .fold(0, |acc, c| if c == '\t' { acc + 4 } else { acc + 1 });
            // --- end bulletproof ---

            let current_indent = *indent_stack.last().unwrap();

            if indent > current_indent {
                indent_stack.push(indent);
                tokens.push(Token::Indent);
            } else if indent < current_indent {
                if !indent_stack.contains(&indent) {
                    return Err(CompileError::new(
                        &format!(
                            "Inconsistent indentation: expected {} or {} spaces, found {}",
                            current_indent,
                            indent_stack
                                .get(indent_stack.len().saturating_sub(2))
                                .copied()
                                .unwrap_or(0),
                            indent
                        ),
                        line_number,
                        indent,
                        line,
                        ErrorCode::E0001,
                    ));
                }

                let mut popped = 0;
                while let Some(&top) = indent_stack.last() {
                    if top > indent {
                        indent_stack.pop();
                        popped += 1;
                    } else {
                        break;
                    }
                }
                if popped > 1 {
                    pending_dedents = popped - 1;
                }
                tokens.push(Token::Dedent);
                continue;
            }

            let trimmed = line.trim();
            let trimmed = strip_comment_not_in_string(trimmed);
            let trimmed = trimmed.trim();
            line_idx += 1;
            current_line = line_number;

            let token_count_before = tokens.len();
            let mut char_positions: Vec<usize> = Vec::new();
            Lexer::tokenize_line(trimmed, line_number, line, &mut tokens, &mut char_positions)?;
            let base_column = indent + 1;
            for col in &char_positions {
                token_positions.push((current_line, base_column + col));
            }
            let token_count_after = tokens.len();

            // Record (line, column) for each new token
            // Use actual character positions from tokenization
            let base_column = indent;
            for i in token_count_before..token_count_after {
                let col_offset = i - token_count_before;
                token_positions.push((current_line, base_column + col_offset + 1));
            }
        }

        while indent_stack.len() > 1 {
            indent_stack.pop();
            tokens.push(Token::Dedent);
        }

        tokens.push(Token::Eof);
        // Build positions: use token_positions with padding
        let mut positions: Vec<(usize, usize)> = Vec::with_capacity(tokens.len());
        let mut pos_iter = token_positions.iter();
        let mut last_pos = (current_line, 0);
        for _ in 0..tokens.len() {
            if let Some(pos) = pos_iter.next() {
                last_pos = *pos;
                positions.push(last_pos);
            } else {
                positions.push(last_pos);
            }
        }
        Ok(Lexer { tokens, positions })
    }

    fn tokenize_line(
        trimmed: &str,
        line_number: usize,
        line: &str,
        tokens: &mut Vec<Token>,
        positions: &mut Vec<usize>,
    ) -> Result<()> {
        if trimmed.starts_with("procedure") || trimmed.starts_with("proc") {
            let prefix = if trimmed.starts_with("procedure") {
                "procedure"
            } else {
                "proc"
            };
            positions.push(prefix.len());
            let rest = trimmed.strip_prefix(prefix).unwrap().trim();
            Lexer::parse_declaration(Token::Procedure, rest, tokens, positions);
        } else if trimmed.starts_with("function") {
            let rest = trimmed.strip_prefix("function").unwrap().trim();
            positions.push("function".len());
            Lexer::parse_declaration(Token::Function, rest, tokens, positions);
        } else {
            Lexer::tokenize_expression(trimmed, line_number, line, tokens, positions)?;
        }
        Ok(())
    }

    fn parse_declaration(
        keyword: Token,
        rest: &str,
        tokens: &mut Vec<Token>,
        positions: &mut Vec<usize>,
    ) {
        tokens.push(keyword);
        if !rest.is_empty() {
            // Parse function name
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();

            if !name.is_empty() {
                let name_len = name.len(); // Store length before moving
                positions.push(name.len()); // Track position for name
                tokens.push(Token::Identifier(name));

                // Parse the rest (parameters and return type)
                let after_name = &rest[name_len..];
                Lexer::parse_signature(after_name, tokens, positions);
            }
        }
    }

    fn parse_signature(signature: &str, tokens: &mut Vec<Token>, positions: &mut Vec<usize>) {
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
            } else if c == '-' {
                chars.next();
                if let Some(&'>') = chars.peek() {
                    chars.next();
                    tokens.push(Token::Arrow);
                }
            } else if c.is_alphabetic() || c == '_' {
                let ident = Lexer::read_identifier(&mut chars);
                // Check if ident is "where"
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

    fn tokenize_expression(
        expr: &str,
        line_number: usize,
        line: &str,
        tokens: &mut Vec<Token>,
        positions: &mut Vec<usize>,
    ) -> Result<()> {
        let mut chars = expr.chars().peekable();
        let mut position = 0usize;

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                position += 1;
            } else if c == '"' {
                positions.push(position);
                chars.next();
                position += 1;
                let string_content =
                    Lexer::read_string(&mut chars, &mut position, line_number, line)?;
                tokens.push(Token::StringLit(string_content));
            } else if c.is_alphabetic() || c == '_' {
                positions.push(position);
                let ident = Lexer::read_identifier(&mut chars);
                position += ident.len();
                Lexer::classify_identifier(ident, tokens);
            } else if c.is_numeric()
                || (c == '.' && chars.clone().nth(1).is_some_and(|c| c.is_numeric()))
            {
                positions.push(position);
                let (token, len) = Lexer::read_number(&mut chars)?;
                tokens.push(token);
                position += len;
            } else {
                positions.push(position);
                Lexer::handle_operator(&mut chars, &mut position, line_number, line, tokens)?;
            }
        }
        Ok(())
    }

    fn read_identifier(chars: &mut Peekable<Chars>) -> String {
        let mut ident = String::new();
        while let Some(&ch) = chars.peek() {
            if ch.is_alphanumeric() || ch == '_' || ch == '.' {
                ident.push(ch);
                chars.next();
            } else {
                break;
            }
        }
        ident
    }

    fn classify_identifier(ident: String, tokens: &mut Vec<Token>) {
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

    fn read_string(
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
                // Handle escape sequences
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
                            return Err(CompileError::new(
                                &format!("Invalid escape sequence: \\{}", escaped),
                                line_number,
                                *position,
                                line,
                                ErrorCode::E0001,
                            ));
                        }
                    }
                } else {
                    return Err(CompileError::new(
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

        Err(CompileError::new(
            "Unterminated string literal",
            line_number,
            *position,
            line,
            ErrorCode::E0001,
        ))
    }

    fn read_number(chars: &mut Peekable<Chars>) -> Result<(Token, usize)> {
        let mut num_str = String::new();
        let mut has_dot = false;
        let mut has_exp = false;
        let mut length = 0;

        while let Some(&ch) = chars.peek() {
            if ch.is_numeric() || ch == '_' {
                if ch != '_' {
                    num_str.push(ch);
                }
                chars.next();
                length += 1;
            } else if ch == '.' && !has_dot && !has_exp {
                has_dot = true;
                num_str.push(ch);
                chars.next();
                length += 1;

                // Check if next char is digit (else it's a method call like 5.toString())
                if let Some(&next) = chars.peek() {
                    if !next.is_numeric() {
                        // Remove the dot, it's not part of the number
                        num_str.pop();
                        has_dot = false;
                        length -= 1;
                        break;
                    }
                }
            } else if (ch == 'e' || ch == 'E') && !has_exp {
                has_exp = true;
                num_str.push(ch);
                chars.next();
                length += 1;

                // Handle optional +/- after exponent
                if let Some(&sign) = chars.peek() {
                    if sign == '+' || sign == '-' {
                        num_str.push(sign);
                        chars.next();
                        length += 1;
                    }
                }
            } else {
                break;
            }
        }

        // Remove trailing underscores if any
        let cleaned: String = num_str.chars().filter(|&c| c != '_').collect();

        if has_dot || has_exp {
            if let Ok(val) = cleaned.parse::<f64>() {
                Ok((Token::FloatLit(val), length))
            } else {
                Err(CompileError::new(
                    &format!("Invalid float literal: {}", cleaned),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                ))
            }
        } else {
            if let Ok(val) = cleaned.parse::<i64>() {
                Ok((Token::IntLit(val), length))
            } else {
                Err(CompileError::new(
                    &format!("Invalid integer literal: {}", cleaned),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                ))
            }
        }
    }

    fn handle_operator(
        chars: &mut Peekable<Chars>,
        position: &mut usize,
        line_number: usize,
        line: &str,
        tokens: &mut Vec<Token>,
    ) -> Result<()> {
        let c = chars.next().unwrap();
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
                    tokens.push(Token::Lt); // CHANGED from Token::Less
                }
            }
            '>' => {
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::GreaterEqual);
                } else {
                    tokens.push(Token::Gt); // CHANGED from Token::Greater
                }
            }
            '=' => {
                if let Some(&'=') = chars.peek() {
                    chars.next();
                    *position += 1;
                    tokens.push(Token::Equal);
                } else {
                    return Err(CompileError::new(
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
                    return Err(CompileError::new(
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
                // Check for ... (ellipsis)
                if chars.clone().nth(0) == Some('.') && chars.clone().nth(1) == Some('.') {
                    chars.next(); // consume second .
                    chars.next(); // consume third .
                    *position += 2; // ADD THIS LINE
                    tokens.push(Token::Ellipsis);
                } else if chars.clone().nth(0) == Some('.') {
                    // Check for .. (range)
                    chars.next(); // consume second .
                    *position += 1; // ADD THIS LINE
                    tokens.push(Token::DotDot);
                } else {
                    // Handle single dot (maybe for method calls)
                    return Err(CompileError::new(
                        "Unexpected character: '.'",
                        line_number,
                        *position,
                        line,
                        ErrorCode::E0001,
                    ));
                }
            }
            _ => {
                return Err(CompileError::new(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tokens() {
        let source = "var x := 5";
        let lexer = Lexer::new(source.to_string()).unwrap();
        assert_eq!(
            lexer.tokens,
            vec![
                Token::Var,
                Token::Identifier("x".to_string()),
                Token::Assign,
                Token::IntLit(5),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_indentation() {
        let source = "procedure main\n    var x := 5\n    if x > 3\n        print x";
        let lexer = Lexer::new(source.to_string()).unwrap();

        // Check for proper Indent/Dedent tokens
        assert!(lexer.tokens.contains(&Token::Indent));
        assert!(lexer.tokens.contains(&Token::Dedent));

        // Count indents and dedents
        let indent_count = lexer.tokens.iter().filter(|t| **t == Token::Indent).count();
        let dedent_count = lexer.tokens.iter().filter(|t| **t == Token::Dedent).count();
        assert_eq!(indent_count, dedent_count);
    }

    #[test]
    fn test_comments_in_strings() {
        let source = "var s := \"hello // world\"";
        let lexer = Lexer::new(source.to_string()).unwrap();

        assert!(lexer
            .tokens
            .contains(&Token::StringLit("hello // world".to_string())));
    }

    #[test]
    fn test_string_escapes() {
        let source = "var s := \"hello\\nworld\"";
        let lexer = Lexer::new(source.to_string()).unwrap();

        assert!(lexer
            .tokens
            .contains(&Token::StringLit("hello\nworld".to_string())));
    }

    #[test]
    fn test_number_literals() {
        let source = "var a := 123\nvar b := 45.67\nvar c := 1e10\nvar d := 1_000_000";
        let lexer = Lexer::new(source.to_string()).unwrap();

        assert!(lexer.tokens.contains(&Token::IntLit(123)));
        assert!(lexer.tokens.contains(&Token::FloatLit(45.67)));
        assert!(lexer.tokens.contains(&Token::FloatLit(1e10)));
        assert!(lexer.tokens.contains(&Token::IntLit(1000000)));
    }

    #[test]
    fn test_dotted_identifiers() {
        let source = "var x := Math.sqrt(16)";
        let lexer = Lexer::new(source.to_string()).unwrap();

        // "Math.sqrt" is just an identifier with a dot — not lexer-special
        assert!(lexer
            .tokens
            .contains(&Token::Identifier("Math.sqrt".to_string())));
    }

    #[test]
    fn test_function_declaration() {
        let source = "function add(a: Float, b: Float) -> Float\n    return a + b";
        let lexer = Lexer::new(source.to_string()).unwrap();

        assert!(lexer.tokens.contains(&Token::Function));
        assert!(lexer.tokens.contains(&Token::Identifier("add".to_string())));
        assert!(lexer.tokens.contains(&Token::LParen));
        assert!(lexer.tokens.contains(&Token::Identifier("a".to_string())));
        assert!(lexer.tokens.contains(&Token::Colon));
        assert!(lexer
            .tokens
            .contains(&Token::Identifier("Float".to_string())));
        assert!(lexer.tokens.contains(&Token::Comma));
        assert!(lexer.tokens.contains(&Token::Identifier("b".to_string())));
        assert!(lexer.tokens.contains(&Token::RParen));
        assert!(lexer.tokens.contains(&Token::Colon));
        assert!(lexer
            .tokens
            .contains(&Token::Identifier("Float".to_string())));
        assert!(lexer.tokens.contains(&Token::Return));
    }

    #[test]
    fn test_inconsistent_indentation() {
        let source = "procedure main\n    var x := 5\n  var y = 10";
        let result = Lexer::new(source.to_string());

        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.error_code, ErrorCode::E0001);
        }
    }

    #[test]
    fn test_multiline_dedent() {
        let source =
            "procedure main\n    if true\n        var x := 1\n        var y := 2\nvar z := 3";
        let lexer = Lexer::new(source.to_string()).unwrap();

        // Should have proper dedent handling
        let indent_count = lexer.tokens.iter().filter(|t| **t == Token::Indent).count();
        let dedent_count = lexer.tokens.iter().filter(|t| **t == Token::Dedent).count();
        assert_eq!(indent_count, dedent_count);
    }

    #[test]
    fn test_operators() {
        let source = "if x >= 5 and y <= 10 or z != 3";
        let lexer = Lexer::new(source.to_string()).unwrap();

        assert!(lexer.tokens.contains(&Token::GreaterEqual));
        assert!(lexer.tokens.contains(&Token::LessEqual));
        assert!(lexer.tokens.contains(&Token::And));
        assert!(lexer.tokens.contains(&Token::Or));
        assert!(lexer.tokens.contains(&Token::NotEqual));
    }
}

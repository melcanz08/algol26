// algol26/src/frontend/lexer/mod.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::span::Span;
use std::collections::HashMap;
use std::iter::Peekable;
use std::str::Chars;

mod decl;
mod ident;
mod literal;
mod operator;
#[cfg(test)]
mod tests;
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
    NullPtr,
    Mut,

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
    Dot,         // NEW: for method calls and field access
    DotDot,      // range exclusive (..)
    DotDotEqual, // range inclusive (..=)

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

    End,

    Trait,
    Impl,
    SelfType,
    Case, // NEW: case keyword for match arms
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

#[derive(Clone, Debug)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

pub struct Lexer {
    pub tokens: Vec<SpannedToken>,
}
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
        m.insert("null", Token::NullPtr);
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
        m.insert("end", Token::End);
        m.insert("trait", Token::Trait);
        m.insert("impl", Token::Impl);
        m.insert("Self", Token::SelfType);
        m.insert("case", Token::Case);
        m.insert("null", Token::NullPtr);
        m.insert("mut", Token::Mut);
        m
    };
}
// Helper: Strip comments but not inside string literals.
//
// Handles escape sequences inside strings: `\"` does not toggle the
// in-string state, and `\\` does not start an escape. Without this, a
// line like `print("a\"b") // c` leaves the state machine out of sync
// and the trailing comment is mis-tokenized.
fn strip_comment_not_in_string(line: &str) -> String {
    let mut result = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        if escaped {
            // Previous char was a backslash inside a string; this char
            // is its escape target, whatever it is. Push and reset.
            result.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' && in_string {
            result.push(c);
            escaped = true;
            continue;
        }
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
        // Bracket depth for indentation suppression. While a
        // `[...]` or `(...)` is open across lines, the leading
        // whitespace on continuation lines is layout inside the
        // bracketed expression, not block structure. Without this,
        // a multi-line list literal produces a spurious `Indent`.
        let mut bracket_depth: i32 = 0;

        let lines: Vec<&str> = source.lines().collect();
        let mut line_idx = 0;
        let mut token_positions: Vec<(usize, usize)> = Vec::new();
        let mut current_line = 1usize;

        while line_idx < lines.len() || pending_dedents > 0 {
            if pending_dedents > 0 {
                pending_dedents -= 1;
                tokens.push(Token::Dedent);
                // Add dummy position (approximate)
                token_positions.push((current_line, 1));
                continue;
            }

            if line_idx >= lines.len() {
                if indent_stack.len() > 1 {
                    indent_stack.pop();
                    tokens.push(Token::Dedent);
                    token_positions.push((current_line, 1));
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
                return Err(CompileError::simple(
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

            let current_indent = indent_stack.last().copied().unwrap_or(0);

            // Continuation lines inside an open bracket do not
            // participate in indentation tracking.
            let in_bracket_continuation = bracket_depth > 0;

            if !in_bracket_continuation && indent > current_indent {
                indent_stack.push(indent);
                tokens.push(Token::Indent);
                token_positions.push((line_number, 1)); // dummy
            } else if !in_bracket_continuation && indent < current_indent {
                if !indent_stack.contains(&indent) {
                    return Err(CompileError::simple(
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
                token_positions.push((line_number, 1)); // dummy
                continue;
            }

            let trimmed = line.trim();
            let trimmed = strip_comment_not_in_string(trimmed);
            let trimmed = trimmed.trim();
            line_idx += 1;
            current_line = line_number;

            let tokens_before = tokens.len();
            let mut char_positions: Vec<usize> = Vec::new();
            Lexer::tokenize_line(trimmed, line_number, line, &mut tokens, &mut char_positions)?;
            let base_column = indent + 1;

            // Update bracket depth from the tokens this line emitted.
            // `[` and `(` open a continuation; `]` and `)` close one.
            // Bracket pairs on a single line net to zero and are
            // ignored, which is what we want.
            for tok in &tokens[tokens_before..] {
                match tok {
                    Token::LBracket | Token::LParen => bracket_depth += 1,
                    Token::RBracket | Token::RParen => {
                        bracket_depth = (bracket_depth - 1).max(0);
                    }
                    _ => {}
                }
            }

            let tokens_added = tokens.len() - tokens_before;
            if tokens_added != char_positions.len() {
                return Err(CompileError::simple(
                    &format!(
                        "internal lexer error: line {} added {} token(s) but {} position(s)",
                        line_number,
                        tokens_added,
                        char_positions.len()
                    ),
                    line_number,
                    0,
                    line,
                    ErrorCode::E0001,
                ));
            }

            for col in &char_positions {
                token_positions.push((current_line, base_column + col));
            }
        }

        while indent_stack.len() > 1 {
            indent_stack.pop();
            tokens.push(Token::Dedent);
            token_positions.push((current_line, 1));
        }

        tokens.push(Token::Eof);
        token_positions.push((current_line, 0));

        // Ensure lengths match (should, but just in case)
        if tokens.len() != token_positions.len() {
            return Err(CompileError::simple(
                &format!(
                    "internal lexer invariant violated: {} token(s) but {} position(s)",
                    tokens.len(),
                    token_positions.len()
                ),
                current_line,
                0,
                "",
                ErrorCode::E0001,
            ));
        }

        let spanned: Vec<SpannedToken> = tokens
            .into_iter()
            .zip(token_positions)
            .map(|(token, (line, column))| SpannedToken {
                token,
                span: Span::point(line, column),
            })
            .collect();

        Ok(Lexer { tokens: spanned })
    }
    fn tokenize_line(
        trimmed: &str,
        line_number: usize,
        line: &str,
        tokens: &mut Vec<Token>,
        positions: &mut Vec<usize>,
    ) -> Result<()> {
        if trimmed.starts_with("procedure") {
            Lexer::parse_declaration(
                Token::Procedure,
                "procedure".len(),
                trimmed,
                tokens,
                positions,
            );
        } else if trimmed.starts_with("proc") {
            Lexer::parse_declaration(Token::Procedure, "proc".len(), trimmed, tokens, positions);
        } else if trimmed.starts_with("function") {
            Lexer::parse_declaration(
                Token::Function,
                "function".len(),
                trimmed,
                tokens,
                positions,
            );
        } else {
            Lexer::tokenize_expression(trimmed, line_number, line, tokens, positions)?;
        }
        Ok(())
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
                position += ident.chars().count();
                Lexer::classify_identifier(ident, tokens);
            } else if c.is_numeric() {
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
}

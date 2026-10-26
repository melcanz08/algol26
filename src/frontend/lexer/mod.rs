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
    Proc,
    Fn,
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
    /// A char literal: exactly one Unicode scalar value between
    /// single quotes. `u32` is the codepoint (Rust's `char as u32`).
    CharLit(u32),
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
    LBrace, // {
    RBrace, // }
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
    Rec,
    Union,
    SelfType,
    Case,  // case keyword for match arms
    Const, // `const` keyword for associated constants
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

/// ADR 0058. A comment retained by the lexer for the formatter.
///
/// The analyzer and compiler never read these; the lexer discards
/// comments from the token stream. The formatter reads both
/// `Lexer::tokens` (structure) and `Lexer::comments` (trivia) and
/// interleaves them by source position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    /// The comment text including the leading `//`. No trailing
    /// newline.
    pub text: String,
    /// 1-based line number of the comment.
    pub line: usize,
    /// 1-based column of the first character of `text` (the first
    /// `/`).
    pub start_col: usize,
    /// 1-based column of the last character of `text`. Inclusive.
    pub end_col: usize,
    /// True if the comment is alone on its line — nothing before it
    /// but whitespace. False for a trailing comment after code.
    /// The formatter uses this to decide whether the comment goes on
    /// its own line or stays attached to the preceding line.
    pub standalone: bool,
}

pub struct Lexer {
    pub tokens: Vec<SpannedToken>,
    /// ADR 0058. Comments collected during lexing, in source order.
    /// The analyzer and IR builder ignore this field; the formatter
    /// consumes it. Empty when the source has no comments.
    pub comments: Vec<Comment>,
}
lazy_static::lazy_static! {
    static ref KEYWORDS: HashMap<&'static str, Token> = {
        let mut m = HashMap::new();
        m.insert("proc", Token::Proc);
        m.insert("fn", Token::Fn);
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
        m.insert("rec", Token::Rec);
        m.insert("union", Token::Union);
        m.insert("Self", Token::SelfType);
        m.insert("case", Token::Case);
        m.insert("const", Token::Const);
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
/// True when `line` begins with `kw` followed by a non-identifier
/// character (or end-of-line). Used by `tokenize_line` for the very
/// short `fn` keyword: a bare `starts_with("fn")` would also match
/// an identifier like `fnord`.
fn starts_with_keyword(line: &str, kw: &str) -> bool {
    if !line.starts_with(kw) {
        return false;
    }
    line[kw.len()..]
        .chars()
        .next()
        .map_or(true, |c| !c.is_alphanumeric() && c != '_')
}

fn strip_comment_not_in_string(line: &str) -> (String, Option<(usize, String)>) {
    let mut result = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut col = 0usize;
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        col += 1;
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
                // ADR 0058. The `//` starts a comment. `c` is
                // the first `/`; `chars` is positioned at the
                // second `/`. `col` was incremented at the top of
                // the loop, so it is the column of the first `/`.
                let comment_start = col;
                let mut comment_text = String::new();
                comment_text.push(c);
                for c in chars {
                    comment_text.push(c);
                }
                let comment_text = comment_text.trim_end().to_string();
                return (result, Some((comment_start, comment_text)));
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    (result, None)
}
impl Lexer {
    /// ADR 0057. True when `tok` cannot legally end a complete
    /// statement — the next non-blank line is a continuation of
    /// the expression, not a new block. Used to suppress the
    /// lexer's `Indent` emission on such continuations; without
    /// this, `val total := a +\n    b` produced an `Indent`
    /// before `b`, and the parser rejected the `Indent` in
    /// `parse_unary`.
    ///
    /// The set is deliberately narrow: tokens that cannot end a
    /// complete statement under any interpretation. Four
    /// exclusions are load-bearing:
    ///
    /// - `Ampersand` and `Star` can *start* an expression (`&x`,
    ///   `*x`) as well as appear in the middle (`a * b`). Not
    ///   including them here means `val x := a *` at end of line
    ///   is not yet handled; add `Star` if a fixture needs it and
    ///   no regression surfaces.
    /// - `Lt` and `Gt` are excluded because they are both the
    ///   comparison operators AND the generic type brackets.
    ///   `proc f() -> Option<Student>` ends with `Gt` (the
    ///   closing `>` of the generic), and treating that as a
    ///   continuation would suppress the `Indent` for the
    ///   function body on the next line. The unambiguous
    ///   compound forms `LessEqual` / `GreaterEqual` are safe
    ///   and stay.
    /// - `And` and `Or` are excluded because `And` doubles as the
    ///   keyword separating `parallel` blocks:
    ///
    ///   ```text
    ///   parallel do
    ///       stmt
    ///   and
    ///       stmt
    ///   ```
    ///
    ///   After `and`, the next line's `Indent` *must* be emitted
    ///   so the parser sees the second block. Treating `and` as
    ///   a continuation suppresses that `Indent` and merges the
    ///   second block into the first. (Regression found in
    ///   tests/conformance/valid/concurrency/parallel_straightline.gol.)
    fn token_requires_continuation(tok: &Token) -> bool {
        matches!(
            tok,
            Token::Plus
                | Token::Minus
                | Token::Slash
                | Token::Equal
                | Token::NotEqual
                | Token::LessEqual
                | Token::GreaterEqual
                | Token::In
                | Token::Comma
        )
    }

    pub fn new(source: String) -> Result<Self> {
        let mut tokens = Vec::new();
        // ADR 0058. Comments are collected as a side channel so the
        // formatter can interleave them with AST-derived output.
        let mut comments: Vec<Comment> = Vec::new();
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
        let mut token_positions: Vec<(usize, usize, usize)> = Vec::new();
        let mut current_line = 1usize;

        while line_idx < lines.len() || pending_dedents > 0 {
            if pending_dedents > 0 {
                pending_dedents -= 1;
                tokens.push(Token::Dedent);
                // Add dummy position (approximate)
                token_positions.push((current_line, 1, 1));
                continue;
            }

            if line_idx >= lines.len() {
                if indent_stack.len() > 1 {
                    indent_stack.pop();
                    tokens.push(Token::Dedent);
                    token_positions.push((current_line, 1, 1));
                }
                break;
            }

            let line = lines[line_idx];
            let line_number = line_idx + 1;

            // ADR 0058. Whole-line comments are recorded before
            // being skipped. They are standalone: nothing before
            // them on the line but whitespace.
            if line.trim().starts_with("//") {
                let trimmed = line.trim_start();
                let indent = line.len() - trimmed.len();
                let start_col = indent + 1;
                let end_col = start_col + trimmed.chars().count() - 1;
                comments.push(Comment {
                    text: trimmed.trim_end().to_string(),
                    line: line_number,
                    start_col,
                    end_col,
                    standalone: true,
                });
                line_idx += 1;
                continue;
            }
            if line.trim().is_empty() || line.trim().starts_with("--") {
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

            // A line whose first non-whitespace character is `.`
            // continues the previous expression's method chain.
            // Suppress both the Indent and Dedent machinery for this
            // line so the parser sees `expr . method` as one logical
            // statement.
            let is_chain_continuation = line.trim_start().starts_with('.');

            // ADR 0057. A line whose previous non-blank line ended
            // with a binary operator is a continuation of the
            // expression. Suppress the Indent (and Dedent) for it,
            // same as a bracket continuation or a chain
            // continuation. Without this, `val total := a +` on one
            // line and `    b` on the next produced a spurious
            // Indent before `b`, and `parse_unary` rejected it.
            let is_operator_continuation = tokens
                .last()
                .map(Self::token_requires_continuation)
                .unwrap_or(false);

            if !in_bracket_continuation
                && !is_chain_continuation
                && !is_operator_continuation
                && indent > current_indent
            {
                indent_stack.push(indent);
                tokens.push(Token::Indent);
                token_positions.push((line_number, 1, 1));
            } else if !in_bracket_continuation
                && !is_chain_continuation
                && !is_operator_continuation
                && indent < current_indent
            {
                // ... existing dedent logic unchanged
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
                token_positions.push((line_number, 1, 1)); // dummy
                continue;
            }

            let trimmed = line.trim();
            let (trimmed, trailing_comment) = strip_comment_not_in_string(trimmed);
            let trimmed = trimmed.trim().to_string();
            // ADR 0058. Record the trailing comment (if any) with
            // its column. `standalone` is false — this comment is
            // attached to a code line.
            if let Some((col, text)) = trailing_comment {
                let end_col = col + text.chars().count() - 1;
                comments.push(Comment {
                    text,
                    line: line_number,
                    start_col: col,
                    end_col,
                    standalone: false,
                });
            }
            line_idx += 1;
            current_line = line_number;

            let tokens_before = tokens.len();
            let mut char_positions: Vec<(usize, usize)> = Vec::new();
            Lexer::tokenize_line(
                &trimmed,
                line_number,
                line,
                &mut tokens,
                &mut char_positions,
            )?;
            let base_column = indent + 1;

            // Update bracket depth from the tokens this line emitted.
            // `[` and `(` open a continuation; `]` and `)` close one.
            // Bracket pairs on a single line net to zero and are
            // ignored, which is what we want.
            for tok in &tokens[tokens_before..] {
                match tok {
                    Token::LBracket | Token::LParen | Token::LBrace => bracket_depth += 1,
                    Token::RBracket | Token::RParen | Token::RBrace => {
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

            for (start, end) in &char_positions {
                token_positions.push((
                    current_line,
                    base_column + start,
                    base_column + end.saturating_sub(1),
                ));
            }
        }

        while indent_stack.len() > 1 {
            indent_stack.pop();
            tokens.push(Token::Dedent);
            token_positions.push((current_line, 1, 1));
        }

        tokens.push(Token::Eof);
        token_positions.push((current_line, 0, 0));

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
            .map(|(token, (line, start, end))| SpannedToken {
                token,
                span: Span::new(line, start, line, end),
            })
            .collect();

        Ok(Lexer {
            tokens: spanned,
            comments,
        })
    }
    fn tokenize_line(
        trimmed: &str,
        line_number: usize,
        line: &str,
        tokens: &mut Vec<Token>,
        positions: &mut Vec<(usize, usize)>,
    ) -> Result<()> {
        if starts_with_keyword(trimmed, "proc") {
            Lexer::parse_declaration(Token::Proc, "proc".len(), trimmed, tokens, positions);
        } else if starts_with_keyword(trimmed, "fn") {
            // ADR 0048. `fn` is only two characters, so a bare
            // `starts_with("fn")` would also match an identifier
            // like `fnord`. Recognize the keyword only when it is
            // followed by a non-identifier character (space, `(`,
            // `<`, or end-of-line).
            Lexer::parse_declaration(Token::Fn, "fn".len(), trimmed, tokens, positions);
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
        positions: &mut Vec<(usize, usize)>,
    ) -> Result<()> {
        let mut chars = expr.chars().peekable();
        let mut position = 0usize;

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
                position += 1;
            } else if c == '"' {
                let start = position;
                chars.next();
                position += 1;
                let string_content =
                    Lexer::read_string(&mut chars, &mut position, line_number, line)?;
                tokens.push(Token::StringLit(string_content));
                positions.push((start, position));
            } else if c == '\'' {
                let start = position;
                chars.next();
                position += 1;
                let tok = Lexer::read_char(&mut chars, &mut position, line_number, line)?;
                tokens.push(tok);
                positions.push((start, position));
            } else if c.is_alphabetic() || c == '_' {
                let start = position;
                let ident = Lexer::read_identifier(&mut chars);
                position += ident.chars().count();
                Lexer::classify_identifier(ident, tokens);
                positions.push((start, position));
            } else if c.is_numeric() {
                let start = position;
                let (token, len) = Lexer::read_number(&mut chars)?;
                tokens.push(token);
                position += len;
                positions.push((start, position));
            } else {
                let start = position;
                Lexer::handle_operator(&mut chars, &mut position, line_number, line, tokens)?;
                positions.push((start, position));
            }
        }
        Ok(())
    }
}

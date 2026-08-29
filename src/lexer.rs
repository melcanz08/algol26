// algole26/src/lexer.rs

use crate::diagnostics::{CompileError, ErrorCode, Result};

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
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Greater,
    Less,
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
    
    // Modules
    Import,
    
    // Error Handling
    Try,
    Catch,
    Finally,
}

pub struct Lexer {
    pub tokens: Vec<Token>,
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

            if line.trim().is_empty() || line.trim().starts_with("//") || line.trim().starts_with("--") {
                line_idx += 1;
                continue;
            }

            let spaces = line.chars().take_while(|c| *c == ' ').count();
            let current_indent = *indent_stack.last().unwrap();

            if spaces > current_indent {
                indent_stack.push(spaces);
                tokens.push(Token::Indent);
            } else if spaces < current_indent {
                if !indent_stack.contains(&spaces) {
                    return Err(CompileError::new(
                        &format!(
                            "Inconsistent indentation: expected {} or {} spaces, found {}",
                            current_indent, 
                            indent_stack.get(indent_stack.len().saturating_sub(2)).copied().unwrap_or(0),
                            spaces
                        ),
                        line_number,
                        spaces,
                        line,
                        ErrorCode::E0001
                    ));
                }
                
                let mut popped = 0;
                while let Some(&top) = indent_stack.last() {
                    if top > spaces {
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

            if trimmed.starts_with("procedure") || trimmed.starts_with("proc") {
                tokens.push(Token::Procedure);
                let rest = if trimmed.starts_with("procedure") {
                    trimmed.strip_prefix("procedure").unwrap().trim()
                } else {
                    trimmed.strip_prefix("proc").unwrap().trim()
                };
                if !rest.is_empty() {
                    let name = rest.split_whitespace().next().unwrap_or("");
                    if !name.is_empty() {
                        tokens.push(Token::Identifier(name.to_string()));
                    }
                }
            } else if trimmed.starts_with("function") {
                tokens.push(Token::Function);
                let rest = trimmed.strip_prefix("function").unwrap().trim();
                if !rest.is_empty() {
                    // Parse function name (alphanumeric + underscore)
                    let name: String = rest.chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    
                    if !name.is_empty() {
                        tokens.push(Token::Identifier(name.clone()));
                    }
                    
                    // Tokenize the rest (params and return type)
                    let after_name = &rest[name.len()..];
                    let mut chars = after_name.chars().peekable();
                    
                    while let Some(&c) = chars.peek() {
                        if c.is_whitespace() {
                            chars.next();
                        } else if c == '(' {
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
                        } else if c == '-' {
                            chars.next();
                            if let Some(&'>') = chars.peek() {
                                chars.next();
                                // Convert -> to : for return type
                                tokens.push(Token::Colon);
                            }
                        } else if c.is_alphabetic() || c == '_' {
                            let mut ident = String::new();
                            while let Some(&ch) = chars.peek() {
                                if ch.is_alphanumeric() || ch == '_' {
                                    ident.push(ch);
                                    chars.next();
                                } else {
                                    break;
                                }
                            }
                            tokens.push(Token::Identifier(ident));
                        } else {
                            chars.next();
                        }
                    }
                }
            } else {
                let mut chars = trimmed.chars().peekable();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() {
                        chars.next();
                    } else if c == '"' {
                        chars.next();
                        let mut string_content = String::new();
                        while let Some(&ch) = chars.peek() {
                            if ch == '"' {
                                chars.next();
                                break;
                            }
                            string_content.push(ch);
                            chars.next();
                        }
                        tokens.push(Token::StringLit(string_content));
                    } else if c.is_alphabetic() || c == '_' {
                        let mut ident = String::new();
                        while let Some(&ch) = chars.peek() {
                            if ch.is_alphanumeric() || ch == '_' || ch == '.' {
                                ident.push(ch);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        match ident.as_str() {
                            "var" => tokens.push(Token::Var),
                            "val" => tokens.push(Token::Val),
                            "if" => tokens.push(Token::If),
                            "else" => tokens.push(Token::Else),
                            "for" => tokens.push(Token::For),
                            "while" => tokens.push(Token::While),
                            "in" => tokens.push(Token::In),
                            "do" => tokens.push(Token::Do),
                            "true" => tokens.push(Token::True),
                            "false" => tokens.push(Token::False),
                            "and" => tokens.push(Token::And),
                            "or" => tokens.push(Token::Or),
                            "not" => tokens.push(Token::Not),
                            "Terminal.print" | "print" => tokens.push(Token::Print),
                            "Math.sqrt" => tokens.push(Token::Identifier("Math.sqrt".to_string())),
                            "Math.pow" => tokens.push(Token::Identifier("Math.pow".to_string())),
                            "Math.sin" => tokens.push(Token::Identifier("Math.sin".to_string())),
                            "Math.cos" => tokens.push(Token::Identifier("Math.cos".to_string())),
                            "Math.abs" => tokens.push(Token::Identifier("Math.abs".to_string())),
                            "Math.floor" => tokens.push(Token::Identifier("Math.floor".to_string())),
                            "Math.ceil" => tokens.push(Token::Identifier("Math.ceil".to_string())),
                            "Math.exp" => tokens.push(Token::Identifier("Math.exp".to_string())),
                            "Math.log" => tokens.push(Token::Identifier("Math.log".to_string())),
                            "Math.tan" => tokens.push(Token::Identifier("Math.tan".to_string())),
                            "File.read" => tokens.push(Token::Identifier("File.read".to_string())),
                            "File.write" => tokens.push(Token::Identifier("File.write".to_string())),
                            "File.append" => tokens.push(Token::Identifier("File.append".to_string())),
                            "List.length" => tokens.push(Token::Identifier("List.length".to_string())),
                            "List.sum" => tokens.push(Token::Identifier("List.sum".to_string())),
                            "List.max" => tokens.push(Token::Identifier("List.max".to_string())),
                            "List.min" => tokens.push(Token::Identifier("List.min".to_string())),
                            "return" => tokens.push(Token::Return),
                            "spawn" => tokens.push(Token::Spawn),
                            "channel" => tokens.push(Token::Channel),
                            "send" => tokens.push(Token::Send),
                            "receive" => tokens.push(Token::Receive),
                            "parallel" => tokens.push(Token::Parallel),
                            "Some" => tokens.push(Token::Some),
                            "None" => tokens.push(Token::None),
                            "Ok" => tokens.push(Token::Ok),
                            "Error" => tokens.push(Token::Error),
                            "defer" => tokens.push(Token::Identifier("defer".to_string())),
                            "break" => tokens.push(Token::Break),
                            "continue" => tokens.push(Token::Continue),
                            "match" => tokens.push(Token::Match),
                            "import" => tokens.push(Token::Import),
                            "try" => tokens.push(Token::Try),
                            "catch" => tokens.push(Token::Catch),
                            "finally" => tokens.push(Token::Finally),
                            _ => tokens.push(Token::Identifier(ident)),
                        }
                    } else if c.is_numeric() {
                        let mut num_str = String::new();
                        while let Some(&ch) = chars.peek() {
                            if ch.is_numeric() || ch == '.' {
                                num_str.push(ch);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        if num_str.contains('.') {
                            if let Ok(val) = num_str.parse::<f64>() {
                                tokens.push(Token::FloatLit(val));
                            }
                        } else {
                            if let Ok(val) = num_str.parse::<i64>() {
                                tokens.push(Token::IntLit(val));
                            }
                        }
                    } else {
                        match c {
                            ':' => {
                                chars.next();
                                if let Some(&'=') = chars.peek() {
                                    chars.next();
                                    tokens.push(Token::Assign);
                                } else {
                                    tokens.push(Token::Colon);
                                }
                            }
                            '+' => { chars.next(); tokens.push(Token::Plus); }
                            '-' => { chars.next(); tokens.push(Token::Minus); }
                            '*' => { chars.next(); tokens.push(Token::Star); }
                            '/' => { chars.next(); tokens.push(Token::Slash); }
                            '>' => {
                                chars.next();
                                if let Some(&'=') = chars.peek() {
                                    chars.next();
                                    tokens.push(Token::GreaterEqual);
                                } else {
                                    tokens.push(Token::Greater);
                                }
                            }
                            '<' => {
                                chars.next();
                                if let Some(&'=') = chars.peek() {
                                    chars.next();
                                    tokens.push(Token::LessEqual);
                                } else {
                                    tokens.push(Token::Less);
                                }
                            }
                            '=' => {
                                chars.next();
                                if let Some(&'=') = chars.peek() {
                                    chars.next();
                                    tokens.push(Token::Equal);
                                }
                            }
                            '!' => {
                                chars.next();
                                if let Some(&'=') = chars.peek() {
                                    chars.next();
                                    tokens.push(Token::NotEqual);
                                }
                            }
                            '[' => { chars.next(); tokens.push(Token::LBracket); }
                            ']' => { chars.next(); tokens.push(Token::RBracket); }
                            ',' => { chars.next(); tokens.push(Token::Comma); }
                            '(' => { chars.next(); tokens.push(Token::LParen); }
                            ')' => { chars.next(); tokens.push(Token::RParen); }
                            _ => {
                                return Err(CompileError::new(
                                    &format!("Unexpected character: '{}'", c),
                                    line_number,
                                    trimmed.len() - chars.clone().count(),
                                    line,
                                    ErrorCode::E0001
                                ));
                            }
                        }
                    }
                }
            }
        }

        while indent_stack.len() > 1 {
            indent_stack.pop();
            tokens.push(Token::Dedent);
        }

        tokens.push(Token::Eof);
        Ok(Lexer { tokens })
    }
}
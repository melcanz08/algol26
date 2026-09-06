#![allow(dead_code)]

// algol26/src/frontend/ast.rs - 100% Orthogonal: Everything is an Expression

use crate::common::span::Span;

#[derive(Clone, Debug)]
pub enum Expr {
    Number(f64),
    Int(i64),
    String(String),
    Bool(bool),
    Var(String, Span),
    /// Represents an inline scoped block of code that evaluates to a value.
    /// Example: val result := val a := 5; a + 10 end
    Block {
        statements: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
    },
    /// Evolved If-Else that acts as a value-producing expression.
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,         // Must be an Expr::Block
        else_branch: Option<Box<Expr>>, // Must be an Expr::Block
    },
    /// Evolved Match that acts as a value-producing expression.
    Match {
        value: Box<Expr>,
        cases: Vec<MatchCaseExpr>,
    },
    Borrow {
        expr: Box<Expr>,
    },
    MutBorrow {
        expr: Box<Expr>,
    },
    Deref {
        expr: Box<Expr>,
    },
    AddrOf {
        expr: Box<Expr>,
    },
    List(Vec<Expr>),
    ArrayAccess {
        array: Box<Expr>,
        index: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
        span: Span,
    },
    // Option type
    Some {
        value: Box<Expr>,
    },
    None,
    // Result type
    Ok {
        value: Box<Expr>,
    },
    /// Evolved Try-Catch that acts as a value-producing expression.
    TryCatch {
        try_branch: Box<Expr>, // Must be an Expr::Block
        catch_var: Option<String>,
        catch_branch: Box<Expr>,         // Must be an Expr::Block
        finally_body: Option<Vec<Stmt>>, // Finalizing side-effects (runs regardless)
    },
    Error {
        value: Box<Expr>,
    },
    // --- ORTHOGONAL: Loops as expressions ---
    /// For loop as expression - returns last trailing_expr from last iteration, or Void
    /// Example: val sum := for item in [1.0, 2.0, 3.0] do item + 1.0
    For {
        var: String,
        iterable: Box<Expr>,
        body: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
        span: Span,
    },
    /// While loop as expression - returns last trailing_expr, or Void
    /// Example: val result := while x < 10.0 do x := x + 1.0; x
    While {
        condition: Box<Expr>,
        body: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
        span: Span,
    },
    /// Raw pointer value (only valid in unsafe blocks)
    PtrLiteral(usize),
    /// Null pointer
    NullPtr,
    /// Cast expression for FFI
    Cast {
        expr: Box<Expr>,
        target_type: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum UnaryOp {
    Negate, // -x
    Not,    // not x
}

#[derive(Clone, Debug, PartialEq)]
pub enum BinOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum Stmt {
    VarDecl {
        name: String,
        value: Expr,
        type_annotation: Option<String>,
        mutable: bool,
        span: Span,
    },
    Import {
        path: String,
    },
    RegionBlock {
        name: String,
        body: Vec<Stmt>,
    },
    UnsafeBlock {
        body: Vec<Stmt>,
    },
    Assign {
        name: String,
        value: Expr,
    },
    ArrayAssign {
        array: String,
        index: Expr,
        value: Expr,
    },
    Return {
        value: Option<Expr>,
    },
    Print {
        expr: Expr,
    },
    Defer {
        stmt: Box<Stmt>,
    },
    Break,
    Continue,
    Spawn {
        body: Vec<Stmt>,
    },
    Parallel {
        blocks: Vec<Vec<Stmt>>,
    },
    ChannelDecl {
        name: String,
    },
    Send {
        channel: String,
        value: Expr,
    },
    Receive {
        channel: String,
        target: String,
    },
    /// THE UNIFIER: Allows any standalone Expr to be executed as a basic statement.
    /// This entirely replaces the old standalone Stmt::If, Stmt::Match, Stmt::TryCatch,
    /// and Stmt::FunctionCall variants!
    Expression(Expr),
}

#[derive(Clone, Debug)]
pub struct WhereClause {
    pub type_param: String,
    pub trait_name: String,
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<(String, Option<TypeSyntax>)>,
    pub return_type: Option<TypeSyntax>,
    pub body: Vec<Stmt>,
    pub is_extern: bool,
    pub ffi_info: Option<ExternDecl>,
    pub type_params: Vec<String>,
    pub where_clauses: Vec<WhereClause>,
}

#[derive(Clone, Debug)]
pub struct MatchCaseExpr {
    pub pattern: Pattern,
    pub body: Expr, // Evaluates directly to an Expr (usually an Expr::Block)
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Some(String),
    None,
    Ok(String),
    Error(String),
    Wildcard,
    Binding(String),
    Literal(Expr),
    // NEW: Nested patterns
    SomeNested(Box<Pattern>),
    OkNested(Box<Pattern>),
    ErrorNested(Box<Pattern>),
    // NEW: Pattern guards
    Guarded {
        pattern: Box<Pattern>,
        condition: Box<Expr>,
    },
    // NEW: List destructuring
    ListDestructure {
        first: Option<Box<Pattern>>,
        rest: Option<Box<Pattern>>,
    },
    // NEW: Range patterns
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
}

#[derive(Clone, Debug)]
pub struct TraitDecl {
    pub name: String,
    pub methods: Vec<TraitMethod>,
}

#[derive(Clone, Debug)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<(String, Option<TypeSyntax>)>, // (param_name, type)
    pub return_type: Option<TypeSyntax>,
}

#[derive(Clone, Debug)]
pub struct ImplBlock {
    pub trait_name: String,
    pub target_type: String,
    pub methods: Vec<FunctionDecl>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeSyntax {
    /// Named type like Int, Float, String, Self
    Named(String),
    /// Generic type like Option<Int>, Result<Int, String>
    Generic { name: String, args: Vec<TypeSyntax> },
    /// Unknown/inferred type (no annotation)
    Unknown,
}

impl TypeSyntax {
    /// Convert TypeSyntax to string for backward compatibility
    pub fn to_string_rep(&self) -> String {
        match self {
            TypeSyntax::Named(name) => name.clone(),
            TypeSyntax::Generic { name, args } => {
                let args_str: Vec<String> = args.iter().map(|a| a.to_string_rep()).collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
            TypeSyntax::Unknown => String::new(),
        }
    }

    /// Get the string representation (for semantic analysis)
    pub fn as_str(&self) -> &str {
        match self {
            TypeSyntax::Named(name) => name,
            TypeSyntax::Generic { name, .. } => name,
            TypeSyntax::Unknown => "",
        }
    }

    /// Create TypeSyntax from a type name string
    pub fn from_string(s: &str) -> Self {
        if s.is_empty() {
            TypeSyntax::Unknown
        } else if s.contains('<') || s.contains('[') {
            let (open_char, close_char) = if s.contains('<') {
                ('<', '>')
            } else {
                ('[', ']')
            };
            let Some(open_pos) = s.find(open_char) else {
                return TypeSyntax::Unknown;
            };
            let close_pos = s.rfind(close_char).unwrap_or(s.len());
            let name = &s[..open_pos];
            let args_str = &s[open_pos + 1..close_pos];
            let args: Vec<TypeSyntax> = args_str
                .split(',')
                .map(|a| TypeSyntax::from_string(a.trim()))
                .collect();
            TypeSyntax::Generic {
                name: name.to_string(),
                args,
            }
        } else {
            TypeSyntax::Named(s.to_string())
        }
    }
}

/// Raw FFI declaration syntax captured by the parser
/// The FFI lowering pass converts this to FFIInfo
#[derive(Clone, Debug, Default)]
pub struct ExternDecl {
    pub abi: Option<String>,
    pub library: Option<String>,
    pub symbol_name: Option<String>,
    pub variadic: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub imports: Vec<String>,
    pub functions: Vec<FunctionDecl>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}

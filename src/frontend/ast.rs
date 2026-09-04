#![allow(dead_code)]

// algol26/src/ast.rs - 100% Orthogonal: Everything is an Expression

use crate::ffi::c::{FFIInfo};

#[derive(Clone, Debug)]
pub enum Expr {
    Number(f64),
    Int(i64),
    String(String),
    Bool(bool),
    Var(String, (usize, usize)),
    /// Represents an inline scoped block of code that evaluates to a value.
    /// Example: val result := val a := 5; a + 10 end
    Block {
        statements: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
    },
    /// Evolved If-Else that acts as a value-producing expression.
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,          // Must be an Expr::Block
        else_branch: Option<Box<Expr>>,  // Must be an Expr::Block
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
    FunctionCall {
        name: String,
        args: Vec<Expr>,
        span: (usize, usize),
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
        try_branch: Box<Expr>,          // Must be an Expr::Block
        catch_var: Option<String>,
        catch_branch: Box<Expr>,        // Must be an Expr::Block
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
        span: (usize, usize),
    },
    /// While loop as expression - returns last trailing_expr, or Void
    /// Example: val result := while x < 10.0 do x := x + 1.0; x
    While {
        condition: Box<Expr>,
        body: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
        span: (usize, usize),
    },
    /// Raw pointer value (only valid in unsafe blocks)
    PtrLiteral(usize),
    /// Null pointer
    NullPtr,
    /// Cast expression for FFI
    Cast {
        expr: Box<Expr>,
        target_type: String,
    }
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
        span: (usize, usize),
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
    For {
        var: String,
        iterable: Expr,
        body: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
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
    pub params: Vec<(String, String)>,
    pub return_type: Option<String>,
    pub body: Vec<Stmt>,
    pub is_extern: bool,
    pub ffi_info: Option<FFIInfo>,
    pub type_params: Vec<String>,
    pub where_clauses: Vec<WhereClause>,
}

#[derive(Clone, Debug)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
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
    pub params: Vec<(String, String)>,  // (param_name, type)
    pub return_type: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ImplBlock {
    pub trait_name: String,
    pub target_type: String,
    pub methods: Vec<FunctionDecl>,
}

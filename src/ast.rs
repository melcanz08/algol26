#![allow(dead_code)]

// algol26/src/ast.rs

#[derive(Clone, Debug)]
pub enum Expr {
    Number(f64),
    Int(i64),
    String(String),
    Bool(bool),
    Var(String),
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
    Error {
        value: Box<Expr>,
    },
}

#[derive(Clone, Debug)]
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
    },
    Import {
        path: String,
    },
    TryCatch {
        try_body: Vec<Stmt>,
        catch_var: Option<String>,
        catch_body: Vec<Stmt>,
        finally_body: Option<Vec<Stmt>>,
    },
    Assign { 
        name: String, 
        value: Expr 
    },
    ArrayAssign {
        array: String,
        index: Expr,
        value: Expr,
    },
    If { 
        condition: Expr, 
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    For { 
        var: String, 
        iterable: Expr, 
        body: Vec<Stmt> 
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    Return {
        value: Option<Expr>,
    },
    Print { 
        expr: Expr 
    },
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },
    Defer {
        stmt: Box<Stmt>,
    },
    Break,
    Continue,
    Match {
        value: Expr,
        cases: Vec<MatchCase>,
    },
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
}

#[derive(Clone, Debug)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<(String, String)>,
    pub return_type: Option<String>,
    pub body: Vec<Stmt>,
}


#[derive(Clone, Debug)]
pub struct MatchCase {
    pub pattern: Pattern,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Some(String),
    None,
    Ok(String),
    Error(String),
    Wildcard,
    Literal(Expr),
}

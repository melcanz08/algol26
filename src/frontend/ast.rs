// algol26/src/frontend/ast.rs

use crate::common::span::Span;

/// Stable identity for an AST expression node.
///
/// Assigned exactly once by `assign_expr_ids`, after the last AST
/// transformation (impl expansion) and before semantic analysis.
/// `UNASSIGNED` is the construction-time sentinel; any node reachable
/// from the typed AST must have a real ID by the time the analyzer runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExprId(pub u32);

impl ExprId {
    /// Sentinel for a freshly constructed node that has not yet been
    /// numbered. The numbering pass replaces every occurrence.
    pub const UNASSIGNED: Self = Self(u32::MAX);

    pub fn is_assigned(self) -> bool {
        self != Self::UNASSIGNED
    }
}

/// An expression node: stable identity + structural kind.
#[derive(Clone, Debug)]
pub struct Expr {
    pub id: ExprId,
    pub kind: ExprKind,
}

impl Expr {
    /// Construct a node with the `UNASSIGNED` sentinel. The numbering
    /// pass replaces it before the AST reaches semantic analysis.
    pub fn new(kind: ExprKind) -> Self {
        Self {
            id: ExprId::UNASSIGNED,
            kind,
        }
    }

    /// Convenience for the very common `Box::new(Expr::new(kind))` shape.
    pub fn boxed(kind: ExprKind) -> Box<Self> {
        Box::new(Self::new(kind))
    }

    /// Source span of this expression node.
    pub fn span(&self) -> Span {
        self.kind.span()
    }
}

/// The structural content of an expression, without its identity.
#[derive(Clone, Debug)]
pub enum ExprKind {
    // ─── Literals ───
    Number(f64, Span),
    Int(i64, Span),
    String(String, Span),
    Bool(bool, Span),
    NullPtr(Span),
    PtrLiteral(usize, Span),

    Var(String, Span),

    /// Inline scoped block.
    Block {
        statements: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
        span: Span,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
        span: Span,
    },
    Match {
        value: Box<Expr>,
        cases: Vec<MatchCaseExpr>,
        span: Span,
    },
    Borrow {
        expr: Box<Expr>,
        span: Span,
    },
    MutBorrow {
        expr: Box<Expr>,
        span: Span,
    },
    Deref {
        expr: Box<Expr>,
        span: Span,
    },
    AddrOf {
        expr: Box<Expr>,
        span: Span,
    },
    List(Vec<Expr>, Span),
    ArrayAccess {
        array: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
        span: Span,
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
    Some {
        value: Box<Expr>,
        span: Span,
    },
    None(Span),
    Ok {
        value: Box<Expr>,
        span: Span,
    },
    Error {
        value: Box<Expr>,
        span: Span,
    },
    TryCatch {
        try_branch: Box<Expr>,
        catch_var: Option<String>,
        catch_branch: Box<Expr>,
        finally_body: Option<Vec<Stmt>>,
        span: Span,
    },
    For {
        var: String,
        iterable: Box<Expr>,
        body: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
        span: Span,
    },
    While {
        condition: Box<Expr>,
        body: Vec<Stmt>,
        trailing_expr: Option<Box<Expr>>,
        span: Span,
    },
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
        span: Span,
    },
    FieldAccess {
        object: Box<Expr>,
        field: String,
        span: Span,
    },
    RecordLiteral {
        name: String,
        type_args: Vec<TypeSyntax>,
        fields: Vec<(String, Expr)>,
        span: Span,
    },
}

impl ExprKind {
    /// Source span of this expression kind.
    pub fn span(&self) -> Span {
        match self {
            ExprKind::Number(_, s) => *s,
            ExprKind::Int(_, s) => *s,
            ExprKind::String(_, s) => *s,
            ExprKind::Bool(_, s) => *s,
            ExprKind::NullPtr(s) => *s,
            ExprKind::PtrLiteral(_, s) => *s,
            ExprKind::Var(_, s) => *s,
            ExprKind::Block { span, .. } => *span,
            ExprKind::If { span, .. } => *span,
            ExprKind::Match { span, .. } => *span,
            ExprKind::Borrow { span, .. } => *span,
            ExprKind::MutBorrow { span, .. } => *span,
            ExprKind::Deref { span, .. } => *span,
            ExprKind::AddrOf { span, .. } => *span,
            ExprKind::List(_, s) => *s,
            ExprKind::ArrayAccess { span, .. } => *span,
            ExprKind::Binary { span, .. } => *span,
            ExprKind::Unary { span, .. } => *span,
            ExprKind::FunctionCall { span, .. } => *span,
            ExprKind::Some { span, .. } => *span,
            ExprKind::None(s) => *s,
            ExprKind::Ok { span, .. } => *span,
            ExprKind::Error { span, .. } => *span,
            ExprKind::TryCatch { span, .. } => *span,
            ExprKind::For { span, .. } => *span,
            ExprKind::While { span, .. } => *span,
            ExprKind::Range { span, .. } => *span,
            ExprKind::FieldAccess { span, .. } => *span,
            ExprKind::RecordLiteral { span, .. } => *span,
        }
    }
}

// Everything below this line is unchanged from the current file.

#[derive(Clone, Debug, PartialEq)]
pub enum UnaryOp {
    Negate,
    Not,
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

#[derive(Clone, Debug)]
pub enum Stmt {
    VarDecl {
        name: String,
        value: Expr,
        type_annotation: Option<TypeSyntax>,
        mutable: bool,
        span: Span,
    },
    Import {
        path: String,
        span: Span,
    },
    RegionBlock {
        name: String,
        body: Vec<Stmt>,
        span: Span,
    },
    UnsafeBlock {
        body: Vec<Stmt>,
        span: Span,
    },
    Assign {
        name: String,
        value: Expr,
        span: Span,
    },
    ArrayAssign {
        array: String,
        index: Expr,
        value: Expr,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    Print {
        expr: Expr,
        span: Span,
    },
    Defer {
        stmt: Box<Stmt>,
        span: Span,
    },
    Break(Span),
    Continue(Span),
    Spawn {
        body: Vec<Stmt>,
        span: Span,
    },
    Parallel {
        blocks: Vec<Vec<Stmt>>,
        span: Span,
    },
    ChannelDecl {
        name: String,
        span: Span,
    },
    Send {
        channel: String,
        value: Expr,
        span: Span,
    },
    Receive {
        channel: String,
        target: String,
        span: Span,
    },
    /// Wraps any expression in statement position. The inner `Expr`
    /// already carries its own span; no separate span on this variant.
    Expression(Expr),
    FieldAssign {
        target: String,
        field: String,
        value: Expr,
        span: Span,
    },
}

impl Stmt {
    /// Source span of this statement node.
    pub fn span(&self) -> Span {
        match self {
            Stmt::VarDecl { span, .. } => *span,
            Stmt::Import { span, .. } => *span,
            Stmt::RegionBlock { span, .. } => *span,
            Stmt::UnsafeBlock { span, .. } => *span,
            Stmt::Assign { span, .. } => *span,
            Stmt::ArrayAssign { span, .. } => *span,
            Stmt::Return { span, .. } => *span,
            Stmt::Print { span, .. } => *span,
            Stmt::Defer { span, .. } => *span,
            Stmt::Break(s) => *s,
            Stmt::Continue(s) => *s,
            Stmt::Spawn { span, .. } => *span,
            Stmt::Parallel { span, .. } => *span,
            Stmt::ChannelDecl { span, .. } => *span,
            Stmt::Send { span, .. } => *span,
            Stmt::Receive { span, .. } => *span,
            Stmt::Expression(e) => e.span(),
            Stmt::FieldAssign { span, .. } => *span,
        }
    }
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
pub struct RecordDecl {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, TypeSyntax)>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct MatchCaseExpr {
    pub pattern: Pattern,
    pub body: Expr,
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
    SomeNested(Box<Pattern>),
    OkNested(Box<Pattern>),
    ErrorNested(Box<Pattern>),
    Guarded {
        pattern: Box<Pattern>,
        condition: Box<Expr>,
    },
    ListDestructure {
        first: Option<Box<Pattern>>,
        rest: Option<Box<Pattern>>,
    },
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Record {
        name: String,
        bindings: Vec<String>,
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
    pub params: Vec<(String, Option<TypeSyntax>)>,
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
    Named(String),
    Generic { name: String, args: Vec<TypeSyntax> },
    Unknown,
}

impl TypeSyntax {
    pub fn to_type(&self) -> crate::common::types::Type {
        use crate::common::types::Type;
        match self {
            TypeSyntax::Named(name) => match name.as_str() {
                "Int" | "int" => Type::Int,
                "Float" | "float" => Type::Float,
                "String" | "string" => Type::String,
                "Bool" | "bool" => Type::Bool,
                "Void" | "void" => Type::Void,
                "Self" => Type::TypeVar("Self".to_string()),
                _ if name.len() == 1
                    && name
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false) =>
                {
                    Type::TypeVar(name.clone())
                }
                _ => Type::Unknown,
            },
            TypeSyntax::Generic { name, args } => match name.to_lowercase().as_str() {
                "borrow" if args.len() == 1 => Type::borrow(args[0].to_type()),
                "mutborrow" | "mut_borrow" if args.len() == 1 => {
                    Type::mut_borrow(args[0].to_type())
                }
                "list" if args.len() == 1 => Type::list(args[0].to_type()),
                "option" if args.len() == 1 => Type::option(args[0].to_type()),
                "pointer" | "ptr" if args.len() == 1 => Type::pointer(args[0].to_type()),
                "channel" if args.len() == 1 => Type::channel(args[0].to_type()),
                "result" if args.len() == 2 => Type::result(args[0].to_type(), args[1].to_type()),
                _ => Type::Unknown,
            },
            TypeSyntax::Unknown => Type::Unknown,
        }
    }

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

    pub fn as_str(&self) -> &str {
        match self {
            TypeSyntax::Named(name) => name,
            TypeSyntax::Generic { name, .. } => name,
            TypeSyntax::Unknown => "",
        }
    }

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
    pub records: Vec<RecordDecl>,
}

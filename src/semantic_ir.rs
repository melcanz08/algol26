#![allow(dead_code)]

// ALGOL26 Canonical Semantic IR - CLEAN REWRITE
// No duplicates. All instructions in one enum.

use crate::semantic_type::SemanticType;

#[derive(Debug, Clone, PartialEq)]
pub enum TypedIRValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<TypedIRValue>),
    Some(Box<TypedIRValue>),
    None,
    Ok(Box<TypedIRValue>),
    Error(Box<TypedIRValue>),
    Variable(String, SemanticType),
    Cast {
        value: Box<TypedIRValue>,
        target_type: SemanticType,
    },
    BinaryOp {
        op: SemanticBinOp,
        left: Box<TypedIRValue>,
        right: Box<TypedIRValue>,
        result_type: SemanticType,
    },
    Call {
        function: String,
        args: Vec<TypedIRValue>,
        return_type: SemanticType,
    },
    ArrayAccess {
        array: Box<TypedIRValue>,
        index: Box<TypedIRValue>,
        element_type: SemanticType,
    },
}

impl TypedIRValue {
    pub fn as_constant_f64(&self) -> Option<f64> {
        match self {
            TypedIRValue::Float(f) => Some(*f),
            TypedIRValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
    pub fn type_of(&self) -> SemanticType {
        match self {
            TypedIRValue::Int(_) => SemanticType::Int,
            TypedIRValue::Float(_) => SemanticType::Float,
            TypedIRValue::String(_) => SemanticType::String,
            TypedIRValue::Bool(_) => SemanticType::Bool,
            TypedIRValue::List(v) => {
                if let Some(first) = v.first() {
                    SemanticType::List(Box::new(first.type_of()))
                } else {
                    SemanticType::List(Box::new(SemanticType::Unknown))
                }
            }
            TypedIRValue::Some(v) => SemanticType::Option(Box::new(v.type_of())),
            TypedIRValue::None => SemanticType::Option(Box::new(SemanticType::Unknown)),
            TypedIRValue::Ok(v) => SemanticType::Result {
                ok: Box::new(v.type_of()),
                error: Box::new(SemanticType::Unknown),
            },
            TypedIRValue::Error(v) => SemanticType::Result {
                ok: Box::new(SemanticType::Unknown),
                error: Box::new(v.type_of()),
            },
            TypedIRValue::Variable(_, t) => t.clone(),
            TypedIRValue::Cast { target_type, .. } => target_type.clone(),
            TypedIRValue::BinaryOp { result_type, .. } => result_type.clone(),
            TypedIRValue::Call { return_type, .. } => return_type.clone(),
            TypedIRValue::ArrayAccess { element_type, .. } => element_type.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticBinOp {
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

#[derive(Debug, Clone)]
pub enum SemanticPattern {
    Some { binding: String },
    None,
    Ok { binding: String },
    Error { binding: String },
    Literal(TypedIRValue),
    Wildcard,
}

#[derive(Debug, Clone)]
pub enum SemanticInstruction {
    Nop,
    Declare {
        name: String,
        mutable: bool,
        type_: SemanticType,
        value: TypedIRValue,
    },
    Assign {
        target: String,
        value: TypedIRValue,
    },
    ArrayAssign {
        array: Box<TypedIRValue>,
        index: Box<TypedIRValue>,
        value: TypedIRValue,
    },
    Print {
        value: TypedIRValue,
    },
    Return {
        value: Option<TypedIRValue>,
        type_: SemanticType,
    },
    Branch {
        condition: TypedIRValue,
        then_block: usize,
        else_block: usize,
    },
    Jump {
        block: usize,
    },
    Switch {
        value: TypedIRValue,
        cases: Vec<(SemanticPattern, usize)>,
        default_block: Option<usize>,
    },
    Call {
        result: Option<String>,
        function: String,
        args: Vec<TypedIRValue>,
        return_type: SemanticType,
    },
    IteratorInit {
        iterator: String,
        iterable: TypedIRValue,
    },
    IteratorNext {
        iterator: String,
        target: String,
        body_block: usize,
        exit_block: usize,
    },
    Spawn {
        entry_block: usize,
    },
    Fork {
        blocks: Vec<usize>,
        join_block: usize,
    },
    Defer {
        cleanup_block: usize,
    },
    ChannelDecl {
        name: String,
        type_: SemanticType,
    },
    Send {
        channel: String,
        value: TypedIRValue,
    },
    Receive {
        channel: String,
        target: String,
    },
}

#[derive(Debug, Clone)]
pub struct SemanticBlock {
    pub id: usize,
    pub instructions: Vec<SemanticInstruction>,
}

#[derive(Debug, Clone)]
pub struct SemanticFunction {
    pub name: String,
    pub params: Vec<(String, SemanticType)>,
    pub return_type: SemanticType,
    pub blocks: Vec<SemanticBlock>,
    pub entry_block: usize,
}

#[derive(Debug, Default)]
pub struct SemanticProgram {
    pub functions: Vec<SemanticFunction>,
    pub next_block_id: usize,
}

impl SemanticProgram {
    pub fn new() -> Self {
        SemanticProgram {
            functions: Vec::new(),
            next_block_id: 0,
        }
    }
    
    pub fn new_block_id(&mut self) -> usize {
        let id = self.next_block_id;
        self.next_block_id += 1;
        id
    }
}

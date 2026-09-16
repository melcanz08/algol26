// src/ir/semantic_ir.rs
//
// The IR data types: SemanticProgram, SemanticFunction, SemanticBlock,
// Instruction, Terminator, SemanticPattern, SemanticBinOp, and
// TypedIRValue.
//
// `TypedIRValue::type_of()` returns the type *claimed* by the value
// node itself. It does not prove that the claim is true — that is the
// job of verifier. Downstream consumers should treat the
// claimed type as authoritative only after verification passes.
use crate::common::types::Type;
use std::collections::HashMap;

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
}

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticPattern {
    Some { binding: String },
    None,
    Ok { binding: String },
    Error { binding: String },
    Wildcard,
    Literal(TypedIRValue),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedIRValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Void,
    PtrLiteral(usize),
    NullPtr,
    List(Vec<TypedIRValue>, Type),
    Some(Box<TypedIRValue>),
    None {
        option_type: Type,
    },
    Ok {
        value: Box<TypedIRValue>,
        result_type: Type,
    },
    Error {
        value: Box<TypedIRValue>,
        result_type: Type,
    },
    Variable(String, Type),
    Cast {
        value: Box<TypedIRValue>,
        target_type: Type,
    },
    BinaryOp {
        op: SemanticBinOp,
        left: Box<TypedIRValue>,
        right: Box<TypedIRValue>,
        result_type: Type,
    },
    Call {
        function: String,
        args: Vec<TypedIRValue>,
        return_type: Type,
    },
    ArrayAccess {
        array: Box<TypedIRValue>,
        index: Box<TypedIRValue>,
        element_type: Type,
    },
    Borrow {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    MutBorrow {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    Deref {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    AddrOf {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    // NEW: Array literal
    Array(Vec<TypedIRValue>, Type, usize), // elements, element_type, length

    // NEW: Range
    Range(Box<TypedIRValue>, Box<TypedIRValue>), // start, end

    // NEW: Field access
    FieldAccess {
        object: Box<TypedIRValue>,
        field: String,
        field_type: Type,
    },
}
impl TypedIRValue {
    pub fn type_of(&self) -> Type {
        match self {
            TypedIRValue::Int(_) => Type::Int,
            TypedIRValue::Float(_) => Type::Float,
            TypedIRValue::String(_) => Type::String,
            TypedIRValue::Bool(_) => Type::Bool,
            TypedIRValue::Void => Type::Void,
            TypedIRValue::PtrLiteral(_) => Type::Ptr,
            TypedIRValue::NullPtr => Type::Ptr,
            TypedIRValue::List(_, t) => Type::list(t.clone()),
            TypedIRValue::Some(v) => Type::option(v.type_of()),
            TypedIRValue::None { option_type } => option_type.clone(),
            TypedIRValue::Ok { result_type, .. } => result_type.clone(),
            TypedIRValue::Error { result_type, .. } => result_type.clone(),
            TypedIRValue::Variable(_, t) => t.clone(),
            TypedIRValue::Cast { target_type, .. } => target_type.clone(),
            TypedIRValue::BinaryOp { result_type, .. } => result_type.clone(),
            TypedIRValue::Call { return_type, .. } => return_type.clone(),
            TypedIRValue::ArrayAccess { element_type, .. } => element_type.clone(),
            TypedIRValue::Borrow { target_type, .. } => target_type.clone(),
            TypedIRValue::MutBorrow { target_type, .. } => target_type.clone(),
            TypedIRValue::Deref { target_type, .. } => target_type.clone(),
            TypedIRValue::AddrOf { target_type, .. } => target_type.clone(),
            _ => Type::Unknown,
        }
    }
    pub fn as_constant_f64(&self) -> Option<f64> {
        match self {
            TypedIRValue::Float(f) => Some(*f),
            TypedIRValue::Int(i) => Some(*i as f64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Instruction {
    Nop,
    Declare {
        name: String,
        mutable: bool,
        type_: Type,
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
    Call {
        func: String,
        args: Vec<TypedIRValue>,
        result: Option<String>,
    },
    IteratorInit {
        iterator: String,
        iterable: TypedIRValue,
    },
    ChannelDecl {
        name: String,
        type_: Type,
    },
    Send {
        channel: String,
        value: TypedIRValue,
    },
    Receive {
        channel: String,
        target: String,
    },
    // NEW: Channel operations
    ChannelSend {
        channel: String,
        value: TypedIRValue,
    },
    ChannelReceive {
        channel: String,
        target: String,
    },

    // NEW: Memory operations
    Allocate {
        target: String,
        size: TypedIRValue,
        type_: Type,
    },
    Free {
        ptr: TypedIRValue,
    },

    /// Enter a `region NAME` block. The interpreter pushes a new
    /// region frame; allocations between `RegionEnter` and the
    /// matching `RegionExit` are attributed to it and freed
    /// automatically on exit. LLVM treats both as no-ops — the
    /// capability check refuses any program that actually allocs,
    /// so a region without alloc has no runtime meaning.
    RegionEnter { name: String },
    /// Exit a `region NAME` block.
    RegionExit { name: String },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Return {
        value: Option<TypedIRValue>,
        type_: Type,
    },
    Jump {
        block: usize,
    },
    Branch {
        condition: TypedIRValue,
        then_block: usize,
        else_block: usize,
    },
    Switch {
        value: TypedIRValue,
        cases: Vec<(SemanticPattern, usize)>,
        default_block: Option<usize>,
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
}

pub type SemanticInstruction = Instruction;

#[derive(Debug, Clone)]
pub struct SemanticBlock {
    pub id: usize,
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}
impl SemanticBlock {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            instructions: vec![],
            terminator: None,
        }
    }
    pub fn successors(&self) -> Vec<usize> {
        if let Some(t) = &self.terminator {
            t.successors()
        } else {
            vec![]
        }
    }
    pub fn is_terminated(&self) -> bool {
        self.terminator.is_some()
    }
}
impl Terminator {
    pub fn successors(&self) -> Vec<usize> {
        match self {
            Self::Jump { block } => vec![*block],
            Self::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            Self::IteratorNext {
                body_block,
                exit_block,
                ..
            } => vec![*body_block, *exit_block],
            Self::Switch {
                cases,
                default_block,
                ..
            } => {
                let mut v: Vec<_> = cases.iter().map(|c| c.1).collect();
                if let Some(d) = default_block {
                    v.push(*d);
                }
                v
            }
            Self::Spawn { entry_block } => vec![*entry_block],
            Self::Fork { blocks, join_block } => {
                let mut v = blocks.clone();
                v.push(*join_block);
                v
            }
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct SemanticFunction {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub blocks: Vec<SemanticBlock>,
    pub entry_block: usize,
    pub is_extern: bool,
}

#[derive(Debug, Clone)]
pub struct SemanticProgram {
    pub functions: Vec<SemanticFunction>,
    pub block_counter: usize,
    /// Map from an extern function's ALGOL26 name to its C symbol
    /// name, when `extern ... as "sym"` was declared. Populated
    /// by the IR builder; consumed by LLVM codegen so the
    /// emitted call uses the C symbol rather than the ALGOL26
    /// name.
    pub ffi_symbols: HashMap<String, String>,
    /// Library names (without `lib` prefix or extension) that
    /// any extern declaration requested via `from "lib"`.
    /// Consumed by the linker driver as `-l<name>` flags.
    pub ffi_libraries: Vec<String>,
    /// Names of extern functions declared variadic
    /// (`extern "C" function f(a: T, ...)`). Consumed by LLVM
    /// codegen so the declared function type is variadic, and
    /// by the IR verifier to relax its arity check.
    pub variadic_functions: std::collections::HashSet<String>,
}
impl Default for SemanticProgram {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticProgram {
    pub fn new() -> Self {
        Self {
            functions: vec![],
            block_counter: 0,
            ffi_symbols: HashMap::new(),
            ffi_libraries: Vec::new(),
            variadic_functions: std::collections::HashSet::new(),
        }
    }
    pub fn new_block_id(&mut self) -> usize {
        let id = self.block_counter;
        self.block_counter += 1;
        id
    }
    pub fn verify(&self) -> Result<(), String> {
        crate::ir::verifier::verify(self)
    }
}

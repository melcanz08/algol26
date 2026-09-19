// src/ir/semantic_ir/values.rs
//
// IR value types: `TypedIRValue` (the value universe) and
// `SemanticBinOp` (the operator enum it uses).
//
// `TypedIRValue::type_of()` returns the type *claimed* by the value
// node itself. It does not prove the claim is true — that is the
// verifier's job. Downstream consumers should treat the claimed
// type as authoritative only after verification passes.

use crate::common::types::Type;

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
    // Array literal: elements, element_type, length
    Array(Vec<TypedIRValue>, Type, usize),

    // Range: start, end
    Range(Box<TypedIRValue>, Box<TypedIRValue>),

    // Field access: object, field, field_type
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

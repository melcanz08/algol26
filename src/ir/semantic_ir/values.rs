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
    /// Set operations (ADR 0032). All operands are `Set<T>` for some
    /// `T`; the result type is either `Set<T>` (the first three) or
    /// `Bool` (the last five).
    SetUnion,
    SetDifference,
    SetIntersection,
    SetMember,
    SetSubset,
    SetStrictSubset,
    SetSuperset,
    SetStrictSuperset,
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
    Record {
        name: String,
        fields: Vec<(String, TypedIRValue)>,
        record_type: Type,
    },
    Map {
        key_type: Type,
        value_type: Type,
        /// Entries in declaration order. The interpreter is free to
        /// store them however it likes; the IR is a compile-time
        /// snapshot and preserves the source order for display and
        /// equality.
        entries: Vec<(TypedIRValue, TypedIRValue)>,
        /// The full `Type::Map(K, V)`. Kept here so `type_of()`
        /// doesn't have to reassemble it from the key and value
        /// types, and so the verifier has a single source of truth.
        map_type: Type,
    },
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
    /// ADR 0038 D4b. A method call through a `&dyn Trait` receiver.
    /// Produced by `translate_expr` at the source call site, then
    /// consumed by the two instruction-emission sites
    /// (`Stmt::VarDecl` and `Stmt::Expression`) which convert it to
    /// `Instruction::VirtualCall`. `receiver` is the fat pointer;
    /// `args` are the user args, without the receiver.
    VirtualCall {
        receiver: Box<TypedIRValue>,
        method_name: String,
        slot: usize,
        args: Vec<TypedIRValue>,
        return_type: Type,
    },
    ArrayAccess {
        array: Box<TypedIRValue>,
        index: Box<TypedIRValue>,
        element_type: Type,
    },
    BorrowShared {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    BorrowMutable {
        expr: Box<TypedIRValue>,
        target_type: Type,
    },
    /// ADR 0038. A `&dyn Trait` / `&mut dyn Trait` fat pointer: the
    /// concrete value plus a vtable key. `data` is the concrete
    /// value (typically a variable referring to the underlying
    /// object); `vtable_id` identifies the `(trait, concrete)`
    /// pair the backend must use for dispatch. `target_type` is
    /// the full `Borrow(DynTrait)` or `MutBorrow(DynTrait)` the
    /// value wears, matching the sibling reference forms.
    ///
    /// The builder produces this at a coercion site — where the
    /// analyzer's type table says `&dyn Trait` but the source
    /// expression is `&concrete`. Ordinary `&T` / `&mut T` uses
    /// continue to use `BorrowShared` / `BorrowMutable`.
    DynTrait {
        data: Box<TypedIRValue>,
        vtable_id: String,
        target_type: Type,
    },
    ReadReference {
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
    /// A constant set value: every element known at IR-build time.
    /// `bits` is a `u64` with bit `i` set iff domain element `i` is
    /// a member. `element_type` is the set's element type (`Day`,
    /// `WorkDay`, `Bool`, etc.).
    ///
    /// Non-constant sets — where at least one element is not a
    /// literal — are built at runtime from a chain of `SetInsert`
    /// operations rooted at `Set { bits: 0, .. }`. Added in A5b.
    ///
    /// See ADR 0032.
    Set {
        bits: u64,
        element_type: Type,
    },
    /// A runtime singleton set: `{ element }`. Element is a value
    /// of `element_type`; the backend lowers to `1 << bit_index`
    /// where `bit_index` accounts for the element type's `low`
    /// offset (subrange). See ADR 0032.
    SetSingleton {
        element: Box<TypedIRValue>,
        element_type: Type,
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
            TypedIRValue::Array(_, elem_type, len) => Type::array(elem_type.clone(), *len),
            TypedIRValue::Range(start, end) => {
                Type::list(start.type_of().common_supertype(&end.type_of()))
            }
            TypedIRValue::Record { record_type, .. } => record_type.clone(),
            TypedIRValue::Map { map_type, .. } => map_type.clone(),
            TypedIRValue::Some(v) => Type::option(v.type_of()),
            TypedIRValue::None { option_type } => option_type.clone(),
            TypedIRValue::Ok { result_type, .. } => result_type.clone(),
            TypedIRValue::Error { result_type, .. } => result_type.clone(),
            TypedIRValue::Variable(_, t) => t.clone(),
            TypedIRValue::Cast { target_type, .. } => target_type.clone(),
            TypedIRValue::BinaryOp { result_type, .. } => result_type.clone(),
            TypedIRValue::Call { return_type, .. } => return_type.clone(),
            TypedIRValue::VirtualCall { return_type, .. } => return_type.clone(),
            TypedIRValue::ArrayAccess { element_type, .. } => element_type.clone(),
            TypedIRValue::BorrowShared { target_type, .. } => target_type.clone(),
            TypedIRValue::BorrowMutable { target_type, .. } => target_type.clone(),
            TypedIRValue::DynTrait { target_type, .. } => target_type.clone(),
            TypedIRValue::ReadReference { target_type, .. } => target_type.clone(),
            TypedIRValue::AddrOf { target_type, .. } => target_type.clone(),
            TypedIRValue::FieldAccess { field_type, .. } => field_type.clone(),
            TypedIRValue::Set { element_type, .. } => Type::set(element_type.clone()),
            TypedIRValue::SetSingleton { element_type, .. } => Type::set(element_type.clone()),
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

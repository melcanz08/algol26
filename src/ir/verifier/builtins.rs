// src/ir/verifier/builtins.rs

use super::{FunctionSignature};
use crate::common::types::Type;
use std::collections::HashMap;

/// True if the type mentions a type variable anywhere in its
/// structure. Used to skip return-type comparison when the
/// signature is generic — the analyzer has already performed the
/// substitution, and the verifier has no scope to re-derive it.
pub(super) fn contains_type_var(t: &Type) -> bool {
    match t {
        Type::TypeVar(_) => true,
        Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Borrow(inner)
        | Type::MutBorrow(inner)
        | Type::Channel(inner)
        | Type::Array(inner, _) => contains_type_var(inner),
        Type::Result { ok, error } => contains_type_var(ok) || contains_type_var(error),
        Type::Tuple(elems) => elems.iter().any(contains_type_var),
        _ => false,
    }
}

/// Signatures for built-in functions that the IR builder registers
/// but that do not appear as `SemanticFunction` entries in the program.
/// Keep this in sync with `SemanticIRBuilder::build_impl`.
pub(super) fn builtin_signatures() -> HashMap<String, FunctionSignature> {
    let mut m = HashMap::new();

    let mut reg = |name: &str, params: Vec<(&str, Type)>, ret: Type| {
        m.insert(
            name.to_string(),
            FunctionSignature {
                params: params.into_iter().map(|(n, t)| (n.to_string(), t)).collect(),
                return_type: ret,
            },
        );
    };

    // Math
    for name in &["Math.sqrt", "Math.sin", "Math.cos", "Math.tan",
                  "Math.abs", "Math.floor", "Math.ceil", "Math.exp", "Math.log"] {
        reg(name, vec![("x", Type::Float)], Type::Float);
    }
    reg("Math.pow", vec![("x", Type::Float), ("y", Type::Float)], Type::Float);

    // String
    reg("String.length",    vec![("s", Type::String)], Type::Int);
    reg("String.concat",    vec![("s1", Type::String), ("s2", Type::String)], Type::String);
    reg("String.substring", vec![("s", Type::String), ("start", Type::Int), ("length", Type::Int)], Type::String);
    reg("String.to_upper",  vec![("s", Type::String)], Type::String);
    reg("String.to_lower",  vec![("s", Type::String)], Type::String);

    // File
    reg("File.read",   vec![("path", Type::String)], Type::String);
    reg("File.write",  vec![("path", Type::String), ("content", Type::String)], Type::Int);
    reg("File.append", vec![("path", Type::String), ("content", Type::String)], Type::Int);

    // List
    reg("List.length", vec![("arr", Type::list(Type::Unknown))], Type::Int);
    reg("List.sum",    vec![("arr", Type::list(Type::Unknown))], Type::Float);
    reg("List.max",    vec![("arr", Type::list(Type::Unknown))], Type::Float);
    reg("List.min",    vec![("arr", Type::list(Type::Unknown))], Type::Float);

    // Raw memory
    reg("alloc", vec![("size", Type::Int)], Type::pointer(Type::Unknown));
    reg("free",  vec![("ptr", Type::pointer(Type::Unknown))], Type::Void);

    m
}
// src/ir/verifier/builtins.rs

use super::FunctionSignature;
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

/// True if `name` is one of the compiler's built-in functions.
/// Used by the capability scan to distinguish built-ins from user
/// functions that happen to share a namespace prefix (e.g. a
/// user-defined `String.helper`).
pub fn is_builtin_name(name: &str) -> bool {
    builtin_signatures().contains_key(name)
}

/// Signatures for built-in functions that the IR builder registers
/// but that do not appear as `SemanticFunction` entries in the program.
/// Keep this in sync with `SemanticIRBuilder::build_impl`.
pub(super) fn builtin_signatures() -> HashMap<String, FunctionSignature> {
    // The signatures live in `crate::builtins`. This function converts
    // them into the verifier's own `FunctionSignature` shape. Before
    // the unification, this function had its own copy of the table
    // and could drift from the analyzer's and builder's copies — see
    // `src/builtins.rs` for the full story.
    crate::builtins::builtins()
        .into_iter()
        .map(|sig| {
            (
                sig.name.to_string(),
                FunctionSignature {
                    params: sig
                        .params
                        .into_iter()
                        .map(|(n, t)| (n.to_string(), t))
                        .collect(),
                    return_type: sig.return_type,
                    variadic: false,
                },
            )
        })
        .collect()
}

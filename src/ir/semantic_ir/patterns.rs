// src/ir/semantic_ir/patterns.rs
//
// `SemanticPattern` — the pattern language used by
// `Terminator::Switch` for `match` lowering.

use super::values::TypedIRValue;

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticPattern {
    Some { binding: String },
    None,
    Ok { binding: String },
    Error { binding: String },
    Wildcard,
    Literal(TypedIRValue),
    Record { name: String, bindings: Vec<String> },
}

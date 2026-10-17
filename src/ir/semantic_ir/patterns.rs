// src/ir/semantic_ir/patterns.rs
//
// `SemanticPattern` — the pattern language used by
// `Terminator::Switch` for `match` lowering.

use super::values::TypedIRValue;

#[derive(Debug, Clone, PartialEq)]
pub enum SemanticPattern {
    Some {
        binding: String,
    },
    None,
    Ok {
        binding: String,
    },
    Error {
        binding: String,
    },
    Wildcard,
    Literal(TypedIRValue),
    /// ADR 0055 Session 3. A union variant pattern. `tag` is the
    /// variant's index in `Type::Union::variants`; `bindings` are
    /// the names the payload binds to, in order.
    VariantWithPayload {
        tag: u32,
        variant_name: String,
        bindings: Vec<String>,
    },
    Record {
        name: String,
        bindings: Vec<String>,
    },
    /// Enum variant match. `ordinal` is the variant's index within
    /// the enum, resolved at IR-build time from the matched
    /// expression's `Type::Enum`. The interpreter and codegen
    /// compare `value == ordinal`; the name is carried for display
    /// and diagnostics only. See ADR 0030.
    Variant {
        name: String,
        ordinal: i64,
    },
}

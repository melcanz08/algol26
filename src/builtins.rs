//! Canonical built-in function signatures.
//!
//! Three passes used to maintain this table independently:
//!
//!   - `SemanticAnalyzer::register_builtin_functions`
//!     (`src/semantics/analyzer/items.rs`)
//!   - `SemanticIRBuilder::build_impl`'s builtin block
//!     (`src/semantics/builder/build.rs`)
//!   - `verifier::builtins::builtin_signatures`
//!     (`src/ir/verifier/builtins.rs`)
//!
//! They drifted. `List.length` / `List.sum` / `List.max` / `List.min`
//! were `List<Unknown>` in the analyzer and verifier, and `List<Float>`
//! in the builder. The sync test (`builtin_signatures_match_analyzer_table`)
//! only compared two of the three sites, so the drift went unnoticed.
//!
//! This module is the fix. Every consumer calls `builtins()` and
//! converts each `BuiltinSig` into its own function-signature type.

use crate::common::types::Type;

/// One built-in function's signature.
///
/// The `name` is the qualified call name (`Math.sqrt`, `List.length`,
/// `alloc`). `params` is a list of `(parameter_name, parameter_type)`.
/// `Unknown` inside a composite type acts as a wildcard — e.g.
/// `List<Unknown>` matches any `List<T>` argument.
pub struct BuiltinSig {
    pub name: &'static str,
    pub params: Vec<(&'static str, Type)>,
    pub return_type: Type,
}

/// All built-in functions known to the compiler.
///
/// Call sites in this function are the single source of truth. Every
/// pass that needs built-in signatures reads this list; no pass
/// maintains its own copy.
pub fn builtins() -> Vec<BuiltinSig> {
    let mut v = Vec::new();

    // ─── Math ────────────────────────────────────────────────────
    for name in &[
        "Math.sqrt",
        "Math.sin",
        "Math.cos",
        "Math.tan",
        "Math.abs",
        "Math.floor",
        "Math.ceil",
        "Math.exp",
        "Math.log",
    ] {
        v.push(BuiltinSig {
            name,
            params: vec![("x", Type::Float)],
            return_type: Type::Float,
        });
    }
    v.push(BuiltinSig {
        name: "Math.pow",
        params: vec![("x", Type::Float), ("y", Type::Float)],
        return_type: Type::Float,
    });

    // ─── String ──────────────────────────────────────────────────
    v.push(BuiltinSig {
        name: "String.length",
        params: vec![("s", Type::String)],
        return_type: Type::Int,
    });
    v.push(BuiltinSig {
        name: "String.concat",
        params: vec![("s1", Type::String), ("s2", Type::String)],
        return_type: Type::String,
    });
    v.push(BuiltinSig {
        name: "String.substring",
        params: vec![
            ("s", Type::String),
            ("start", Type::Int),
            ("length", Type::Int),
        ],
        return_type: Type::String,
    });
    v.push(BuiltinSig {
        name: "String.to_upper",
        params: vec![("s", Type::String)],
        return_type: Type::String,
    });
    v.push(BuiltinSig {
        name: "String.to_lower",
        params: vec![("s", Type::String)],
        return_type: Type::String,
    });
    v.push(BuiltinSig {
        name: "String.trim",
        params: vec![("s", Type::String)],
        return_type: Type::String,
    });
    v.push(BuiltinSig {
        name: "String.split",
        params: vec![("s", Type::String), ("sep", Type::String)],
        return_type: Type::list(Type::String),
    });
    v.push(BuiltinSig {
        name: "String.join",
        params: vec![("xs", Type::list(Type::String)), ("sep", Type::String)],
        return_type: Type::String,
    });

    // Byte-wise lexicographic comparison. Returns -1, 0, or 1.
    // Not Unicode-aware — "é" sorts after "z" because 'é' begins
    // with byte 0xC3 > 0x7A. Documented in the language reference
    // §15.2.
    v.push(BuiltinSig {
        name: "String.compare",
        params: vec![("a", Type::String), ("b", Type::String)],
        return_type: Type::Int,
    });

    // ─── File ────────────────────────────────────────────────────
    v.push(BuiltinSig {
        name: "File.read",
        params: vec![("path", Type::String)],
        return_type: Type::String,
    });
    v.push(BuiltinSig {
        name: "File.write",
        params: vec![("path", Type::String), ("content", Type::String)],
        return_type: Type::Int,
    });
    v.push(BuiltinSig {
        name: "File.append",
        params: vec![("path", Type::String), ("content", Type::String)],
        return_type: Type::Int,
    });

    // ─── List ────────────────────────────────────────────────────
    // NOTE: `List<Unknown>` — length / sum / max / min work on any
    // element type. The builder used to say `List<Float>` here, which
    // rejected `List.length(xs)` for non-Float `xs`.
    v.push(BuiltinSig {
        name: "List.length",
        params: vec![("arr", Type::list(Type::Unknown))],
        return_type: Type::Int,
    });
    v.push(BuiltinSig {
        name: "List.sum",
        params: vec![("arr", Type::list(Type::Unknown))],
        return_type: Type::Float,
    });
    v.push(BuiltinSig {
        name: "List.max",
        params: vec![("arr", Type::list(Type::Unknown))],
        return_type: Type::Float,
    });
    v.push(BuiltinSig {
        name: "List.min",
        params: vec![("arr", Type::list(Type::Unknown))],
        return_type: Type::Float,
    });

    // ─── Raw memory ──────────────────────────────────────────────
    v.push(BuiltinSig {
        name: "alloc",
        params: vec![("size", Type::Int)],
        return_type: Type::pointer(Type::Unknown),
    });
    v.push(BuiltinSig {
        name: "free",
        params: vec![("ptr", Type::pointer(Type::Unknown))],
        return_type: Type::Void,
    });

    // ─── Assertion (ADR 0022) ────────────────────────────────────
    v.push(BuiltinSig {
        name: "affirm",
        params: vec![("cond", Type::Bool), ("msg", Type::String)],
        return_type: Type::Void,
    });

    // ─── Command-line arguments (ADR 0023) ───────────────────────
    v.push(BuiltinSig {
        name: "args",
        params: vec![],
        return_type: Type::list(Type::String),
    });

    // ─── Conversions (Track B) ───────────────────────────────────
    v.push(BuiltinSig {
        name: "Int.to_string",
        params: vec![("n", Type::Int)],
        return_type: Type::String,
    });
    v.push(BuiltinSig {
        name: "String.to_int",
        params: vec![("s", Type::String)],
        return_type: Type::option(Type::Int),
    });

    // Float-to-string conversion. Format is `%.6f` with trailing
    // zeros stripped from the fractional part, so 3.14 -> "3.14",
    // 1.0 -> "1", 1.0/3.0 -> "0.333333". All three backends produce
    // identical output for the same input.
    v.push(BuiltinSig {
        name: "Float.to_string",
        params: vec![("x", Type::Float)],
        return_type: Type::String,
    });

    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_duplicate_builtin_names() {
        let mut seen = HashSet::new();
        for sig in builtins() {
            assert!(
                seen.insert(sig.name),
                "duplicate built-in name in table: {}",
                sig.name
            );
        }
    }

    #[test]
    fn list_length_uses_unknown_element_type() {
        // Regression pin: the builder used `List<Float>` here before
        // the unification. Any change back to a concrete element type
        // would re-introduce the bug this table fixes.
        let list_length = builtins()
            .into_iter()
            .find(|s| s.name == "List.length")
            .expect("List.length missing from table");
        assert_eq!(list_length.params.len(), 1);
        let (_, param_ty) = &list_length.params[0];
        assert!(
            matches!(param_ty, Type::List(inner) if matches!(inner.as_ref(), Type::Unknown)),
            "List.length must take List<Unknown>, got {:?}",
            param_ty
        );
    }
}

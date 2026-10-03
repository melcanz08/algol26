#!/usr/bin/env python3
"""
A1a: add NominalTypeId and Type::Distinct to src/common/types.rs.

Exhaustive-match arms that need explicit handling:
  - Display
  - is_copy
  - contains_type_var
  - contains_unknown
  - substitute

All other match sites have a `_ =>` fallback that returns the correct
answer for a nominal type (no coercion, no cast, no numeric).

No constructors are used yet — the parser does not produce Distinct
until A2. This commit is pure type-system surface.
"""

from pathlib import Path

PATH = Path("src/common/types.rs")

REPLACEMENTS = [
    # 1. NominalTypeId struct + Type::Distinct variant
    (
        """#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Void,""",
        """/// Nominal type identity. Two declarations with the same `name`
/// in different modules produce different `NominalTypeId` values, and
/// therefore different types. Identity is `id`; `name` is
/// presentation only.
///
/// Assigned by the analyzer during type declaration registration.
/// Unique within a compilation unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NominalTypeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Void,""",
    ),

    # 2. Add the variant at the end (before closing brace)
    (
        """    /// Key-value container. Keys are restricted to `Int`, `String`,
    /// or `Bool` by the analyzer (ADR 0027); the type itself does not
    /// enforce that restriction.
    Map(Box<Type>, Box<Type>),
}""",
        """    /// Key-value container. Keys are restricted to `Int`, `String`,
    /// or `Bool` by the analyzer (ADR 0027); the type itself does not
    /// enforce that restriction.
    Map(Box<Type>, Box<Type>),
    /// A nominal type. Identity is `id`; `name` is presentation;
    /// `base` is the underlying representation used at lowering.
    /// See ADR 0029.
    ///
    /// The analyzer guarantees `base` is one of `Int`, `Float`,
    /// `Bool`, `String` in v1 (`Ptr` deferred).
    Distinct {
        id: NominalTypeId,
        name: String,
        base: Box<Type>,
    },
}""",
    ),

    # 3. Constructor helper
    (
        """    pub fn record(name: &str, args: Vec<Type>) -> Self {
        Type::Record(name.to_string(), args)
    }""",
        """    pub fn record(name: &str, args: Vec<Type>) -> Self {
        Type::Record(name.to_string(), args)
    }

    /// Construct a nominal type. `id` is the analyzer-assigned
    /// identity; `name` is for display only.
    pub fn distinct(id: NominalTypeId, name: &str, base: Type) -> Self {
        Type::Distinct {
            id,
            name: name.to_string(),
            base: Box::new(base),
        }
    }""",
    ),

    # 4. is_copy — recurse into base
    (
        """    pub fn is_copy(&self) -> bool {
        matches!(self, Type::Int | Type::Float | Type::Bool | Type::Ptr)
    }""",
        """    pub fn is_copy(&self) -> bool {
        match self {
            Type::Int | Type::Float | Type::Bool | Type::Ptr => true,
            // Nominality does not alter ownership semantics: a
            // `distinct Int` is Copy, a `distinct String` is not.
            // See ADR 0029.
            Type::Distinct { base, .. } => base.is_copy(),
            _ => false,
        }
    }""",
    ),

    # 5. contains_type_var — recurse into base
    (
        """            Type::Map(k, v) => k.contains_type_var() || v.contains_type_var(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_type_var()) || return_type.contains_type_var(),
            _ => false,
        }
    }""",
        """            Type::Map(k, v) => k.contains_type_var() || v.contains_type_var(),
            Type::Distinct { base, .. } => base.contains_type_var(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_type_var()) || return_type.contains_type_var(),
            _ => false,
        }
    }""",
    ),

    # 6. contains_unknown — recurse into base
    (
        """            Type::Map(k, v) => k.contains_unknown() || v.contains_unknown(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_unknown()) || return_type.contains_unknown(),
            _ => false,
        }
    }""",
        """            Type::Map(k, v) => k.contains_unknown() || v.contains_unknown(),
            Type::Distinct { base, .. } => base.contains_unknown(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_unknown()) || return_type.contains_unknown(),
            _ => false,
        }
    }""",
    ),

    # 7. substitute — recurse into base, preserving id and name
    (
        """            Type::Map(k, v) => Type::map(k.substitute(substitutions), v.substitute(substitutions)),
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|p| p.substitute(substitutions)).collect(),
                return_type: Box::new(return_type.substitute(substitutions)),
            },
            _ => self.clone(),
        }
    }""",
        """            Type::Map(k, v) => Type::map(k.substitute(substitutions), v.substitute(substitutions)),
            // Substitution flows through the base, not the identity.
            Type::Distinct { id, name, base } => Type::Distinct {
                id: *id,
                name: name.clone(),
                base: Box::new(base.substitute(substitutions)),
            },
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|p| p.substitute(substitutions)).collect(),
                return_type: Box::new(return_type.substitute(substitutions)),
            },
            _ => self.clone(),
        }
    }""",
    ),

    # 8. Display — print the name only
    (
        """            Type::Map(k, v) => format!("Map<{}, {}>", k, v),
            Type::Pointer(t) => format!("*{}", t),""",
        """            Type::Map(k, v) => format!("Map<{}, {}>", k, v),
            // Nominal types print as their declared name. `id` is
            // identity, not presentation. See ADR 0029.
            Type::Distinct { name, .. } => name.clone(),
            Type::Pointer(t) => format!("*{}", t),""",
    ),

    # 9. Tests
    (
        """    #[test]
    fn test_substitute() {
        let mut substitutions = std::collections::HashMap::new();
        substitutions.insert("T".to_string(), Type::Int);

        let ty = Type::TypeVar("T".to_string());
        assert_eq!(ty.substitute(&substitutions), Type::Int);

        let ty = Type::list(Type::TypeVar("T".to_string()));
        assert_eq!(ty.substitute(&substitutions), Type::list(Type::Int));
    }
}""",
        """    #[test]
    fn test_substitute() {
        let mut substitutions = std::collections::HashMap::new();
        substitutions.insert("T".to_string(), Type::Int);

        let ty = Type::TypeVar("T".to_string());
        assert_eq!(ty.substitute(&substitutions), Type::Int);

        let ty = Type::list(Type::TypeVar("T".to_string()));
        assert_eq!(ty.substitute(&substitutions), Type::list(Type::Int));
    }

    // ─── Nominal types (ADR 0029) ─────────────────────────────────

    #[test]
    fn distinct_display_uses_name_only() {
        let t = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(t.to_string(), "UserId");
    }

    #[test]
    fn distinct_identity_is_id_not_name() {
        // Same name, different ids — different types.
        let a = Type::distinct(NominalTypeId(1), "Id", Type::Int);
        let b = Type::distinct(NominalTypeId(2), "Id", Type::Int);
        assert_ne!(a, b);
        assert!(!a.can_coerce_to(&b));
        assert!(!b.can_coerce_to(&a));
    }

    #[test]
    fn distinct_same_id_coerces() {
        let a = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        let b = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(a, b);
        assert!(a.can_coerce_to(&b));
    }

    #[test]
    fn distinct_does_not_coerce_to_base() {
        let id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(!id.can_coerce_to(&Type::Int));
        assert!(!Type::Int.can_coerce_to(&id));
    }

    #[test]
    fn distinct_does_not_coerce_to_sibling() {
        let user_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        let price_cents = Type::distinct(NominalTypeId(2), "PriceCents", Type::Int);
        assert!(!user_id.can_coerce_to(&price_cents));
        assert!(!price_cents.can_coerce_to(&user_id));
    }

    #[test]
    fn distinct_is_never_numeric() {
        let id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(!id.is_numeric());
        let meters = Type::distinct(NominalTypeId(2), "Meters", Type::Float);
        assert!(!meters.is_numeric());
    }

    #[test]
    fn distinct_is_copy_iff_base_is_copy() {
        let int_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(int_id.is_copy());
        let string_name = Type::distinct(NominalTypeId(2), "Name", Type::String);
        assert!(!string_name.is_copy());
    }

    #[test]
    fn distinct_no_common_supertype_with_base_or_sibling() {
        let user_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(user_id.common_supertype(&Type::Int), Type::Unknown);
        let price_cents = Type::distinct(NominalTypeId(2), "PriceCents", Type::Int);
        assert_eq!(user_id.common_supertype(&price_cents), Type::Unknown);
        // Same identity: returns itself.
        let same = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(user_id.common_supertype(&same), user_id);
    }

    #[test]
    fn distinct_contains_unresolved_recurses_into_base() {
        let with_unknown = Type::distinct(NominalTypeId(1), "X", Type::Unknown);
        assert!(with_unknown.contains_unknown());
        assert!(with_unknown.contains_unresolved());
        let with_var = Type::distinct(NominalTypeId(2), "Y", Type::TypeVar("T".to_string()));
        assert!(with_var.contains_type_var());
        let concrete = Type::distinct(NominalTypeId(3), "Z", Type::Int);
        assert!(!concrete.contains_unresolved());
    }

    #[test]
    fn distinct_no_cast() {
        let user_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(!user_id.can_cast_to(&Type::Int));
        assert!(!Type::Int.can_cast_to(&user_id));
    }
}""",
    ),
]


def main():
    if not PATH.exists():
        print(f"ERROR: {PATH} not found. Run from the repo root.")
        raise SystemExit(1)

    src = PATH.read_text()
    for i, (old, new) in enumerate(REPLACEMENTS, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: site {i} matched {count} times; expected 1.")
            print("─" * 60)
            print(old[:250])
            print("─" * 60)
            print("Nothing written.")
            raise SystemExit(1)
        src = src.replace(old, new, 1)

    PATH.write_text(src)
    print(f"OK: patched {PATH}")
    print()
    print("NEXT: cargo build (expect errors in other files that match on Type)")
    print("      cargo test --lib common::types  (should be green)")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""
A1: add SubrangeTypeId and Type::Subrange to src/common/types.rs.

Same shape as A1 for nominal and enum types.

Exhaustive-match arms:
  - Display        -> print name
  - is_copy        -> recurse into base
  - can_cast_to    -> Subrange <-> base as explicit cast
  - contains_type_var / contains_unknown / substitute -> recurse base

Arms that fall through correctly:
  - can_coerce_to   (`_ => false` — same id required, checked by early `self == target`)
  - common_supertype (early `self == other` handles same-id case)
  - is_numeric      (`matches!` returns false)
  - inner_type      (`_ => None`)
"""

from pathlib import Path

PATH = Path("src/common/types.rs")


EDITS = [
    # 1. SubrangeTypeId struct
    (
        """#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumTypeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {""",
        """#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumTypeId(pub u32);

/// Subrange type identity. `type Percentage Int in 0..100` and
/// `type WorkDay Day in Monday..Friday`. Identity is `id`; `name`,
/// `base`, and the bounds are presentation / analysis. See ADR 0031.
///
/// `low` and `high` are inclusive. When `base` is `Type::Enum`,
/// they are ordinals; the variant names are recovered from `base`
/// at diagnostic time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubrangeTypeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {""",
    ),

    # 2. Type::Subrange variant
    (
        """    /// An ordinal enumeration. Identity is `id`; `name` is
    /// presentation; `variants[i]` has ordinal `i`. The runtime
    /// representation is `Int`. See ADR 0030.
    Enum {
        id: EnumTypeId,
        name: String,
        variants: Vec<String>,
    },
}""",
        """    /// An ordinal enumeration. Identity is `id`; `name` is
    /// presentation; `variants[i]` has ordinal `i`. The runtime
    /// representation is `Int`. See ADR 0030.
    Enum {
        id: EnumTypeId,
        name: String,
        variants: Vec<String>,
    },
    /// A subrange of an ordinal type. `low..high` is inclusive.
    /// When `base` is `Type::Enum`, `low` and `high` are ordinals.
    /// Identity is `id`; every other field is presentation or
    /// analysis. See ADR 0031.
    Subrange {
        id: SubrangeTypeId,
        name: String,
        base: Box<Type>,
        low: i64,
        high: i64,
    },
}""",
    ),

    # 3. Constructor helper
    (
        """    /// Construct an ordinal enum type. `id` is the analyzer-assigned
    /// identity; `variants` is in declaration order (ordinals 0..N).
    pub fn enum_type(id: EnumTypeId, name: &str, variants: Vec<String>) -> Self {
        Type::Enum {
            id,
            name: name.to_string(),
            variants,
        }
    }""",
        """    /// Construct an ordinal enum type. `id` is the analyzer-assigned
    /// identity; `variants` is in declaration order (ordinals 0..N).
    pub fn enum_type(id: EnumTypeId, name: &str, variants: Vec<String>) -> Self {
        Type::Enum {
            id,
            name: name.to_string(),
            variants,
        }
    }

    /// Construct a subrange type. `low..high` inclusive. When
    /// `base` is `Type::Enum`, `low` and `high` are ordinals.
    pub fn subrange(
        id: SubrangeTypeId,
        name: &str,
        base: Type,
        low: i64,
        high: i64,
    ) -> Self {
        Type::Subrange {
            id,
            name: name.to_string(),
            base: Box::new(base),
            low,
            high,
        }
    }""",
    ),

    # 4. is_copy — recurse into base
    (
        """            // Enums are scalars. The variant is an integer ordinal;
            // copying it duplicates no state. See ADR 0030.
            Type::Enum { .. } => true,
            _ => false,
        }
    }""",
        """            // Enums are scalars. The variant is an integer ordinal;
            // copying it duplicates no state. See ADR 0030.
            Type::Enum { .. } => true,
            // A subrange copies iff its base copies. Bases are
            // restricted to Int and enums, both of which are Copy,
            // so this is always true in practice — but recursing
            // keeps the rule composable if a future ADR extends the
            // set of base types.
            Type::Subrange { base, .. } => base.is_copy(),
            _ => false,
        }
    }""",
    ),

    # 5. can_cast_to — Subrange <-> base
    (
        """            // Enum ordinal conversion (ADR 0030). `Enum <-> Int` is
            // an explicit cast — the representation is identical.
            // This is what the intrinsic wrap/unwrap for
            // `from_ordinal` / `to_ordinal` lowers to.
            //
            // Enum-to-enum is not allowed even when the ordinal
            // ranges overlap; go through `Int` explicitly.
            (Type::Enum { .. }, Type::Int) => true,
            (Type::Int, Type::Enum { .. }) => true,

            // Default: no cast
            _ => false,""",
        """            // Enum ordinal conversion (ADR 0030). `Enum <-> Int` is
            // an explicit cast — the representation is identical.
            // This is what the intrinsic wrap/unwrap for
            // `from_ordinal` / `to_ordinal` lowers to.
            //
            // Enum-to-enum is not allowed even when the ordinal
            // ranges overlap; go through `Int` explicitly.
            (Type::Enum { .. }, Type::Int) => true,
            (Type::Int, Type::Enum { .. }) => true,

            // Subrange conversions (ADR 0031). A subrange and its
            // base share a runtime representation, so an explicit
            // cast between them is a no-op. This is what the
            // constructor `T(v)` and the extractor `.to_base()`
            // lower to.
            //
            // Subrange-to-subrange is not allowed even when the
            // intervals overlap; go through the base explicitly.
            (Type::Subrange { base, .. }, target)
                if !matches!(target, Type::Subrange { .. }) =>
            {
                base.can_cast_to(target)
            }
            (source, Type::Subrange { base, .. })
                if !matches!(source, Type::Subrange { .. }) =>
            {
                source.can_cast_to(base)
            }

            // Default: no cast
            _ => false,""",
    ),

    # 6. contains_type_var
    (
        """            Type::Distinct { base, .. } => base.contains_type_var(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_type_var()) || return_type.contains_type_var(),""",
        """            Type::Distinct { base, .. } => base.contains_type_var(),
            Type::Enum { .. } => false,
            Type::Subrange { base, .. } => base.contains_type_var(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_type_var()) || return_type.contains_type_var(),""",
    ),

    # 7. contains_unknown
    (
        """            Type::Distinct { base, .. } => base.contains_unknown(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_unknown()) || return_type.contains_unknown(),""",
        """            Type::Distinct { base, .. } => base.contains_unknown(),
            Type::Enum { .. } => false,
            Type::Subrange { base, .. } => base.contains_unknown(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_unknown()) || return_type.contains_unknown(),""",
    ),

    # 8. substitute
    (
        """            // Substitution flows through the base, not the identity.
            Type::Distinct { id, name, base } => Type::Distinct {
                id: *id,
                name: name.clone(),
                base: Box::new(base.substitute(substitutions)),
            },""",
        """            // Substitution flows through the base, not the identity.
            Type::Distinct { id, name, base } => Type::Distinct {
                id: *id,
                name: name.clone(),
                base: Box::new(base.substitute(substitutions)),
            },
            // Enums have no type arguments; identity and variants
            // are fixed at registration.
            Type::Enum { .. } => self.clone(),
            // Same for subranges: low/high and identity are fixed.
            // Only the base could carry a type variable (in theory);
            // recursing keeps the rule uniform.
            Type::Subrange {
                id,
                name,
                base,
                low,
                high,
            } => Type::Subrange {
                id: *id,
                name: name.clone(),
                base: Box::new(base.substitute(substitutions)),
                low: *low,
                high: *high,
            },""",
    ),

    # 9. Display
    (
        """            // Enums print as their declared name. Variant values
            // print as ordinals; the name is not available at
            // runtime in v1. See ADR 0030.
            Type::Enum { name, .. } => name.clone(),""",
        """            // Enums print as their declared name. Variant values
            // print as ordinals; the name is not available at
            // runtime in v1. See ADR 0030.
            Type::Enum { name, .. } => name.clone(),
            // Subranges print as their declared name. See ADR 0031.
            Type::Subrange { name, .. } => name.clone(),""",
    ),
]


def main():
    if not PATH.exists():
        print(f"ERROR: {PATH} not found.")
        raise SystemExit(1)

    src = PATH.read_text()
    for i, (old, new) in enumerate(EDITS, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)

    PATH.write_text(src)
    print(f"OK: patched {PATH}")
    print()
    print("NEXT: cargo build 2>&1 | head -40")


if __name__ == "__main__":
    main()

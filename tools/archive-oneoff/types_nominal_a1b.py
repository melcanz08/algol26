#!/usr/bin/env python3
"""
A1b: add Distinct arms to the two exhaustive matches the compiler
found after A1a.

  1. llvm_codegen::map_type          — unwrap to base.
  2. instantiation_plan::mangled_type_name
                                     — mangle by id, not name.

Plus one test in instantiation_plan.rs verifying that two nominal
types sharing a name but with different ids mangle differently.
"""

from pathlib import Path

LLVM = Path("src/backends/llvm_codegen/types.rs")
PLAN = Path("src/ir/instantiation_plan.rs")

EDIT_LLVM = (
    """            Type::Never => self.context.ptr_type(AddressSpace::default()).into(),

            Type::List(inner) => {""",
    """            Type::Never => self.context.ptr_type(AddressSpace::default()).into(),

            // A nominal type has no runtime representation of its
            // own — it lowers to its base type. The nominal identity
            // lives only in the type system. See ADR 0029.
            Type::Distinct { base, .. } => self.map_type(base),

            Type::List(inner) => {""",
)

EDIT_PLAN_ARM = (
    """        Type::Record(name, args) => {
            let parts: Vec<String> = args.iter().map(mangled_type_name).collect();
            format!("Record_{}_{}_{}", name, args.len(), parts.join("_"))
        }
    }
}""",
    """        Type::Record(name, args) => {
            let parts: Vec<String> = args.iter().map(mangled_type_name).collect();
            format!("Record_{}_{}_{}", name, args.len(), parts.join("_"))
        }
        // Nominal types: mangle by identity, not by name. Two
        // `type Id = distinct Int` declarations in different modules
        // must mangle differently. The name is presentation only and
        // deliberately does not appear here — see ADR 0029.
        //
        // `id` is per-compilation-unit and deterministic for a given
        // source set. A future incremental compiler with persistent
        // IR would need to make these ids stable across runs.
        Type::Distinct { id, .. } => format!("Distinct_{}", id.0),
    }
}""",
)

EDIT_PLAN_TEST = (
    """    #[test]
    fn mangler_distinguishes_map_key_and_value() {
        // Different key types.
        assert_ne!(
            mangled_type_name(&Type::map(Type::String, Type::Int)),
            mangled_type_name(&Type::map(Type::Int, Type::Int)),
        );
        // Different value types.
        assert_ne!(
            mangled_type_name(&Type::map(Type::String, Type::Int)),
            mangled_type_name(&Type::map(Type::String, Type::Float)),
        );
        // No collision with an equally-named Generic.
        assert_ne!(
            mangled_type_name(&Type::map(Type::String, Type::Int)),
            mangled_type_name(&Type::generic("Map", vec![Type::String, Type::Int])),
        );
    }
}""",
    """    #[test]
    fn mangler_distinguishes_map_key_and_value() {
        // Different key types.
        assert_ne!(
            mangled_type_name(&Type::map(Type::String, Type::Int)),
            mangled_type_name(&Type::map(Type::Int, Type::Int)),
        );
        // Different value types.
        assert_ne!(
            mangled_type_name(&Type::map(Type::String, Type::Int)),
            mangled_type_name(&Type::map(Type::String, Type::Float)),
        );
        // No collision with an equally-named Generic.
        assert_ne!(
            mangled_type_name(&Type::map(Type::String, Type::Int)),
            mangled_type_name(&Type::generic("Map", vec![Type::String, Type::Int])),
        );
    }

    #[test]
    fn mangler_distinguishes_nominal_ids() {
        use crate::common::types::NominalTypeId;
        // Same name, different ids — must mangle differently. This is
        // the identity-collision test from ADR 0029: two modules each
        // declaring `type Id = distinct Int`.
        let a = Type::distinct(NominalTypeId(1), "Id", Type::Int);
        let b = Type::distinct(NominalTypeId(2), "Id", Type::Int);
        assert_ne!(mangled_type_name(&a), mangled_type_name(&b));

        // Same id — same mangle, regardless of name. Name is not
        // identity.
        let a2 = Type::distinct(NominalTypeId(1), "Id", Type::Int);
        assert_eq!(mangled_type_name(&a), mangled_type_name(&a2));
        let a3 = Type::distinct(NominalTypeId(1), "DifferentName", Type::Int);
        assert_eq!(mangled_type_name(&a), mangled_type_name(&a3));
    }
}""",
)


def patch(path, edits):
    src = path.read_text()
    for old, new in edits:
        if src.count(old) != 1:
            print(f"FAIL: {path} — pattern matched {src.count(old)} times; expected 1")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


def main():
    for p in (LLVM, PLAN):
        if not p.exists():
            print(f"ERROR: {p} not found. Run from the repo root.")
            raise SystemExit(1)

    patch(LLVM, [EDIT_LLVM])
    patch(PLAN, [EDIT_PLAN_ARM, EDIT_PLAN_TEST])

    print()
    print("NEXT: cargo build (expect clean)")
    print("      cargo test --release")


if __name__ == "__main__":
    main()

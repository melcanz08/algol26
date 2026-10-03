#!/usr/bin/env python3
"""A3 step 4: passes + builder + pipeline test."""

from pathlib import Path

def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        n = src.count(old)
        if n != 1:
            print(f"FAIL: {path} — edit {i} matched {n} times")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            path.write_text(src)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")

patch(Path("src/compiler/passes/type_check.rs"), [
    (
        """            &ast.distincts,
            &ast.enums,
        ) {""",
        """            &ast.distincts,
            &ast.enums,
            &ast.subranges,
        ) {""",
    ),
])

patch(Path("src/compiler/passes/build_ir.rs"), [
    (
        """            typed.nominal_types.clone(),
            typed.enum_types.clone(),
        ) {""",
        """            typed.nominal_types.clone(),
            typed.enum_types.clone(),
            typed.subrange_types.clone(),
        ) {""",
    ),
])

patch(Path("src/semantics/builder/mod.rs"), [
    (
        """        nominal_types: HashMap<String, Type>,
        enum_types: HashMap<String, Type>,
    ) -> (SemanticProgram, Vec<String>) {""",
        """        nominal_types: HashMap<String, Type>,
        enum_types: HashMap<String, Type>,
        subrange_types: HashMap<String, Type>,
    ) -> (SemanticProgram, Vec<String>) {""",
    ),
    (
        """    pub(super) enum_types: HashMap<String, Type>,
}""",
        """    pub(super) enum_types: HashMap<String, Type>,
    /// Subrange type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Subrange` whose `SubrangeTypeId`
    /// matches the one the analyzer assigned. See ADR 0031.
    pub(super) subrange_types: HashMap<String, Type>,
}""",
    ),
    (
        """            nominal_types,
            enum_types,
        };
        let program = builder.build_impl(functions);""",
        """            nominal_types,
            enum_types,
            subrange_types,
        };
        let program = builder.build_impl(functions);""",
    ),
    (
        """                if let Some(nominal) = self.nominal_types.get(name.as_str()) {
                    return nominal.clone();
                }
                if self.record_names.contains(name.as_str()) {""",
        """                if let Some(nominal) = self.nominal_types.get(name.as_str()) {
                    return nominal.clone();
                }
                if let Some(subrange) = self.subrange_types.get(name.as_str()) {
                    return subrange.clone();
                }
                if self.record_names.contains(name.as_str()) {""",
    ),
    (
        """            nominal_types: HashMap::new(),
            enum_types: HashMap::new(),
        }
    }""",
        """            nominal_types: HashMap::new(),
            enum_types: HashMap::new(),
            subrange_types: HashMap::new(),
        }
    }""",
    ),
])

patch(Path("tests/compiler_pipeline_equiv.rs"), [
    (
        """            &parsed.distincts,
            &parsed.enums,
        ) {""",
        """            &parsed.distincts,
            &parsed.enums,
            &parsed.subranges,
        ) {""",
    ),
    (
        """            typed.nominal_types.clone(),
            typed.enum_types.clone(),
        ) {""",
        """            typed.nominal_types.clone(),
            typed.enum_types.clone(),
            typed.subrange_types.clone(),
        ) {""",
    ),
])

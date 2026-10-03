#!/usr/bin/env python3
"""A3 step 1 (v3): analyzer/mod.rs edits.

Only SubrangeDecl is added to mod.rs's frontend::ast import;
SubrangeTypeId goes in items.rs (which uses it) not mod.rs.
"""

from pathlib import Path

PATH = Path("src/semantics/analyzer/mod.rs")
src = PATH.read_text()

def rep(old, new, label):
    global src
    n = src.count(old)
    if n != 1:
        print(f"FAIL: {label} — matched {n} times")
        print("-" * 60)
        print(old[:250])
        print("-" * 60)
        raise SystemExit(1)
    src = src.replace(old, new, 1)
    print(f"OK: {label}")

rep(
    "    Pattern, RecordDecl, Stmt, TraitDecl, WhereClause,\n};",
    "    Pattern, RecordDecl, Stmt, SubrangeDecl, TraitDecl, WhereClause,\n};",
    "frontend::ast import",
)

rep(
    """    enum_types: HashMap<String, Type>,
    /// Monotonic counter for `EnumTypeId`.
    next_enum_id: u32,""",
    """    enum_types: HashMap<String, Type>,
    /// Monotonic counter for `EnumTypeId`.
    next_enum_id: u32,
    /// Subrange type declarations (`type X Base in Low..High`),
    /// keyed by the declared name. Values carry the assigned
    /// `SubrangeTypeId` plus bounds. ADR 0031.
    subrange_types: HashMap<String, Type>,
    /// Monotonic counter for `SubrangeTypeId`.
    next_subrange_id: u32,""",
    "field declarations",
)

rep(
    """            enum_types: HashMap::new(),
            next_enum_id: 0,
        }
    }""",
    """            enum_types: HashMap::new(),
            next_enum_id: 0,
            subrange_types: HashMap::new(),
            next_subrange_id: 0,
        }
    }""",
    "field initializers",
)

rep(
    """    pub fn take_enum_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.enum_types)
    }""",
    """    pub fn take_enum_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.enum_types)
    }

    /// Take ownership of the resolved subrange type table.
    /// See ADR 0031.
    pub fn take_subrange_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.subrange_types)
    }""",
    "take_subrange_types",
)

rep(
    "        self.analyze_with_spans(functions, &[], &[], &[], &[], &[])\n    }",
    "        self.analyze_with_spans(functions, &[], &[], &[], &[], &[], &[])\n    }",
    "analyze()",
)

rep(
    """        distincts: &[DistinctDecl],
        enums: &[EnumDecl],
    ) -> Result<()> {""",
    """        distincts: &[DistinctDecl],
        enums: &[EnumDecl],
        subranges: &[SubrangeDecl],
    ) -> Result<()> {""",
    "analyze_with_spans signature",
)

rep(
    """        self.register_nominal_types(distincts)?;

        self.register_user_functions(functions)?;""",
    """        self.register_nominal_types(distincts)?;

        // Subranges are registered after enums and nominal types
        // because their base may be an enum. See ADR 0031.
        self.register_subrange_types(subranges)?;

        self.register_user_functions(functions)?;""",
    "register_subrange_types call",
)

rep(
    """        self.analyze_with_spans(functions, traits, impls, records, &[], &[])
    }""",
    """        self.analyze_with_spans(functions, traits, impls, records, &[], &[], &[])
    }""",
    "analyze_with_traits delegation",
)

PATH.write_text(src)
print()
print("OK: analyzer/mod.rs patched")

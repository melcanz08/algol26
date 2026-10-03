#!/usr/bin/env python3
"""A3 step 3: compiler.rs edits for subrange types."""

from pathlib import Path

PATH = Path("src/compiler.rs")
src = PATH.read_text()

def rep(old, new, label):
    global src
    n = src.count(old)
    if n != 1:
        print(f"FAIL: {label} — matched {n} times")
        print("-" * 60)
        print(old[:250])
        print("-" * 60)
        PATH.write_text(src)
        raise SystemExit(1)
    src = src.replace(old, new, 1)
    PATH.write_text(src)
    print(f"OK: {label}")

# 1. TypedProgram field
rep(
    """    /// Resolved enum type declarations, keyed by name. Values carry
    /// the `EnumTypeId` the analyzer assigned. Same single-source
    /// discipline as `nominal_types`. See ADR 0030.
    pub enum_types: std::collections::HashMap<String, crate::common::types::Type>,
}""",
    """    /// Resolved enum type declarations, keyed by name. Values carry
    /// the `EnumTypeId` the analyzer assigned. Same single-source
    /// discipline as `nominal_types`. See ADR 0030.
    pub enum_types: std::collections::HashMap<String, crate::common::types::Type>,
    /// Resolved subrange type declarations, keyed by name. Values
    /// carry the `SubrangeTypeId` the analyzer assigned. Same
    /// single-source discipline. See ADR 0031.
    pub subrange_types: std::collections::HashMap<String, crate::common::types::Type>,
}""",
    "TypedProgram field",
)

# 2. type_check_program signature — add parameter
rep(
    """    enums: &[crate::frontend::ast::EnumDecl],     // ← ADR 0030
) -> Result<TypedProgram> {""",
    """    enums: &[crate::frontend::ast::EnumDecl],     // ← ADR 0030
    subranges: &[crate::frontend::ast::SubrangeDecl], // ← ADR 0031
) -> Result<TypedProgram> {""",
    "type_check_program signature",
)

# 3. take_subrange_types
rep(
    "    let enum_types = analyzer.take_enum_types();",
    "    let enum_types = analyzer.take_enum_types();\n    let subrange_types = analyzer.take_subrange_types();",
    "take_subrange_types",
)

# 4. TypedProgram construction
rep(
    """        records: records.to_vec(),
        nominal_types,
        enum_types,
    })
}""",
    """        records: records.to_vec(),
        nominal_types,
        enum_types,
        subrange_types,
    })
}""",
    "TypedProgram construction",
)

# 5. build_semantic_ir_program signature + call
rep(
    """    enum_types: std::collections::HashMap<String, crate::common::types::Type>,
) -> Result<crate::ir::semantic_ir::SemanticProgram> {
    use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode};
    use crate::semantics::builder::SemanticIRBuilder;

    let (program, diagnostics) = SemanticIRBuilder::build(
        functions,
        type_table_id,
        plan,
        records,
        nominal_types,
        enum_types,
    );""",
    """    enum_types: std::collections::HashMap<String, crate::common::types::Type>,
    subrange_types: std::collections::HashMap<String, crate::common::types::Type>,
) -> Result<crate::ir::semantic_ir::SemanticProgram> {
    use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode};
    use crate::semantics::builder::SemanticIRBuilder;

    let (program, diagnostics) = SemanticIRBuilder::build(
        functions,
        type_table_id,
        plan,
        records,
        nominal_types,
        enum_types,
        subrange_types,
    );""",
    "build_semantic_ir_program",
)

print()
print("OK: compiler.rs patched")

#!/usr/bin/env python3
"""A3 step 1 (v4): analyzer/mod.rs edits.

Fixes two problems from the previous attempt:
1. The register_subrange_types call site has other lines between
   register_nominal_types and register_user_functions. Insert by
   line anchor instead of exact block.
2. Writes the file incrementally after each successful edit, so a
   later failure doesn't discard earlier work.
"""

from pathlib import Path

PATH = Path("src/semantics/analyzer/mod.rs")
src = PATH.read_text()
successful = []

def rep(old, new, label):
    global src
    n = src.count(old)
    if n != 1:
        print(f"FAIL: {label} — matched {n} times")
        print("-" * 60)
        print(old[:250])
        print("-" * 60)
        # Write whatever has succeeded so far
        PATH.write_text(src)
        print(f"WROTE partial: {len(successful)} edit(s)")
        raise SystemExit(1)
    src = src.replace(old, new, 1)
    successful.append(label)
    PATH.write_text(src)  # incremental write
    print(f"OK: {label}")

# 1. frontend::ast import
rep(
    "    Pattern, RecordDecl, Stmt, TraitDecl, WhereClause,\n};",
    "    Pattern, RecordDecl, Stmt, SubrangeDecl, TraitDecl, WhereClause,\n};",
    "frontend::ast import",
)

# 2. Field declarations
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

# 3. Field initializers
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

# 4. take_subrange_types accessor
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

# 5. analyze() 6-arg call
rep(
    "        self.analyze_with_spans(functions, &[], &[], &[], &[], &[])\n    }",
    "        self.analyze_with_spans(functions, &[], &[], &[], &[], &[], &[])\n    }",
    "analyze()",
)

# 6. analyze_with_spans signature
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

# 7. Insert register_subrange_types call — line-based (after
#    `self.register_nominal_types(distincts)?;`)
lines = src.split("\n")
insert_idx = None
for i, line in enumerate(lines):
    if "self.register_nominal_types(distincts)?;" in line:
        insert_idx = i
        break
if insert_idx is None:
    PATH.write_text(src)
    print("FAIL: register_subrange_types anchor not found")
    raise SystemExit(1)
indent = lines[insert_idx][: len(lines[insert_idx]) - len(lines[insert_idx].lstrip())]
lines.insert(insert_idx + 1, "")
lines.insert(
    insert_idx + 2,
    f"{indent}// Subranges are registered after enums and nominal types",
)
lines.insert(
    insert_idx + 3,
    f"{indent}// because their base may be an enum. See ADR 0031.",
)
lines.insert(insert_idx + 4, f"{indent}self.register_subrange_types(subranges)?;")
src = "\n".join(lines)
PATH.write_text(src)
print("OK: register_subrange_types call")
successful.append("register_subrange_types call")

# 8. analyze_with_traits delegation
rep(
    """        self.analyze_with_spans(functions, traits, impls, records, &[], &[])
    }""",
    """        self.analyze_with_spans(functions, traits, impls, records, &[], &[], &[])
    }""",
    "analyze_with_traits delegation",
)

print()
print(f"OK: analyzer/mod.rs patched ({len(successful)} edits)")

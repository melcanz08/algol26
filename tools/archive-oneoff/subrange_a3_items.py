#!/usr/bin/env python3
"""A3 step 2: analyzer/items.rs."""

from pathlib import Path

PATH = Path("src/semantics/analyzer/items.rs")
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

# 1. Import SubrangeTypeId
rep(
    "use crate::common::types::EnumTypeId;",
    "use crate::common::types::{EnumTypeId, SubrangeTypeId};",
    "import SubrangeTypeId",
)

# 2. Insert register_subrange_types + resolve_subrange_bounds
SUB_METHOD = r'''    /// Register every `type X Base in Low..High` declaration.
    ///
    /// Assigns a fresh `SubrangeTypeId` per declaration, in
    /// declaration order. Validates: base is Int or a registered
    /// enum; bounds are Int literals (Int base) or variant names
    /// (enum base); low <= high. See ADR 0031.
    pub(super) fn register_subrange_types(
        &mut self,
        decls: &[crate::frontend::ast::SubrangeDecl],
    ) -> Result<()> {
        for decl in decls {
            if self.subrange_types.contains_key(&decl.name) {
                return Err(CompileError::at(
                    decl.span,
                    &format!("Duplicate subrange declaration '{}'", decl.name),
                    ErrorCode::E0009,
                ));
            }
            let base = self.resolve_type_syntax(&decl.base)?;
            let (low, high) = self.resolve_subrange_bounds(&decl.name, &base, decl)?;
            if low > high {
                return Err(CompileError::at(
                    decl.span,
                    &format!(
                        "Subrange '{}' has low > high ({}..{})",
                        decl.name, low, high
                    ),
                    ErrorCode::E0002,
                ));
            }
            let id = SubrangeTypeId(self.next_subrange_id);
            self.next_subrange_id += 1;
            let ty = Type::subrange(id, &decl.name, base, low, high);
            self.subrange_types.insert(decl.name.clone(), ty);
        }
        Ok(())
    }

    /// Resolve subrange bounds. For Int bases, both bounds must be
    /// Int literals. For enum bases, they must be variant names,
    /// resolved to their ordinals. See ADR 0031.
    fn resolve_subrange_bounds(
        &self,
        decl_name: &str,
        base: &Type,
        decl: &crate::frontend::ast::SubrangeDecl,
    ) -> Result<(i64, i64)> {
        use crate::frontend::ast::ExprKind;
        match base {
            Type::Int => {
                let low = match &decl.low.kind {
                    ExprKind::Int(n, _) => *n,
                    _ => {
                        return Err(CompileError::at(
                            decl.low.span(),
                            &format!(
                                "Int subrange '{}' requires an integer literal for its lower bound",
                                decl_name
                            ),
                            ErrorCode::E0002,
                        ));
                    }
                };
                let high = match &decl.high.kind {
                    ExprKind::Int(n, _) => *n,
                    _ => {
                        return Err(CompileError::at(
                            decl.high.span(),
                            &format!(
                                "Int subrange '{}' requires an integer literal for its upper bound",
                                decl_name
                            ),
                            ErrorCode::E0002,
                        ));
                    }
                };
                Ok((low, high))
            }
            Type::Enum {
                name: enum_name,
                variants,
                ..
            } => {
                let resolve = |expr: &crate::frontend::ast::Expr,
                               which: &str|
                 -> Result<i64> {
                    match &expr.kind {
                        ExprKind::Var(n, _) => variants
                            .iter()
                            .position(|v| v == n)
                            .map(|i| i as i64)
                            .ok_or_else(|| {
                                CompileError::at(
                                    expr.span(),
                                    &format!("no variant '{}' on enum '{}'", n, enum_name),
                                    ErrorCode::E0004,
                                )
                            }),
                        _ => Err(CompileError::at(
                            expr.span(),
                            &format!(
                                "enum subrange '{}' requires a variant name for its {} bound",
                                decl_name, which
                            ),
                            ErrorCode::E0002,
                        )),
                    }
                };
                let low = resolve(&decl.low, "lower")?;
                let high = resolve(&decl.high, "upper")?;
                Ok((low, high))
            }
            other => Err(CompileError::at(
                decl.span,
                &format!(
                    "Subrange '{}' requires an Int or enum base, found {}",
                    decl_name, other
                ),
                ErrorCode::E0002,
            )),
        }
    }

'''

rep(
    "    /// Register every `type X distinct Y` declaration.",
    SUB_METHOD + "    /// Register every `type X distinct Y` declaration.",
    "insert register_subrange_types",
)

# 3. Subranges snapshot in register_user_functions
rep(
    """        let enums_snapshot = self.enum_types.clone();
        for func in functions {""",
    """        let enums_snapshot = self.enum_types.clone();
        let subranges_snapshot = self.subrange_types.clone();
        for func in functions {""",
    "subranges_snapshot local",
)

# 4. Thread subranges_snapshot to params resolution
rep(
    """                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                            &enums_snapshot,
                        )?,""",
    """                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                            &enums_snapshot,
                            &subranges_snapshot,
                        )?,""",
    "params resolution call",
)

# 5. Thread to return-type resolution
rep(
    """                Some(t) => Self::resolve_syntax_with_records(
                    t,
                    &records_snapshot,
                    &nominals_snapshot,
                    &enums_snapshot,
                )?,""",
    """                Some(t) => Self::resolve_syntax_with_records(
                    t,
                    &records_snapshot,
                    &nominals_snapshot,
                    &enums_snapshot,
                    &subranges_snapshot,
                )?,""",
    "return resolution call",
)

# 6. resolve_type_syntax — check subranges
rep(
    """                if let Some(nominal) = self.nominal_types.get(name.as_str()).cloned() {
                    return Ok(nominal);
                }
                if let Some(rec) = self.records.get(name.as_str()).cloned() {
                    let args: Vec<Type> = rec.type_params.iter().map(|_| Type::Unknown).collect();
                    return Ok(Type::record(name, args));
                }""",
    """                if let Some(nominal) = self.nominal_types.get(name.as_str()).cloned() {
                    return Ok(nominal);
                }
                if let Some(subrange) = self.subrange_types.get(name.as_str()).cloned() {
                    return Ok(subrange);
                }
                if let Some(rec) = self.records.get(name.as_str()).cloned() {
                    let args: Vec<Type> = rec.type_params.iter().map(|_| Type::Unknown).collect();
                    return Ok(Type::record(name, args));
                }""",
    "resolve_type_syntax subrange check",
)

# 7. resolve_syntax_with_records signature
rep(
    """    fn resolve_syntax_with_records(
        syntax: &TypeSyntax,
        records: &HashMap<String, RecordInfo>,
        nominals: &HashMap<String, Type>,
        enums: &HashMap<String, Type>,
    ) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = enums.get(name.as_str()) {
                    return Ok(enum_ty.clone());
                }
                if let Some(nominal) = nominals.get(name.as_str()) {
                    return Ok(nominal.clone());
                }""",
    """    fn resolve_syntax_with_records(
        syntax: &TypeSyntax,
        records: &HashMap<String, RecordInfo>,
        nominals: &HashMap<String, Type>,
        enums: &HashMap<String, Type>,
        subranges: &HashMap<String, Type>,
    ) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = enums.get(name.as_str()) {
                    return Ok(enum_ty.clone());
                }
                if let Some(nominal) = nominals.get(name.as_str()) {
                    return Ok(nominal.clone());
                }
                if let Some(subrange) = subranges.get(name.as_str()) {
                    return Ok(subrange.clone());
                }""",
    "resolve_syntax_with_records signature + Named",
)

# 8+9. Two recursive calls (loop)
for _ in range(2):
    rep(
        ".map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums))",
        ".map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums, subranges))",
        "recursive call",
    )

print()
print("OK: analyzer/items.rs patched")

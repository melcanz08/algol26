#!/usr/bin/env python3
"""
A3: analyzer registers subrange types and resolves them.

Mirrors A3 for enums. Adds:
- register_subrange_types (validates bounds shape and low <= high)
- subrange check in resolve_type_syntax
- fields + accessor in the analyzer
- threading through TypedProgram, the pass layer, and the IR builder
"""

from pathlib import Path

MOD = Path("src/semantics/analyzer/mod.rs")
ITEMS = Path("src/semantics/analyzer/items.rs")
COMPILER = Path("src/compiler.rs")
TYPE_CHECK = Path("src/compiler/passes/type_check.rs")
BUILD_IR = Path("src/compiler/passes/build_ir.rs")
BUILDER = Path("src/semantics/builder/mod.rs")
PIPELINE_EQ = Path("tests/compiler_pipeline_equiv.rs")


def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60)
            print(old[:250])
            print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


# ─── analyzer/mod.rs ───────────────────────────────────────────────
patch(MOD, [
    (
        "use crate::common::types::{EnumTypeId, NominalTypeId, Type};",
        "use crate::common::types::{EnumTypeId, NominalTypeId, SubrangeTypeId, Type};",
    ),
    (
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr,
    Pattern, RecordDecl, Stmt, TraitDecl, WhereClause,
};""",
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr,
    Pattern, RecordDecl, Stmt, SubrangeDecl, TraitDecl, WhereClause,
};""",
    ),
    (
        """    /// Enum type declarations (`enum Name ...`), keyed by the
    /// declared name. Values carry the assigned `EnumTypeId`.
    /// Identity is the id; the name and variants are presentation.
    /// ADR 0030.
    enum_types: HashMap<String, Type>,
    /// Monotonic counter for `EnumTypeId`.
    next_enum_id: u32,""",
        """    /// Enum type declarations (`enum Name ...`), keyed by the
    /// declared name. Values carry the assigned `EnumTypeId`.
    /// Identity is the id; the name and variants are presentation.
    /// ADR 0030.
    enum_types: HashMap<String, Type>,
    /// Monotonic counter for `EnumTypeId`.
    next_enum_id: u32,
    /// Subrange type declarations (`type X Base in Low..High`),
    /// keyed by the declared name. Values carry the assigned
    /// `SubrangeTypeId` plus bounds. ADR 0031.
    subrange_types: HashMap<String, Type>,
    /// Monotonic counter for `SubrangeTypeId`.
    next_subrange_id: u32,""",
    ),
    (
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
    ),
    (
        """    /// Take ownership of the resolved enum type table so it can be
    /// handed to `TypedProgram`. Same single-source-of-truth
    /// discipline as `take_nominal_types`. See ADR 0030.
    pub fn take_enum_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.enum_types)
    }""",
        """    /// Take ownership of the resolved enum type table so it can be
    /// handed to `TypedProgram`. Same single-source-of-truth
    /// discipline as `take_nominal_types`. See ADR 0030.
    pub fn take_enum_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.enum_types)
    }

    /// Take ownership of the resolved subrange type table.
    /// See ADR 0031.
    pub fn take_subrange_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.subrange_types)
    }""",
    ),
    (
        """    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[], &[], &[])
    }""",
        """    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[], &[], &[], &[])
    }""",
    ),
    (
        """    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
        distincts: &[DistinctDecl],
        enums: &[EnumDecl],
    ) -> Result<()> {""",
        """    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
        distincts: &[DistinctDecl],
        enums: &[EnumDecl],
        subranges: &[SubrangeDecl],
    ) -> Result<()> {""",
    ),
    (
        """        // Enums are registered before nominal types and records: a
        // nominal type or record field could name an enum in a later
        // ADR, and the resolution order in `resolve_type_syntax`
        // matches this. See ADR 0030.
        self.register_enum_types(enums)?;

        // Nominal types are registered before records and user
        // functions: a record field or a function signature may name
        // a nominal type. Base must be primitive (ADR 0029).
        self.register_nominal_types(distincts)?;""",
        """        // Enums are registered before nominal types and records: a
        // nominal type or record field could name an enum in a later
        // ADR, and the resolution order in `resolve_type_syntax`
        // matches this. See ADR 0030.
        self.register_enum_types(enums)?;

        // Nominal types are registered before records and user
        // functions: a record field or a function signature may name
        // a nominal type. Base must be primitive (ADR 0029).
        self.register_nominal_types(distincts)?;

        // Subranges are registered after enums and nominal types
        // because their base may be an enum. See ADR 0031.
        self.register_subrange_types(subranges)?;""",
    ),
    (
        """        // The 4-arg form is retained for callers that predate
        // nominal and enum types. Callers with `DistinctDecl`s or
        // `EnumDecl`s in scope should call `analyze_with_spans`
        // directly.
        self.analyze_with_spans(functions, traits, impls, records, &[], &[])
    }""",
        """        // The 4-arg form is retained for callers that predate
        // nominal, enum, and subrange types. Callers with declarations
        // in scope should call `analyze_with_spans` directly.
        self.analyze_with_spans(functions, traits, impls, records, &[], &[], &[])
    }""",
])

# ─── analyzer/items.rs ─────────────────────────────────────────────
patch(ITEMS, [
    (
        """use super::*;
use crate::common::types::EnumTypeId;
use crate::frontend::ast::{RecordDecl, TypeSyntax};""",
        """use super::*;
use crate::common::types::{EnumTypeId, SubrangeTypeId};
use crate::frontend::ast::{RecordDecl, TypeSyntax};""",
    ),
    (
        """    /// Register every `type X distinct Y` declaration.
    ///
    /// Assigns a fresh `NominalTypeId` per declaration, in
    /// declaration order, so ids are deterministic for a given
    /// source set. Validates that the base is one of `Int`,
    /// `Float`, `Bool`, `String` — the v1 constraint from ADR
    /// 0029.
    pub(super) fn register_nominal_types(""",
        """    /// Register every `type X Base in Low..High` declaration.
    ///
    /// Assigns a fresh `SubrangeTypeId` per declaration, in
    /// declaration order. Validates:
    /// - base is Int or a registered enum
    /// - bounds are Int literals (Int base) or variant names (enum base)
    /// - low <= high
    /// See ADR 0031.
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

            let (low, high) = match &base {
                Type::Int => {
                    let low = match &decl.low.kind {
                        crate::frontend::ast::ExprKind::Int(n, _) => *n,
                        _ => {
                            return Err(CompileError::at(
                                decl.low.span(),
                                &format!(
                                    "Int subrange '{}' requires an integer literal \\
                                     for its lower bound",
                                    decl.name
                                ),
                                ErrorCode::E0002,
                            ));
                        }
                    };
                    let high = match &decl.high.kind {
                        crate::frontend::ast::ExprKind::Int(n, _) => *n,
                        _ => {
                            return Err(CompileError::at(
                                decl.high.span(),
                                &format!(
                                    "Int subrange '{}' requires an integer literal \\
                                     for its upper bound",
                                    decl.name
                                ),
                                ErrorCode::E0002,
                            ));
                        }
                    };
                    (low, high)
                }
                Type::Enum {
                    name: enum_name,
                    variants,
                    ..
                } => {
                    let enum_name = enum_name.clone();
                    let variants = variants.clone();
                    let resolve = |expr: &Expr, which: &str| -> Result<i64> {
                        match &expr.kind {
                            crate::frontend::ast::ExprKind::Var(n, _) => variants
                                .iter()
                                .position(|v| v == n)
                                .map(|i| i as i64)
                                .ok_or_else(|| {
                                    CompileError::at(
                                        expr.span(),
                                        &format!(
                                            "no variant '{}' on enum '{}'",
                                            n, enum_name
                                        ),
                                        ErrorCode::E0004,
                                    )
                                }),
                            _ => Err(CompileError::at(
                                expr.span(),
                                &format!(
                                    "enum subrange '{}' requires a variant name \\
                                     for its {} bound",
                                    decl.name, which
                                ),
                                ErrorCode::E0002,
                            )),
                        }
                    };
                    let low = resolve(&decl.low, "lower")?;
                    let high = resolve(&decl.high, "upper")?;
                    (low, high)
                }
                other => {
                    return Err(CompileError::at(
                        decl.span,
                        &format!(
                            "Subrange '{}' requires an Int or enum base, found {}",
                            decl.name, other
                        ),
                        ErrorCode::E0002,
                    ));
                }
            };

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

    /// Register every `type X distinct Y` declaration.
    ///
    /// Assigns a fresh `NominalTypeId` per declaration, in
    /// declaration order, so ids are deterministic for a given
    /// source set. Validates that the base is one of `Int`,
    /// `Float`, `Bool`, `String` — the v1 constraint from ADR
    /// 0029.
    pub(super) fn register_nominal_types(""",
    ),
    (
        """        let records_snapshot = self.records.clone();
        let nominals_snapshot = self.nominal_types.clone();
        let enums_snapshot = self.enum_types.clone();
        for func in functions {""",
        """        let records_snapshot = self.records.clone();
        let nominals_snapshot = self.nominal_types.clone();
        let enums_snapshot = self.enum_types.clone();
        let subranges_snapshot = self.subrange_types.clone();
        for func in functions {""",
    ),
    (
        """                    let type_ = match t {
                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                            &enums_snapshot,
                        )?,
                        None => Type::Unknown,
                    };""",
        """                    let type_ = match t {
                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                            &enums_snapshot,
                            &subranges_snapshot,
                        )?,
                        None => Type::Unknown,
                    };""",
    ),
    (
        """            let return_type = match &func.return_type {
                Some(t) => Self::resolve_syntax_with_records(
                    t,
                    &records_snapshot,
                    &nominals_snapshot,
                    &enums_snapshot,
                )?,
                None => Type::Void,
            };""",
        """            let return_type = match &func.return_type {
                Some(t) => Self::resolve_syntax_with_records(
                    t,
                    &records_snapshot,
                    &nominals_snapshot,
                    &enums_snapshot,
                    &subranges_snapshot,
                )?,
                None => Type::Void,
            };""",
    ),
    (
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = self.enum_types.get(name.as_str()).cloned() {
                    return Ok(enum_ty);
                }
                if let Some(nominal) = self.nominal_types.get(name.as_str()).cloned() {
                    return Ok(nominal);
                }""",
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = self.enum_types.get(name.as_str()).cloned() {
                    return Ok(enum_ty);
                }
                if let Some(nominal) = self.nominal_types.get(name.as_str()).cloned() {
                    return Ok(nominal);
                }
                if let Some(subrange) = self.subrange_types.get(name.as_str()).cloned() {
                    return Ok(subrange);
                }""",
    ),
    (
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
    ),
    (
        """                        .map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums))""",
        """                        .map(|a| Self::resolve_syntax_with_records(
                            a,
                            records,
                            nominals,
                            enums,
                            subranges,
                        ))""",
    ),
    (
        """                    .map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums))""",
        """                    .map(|a| Self::resolve_syntax_with_records(
                        a,
                        records,
                        nominals,
                        enums,
                        subranges,
                    ))""",
    ),
])

# ─── compiler.rs ───────────────────────────────────────────────────
patch(COMPILER, [
    (
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
    ),
    (
        """pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    records: &[crate::frontend::ast::RecordDecl], // ← NEW
    distincts: &[crate::frontend::ast::DistinctDecl], // ← ADR 0029
    enums: &[crate::frontend::ast::EnumDecl],     // ← ADR 0030
) -> Result<TypedProgram> {""",
        """pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    records: &[crate::frontend::ast::RecordDecl], // ← NEW
    distincts: &[crate::frontend::ast::DistinctDecl], // ← ADR 0029
    enums: &[crate::frontend::ast::EnumDecl],     // ← ADR 0030
    subranges: &[crate::frontend::ast::SubrangeDecl], // ← ADR 0031
) -> Result<TypedProgram> {""",
    ),
    (
        """    analyzer.analyze_with_spans(functions, traits, impls, records, distincts, enums)?;""",
        """    analyzer.analyze_with_spans(functions, traits, impls, records, distincts, enums, subranges)?;""",
    ),
    (
        """    let nominal_types = analyzer.take_nominal_types();
    let enum_types = analyzer.take_enum_types();
    let mut plan = InstantiationPlan::from_instantiations(&instantiations);""",
        """    let nominal_types = analyzer.take_nominal_types();
    let enum_types = analyzer.take_enum_types();
    let subrange_types = analyzer.take_subrange_types();
    let mut plan = InstantiationPlan::from_instantiations(&instantiations);""",
    ),
    (
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
    ),
    (
        """pub fn build_semantic_ir_program(
    functions: &[crate::frontend::ast::FunctionDecl],
    type_table_id: std::collections::HashMap<
        crate::frontend::ast::ExprId,
        crate::common::types::Type,
    >,
    plan: crate::ir::instantiation_plan::InstantiationPlan,
    records: &[crate::frontend::ast::RecordDecl],
    nominal_types: std::collections::HashMap<String, crate::common::types::Type>,
    enum_types: std::collections::HashMap<String, crate::common::types::Type>,
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
        """pub fn build_semantic_ir_program(
    functions: &[crate::frontend::ast::FunctionDecl],
    type_table_id: std::collections::HashMap<
        crate::frontend::ast::ExprId,
        crate::common::types::Type,
    >,
    plan: crate::ir::instantiation_plan::InstantiationPlan,
    records: &[crate::frontend::ast::RecordDecl],
    nominal_types: std::collections::HashMap<String, crate::common::types::Type>,
    enum_types: std::collections::HashMap<String, crate::common::types::Type>,
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
    ),
])

# ─── type_check.rs pass ────────────────────────────────────────────
patch(TYPE_CHECK, [
    (
        """        match crate::compiler::type_check_program(
            &ast.functions,
            &ast.traits,
            &ast.impls,
            &ast.records,
            &ast.distincts,
            &ast.enums,
        ) {""",
        """        match crate::compiler::type_check_program(
            &ast.functions,
            &ast.traits,
            &ast.impls,
            &ast.records,
            &ast.distincts,
            &ast.enums,
            &ast.subranges,
        ) {""",
    ),
])

# ─── build_ir.rs pass ──────────────────────────────────────────────
patch(BUILD_IR, [
    (
        """        match crate::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
            typed.nominal_types.clone(),
            typed.enum_types.clone(),
        ) {""",
        """        match crate::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
            typed.nominal_types.clone(),
            typed.enum_types.clone(),
            typed.subrange_types.clone(),
        ) {""",
    ),
])

# ─── builder/mod.rs ────────────────────────────────────────────────
patch(BUILDER, [
    (
        """    pub fn build(
        functions: &[FunctionDecl],
        type_table_id: HashMap<ExprId, Type>,
        plan: InstantiationPlan,
        records: &[RecordDecl],
        nominal_types: HashMap<String, Type>,
        enum_types: HashMap<String, Type>,
    ) -> (SemanticProgram, Vec<String>) {
        let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();

        let mut builder = SemanticIRBuilder {""",
        """    pub fn build(
        functions: &[FunctionDecl],
        type_table_id: HashMap<ExprId, Type>,
        plan: InstantiationPlan,
        records: &[RecordDecl],
        nominal_types: HashMap<String, Type>,
        enum_types: HashMap<String, Type>,
        subrange_types: HashMap<String, Type>,
    ) -> (SemanticProgram, Vec<String>) {
        let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();

        let mut builder = SemanticIRBuilder {""",
    ),
    (
        """    /// Enum type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Enum` whose `EnumTypeId` matches the
    /// one the analyzer assigned. See ADR 0030.
    pub(super) enum_types: HashMap<String, Type>,
}""",
        """    /// Enum type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Enum` whose `EnumTypeId` matches the
    /// one the analyzer assigned. See ADR 0030.
    pub(super) enum_types: HashMap<String, Type>,
    /// Subrange type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Subrange` whose `SubrangeTypeId`
    /// matches the one the analyzer assigned. See ADR 0031.
    pub(super) subrange_types: HashMap<String, Type>,
}""",
    ),
    (
        """            plan,
            record_names,
            nominal_types,
            enum_types,
        };
        let program = builder.build_impl(functions);""",
        """            plan,
            record_names,
            nominal_types,
            enum_types,
            subrange_types,
        };
        let program = builder.build_impl(functions);""",
    ),
    (
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Type {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = self.enum_types.get(name.as_str()) {
                    return enum_ty.clone();
                }
                if let Some(nominal) = self.nominal_types.get(name.as_str()) {
                    return nominal.clone();
                }""",
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Type {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = self.enum_types.get(name.as_str()) {
                    return enum_ty.clone();
                }
                if let Some(nominal) = self.nominal_types.get(name.as_str()) {
                    return nominal.clone();
                }
                if let Some(subrange) = self.subrange_types.get(name.as_str()) {
                    return subrange.clone();
                }""",
    ),
    (
        """            plan: InstantiationPlan::default(),
            record_names: HashSet::new(),
            nominal_types: HashMap::new(),
            enum_types: HashMap::new(),
        }
    }""",
        """            plan: InstantiationPlan::default(),
            record_names: HashSet::new(),
            nominal_types: HashMap::new(),
            enum_types: HashMap::new(),
            subrange_types: HashMap::new(),
        }
    }""",
    ),
])

# ─── pipeline_equiv test ───────────────────────────────────────────
patch(PIPELINE_EQ, [
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
        """            &typed.records,
            typed.nominal_types.clone(),
            typed.enum_types.clone(),
        ) {""",
        """            &typed.records,
            typed.nominal_types.clone(),
            typed.enum_types.clone(),
            typed.subrange_types.clone(),
        ) {""",
    ),
])

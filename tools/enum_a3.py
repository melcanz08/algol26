#!/usr/bin/env python3
"""
A3: analyzer registers enum types and resolves them.

Same shape as A3 for nominal types:
- SemanticAnalyzer gains enum_types + next_enum_id
- register_enum_types assigns ids in declaration order
- resolve_type_syntax checks enum_types before nominal_types
- register_user_functions threads an enum snapshot through
  resolve_syntax_with_records
- analyze_with_spans gains an `enums` parameter
- type_check_program gains an `enums` parameter and populates
  TypedProgram.enum_types (single-source-of-truth for enum identity)
- build_semantic_ir_program and SemanticIRBuilder::build take the
  resolved enum_types map
"""

from pathlib import Path

MOD = Path("src/semantics/analyzer/mod.rs")
ITEMS = Path("src/semantics/analyzer/items.rs")
COMPILER = Path("src/compiler.rs")
BUILD_IR = Path("src/compiler/passes/build_ir.rs")
PIPELINE_EQ = Path("tests/compiler_pipeline_equiv.rs")
BUILDER_MOD = Path("src/semantics/builder/mod.rs")


MOD_EDITS = [
    # 1. AST import
    (
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern,
    RecordDecl, Stmt, TraitDecl, WhereClause,
};""",
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, EnumDecl, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr,
    Pattern, RecordDecl, Stmt, TraitDecl, WhereClause,
};""",
    ),
    # 2. Add fields
    (
        """    /// Nominal type declarations (`type X distinct Y`), keyed by the
    /// declared name. Values carry the assigned `NominalTypeId`.
    /// Identity is the id; the name is presentation only. ADR 0029.
    nominal_types: HashMap<String, Type>,
    /// Monotonic counter for `NominalTypeId`.
    next_nominal_id: u32,""",
        """    /// Nominal type declarations (`type X distinct Y`), keyed by the
    /// declared name. Values carry the assigned `NominalTypeId`.
    /// Identity is the id; the name is presentation only. ADR 0029.
    nominal_types: HashMap<String, Type>,
    /// Monotonic counter for `NominalTypeId`.
    next_nominal_id: u32,
    /// Enum type declarations (`enum Name ...`), keyed by the
    /// declared name. Values carry the assigned `EnumTypeId`.
    /// Identity is the id; the name and variants are presentation.
    /// ADR 0030.
    enum_types: HashMap<String, Type>,
    /// Monotonic counter for `EnumTypeId`.
    next_enum_id: u32,""",
    ),
    # 3. Initialize
    (
        """            records: HashMap::new(),
            nominal_types: HashMap::new(),
            next_nominal_id: 0,
        }
    }""",
        """            records: HashMap::new(),
            nominal_types: HashMap::new(),
            next_nominal_id: 0,
            enum_types: HashMap::new(),
            next_enum_id: 0,
        }
    }""",
    ),
    # 4. take_enum_types accessor, next to take_nominal_types
    (
        """    /// Take ownership of the resolved nominal type table so it can be
    /// handed to `TypedProgram`. `NominalTypeId` is assigned by
    /// `register_nominal_types`; this is the single source of that
    /// identity. Downstream consumers receive the resolved map and
    /// never reconstruct ids. See ADR 0029.
    pub fn take_nominal_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.nominal_types)
    }""",
        """    /// Take ownership of the resolved nominal type table so it can be
    /// handed to `TypedProgram`. `NominalTypeId` is assigned by
    /// `register_nominal_types`; this is the single source of that
    /// identity. Downstream consumers receive the resolved map and
    /// never reconstruct ids. See ADR 0029.
    pub fn take_nominal_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.nominal_types)
    }

    /// Take ownership of the resolved enum type table so it can be
    /// handed to `TypedProgram`. Same single-source-of-truth
    /// discipline as `take_nominal_types`. See ADR 0030.
    pub fn take_enum_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.enum_types)
    }""",
    ),
    # 5. analyze() passes empty enums
    (
        """    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[], &[])
    }""",
        """    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[], &[], &[])
    }""",
    ),
    # 6. analyze_with_spans signature
    (
        """    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
        distincts: &[DistinctDecl],
    ) -> Result<()> {""",
        """    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
        distincts: &[DistinctDecl],
        enums: &[EnumDecl],
    ) -> Result<()> {""",
    ),
    # 7. Register enums before nominal types
    (
        """        self.register_builtin_functions();

        // Nominal types are registered before records and user
        // functions: a record field or a function signature may name
        // a nominal type. Base must be primitive (ADR 0029).
        self.register_nominal_types(distincts)?;""",
        """        self.register_builtin_functions();

        // Enums are registered before nominal types and records: a
        // nominal type or record field could name an enum in a later
        // ADR, and the resolution order in `resolve_type_syntax`
        // matches this. See ADR 0030.
        self.register_enum_types(enums)?;

        // Nominal types are registered before records and user
        // functions: a record field or a function signature may name
        // a nominal type. Base must be primitive (ADR 0029).
        self.register_nominal_types(distincts)?;""",
    ),
    # 8. analyze_with_traits delegates with &[] for both
    (
        """        // The 4-arg form is retained for callers that predate
        // nominal types. Callers with `DistinctDecl`s in scope
        // should call `analyze_with_spans` directly.
        self.analyze_with_spans(functions, traits, impls, records, &[])
    }""",
        """        // The 4-arg form is retained for callers that predate
        // nominal and enum types. Callers with `DistinctDecl`s or
        // `EnumDecl`s in scope should call `analyze_with_spans`
        // directly.
        self.analyze_with_spans(functions, traits, impls, records, &[], &[])
    }""",
    ),
]


ITEMS_EDITS = [
    # 1. Add register_enum_types before register_nominal_types
    (
        """    /// Register every `type X distinct Y` declaration.
    ///
    /// Assigns a fresh `NominalTypeId` per declaration, in
    /// declaration order, so ids are deterministic for a given
    /// source set. Validates that the base is one of `Int`,
    /// `Float`, `Bool`, `String` — the v1 constraint from ADR
    /// 0029.
    pub(super) fn register_nominal_types(""",
        """    /// Register every `enum Name ...` declaration.
    ///
    /// Assigns a fresh `EnumTypeId` per declaration, in declaration
    /// order, so ids are deterministic for a given source set.
    /// Rejects empty enums (already caught at parse time) and
    /// duplicate variant names within a declaration.
    pub(super) fn register_enum_types(
        &mut self,
        decls: &[crate::frontend::ast::EnumDecl],
    ) -> Result<()> {
        for decl in decls {
            if self.enum_types.contains_key(&decl.name) {
                return Err(CompileError::at(
                    decl.span,
                    &format!("Duplicate enum declaration '{}'", decl.name),
                    ErrorCode::E0009,
                ));
            }
            let mut seen = HashSet::new();
            for variant in &decl.variants {
                if !seen.insert(variant.clone()) {
                    return Err(CompileError::at(
                        decl.span,
                        &format!("Duplicate variant '{}' in enum '{}'", variant, decl.name),
                        ErrorCode::E0009,
                    ));
                }
            }
            let id = EnumTypeId(self.next_enum_id);
            self.next_enum_id += 1;
            let ty = Type::enum_type(id, &decl.name, decl.variants.clone());
            self.enum_types.insert(decl.name.clone(), ty);
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
    # 2. register_user_functions: snapshot enums
    (
        """        let records_snapshot = self.records.clone();
        let nominals_snapshot = self.nominal_types.clone();
        for func in functions {""",
        """        let records_snapshot = self.records.clone();
        let nominals_snapshot = self.nominal_types.clone();
        let enums_snapshot = self.enum_types.clone();
        for func in functions {""",
    ),
    # 3. register_user_functions: pass enums snapshot to params resolution
    (
        """                    let type_ = match t {
                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                        )?,
                        None => Type::Unknown,
                    };""",
        """                    let type_ = match t {
                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                            &enums_snapshot,
                        )?,
                        None => Type::Unknown,
                    };""",
    ),
    # 4. register_user_functions: return-type resolution
    (
        """            let return_type = match &func.return_type {
                Some(t) => {
                    Self::resolve_syntax_with_records(t, &records_snapshot, &nominals_snapshot)?
                }
                None => Type::Void,
            };""",
        """            let return_type = match &func.return_type {
                Some(t) => Self::resolve_syntax_with_records(
                    t,
                    &records_snapshot,
                    &nominals_snapshot,
                    &enums_snapshot,
                )?,
                None => Type::Void,
            };""",
    ),
    # 5. resolve_type_syntax: check enum_types before nominal_types
    (
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
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
                }""",
    ),
    # 6. resolve_syntax_with_records signature + Named arm
    (
        """    fn resolve_syntax_with_records(
        syntax: &TypeSyntax,
        records: &HashMap<String, RecordInfo>,
        nominals: &HashMap<String, Type>,
    ) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(nominal) = nominals.get(name.as_str()) {
                    return Ok(nominal.clone());
                }""",
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
    ),
    # 7. Two recursive calls inside resolve_syntax_with_records
    (
        """                        .map(|a| Self::resolve_syntax_with_records(a, records, nominals))""",
        """                        .map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums))""",
    ),
    (
        """                    .map(|a| Self::resolve_syntax_with_records(a, records, nominals))""",
        """                    .map(|a| Self::resolve_syntax_with_records(a, records, nominals, enums))""",
    ),
]


COMPILER_EDITS = [
    # 1. TypedProgram gains enum_types field
    (
        """    /// Resolved nominal type declarations, keyed by name. Values
    /// carry the `NominalTypeId` the analyzer assigned. Downstream
    /// consumers read this map and never reconstruct ids — the
    /// analyzer is the single source of nominal identity.
    /// See ADR 0029.
    pub nominal_types: std::collections::HashMap<String, crate::common::types::Type>,
}""",
        """    /// Resolved nominal type declarations, keyed by name. Values
    /// carry the `NominalTypeId` the analyzer assigned. Downstream
    /// consumers read this map and never reconstruct ids — the
    /// analyzer is the single source of nominal identity.
    /// See ADR 0029.
    pub nominal_types: std::collections::HashMap<String, crate::common::types::Type>,
    /// Resolved enum type declarations, keyed by name. Values carry
    /// the `EnumTypeId` the analyzer assigned. Same single-source
    /// discipline as `nominal_types`. See ADR 0030.
    pub enum_types: std::collections::HashMap<String, crate::common::types::Type>,
}""",
    ),
    # 2. type_check_program signature gains enums
    (
        """pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    records: &[crate::frontend::ast::RecordDecl], // ← NEW
    distincts: &[crate::frontend::ast::DistinctDecl], // ← ADR 0029
) -> Result<TypedProgram> {""",
        """pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    records: &[crate::frontend::ast::RecordDecl], // ← NEW
    distincts: &[crate::frontend::ast::DistinctDecl], // ← ADR 0029
    enums: &[crate::frontend::ast::EnumDecl],     // ← ADR 0030
) -> Result<TypedProgram> {""",
    ),
    # 3. type_check_program body
    (
        """    analyzer.analyze_with_spans(functions, traits, impls, records, distincts)?;""",
        """    analyzer.analyze_with_spans(functions, traits, impls, records, distincts, enums)?;""",
    ),
    # 4. type_check_program: populate enum_types
    (
        """    let type_table_id = analyzer.take_type_table_id();
    let instantiations = analyzer.take_instantiations();
    let nominal_types = analyzer.take_nominal_types();
    let mut plan = InstantiationPlan::from_instantiations(&instantiations);
    plan.close(functions);

    Ok(TypedProgram {
        functions: Rc::clone(functions),
        type_info: TypeInfo {
            total_functions: functions.len(),
            total_variables: 0,
            types_checked: true,
        },
        type_table_id,
        plan,
        records: records.to_vec(),
        nominal_types,
    })
}""",
        """    let type_table_id = analyzer.take_type_table_id();
    let instantiations = analyzer.take_instantiations();
    let nominal_types = analyzer.take_nominal_types();
    let enum_types = analyzer.take_enum_types();
    let mut plan = InstantiationPlan::from_instantiations(&instantiations);
    plan.close(functions);

    Ok(TypedProgram {
        functions: Rc::clone(functions),
        type_info: TypeInfo {
            total_functions: functions.len(),
            total_variables: 0,
            types_checked: true,
        },
        type_table_id,
        plan,
        records: records.to_vec(),
        nominal_types,
        enum_types,
    })
}""",
    ),
    # 5. build_semantic_ir_program signature gains enum_types
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
) -> Result<crate::ir::semantic_ir::SemanticProgram> {
    use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode};
    use crate::semantics::builder::SemanticIRBuilder;

    let (program, diagnostics) =
        SemanticIRBuilder::build(functions, type_table_id, plan, records, nominal_types);""",
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
    ),
]


BUILD_IR_EDITS = [
    (
        """        match crate::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
            typed.nominal_types.clone(),
        ) {""",
        """        match crate::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
            typed.nominal_types.clone(),
            typed.enum_types.clone(),
        ) {""",
    ),
]


PIPELINE_EQ_EDITS = [
    (
        """        let direct = match algol26::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
            typed.nominal_types.clone(),
        ) {""",
        """        let direct = match algol26::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
            typed.nominal_types.clone(),
            typed.enum_types.clone(),
        ) {""",
    ),
]


BUILDER_MOD_EDITS = [
    # 1. build signature gains enum_types
    (
        """    pub fn build(
        functions: &[FunctionDecl],
        type_table_id: HashMap<ExprId, Type>,
        plan: InstantiationPlan,
        records: &[RecordDecl],
        nominal_types: HashMap<String, Type>,
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
    ) -> (SemanticProgram, Vec<String>) {
        let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();

        let mut builder = SemanticIRBuilder {""",
    ),
    # 2. init struct
    (
        """            plan,
            record_names,
            nominal_types,
        };
        let program = builder.build_impl(functions);""",
        """            plan,
            record_names,
            nominal_types,
            enum_types,
        };
        let program = builder.build_impl(functions);""",
    ),
    # 3. Add field to struct
    (
        """    /// Nominal type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Distinct` whose `NominalTypeId` matches
    /// the one the analyzer assigned (both iterate `distincts` in
    /// declaration order). See ADR 0029.
    pub(super) nominal_types: HashMap<String, Type>,
}""",
        """    /// Nominal type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Distinct` whose `NominalTypeId` matches
    /// the one the analyzer assigned. See ADR 0029.
    pub(super) nominal_types: HashMap<String, Type>,
    /// Enum type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Enum` whose `EnumTypeId` matches the
    /// one the analyzer assigned. See ADR 0030.
    pub(super) enum_types: HashMap<String, Type>,
}""",
    ),
    # 4. resolve_type_syntax: check enum_types before nominal_types
    (
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Type {
        match syntax {
            TypeSyntax::Named(name) => {
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
                }""",
    ),
    # 5. make_builder test helper: add field
    (
        """            plan: InstantiationPlan::default(),
            record_names: HashSet::new(),
            nominal_types: HashMap::new(),
        }
    }""",
        """            plan: InstantiationPlan::default(),
            record_names: HashSet::new(),
            nominal_types: HashMap::new(),
            enum_types: HashMap::new(),
        }
    }""",
    ),
]


def patch(path, edits, label_prefix=""):
    if not path.exists():
        print(f"ERROR: {path} not found.")
        raise SystemExit(1)
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


def main():
    patch(MOD, MOD_EDITS)
    patch(ITEMS, ITEMS_EDITS)
    patch(COMPILER, COMPILER_EDITS)
    patch(BUILD_IR, BUILD_IR_EDITS)
    patch(PIPELINE_EQ, PIPELINE_EQ_EDITS)
    patch(BUILDER_MOD, BUILDER_MOD_EDITS)
    print()
    print("NEXT: cargo build 2>&1 | head -40")


if __name__ == "__main__":
    main()

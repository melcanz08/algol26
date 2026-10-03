#!/usr/bin/env python3
"""
A6: single-source nominal identity + ADR 0029 cleanup.

- TypedProgram carries the analyzer's resolved nominal_types map
  (name -> Type::Distinct) instead of raw DistinctDecl slices.
  NominalTypeId is assigned exactly once, in the analyzer, and
  propagated.
- SemanticIRBuilder::build receives the resolved map, no
  reconstruction.
- Equality on Distinct values is rejected in the analyzer
  (ADR 0029 says equality requires a trait impl).
- is_hashable_key recurses through Distinct so `Map<UserId, V>` is
  valid when the base is a valid key.
- ADR status updated to Accepted.

Trait-registry id verification and WASM/interpreter Cast support
are deferred to A7.
"""

from pathlib import Path

ANALYZER_MOD = Path("src/semantics/analyzer/mod.rs")
COMPILER = Path("src/compiler.rs")
BUILDER_MOD = Path("src/semantics/builder/mod.rs")
EXPR = Path("src/semantics/analyzer/expr.rs")
BUILD_IR = Path("src/compiler/passes/build_ir.rs")
PIPELINE_EQ = Path("tests/compiler_pipeline_equiv.rs")
ADR = Path("docs/decisions/0029-nominal-types.md")


ANALYZER_MOD_EDITS = [
    # Add take_nominal_types next to take_instantiations.
    (
        """    /// Take ownership of the instantiation list so it can be handed
    /// to `TypedProgram`.
    pub fn take_instantiations(&mut self) -> Vec<Instantiation> {
        std::mem::take(&mut self.instantiations)
    }""",
        """    /// Take ownership of the instantiation list so it can be handed
    /// to `TypedProgram`.
    pub fn take_instantiations(&mut self) -> Vec<Instantiation> {
        std::mem::take(&mut self.instantiations)
    }

    /// Take ownership of the resolved nominal type table so it can be
    /// handed to `TypedProgram`. `NominalTypeId` is assigned by
    /// `register_nominal_types`; this is the single source of that
    /// identity. Downstream consumers receive the resolved map and
    /// never reconstruct ids. See ADR 0029.
    pub fn take_nominal_types(&mut self) -> HashMap<String, Type> {
        std::mem::take(&mut self.nominal_types)
    }""",
    ),
]


COMPILER_EDITS = [
    # 1. TypedProgram: replace the distincts field with the resolved map.
    (
        """    /// Record declarations from the frontend. Forwarded to the IR
    /// builder so it can resolve record names in user function
    /// signatures to `Type::Record(...)` rather than `Type::Unknown`.
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    /// Nominal type declarations from the frontend. Forwarded to the
    /// IR builder so `UserId` resolves to `Type::Distinct` in
    /// annotations and conversion intrinsics are recognized.
    /// See ADR 0029.
    pub distincts: Vec<crate::frontend::ast::DistinctDecl>,
}""",
        """    /// Record declarations from the frontend. Forwarded to the IR
    /// builder so it can resolve record names in user function
    /// signatures to `Type::Record(...)` rather than `Type::Unknown`.
    pub records: Vec<crate::frontend::ast::RecordDecl>,
    /// Resolved nominal type declarations, keyed by name. Values
    /// carry the `NominalTypeId` the analyzer assigned. Downstream
    /// consumers read this map and never reconstruct ids — the
    /// analyzer is the single source of nominal identity.
    /// See ADR 0029.
    pub nominal_types: std::collections::HashMap<String, crate::common::types::Type>,
}""",
    ),
    # 2. type_check_program: populate nominal_types from the analyzer.
    (
        """    let type_table_id = analyzer.take_type_table_id();
    let instantiations = analyzer.take_instantiations();
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
        distincts: distincts.to_vec(),
    })
}""",
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
    ),
    # 3. build_semantic_ir_program: signature + call.
    (
        """pub fn build_semantic_ir_program(
    functions: &[crate::frontend::ast::FunctionDecl],
    type_table_id: std::collections::HashMap<
        crate::frontend::ast::ExprId,
        crate::common::types::Type,
    >,
    plan: crate::ir::instantiation_plan::InstantiationPlan,
    records: &[crate::frontend::ast::RecordDecl],
    distincts: &[crate::frontend::ast::DistinctDecl],
) -> Result<crate::ir::semantic_ir::SemanticProgram> {
    use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode};
    use crate::semantics::builder::SemanticIRBuilder;

    let (program, diagnostics) =
        SemanticIRBuilder::build(functions, type_table_id, plan, records, distincts);""",
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
    ),
]


BUILDER_MOD_EDITS = [
    # 1. build signature + remove the reconstruction loop.
    (
        """    pub fn build(
        functions: &[FunctionDecl],
        type_table_id: HashMap<ExprId, Type>,
        plan: InstantiationPlan,
        records: &[RecordDecl],
        distincts: &[DistinctDecl],
    ) -> (SemanticProgram, Vec<String>) {
        let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();

        // Reconstruct the nominal type table. The analyzer iterates
        // `distincts` in declaration order and assigns ids 0..N; we
        // do the same so `Type::Distinct { id, .. }` values compare
        // equal across the two passes.
        let mut nominal_types: HashMap<String, Type> = HashMap::new();
        for (i, decl) in distincts.iter().enumerate() {
            let base = decl.base.to_type();
            let ty = Type::distinct(NominalTypeId(i as u32), &decl.name, base);
            nominal_types.insert(decl.name.clone(), ty);
        }

        let mut builder = SemanticIRBuilder {""",
        """    pub fn build(
        functions: &[FunctionDecl],
        type_table_id: HashMap<ExprId, Type>,
        plan: InstantiationPlan,
        records: &[RecordDecl],
        nominal_types: HashMap<String, Type>,
    ) -> (SemanticProgram, Vec<String>) {
        let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();

        let mut builder = SemanticIRBuilder {""",
    ),
    # 2. Drop the now-unused NominalTypeId import.
    (
        """use crate::common::types::NominalTypeId;
use crate::frontend::ast::{""",
        """use crate::frontend::ast::{""",
    ),
    # 3. Drop the DistinctDecl import (no longer referenced in mod.rs).
    (
        """use crate::frontend::ast::{
    BinOp, DistinctDecl, Expr, ExprId, ExprKind, FunctionDecl, MatchCaseExpr, Pattern, RecordDecl,
    Stmt, TypeSyntax,
};""",
        """use crate::frontend::ast::{
    BinOp, Expr, ExprId, ExprKind, FunctionDecl, MatchCaseExpr, Pattern, RecordDecl, Stmt,
    TypeSyntax,
};""",
    ),
]


EXPR_EDITS = [
    # is_hashable_key recursion.
    (
        """    /// True when `ty` is a legal `Map` key type — `Int`, `String`,
    /// `Bool`, or `Unknown` (not yet inferred). See ADR 0027.
    pub(super) fn is_hashable_key(ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::String | Type::Bool | Type::Unknown)
    }""",
        """    /// True when `ty` is a legal `Map` key type — `Int`, `String`,
    /// `Bool`, or `Unknown` (not yet inferred). See ADR 0027.
    ///
    /// ADR 0029: a nominal type is a valid key iff its base is.
    /// The identity carries through the map implementation, but
    /// the underlying representation is what hashes.
    pub(super) fn is_hashable_key(ty: &Type) -> bool {
        match ty {
            Type::Int | Type::String | Type::Bool | Type::Unknown => true,
            Type::Distinct { base, .. } => Self::is_hashable_key(base),
            _ => false,
        }
    }""",
    ),
    # Equality guard for nominal types.
    (
        """                    BinOp::Equal | BinOp::NotEqual => {
                        if left_type == right_type
                            || (left_type.is_numeric() && right_type.is_numeric())
                        {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Equality requires matching types, found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use matching types for equality comparison"))
                        }
                    }""",
        """                    BinOp::Equal | BinOp::NotEqual => {
                        // ADR 0029: nominal types do not auto-implement
                        // equality. Structural `PartialEq` would accept
                        // `a == b` when both sides have the same
                        // `NominalTypeId`, so the guard is explicit.
                        // Users opt in by writing a trait impl.
                        if matches!(left_type, Type::Distinct { .. })
                            || matches!(right_type, Type::Distinct { .. })
                        {
                            return Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Equality on nominal types requires a trait impl; \\
                                     found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion(
                                "Write an `impl Eq for T` (or the equivalent) to \\
                                 enable equality on this nominal type",
                            ));
                        }
                        if left_type == right_type
                            || (left_type.is_numeric() && right_type.is_numeric())
                        {
                            Ok(Type::Bool)
                        } else {
                            Err(CompileError::at(
                                self.current_span,
                                &format!(
                                    "Equality requires matching types, found {} and {}",
                                    left_type, right_type
                                ),
                                ErrorCode::E0002,
                            )
                            .with_suggestion("Use matching types for equality comparison"))
                        }
                    }""",
    ),
]


BUILD_IR_EDITS = [
    (
        """            &typed.records,
            &typed.distincts,
        ) {""",
        """            &typed.records,
            typed.nominal_types.clone(),
        ) {""",
    ),
]


PIPELINE_EQ_EDITS = [
    (
        """            &typed.records,
            &typed.distincts,
        ) {""",
        """            &typed.records,
            typed.nominal_types.clone(),
        ) {""",
    ),
]


ADR_EDITS = [
    (
        """## Status

Proposed. Not yet implemented.""",
        """## Status

Accepted. A1–A6 implemented; A7–A8 pending.

`NominalTypeId` is assigned once, in the analyzer's
`register_nominal_types`, and propagated through `TypedProgram`'s
`nominal_types` map to every downstream consumer. No consumer
reconstructs it — an earlier iteration of A5 had the IR builder
re-derive the id from the raw `DistinctDecl` slice, and that was
removed because it violated the "identity is created once"
principle. See the "Identity must participate everywhere" section.""",
    ),
]


def patch(path, edits, strict=True):
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
            if strict:
                raise SystemExit(1)
            continue
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")


def main():
    patch(ANALYZER_MOD, ANALYZER_MOD_EDITS)
    patch(COMPILER, COMPILER_EDITS)
    patch(BUILDER_MOD, BUILDER_MOD_EDITS)
    patch(EXPR, EXPR_EDITS)
    patch(BUILD_IR, BUILD_IR_EDITS)
    patch(PIPELINE_EQ, PIPELINE_EQ_EDITS)
    patch(ADR, ADR_EDITS)

    print()
    print("NEXT: update remaining test call sites via /tmp/fix_a6_tests.py")
    print("      cargo build 2>&1 | head -40")


if __name__ == "__main__":
    main()

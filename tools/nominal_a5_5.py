#!/usr/bin/env python3
"""
A5.5: IR-builder side of nominal type conversion intrinsics.

Threads `distincts` from the frontend through TypedProgram and
SemanticIRBuilder. The builder learns which names are nominal types,
resolves them in type annotations, and intercepts:

    T.from_base(x)   -> Cast { value: x, target_type: T }
    v.to_base()      -> Cast { value: v, target_type: base }
    v.to_base        -> same (FieldAccess form)

The Cast wrapper is how the IR carries the nominal type: the value
representation is identical to the base, so LLVM codegen handles the
cast as a no-op.
"""

from pathlib import Path

MOD = Path("src/semantics/builder/mod.rs")
EXPR = Path("src/semantics/builder/expr.rs")
COMPILER = Path("src/compiler.rs")
LLVM_VALUE = Path("src/backends/llvm_codegen/value.rs")
PIPELINE_EQ = Path("tests/compiler_pipeline_equiv.rs")

MOD_EDITS = [
    # 1. import DistinctDecl and NominalTypeId
    (
        """use crate::frontend::ast::{
    BinOp, Expr, ExprId, ExprKind, FunctionDecl, MatchCaseExpr, Pattern, RecordDecl, Stmt,
    TypeSyntax,
};""",
        """use crate::common::types::NominalTypeId;
use crate::frontend::ast::{
    BinOp, DistinctDecl, Expr, ExprId, ExprKind, FunctionDecl, MatchCaseExpr, Pattern, RecordDecl,
    Stmt, TypeSyntax,
};""",
    ),
    # 2. struct field
    (
        """    /// Names of record declarations from the frontend. Used to
    /// resolve record names in user function signatures when
    /// registering `function_types` — `Option<Sale>` becomes
    /// `Option<Record("Sale", []))>` rather than `Option<Unknown>`.
    pub(super) record_names: HashSet<String>,
}""",
        """    /// Names of record declarations from the frontend. Used to
    /// resolve record names in user function signatures when
    /// registering `function_types` — `Option<Sale>` becomes
    /// `Option<Record("Sale", []))>` rather than `Option<Unknown>`.
    pub(super) record_names: HashSet<String>,
    /// Nominal type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Distinct` whose `NominalTypeId` matches
    /// the one the analyzer assigned (both iterate `distincts` in
    /// declaration order). See ADR 0029.
    pub(super) nominal_types: HashMap<String, Type>,
}""",
    ),
    # 3. build signature
    (
        """    pub fn build(
        functions: &[FunctionDecl],
        type_table_id: HashMap<ExprId, Type>,
        plan: InstantiationPlan,
        records: &[RecordDecl],
    ) -> (SemanticProgram, Vec<String>) {
        let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();
        let mut builder = SemanticIRBuilder {""",
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
    ),
    # 4. init struct
    (
        """            plan,
            record_names,
        };
        let program = builder.build_impl(functions);""",
        """            plan,
            record_names,
            nominal_types,
        };
        let program = builder.build_impl(functions);""",
    ),
    # 5. resolve_type_syntax Named arm
    (
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Type {
        match syntax {
            TypeSyntax::Named(name) => {
                if self.record_names.contains(name.as_str()) {
                    return Type::record(name, Vec::new());
                }
                syntax.to_type()
            }""",
        """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Type {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(nominal) = self.nominal_types.get(name.as_str()) {
                    return nominal.clone();
                }
                if self.record_names.contains(name.as_str()) {
                    return Type::record(name, Vec::new());
                }
                syntax.to_type()
            }""",
    ),
    # 6. make_builder test helper — add field
    (
        """            plan: InstantiationPlan::default(),
            record_names: HashSet::new(),
        }
    }""",
        """            plan: InstantiationPlan::default(),
            record_names: HashSet::new(),
            nominal_types: HashMap::new(),
        }
    }""",
    ),
]

EXPR_EDITS = [
    # 1. from_base and to_base intercept in FunctionCall arm.
    (
        """            ExprKind::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                // ─── METHOD CALL DISAMBIGUATION ───""",
        """            ExprKind::FunctionCall { name, args, .. } => {
                let clean_name = name.trim_end_matches("()");

                // ADR 0029: nominal type conversion intrinsics.
                // Lower to a no-op Cast that carries the nominal type.
                if let Some(dot) = clean_name.find('.') {
                    let (receiver, method) = (&clean_name[..dot], &clean_name[dot + 1..]);

                    // T.from_base(x): x already has the base
                    // representation; wrap it with the nominal type.
                    if method == "from_base" {
                        if let Some(nominal) = self.nominal_types.get(receiver).cloned() {
                            let inner = if let Some(arg) = args.first() {
                                self.translate_expr(program, func, current_block, arg)
                            } else {
                                TypedIRValue::Void
                            };
                            return TypedIRValue::Cast {
                                value: Box::new(inner),
                                target_type: nominal,
                            };
                        }
                    }

                    // v.to_base(): the receiver's nominal type wraps
                    // its base at runtime; unwrap the type annotation.
                    if method == "to_base" && args.is_empty() {
                        if let Some(info) = self.lookup_var(receiver) {
                            if let Type::Distinct { base, .. } = &info.type_ {
                                let receiver_value = TypedIRValue::Variable(
                                    receiver.to_string(),
                                    info.type_.clone(),
                                );
                                let base_ty = (**base).clone();
                                return TypedIRValue::Cast {
                                    value: Box::new(receiver_value),
                                    target_type: base_ty,
                                };
                            }
                        }
                    }
                }

                // ─── METHOD CALL DISAMBIGUATION ───""",
    ),
    # 2. FieldAccess .to_base intercept.
    (
        """            ExprKind::FieldAccess { object, field, .. } => {
                let obj = self.translate_expr(program, func, current_block, object);
                let obj_ty = obj.type_of();
                // ─── Map zero-arg methods (ADR 0027) ───""",
        """            ExprKind::FieldAccess { object, field, .. } => {
                let obj = self.translate_expr(program, func, current_block, object);
                let obj_ty = obj.type_of();

                // ADR 0029: `v.to_base` (no-parens form). Same as the
                // parenthesized FunctionCall form above.
                if field == "to_base" {
                    if let Type::Distinct { base, .. } = &obj_ty {
                        let base_ty = (**base).clone();
                        return TypedIRValue::Cast {
                            value: Box::new(obj),
                            target_type: base_ty,
                        };
                    }
                }

                // ─── Map zero-arg methods (ADR 0027) ───""",
    ),
]

COMPILER_EDITS = [
    # 1. TypedProgram field
    (
        """    /// Record declarations from the frontend. Forwarded to the IR
    /// builder so it can resolve record names in user function
    /// signatures to `Type::Record(...)` rather than `Type::Unknown`.
    pub records: Vec<crate::frontend::ast::RecordDecl>,
}""",
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
    ),
    # 2. type_check_program return
    (
        """        type_table_id,
        plan,
        records: records.to_vec(),
    })
}""",
        """        type_table_id,
        plan,
        records: records.to_vec(),
        distincts: distincts.to_vec(),
    })
}""",
    ),
    # 3. build_semantic_ir_program signature
    (
        """pub fn build_semantic_ir_program(
    functions: &[crate::frontend::ast::FunctionDecl],
    type_table_id: std::collections::HashMap<
        crate::frontend::ast::ExprId,
        crate::common::types::Type,
    >,
    plan: crate::ir::instantiation_plan::InstantiationPlan,
    records: &[crate::frontend::ast::RecordDecl],
) -> Result<crate::ir::semantic_ir::SemanticProgram> {
    use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode};
    use crate::semantics::builder::SemanticIRBuilder;

    let (program, diagnostics) = SemanticIRBuilder::build(functions, type_table_id, plan, records);""",
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
    ),
]

LLVM_EDITS = [
    # Cast arm: add Distinct case before the catch-all.
    (
        """                    (BasicValueEnum::IntValue(_), Type::Int) => {
                        // Already an int - no cast needed
                        v
                    }
                    (source_llvm, target) => {""",
        """                    (BasicValueEnum::IntValue(_), Type::Int) => {
                        // Already an int - no cast needed
                        v
                    }
                    // ADR 0029: nominal wrap/unwrap. The nominal and
                    // its base share the same runtime representation,
                    // so the cast is a no-op. The inner value's LLVM
                    // type already matches the base.
                    (_, Type::Distinct { base, .. }) => match (&v, &**base) {
                        (BasicValueEnum::IntValue(_), Type::Int | Type::Bool) => v,
                        (BasicValueEnum::FloatValue(_), Type::Float) => v,
                        (BasicValueEnum::PointerValue(_), Type::String) => v,
                        _ => {
                            return Err(CompileError::unsupported_operation(
                                "cast to nominal type with mismatched LLVM representation",
                                "llvm",
                            ));
                        }
                    },
                    (source_llvm, target) => {""",
    ),
]

PIPELINE_EQ_EDITS = [
    # build_semantic_ir_program call needs &typed.distincts
    (
        """        let direct = match algol26::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
        ) {""",
        """        let direct = match algol26::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
            &typed.records,
            &typed.distincts,
        ) {""",
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
    patch(MOD, MOD_EDITS)
    patch(EXPR, EXPR_EDITS)
    patch(COMPILER, COMPILER_EDITS)
    patch(LLVM_VALUE, LLVM_EDITS)
    patch(PIPELINE_EQ, PIPELINE_EQ_EDITS)

    print()
    print("NEXT: cargo build 2>&1 | head -40")
    print("      Expect 1-2 call sites still needing `distincts` threaded")
    print("      (the BuildSemanticIRPass pass). Paste the errors.")


if __name__ == "__main__":
    main()

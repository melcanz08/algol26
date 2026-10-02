#!/usr/bin/env python3
"""
A3: analyzer registers nominal types and resolves them.

- SemanticAnalyzer gains nominal_types + next_nominal_id
- register_nominal_types assigns ids and validates the base is
  one of Int/Float/Bool/String (per ADR 0029)
- resolve_type_syntax checks nominal_types before records
- register_user_functions threads a nominals snapshot through
  resolve_syntax_with_records
- analyze_with_spans gains a `distincts` parameter; analyze_with_traits
  keeps its 4-arg signature and passes `&[]`
- type_check_program gains a `distincts` parameter
"""

from pathlib import Path

MOD = Path("src/semantics/analyzer/mod.rs")
ITEMS = Path("src/semantics/analyzer/items.rs")
COMPILER = Path("src/compiler.rs")

MOD_EDITS = [
    ("use crate::common::types::Type;",
     "use crate::common::types::{NominalTypeId, Type};"),
    ("""use crate::frontend::ast::{
    BinOp, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern, RecordDecl,
    Stmt, TraitDecl, WhereClause,
};""",
     """use crate::frontend::ast::{
    BinOp, DistinctDecl, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern,
    RecordDecl, Stmt, TraitDecl, WhereClause,
};"""),
    ("""    deferred_captures: Vec<HashSet<String>>,
    records: HashMap<String, RecordInfo>,""",
     """    deferred_captures: Vec<HashSet<String>>,
    records: HashMap<String, RecordInfo>,
    /// Nominal type declarations (`type X distinct Y`), keyed by the
    /// declared name. Values carry the assigned `NominalTypeId`.
    /// Identity is the id; the name is presentation only. ADR 0029.
    nominal_types: HashMap<String, Type>,
    /// Monotonic counter for `NominalTypeId`.
    next_nominal_id: u32,"""),
    ("""            loop_stack: Vec::new(),
            unsafe_depth: 0,
            records: HashMap::new(),
        }
    }""",
     """            loop_stack: Vec::new(),
            unsafe_depth: 0,
            records: HashMap::new(),
            nominal_types: HashMap::new(),
            next_nominal_id: 0,
        }
    }"""),
    ("""    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[])
    }""",
     """    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[], &[])
    }"""),
    ("""    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
    ) -> Result<()> {""",
     """    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
        distincts: &[DistinctDecl],
    ) -> Result<()> {"""),
    ("""        self.register_builtin_functions();

        // Records must be registered before user functions:""",
     """        self.register_builtin_functions();

        // Nominal types are registered before records and user
        // functions: a record field or a function signature may name
        // a nominal type. Base must be primitive (ADR 0029).
        self.register_nominal_types(distincts)?;

        // Records must be registered before user functions:"""),
    ("""        self.analyze_with_spans(functions, traits, impls, records)
    }""",
     """        // The 4-arg form is retained for callers that predate
        // nominal types. Callers with `DistinctDecl`s in scope
        // should call `analyze_with_spans` directly.
        self.analyze_with_spans(functions, traits, impls, records, &[])
    }"""),
]

ITEMS_EDITS = [
    ("""    pub(super) fn register_user_functions(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        // Snapshot the record table so the closure can read it
        // without conflicting with the `&mut self` of the loop.
        let records_snapshot = self.records.clone();""",
     """    /// Register every `type X distinct Y` declaration.
    ///
    /// Assigns a fresh `NominalTypeId` per declaration, in
    /// declaration order, so ids are deterministic for a given
    /// source set. Validates that the base is one of `Int`,
    /// `Float`, `Bool`, `String` — the v1 constraint from ADR
    /// 0029.
    pub(super) fn register_nominal_types(
        &mut self,
        decls: &[crate::frontend::ast::DistinctDecl],
    ) -> Result<()> {
        for decl in decls {
            if self.nominal_types.contains_key(&decl.name) {
                return Err(CompileError::at(
                    decl.span,
                    &format!("Duplicate nominal type '{}'", decl.name),
                    ErrorCode::E0009,
                ));
            }
            let base = self.resolve_type_syntax(&decl.base)?;
            if !matches!(
                base,
                Type::Int | Type::Float | Type::Bool | Type::String
            ) {
                return Err(CompileError::at(
                    decl.span,
                    &format!(
                        "Nominal type '{}' must have a primitive base \\
                         (Int, Float, Bool, or String), found {}",
                        decl.name, base
                    ),
                    ErrorCode::E0002,
                ));
            }
            let id = NominalTypeId(self.next_nominal_id);
            self.next_nominal_id += 1;
            let ty = Type::distinct(id, &decl.name, base);
            self.nominal_types.insert(decl.name.clone(), ty);
        }
        Ok(())
    }

    pub(super) fn register_user_functions(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        // Snapshot the record and nominal tables so the closure can
        // read them without conflicting with the `&mut self` of the
        // loop.
        let records_snapshot = self.records.clone();
        let nominals_snapshot = self.nominal_types.clone();"""),
    ("""                    let type_ = match t {
                        Some(ts) => Self::resolve_syntax_with_records(ts, &records_snapshot)?,
                        None => Type::Unknown,
                    };""",
     """                    let type_ = match t {
                        Some(ts) => Self::resolve_syntax_with_records(
                            ts,
                            &records_snapshot,
                            &nominals_snapshot,
                        )?,
                        None => Type::Unknown,
                    };"""),
    ("""            let return_type = match &func.return_type {
                Some(t) => Self::resolve_syntax_with_records(t, &records_snapshot)?,
                None => Type::Void,
            };""",
     """            let return_type = match &func.return_type {
                Some(t) => {
                    Self::resolve_syntax_with_records(t, &records_snapshot, &nominals_snapshot)?
                }
                None => Type::Void,
            };"""),
    ("""    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(rec) = self.records.get(name.as_str()).cloned() {""",
     """    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(nominal) = self.nominal_types.get(name.as_str()).cloned() {
                    return Ok(nominal);
                }
                if let Some(rec) = self.records.get(name.as_str()).cloned() {"""),
    ("""    fn resolve_syntax_with_records(
        syntax: &TypeSyntax,
        records: &HashMap<String, RecordInfo>,
    ) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(rec) = records.get(name.as_str()) {""",
     """    fn resolve_syntax_with_records(
        syntax: &TypeSyntax,
        records: &HashMap<String, RecordInfo>,
        nominals: &HashMap<String, Type>,
    ) -> Result<Type> {
        match syntax {
            TypeSyntax::Named(name) => {
                if let Some(nominal) = nominals.get(name.as_str()) {
                    return Ok(nominal.clone());
                }
                if let Some(rec) = records.get(name.as_str()) {"""),
    # Two recursive calls inside resolve_syntax_with_records
    ("""                        .map(|a| Self::resolve_syntax_with_records(a, records))""",
     """                        .map(|a| Self::resolve_syntax_with_records(a, records, nominals))"""),
    ("""                    .map(|a| Self::resolve_syntax_with_records(a, records))""",
     """                    .map(|a| Self::resolve_syntax_with_records(a, records, nominals))"""),
]

COMPILER_EDITS = [
    ("""pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    records: &[crate::frontend::ast::RecordDecl], // ← NEW
) -> Result<TypedProgram> {""",
     """pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    records: &[crate::frontend::ast::RecordDecl], // ← NEW
    distincts: &[crate::frontend::ast::DistinctDecl], // ← ADR 0029
) -> Result<TypedProgram> {"""),
    ("""    analyzer.analyze_with_traits(functions, traits, impls, records)?;""",
     """    analyzer.analyze_with_spans(functions, traits, impls, records, distincts)?;"""),
]

def patch(path, edits):
    src = path.read_text()
    for i, (old, new) in enumerate(edits, start=1):
        count = src.count(old)
        if count != 1:
            print(f"FAIL: {path} — edit {i} matched {count} times; expected 1.")
            print("-" * 60); print(old[:250]); print("-" * 60)
            raise SystemExit(1)
        src = src.replace(old, new, 1)
    path.write_text(src)
    print(f"OK: {path}")

def main():
    for p in (MOD, ITEMS, COMPILER):
        if not p.exists():
            print(f"ERROR: {p} not found. Run from the repo root.")
            raise SystemExit(1)
    patch(MOD, MOD_EDITS)
    patch(ITEMS, ITEMS_EDITS)
    patch(COMPILER, COMPILER_EDITS)
    print()
    print("NEXT: cargo build 2>&1 | head -40")

if __name__ == "__main__":
    main()

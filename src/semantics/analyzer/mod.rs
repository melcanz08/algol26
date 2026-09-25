// src/semantics/analyzer/mod.rs
//
// Ownership and borrow checking, type inference, trait bounds, and
// scope-based lifetime rules. Produces a type table keyed by AST node
// address, which the IR builder consumes to avoid re-inferring types.

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::span::Span;
use crate::common::types::Type;
use crate::frontend::ast::{
    BinOp, Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, MatchCaseExpr, Pattern, RecordDecl,
    Stmt, TraitDecl, WhereClause,
};
use crate::semantics::state::{BorrowKind, BorrowLifetime, SemanticState, VarState};
use crate::semantics::trait_registry::TraitRegistry;
use std::collections::{HashMap, HashSet};

mod expr;
mod items;
mod ownership;
mod scopes;
mod stmt;
#[cfg(test)]
mod tests;

// ─── Borrow-checking model ──────────────────────────────────────────────
//
// Borrow lifetimes are *lexical*: a borrow created inside scope S stays
// alive until S is popped. Ownership, borrow, and initialization state
// live in a single `SemanticState`, which is joined at every branch
// point (`if`, `match`, loops) via `SemanticState::join`. The analyzer
// does not maintain any parallel bookkeeping — `state` is the sole
// authority.
//
// Known limitation: NLL is not implemented, so a borrow lives until
// the end of its enclosing block, not until the last use of the
// reference. Programs that rely on NLL may be rejected conservatively.
// ────────────────────────────────────────────────────────────────────────
pub struct SemanticAnalyzer {
    scopes: Vec<HashMap<String, (Type, bool)>>,
    /// Variables whose value is statically known to be `null`. Only
    /// `val` bindings appear here: an immutable binding initialized to
    /// `null` cannot be reassigned, so it is permanently null. `var`
    /// bindings are excluded because they can be reassigned and the
    /// analyzer does not perform value-flow tracking.
    null_bindings: Vec<HashSet<String>>,
    in_mut_borrow: bool,
    functions: HashMap<String, FunctionInfo>,
    current_return_type: Option<Type>,
    list_lengths: Vec<HashMap<String, usize>>,
    list_values: Vec<HashMap<String, Vec<Expr>>>,
    type_params: Vec<HashMap<String, Type>>,
    type_constraints: Vec<HashMap<String, Vec<String>>>,
    trait_registry: TraitRegistry,
    deferred_captures: Vec<HashSet<String>>,
    records: HashMap<String, RecordInfo>,
    /// Function names declared variadic via `extern "C" ...(...)`.
    /// Used to relax the arity check from "exactly N" to "at
    /// least N" for those functions.
    variadic_functions: HashSet<String>,

    /// Inferred type of each expression, keyed by its stable `ExprId`.
    /// Written by `analyze_expr_with_context` on every expression the
    /// analyzer visits.
    pub type_table_id: HashMap<ExprId, Type>,
    /// Generic instantiation facts recorded during analysis. Stage
    /// 3.1 writes this list; no consumer reads it yet. See
    /// `Instantiation` and ADR 0013.
    instantiations: Vec<Instantiation>,
    // Single source of truth - unified with dataflow engine
    pub(crate) state: SemanticState,
    /// Span of the node currently being analyzed. Updated at the top
    /// of `analyze_expr_with_context` and `analyze_stmt`. Used by
    /// error sites that don't have direct access to the node.
    current_span: Span,
    /// Number of `region NAME` blocks currently open in the
    /// enclosing function. Incremented only in the `Stmt::RegionBlock`
    /// arm of `analyze_stmt`, matching the IR builder's emission of
    /// `Instruction::RegionEnter` / `RegionExit`. Distinct from
    /// `self.scopes.len()` because plain blocks (if-branches, loop
    /// bodies, unsafe blocks) also push a scope but do not open a
    /// region.
    region_depth: usize,
    /// Loop nesting stack. Each entry records the `region_depth` at
    /// the moment the loop was entered. `break` and `continue` are
    /// rejected if the current `region_depth` exceeds the innermost
    /// loop's entry depth — that shape would skip the region's
    /// `RegionExit` and leak its allocations until function return.
    loop_stack: Vec<LoopContext>,
    /// Unsafe-block depth. Zero outside any `unsafe { ... }`. The
    /// operations gated by ADR 0015 — raw pointer deref, `alloc`,
    /// `free` — are permitted only when this is greater than zero.
    /// A depth counter rather than a bool because unsafe blocks
    /// nest.
    unsafe_depth: usize,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    params: Vec<(String, Type)>,
    return_type: Type,
    /// Declared type parameter names, in declaration order. Empty
    /// for non-generic functions. Used by `ExprKind::FunctionCall`
    /// to record instantiation facts (Stage 3.1, ADR 0013).
    type_params: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RecordInfo {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, Type)>,
}

/// A recorded generic instantiation. Produced by the analyzer when
/// it resolves a call to a function with non-empty `type_params`;
/// consumed by the monomorphizer in Stage 3.2 and by the IR builder
/// in Stage 3.3. Keyed by the call site's stable `ExprId`.
///
/// See ADR 0013 (`docs/decisions/0013-executable-ir-generic-invariant.md`)
/// for the full design.
#[derive(Debug, Clone)]
pub struct Instantiation {
    /// The call expression where the instantiation occurs. The IR
    /// builder will look up specializations by this ID.
    pub call_site: ExprId,
    /// The name of the generic function being called.
    pub function: String,
    /// The function's declared type parameter names, in declaration
    /// order. `type_args[i]` binds `type_params[i]`. Needed so the
    /// specialization plan can carry `T -> Int`-style mappings, not
    /// just an ordered list of concrete types.
    pub type_params: Vec<String>,
    /// The concrete type arguments the analyzer inferred, in
    /// declaration order. May contain `Type::Unknown` if the
    /// analyzer could not determine an argument's type; the
    /// executable-IR verifier (Stage 3.4) is responsible for
    /// rejecting such cases.
    pub type_args: Vec<Type>,
}

#[derive(Debug, Clone, Copy)]
struct LoopContext {
    region_depth_at_entry: usize,
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            null_bindings: vec![HashSet::new()],
            in_mut_borrow: false,
            current_return_type: None,
            list_lengths: vec![HashMap::new()],
            list_values: vec![HashMap::new()],
            type_params: vec![HashMap::new()],
            type_constraints: vec![HashMap::new()],
            trait_registry: TraitRegistry::new(),
            deferred_captures: vec![HashSet::new()],
            variadic_functions: HashSet::new(),
            type_table_id: HashMap::new(),
            instantiations: Vec::new(),
            state: SemanticState::new(),
            current_span: Span::default(),
            region_depth: 0,
            loop_stack: Vec::new(),
            unsafe_depth: 0,
            records: HashMap::new(),
        }
    }

    /// Analyze `f` on a snapshot of the current state; restore the
    /// entry state on return and hand back the branch's exit state.
    /// The caller is responsible for joining branch exits.
    ///
    /// Panics inside `f` leave `self.state` forked, not restored.
    /// The analyzer aborts on error anyway, so this is acceptable;
    /// add a guard here if that ever changes.
    fn in_branch<F, T>(&mut self, f: F) -> (T, SemanticState)
    where
        F: FnOnce(&mut Self) -> T,
    {
        let entry = self.state.fork();
        let result = f(self);
        let exit = std::mem::replace(&mut self.state, entry);
        (result, exit)
    }

    // ─── UNIFY TYPES ───────────────────────────────────────────────────────
    /// Look up the inferred type of an expression by its `ExprId`.
    pub fn type_of(&self, expr: &Expr) -> Option<&Type> {
        self.type_table_id.get(&expr.id)
    }
    /// Take ownership of the type table so it can be handed to the IR builder.
    pub fn take_type_table_id(&mut self) -> HashMap<ExprId, Type> {
        std::mem::take(&mut self.type_table_id)
    }
    /// Read-only view of the recorded generic instantiations.
    pub fn instantiations(&self) -> &[Instantiation] {
        &self.instantiations
    }
    /// Take ownership of the instantiation list so it can be handed
    /// to `TypedProgram`.
    pub fn take_instantiations(&mut self) -> Vec<Instantiation> {
        std::mem::take(&mut self.instantiations)
    }
    /// Access unified state (for dataflow integration)
    pub fn state(&self) -> &SemanticState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut SemanticState {
        &mut self.state
    }

    fn lookup_list_length(&self, name: &str) -> Option<usize> {
        for scope in self.list_lengths.iter().rev() {
            if let Some(len) = scope.get(name) {
                return Some(*len);
            }
        }
        None
    }

    fn declare_list_length(&mut self, name: &str, len: usize) {
        if let Some(scope) = self.list_lengths.last_mut() {
            scope.insert(name.to_string(), len);
        }
    }

    fn lookup_list_values(&self, name: &str) -> Option<Vec<Expr>> {
        for scope in self.list_values.iter().rev() {
            if let Some(vals) = scope.get(name) {
                return Some(vals.clone());
            }
        }
        None
    }

    fn declare_list_values(&mut self, name: &str, vals: Vec<Expr>) {
        if let Some(scope) = self.list_values.last_mut() {
            scope.insert(name.to_string(), vals);
        }
    }

    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Result<()> {
        self.analyze_with_spans(functions, &[], &[], &[])
    }

    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
    ) -> Result<()> {
        debug_assert!(
            crate::compiler::assert_all_numbered(functions),
            "SemanticAnalyzer::analyze_with_spans called with unnumbered AST — \
             call assign_expr_ids(&mut functions) before analyzing"
        );
        self.register_builtin_functions();
        self.register_user_functions(functions);

        for rec in records {
            self.register_record(rec)?;
        }
        for trait_decl in traits {
            self.trait_registry.register_trait(trait_decl.clone());
        }
        for impl_block in impls {
            self.trait_registry.register_impl(impl_block.clone());
        }
        for impl_block in impls {
            if let Err(err) = self.trait_registry.validate_impl(impl_block) {
                return Err(CompileError::simple(&err, 0, 0, "", ErrorCode::E0002));
            }
        }
        for func in functions {
            self.analyze_function(func)?;
        }
        Ok(())
    }

    // Keep the old name for compatibility; delegate.
    pub fn analyze_with_traits(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        records: &[RecordDecl],
    ) -> Result<()> {
        debug_assert!(
            crate::compiler::assert_all_numbered(functions),
            "SemanticAnalyzer::analyze_with_traits called with unnumbered AST — \
             call assign_expr_ids(&mut functions) before analyzing"
        );
        self.analyze_with_spans(functions, traits, impls, records)
    }

    fn check_trait_bounds(
        &self,
        _type_params: &[String],
        where_clauses: &[WhereClause],
    ) -> Result<()> {
        for clause in where_clauses {
            let trait_name = &clause.trait_name;
            if !self.trait_registry.trait_exists(trait_name) {
                return Err(CompileError::simple(
                    &format!("Unknown trait '{}'", trait_name),
                    0,
                    0,
                    "",
                    ErrorCode::E0004,
                )
                .with_suggestion(&format!(
                    "Define trait '{}' before using it as a constraint",
                    trait_name
                )));
            }
        }
        Ok(())
    }

    fn resolve_trait_method(&self, type_: &Type, method_name: &str) -> Option<FunctionDecl> {
        self.trait_registry
            .resolve_method(type_, method_name)
            .cloned()
    }
}

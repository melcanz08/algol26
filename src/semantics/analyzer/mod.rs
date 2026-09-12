// src/semantics/semantic.rs
//
// Ownership and borrow checking, type inference, trait bounds, and
// scope-based lifetime rules. Produces a type table keyed by AST node
// address, which the IR builder consumes to avoid re-inferring types.

//! Loop ownership analysis.
//!
//! Both `for` and `while` bodies are analyzed once, even though they
//! execute repeatedly. The analyzer therefore has to reason about what
//! invariants hold across iterations and what state is visible *after*
//! the loop.
//!
//! Borrows (immutable and mutable) are always restored to the state
//! that existed before the loop began. A borrow created inside the
//! loop dies with the loop body's scope — the same rule that applies
//! to any nested block.
//!
//! Moves are treated differently in the two loop forms because the
//! two forms have different iteration semantics:
//!
//! `for x in <iterable>`:
//!   The iterable is a list of known or unknown length. If it is
//!   non-empty (which the compiler cannot always rule out), the body
//!   executes at least once. If the body moves a non-Copy variable,
//!   the next iteration would re-execute the move on an already-moved
//!   value — a use-after-move error the compiler can prove will occur.
//!   So the move is rejected outright: "Cannot move 'x' in loop body".
//!
//! `while cond`:
//!   The condition may be false on entry, in which case the body never
//!   runs and no move occurs. The compiler cannot decide at compile
//!   time whether the loop runs, so it cannot prove the move is always
//!   a problem, nor that it never is. The conservative sound choice is
//!   to mark any variable moved in the body as "potentially moved" in
//!   the enclosing scope: any subsequent use of that variable errors
//!   (because it might have been moved), but the loop itself is
//!   accepted.
//!
//! In short: `for` rejects unconditionally (if the move happens on
//! iteration 1, it happens again on iteration 2); `while` propagates
//! the uncertainty to the caller's scope.
//!
//! Both choices are conservative — they reject some programs that a
//! more precise analysis would accept (e.g. a `for` loop over a list
//! of statically known length 1 that moves its element). Neither
//! choice is unsound: no program that would cause a runtime
//! use-after-move is accepted.

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::frontend::ast::{
    BinOp, Expr, FunctionDecl, ImplBlock, Pattern, Stmt, TraitDecl, WhereClause,
};
use crate::semantics::trait_registry::TraitRegistry;
use std::collections::{HashMap, HashSet};

mod scopes;
mod ownership;
mod items;
mod stmt;
mod expr;
#[cfg(test)]
mod tests;

// ─── Borrow-checking model ──────────────────────────────────────────────
//
// Borrow lifetimes are *lexical*. A borrow of `x` created inside scope S
// stays alive until S is popped. This is the same model Rust used before
// NLL (Non-Lexical Lifetimes) landed.
//
// Bookkeeping lives in four parallel vectors, each one an entry per scope
// on the scope stack:
//
//   borrowed_vars      — immutable borrows of a variable, per scope
//   mutably_borrowed   — mutable borrows of a variable, per scope
//   mutable_borrows    — reference-name → source-name, per scope
//   moved_vars         — variables whose ownership was transferred, per scope
//
// Because each scope has its own entry, `pop_scope` releases all borrows
// introduced in that scope automatically — no explicit cleanup needed.
//
// Known limitation: NLL is not implemented, so a borrow lives until the
// end of its enclosing block, not until the last use of the reference.
// Programs that rely on NLL may be rejected conservatively.
// ────────────────────────────────────────────────────────────────────────
pub struct SemanticAnalyzer {
    span_map: std::collections::HashMap<usize, (usize, usize)>,
    scopes: Vec<HashMap<String, (Type, bool)>>,
    moved_vars: Vec<Vec<String>>,
    borrowed_vars: Vec<HashSet<String>>,
    mutably_borrowed: Vec<HashSet<String>>,
    /// Variables whose value is statically known to be `null`. Only
    /// `val` bindings appear here: an immutable binding initialized to
    /// `null` cannot be reassigned, so it is permanently null. `var`
    /// bindings are excluded because they can be reassigned and the
    /// analyzer does not perform value-flow tracking.
    null_bindings: Vec<HashSet<String>>,
    in_mut_borrow: bool,
    mutable_borrows: Vec<HashMap<String, String>>,
    functions: HashMap<String, FunctionInfo>,
    current_return_type: Option<Type>,
    list_lengths: Vec<HashMap<String, usize>>,
    list_values: Vec<HashMap<String, Vec<Expr>>>,
    type_params: Vec<HashMap<String, Type>>,
    type_constraints: Vec<HashMap<String, Vec<String>>>,
    trait_registry: TraitRegistry,
    deferred_captures: Vec<HashSet<String>>,

    // ─── UNIFY TYPES ─── New: inferred type of each expression, keyed by address.
    // Addresses are stable because the analyzer and IR builder walk the *same*
    // AST without cloning.
    pub type_table: HashMap<usize, Type>,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    params: Vec<(String, Type)>,
    return_type: Type,
}

impl Default for SemanticAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        SemanticAnalyzer {
            span_map: std::collections::HashMap::new(),
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            moved_vars: vec![Vec::new()],
            borrowed_vars: vec![HashSet::new()],
            mutably_borrowed: vec![HashSet::new()],
            in_mut_borrow: false,
            mutable_borrows: vec![HashMap::new()],
            current_return_type: None,
            list_lengths: vec![HashMap::new()],
            list_values: vec![HashMap::new()],
            type_params: vec![HashMap::new()],
            type_constraints: vec![HashMap::new()],
            trait_registry: TraitRegistry::new(),
            deferred_captures: vec![HashSet::new()],
            null_bindings: vec![HashSet::new()],
            // ─── UNIFY TYPES ───
            type_table: HashMap::new(),
        }
    }

    // ─── UNIFY TYPES ───────────────────────────────────────────────────────
    /// Look up the inferred type of an expression by its address.
    pub fn type_of(&self, expr: &Expr) -> Option<&Type> {
        self.type_table.get(&(expr as *const Expr as usize))
    }
    /// Take ownership of the type table so it can be handed to the IR builder.
    pub fn take_type_table(&mut self) -> HashMap<usize, Type> {
        std::mem::take(&mut self.type_table)
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
        self.analyze_with_spans(functions, &[], &[], &std::collections::HashMap::new())
    }

    pub fn analyze_with_spans(
        &mut self,
        functions: &[FunctionDecl],
        traits: &[TraitDecl],
        impls: &[ImplBlock],
        span_map: &std::collections::HashMap<usize, (usize, usize)>,
    ) -> Result<()> {
        self.span_map = span_map.clone();
        self.register_builtin_functions();
        self.register_user_functions(functions);

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
        span_map: &std::collections::HashMap<usize, (usize, usize)>,
    ) -> Result<()> {
        self.analyze_with_spans(functions, traits, impls, span_map)
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
                    0, 0, "", ErrorCode::E0004,
                ).with_suggestion(&format!(
                    "Define trait '{}' before using it as a constraint", trait_name
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


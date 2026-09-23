#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_imports)]
#![allow(unused_assignments)]

// src/semantics/builder/mod.rs

use crate::common::span::Span;
use crate::common::types::Type;
use crate::frontend::ast::{
    BinOp, Expr, ExprId, ExprKind, FunctionDecl, MatchCaseExpr, Pattern, Stmt,
};
use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticInstruction,
    SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};
use crate::semantics::flow_result::{CaptureMode, DeferContext, FlowResult, LoopContext};
use std::borrow::Cow;
use std::collections::HashMap;

mod blocks;
mod build;
mod control_flow;
mod expr;
mod values;

#[derive(Debug, Clone)]
pub(super) struct VariableInfo {
    pub type_: Type,
    pub mutable: bool,
    /// Capture mode for closures/spawns. Currently written by
    /// `declare_var` but not yet consumed; will be used when
    /// escape analysis lands.
    #[allow(dead_code)]
    pub capture_mode: Option<CaptureMode>,
}

pub struct SemanticIRBuilder {
    pub(super) scopes: Vec<HashMap<String, VariableInfo>>,
    pub(super) function_types: HashMap<String, FunctionSignature>,
    pub(super) iter_counter: usize,
    pub diagnostics: Vec<String>, // already pub, stays
    pub(super) loop_stack: Vec<LoopContext>,
    pub(super) defer_stack: Vec<DeferContext>,
    pub(super) list_values: HashMap<String, Vec<Expr>>,
    pub(super) pending_merge: Option<usize>,
    pub(super) type_table_id: HashMap<ExprId, Type>,
    /// Type parameter bindings for the specialization currently
    /// being emitted. Empty when lowering a non-generic function.
    /// Set by Stage 3.2d when emitting each `SemanticFunction` for a
    /// specialization; read by `type_of_expr` to substitute `T` in
    /// the analyzer's recorded types under the correct environment.
    pub(super) current_subst: HashMap<String, Type>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(super) struct FunctionSignature {
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
}

impl SemanticIRBuilder {
    pub fn build(
        functions: &[FunctionDecl],
        type_table_id: HashMap<ExprId, Type>,
    ) -> (SemanticProgram, Vec<String>) {
        let mut builder = SemanticIRBuilder {
            scopes: vec![HashMap::new()],
            function_types: HashMap::new(),
            iter_counter: 0,
            diagnostics: Vec::new(),
            loop_stack: Vec::new(),
            defer_stack: Vec::new(),
            list_values: HashMap::new(),
            pending_merge: None,
            type_table_id,
            current_subst: HashMap::new(),
        };
        let program = builder.build_impl(functions);
        (program, builder.diagnostics)
    }
    /// Base name of a type, ignoring generic arguments: `List<Float>` → `"List"`.
    fn base_type_name(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Int => Some("Int"),
            Type::Float => Some("Float"),
            Type::String => Some("String"),
            Type::Bool => Some("Bool"),
            Type::Void => Some("Void"),
            Type::List(_) => Some("List"),
            Type::Option(_) => Some("Option"),
            Type::Result { .. } => Some("Result"),
            Type::Channel(_) => Some("Channel"),
            Type::Pointer(_) => Some("Pointer"),
            Type::Borrow(_) => Some("Borrow"),
            Type::MutBorrow(_) => Some("MutBorrow"),
            Type::Ptr => Some("Ptr"),
            _ => None,
        }
    }
    // ─── UNIFY TYPES ─── Lookup helper.
    //
    // Returns an owned `Type` because the substitution may construct
    // a new type even when the analyzer's table holds a `TypeVar`.
    // The substitution is a no-op when `current_subst` is empty,
    // which is the case for every non-generic function.
    pub(super) fn type_of_expr(&self, expr: &Expr) -> Option<Type> {
        self.type_table_id
            .get(&expr.id)
            .map(|ty| ty.substitute(&self.current_subst))
    }
    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn pop_scope(&mut self) {
        assert!(self.scopes.len() > 1);
        self.scopes.pop();
    }
    fn declare_var(&mut self, name: &str, type_: Type, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) {
                self.diagnostics.push(format!(
                    "Variable '{}' is already declared in this scope",
                    name
                ));
            } else {
                scope.insert(
                    name.to_string(),
                    VariableInfo {
                        type_,
                        mutable,
                        capture_mode: None,
                    },
                );
            }
        }
    }
    fn lookup_var(&self, name: &str) -> Option<&VariableInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }
    fn is_terminated(block: &SemanticBlock) -> bool {
        block.terminator.is_some()
    }
}

#[cfg(test)]
mod substitution_tests {
    use super::*;
    use crate::common::types::Type;
    use crate::frontend::ast::{Expr, ExprId, ExprKind};

    /// Build a blank builder with a single expression's type in
    /// the table, and a substitution environment.
    fn make_builder(
        expr_id: ExprId,
        recorded_type: Type,
        subst: HashMap<String, Type>,
    ) -> SemanticIRBuilder {
        let mut type_table_id = HashMap::new();
        type_table_id.insert(expr_id, recorded_type);
        SemanticIRBuilder {
            scopes: vec![HashMap::new()],
            function_types: HashMap::new(),
            iter_counter: 0,
            diagnostics: Vec::new(),
            loop_stack: Vec::new(),
            defer_stack: Vec::new(),
            list_values: HashMap::new(),
            pending_merge: None,
            type_table_id,
            current_subst: subst,
        }
    }

    #[test]
    fn type_of_expr_returns_recorded_type_when_subst_empty() {
        let expr = Expr::new(ExprKind::Int(0, Span::default()));
        let builder = make_builder(expr.id, Type::Int, HashMap::new());
        assert_eq!(builder.type_of_expr(&expr), Some(Type::Int));
    }

    #[test]
    fn type_of_expr_substitutes_type_var_under_non_empty_env() {
        let expr = Expr::new(ExprKind::Int(0, Span::default()));
        let mut subst = HashMap::new();
        subst.insert("T".to_string(), Type::Int);
        let builder = make_builder(expr.id, Type::TypeVar("T".to_string()), subst);
        assert_eq!(builder.type_of_expr(&expr), Some(Type::Int));
    }

    #[test]
    fn type_of_expr_substitutes_nested_type_var() {
        let expr = Expr::new(ExprKind::Int(0, Span::default()));
        let mut subst = HashMap::new();
        subst.insert("T".to_string(), Type::Float);
        let builder = make_builder(expr.id, Type::list(Type::TypeVar("T".to_string())), subst);
        assert_eq!(builder.type_of_expr(&expr), Some(Type::list(Type::Float)));
    }

    #[test]
    fn type_of_expr_returns_none_for_unknown_expr() {
        let expr = Expr::new(ExprKind::Int(0, Span::default()));
        let builder = make_builder(ExprId(9999), Type::Int, HashMap::new());
        assert_eq!(builder.type_of_expr(&expr), None);
    }
}

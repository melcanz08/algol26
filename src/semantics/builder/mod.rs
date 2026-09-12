#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_imports)]
#![allow(unused_assignments)]

// src/semantics/builder/mod.rs

use std::borrow::Cow;
use crate::common::span::Span;
use crate::common::types::Type;
use crate::frontend::ast::Pattern;
use crate::frontend::ast::{BinOp, Expr, FunctionDecl, MatchCaseExpr, Stmt};
use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticInstruction,
    SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};
use crate::semantics::control_flow::ControlFlowTranslator;
use crate::semantics::flow_analyzer::FlowAnalyzer;
use crate::semantics::flow_result::{
    CaptureMode, DeferContext, FlowResult, LoopContext, TerminatorKind,
};
use std::collections::HashMap;

mod build;
mod blocks;
mod values;
mod control_flow;
mod expr;

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
    pub diagnostics: Vec<String>,        // already pub, stays
    pub(super) loop_stack: Vec<LoopContext>,
    pub(super) defer_stack: Vec<DeferContext>,
    pub(super) list_values: HashMap<String, Vec<Expr>>,
    pub(super) pending_merge: Option<usize>,
    pub(super) type_table: HashMap<usize, Type>,
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
        type_table: HashMap<usize, Type>,
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
            type_table, // ─── UNIFY TYPES ───
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
    fn type_of_expr(&self, expr: &Expr) -> Option<&Type> {
        self.type_table.get(&(expr as *const Expr as usize))
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
        FlowAnalyzer::is_terminated(block)
    }   
}

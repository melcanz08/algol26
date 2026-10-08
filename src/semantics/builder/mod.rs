#![allow(unused_variables)]
#![allow(unused_mut)]
#![allow(unused_imports)]
#![allow(unused_assignments)]

// src/semantics/builder/mod.rs

use crate::common::span::Span;
use crate::common::types::Type;
use crate::frontend::ast::{
    BinOp, Expr, ExprId, ExprKind, FunctionDecl, MatchCaseExpr, Pattern, RecordDecl, Stmt,
    TypeSyntax,
};
use crate::ir::instantiation_plan::{InstantiationPlan, Specialization};
use crate::ir::semantic_ir::{
    Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticInstruction,
    SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};
use crate::semantics::analyzer::VirtualCallInfo;
use crate::semantics::flow_result::{DeferContext, FlowResult, LoopContext};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

mod blocks;
mod build;
mod control_flow;
mod expr;
mod values;

#[derive(Debug, Clone)]
pub(super) struct VariableInfo {
    pub type_: Type,
    pub mutable: bool,
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
    /// The specialization plan produced by the analyzer. Read by
    /// `build_impl` to emit one `SemanticFunction` per concrete
    /// specialization, and by `resolved_callee_name` to rewrite
    /// generic call sites to their mangled symbols. Stage 3.2d.
    pub(super) plan: InstantiationPlan,
    /// Names of record declarations from the frontend. Used to
    /// resolve record names in user function signatures when
    /// registering `function_types` — `Option<Sale>` becomes
    /// `Option<Record("Sale", []))>` rather than `Option<Unknown>`.
    pub(super) record_names: HashSet<String>,
    pub(super) records: Vec<RecordDecl>,
    /// Nominal type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Distinct` whose `NominalTypeId` matches
    /// the one the analyzer assigned. See ADR 0029.
    pub(super) nominal_types: HashMap<String, Type>,
    /// Enum type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Enum` whose `EnumTypeId` matches the
    /// one the analyzer assigned. See ADR 0030.
    pub(super) enum_types: HashMap<String, Type>,
    /// Subrange type declarations from the frontend, keyed by name.
    /// Values carry a `Type::Subrange` whose `SubrangeTypeId`
    /// matches the one the analyzer assigned. See ADR 0031.
    pub(super) subrange_types: HashMap<String, Type>,
    /// Associated constants from the analyzer, keyed by `"Type::NAME"`.
    /// Read by the Var arm of `translate_expr` to inline the value at
    /// each use site.
    pub(super) const_values: HashMap<String, (Type, Expr)>,
    /// ADR 0038 D4b. Call sites the analyzer resolved through a
    /// `&dyn Trait` receiver, keyed by the call expression's
    /// `ExprId`. Consulted by the FunctionCall arm of `translate_expr`
    /// to produce `TypedIRValue::VirtualCall` instead of the concrete
    /// `TypedIRValue::Call`.
    pub(super) virtual_calls: HashMap<ExprId, VirtualCallInfo>,
    /// ADR 0038 D6. Trait-name -> `TraitId` assignments, forwarded
    /// from the analyzer. Consulted by `resolve_type_syntax` to
    /// reconstruct `Type::DynTrait` from a bare annotation.
    pub(super) trait_ids: HashMap<String, crate::common::types::TraitId>,
    /// ADR 0041. Associated-type bindings, forwarded from the
    /// analyzer. Consulted by `normalize_assoc` after substituting
    /// a projection's base type variable.
    pub(super) assoc_bindings: HashMap<(String, String), HashMap<String, Type>>,
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
        plan: InstantiationPlan,
        records: &[RecordDecl],
        nominal_types: HashMap<String, Type>,
        enum_types: HashMap<String, Type>,
        subrange_types: HashMap<String, Type>,
        const_values: HashMap<String, (Type, Expr)>,
        virtual_calls: HashMap<ExprId, VirtualCallInfo>,
        trait_ids: HashMap<String, crate::common::types::TraitId>,
        assoc_bindings: HashMap<(String, String), HashMap<String, Type>>,
    ) -> (SemanticProgram, Vec<String>) {
        let record_names: HashSet<String> = records.iter().map(|r| r.name.clone()).collect();

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
            plan,
            record_names,
            records: records.to_vec(),
            nominal_types,
            enum_types,
            subrange_types,
            const_values,
            virtual_calls,
            trait_ids,
            assoc_bindings,
        };
        let program = builder.build_impl(functions);
        (program, builder.diagnostics)
    }
    /// Resolve a syntactic callee name to the name that should
    /// actually be emitted. If the call site has a plan entry, this
    /// substitutes the enclosing specialization's bindings into the
    /// recorded type arguments and looks up the specialization's
    /// mangled name. Diagnostics are pushed (rather than silently
    /// falling back) when the call is generic but the plan cannot
    /// resolve it — the call-site invariant from ADR 0013.
    pub(super) fn resolved_callee_name(&mut self, expr: &Expr, fallback: &str) -> String {
        let Some(csi) = self.plan.call_sites.get(&expr.id).cloned() else {
            return fallback.to_string();
        };

        let concrete_args: Vec<Type> = csi
            .type_args
            .iter()
            .map(|t| t.substitute(&self.current_subst))
            .collect();

        if concrete_args.iter().any(|t| t.contains_unresolved()) {
            self.diagnostics.push(format!(
                "Generic call to `{}` still has unresolved type arguments \
                 after substitution; transitive specialization discovery \
                 is not yet implemented (Stage 3.2e)",
                csi.function,
            ));
            return fallback.to_string();
        }

        match self.plan.specialization_for(&csi.function, &concrete_args) {
            Some(spec) => spec.mangled_name.clone(),
            None => {
                self.diagnostics.push(format!(
                    "Generic call to `{}` has no matching specialization in the plan; \
                     the analyzer did not record this instantiation",
                    csi.function,
                ));
                fallback.to_string()
            }
        }
    }
    /// Resolve a type annotation in the builder's context. Like
    /// the analyzer's `resolve_type_syntax`, but without the
    /// "unknown type is an error" check — the analyzer has already
    /// validated names, so the builder only reproduces the
    /// resolved type.
    pub(super) fn resolve_type_syntax(&self, syntax: &TypeSyntax) -> Type {
        match syntax {
            // ADR 0038 D6. Reconstruct `Type::DynTrait` from the
            // forwarded `TraitId` map. The analyzer has already
            // validated that the trait exists and is object-safe;
            // the builder only needs the id.
            TypeSyntax::DynTrait { name } => {
                if let Some(id) = self.trait_ids.get(name).copied() {
                    Type::dyn_trait(id, name)
                } else {
                    // Trait not in the map means the analyzer's
                    // registry didn't see it — a pipeline bug, not
                    // user error. Fall back to `Unknown` so the
                    // pipeline still produces diagnostic-bearing IR
                    // instead of panicking.
                    Type::Unknown
                }
            }
            TypeSyntax::Projection { base, name: _ } => {
                // ADR 0041. The builder re-resolves type syntax from
                // the raw AST; it has no bound-trait information and
                // cannot construct `Type::Associated`. Return
                // `Unknown` — the specialization pass overwrites
                // the concrete return type for each monomorphization,
                // the same way it does for a bare `TypeVar`.
                let _ = self.resolve_type_syntax(base);
                Type::Unknown
            }
            TypeSyntax::Named(name) => {
                if let Some(enum_ty) = self.enum_types.get(name.as_str()) {
                    return enum_ty.clone();
                }
                if let Some(nominal) = self.nominal_types.get(name.as_str()) {
                    return nominal.clone();
                }
                if let Some(subrange) = self.subrange_types.get(name.as_str()) {
                    return subrange.clone();
                }
                if self.record_names.contains(name.as_str()) {
                    return Type::record(name, Vec::new());
                }
                syntax.to_type()
            }
            TypeSyntax::Generic { name, args } => {
                let resolved_args: Vec<Type> =
                    args.iter().map(|a| self.resolve_type_syntax(a)).collect();
                if self.record_names.contains(name.as_str()) {
                    return Type::record(name, resolved_args);
                }
                match (name.to_lowercase().as_str(), resolved_args.as_slice()) {
                    ("list", [inner]) => Type::list(inner.clone()),
                    ("option", [inner]) => Type::option(inner.clone()),
                    ("result", [ok, err]) => Type::result(ok.clone(), err.clone()),
                    ("pointer", [inner]) | ("ptr", [inner]) => Type::pointer(inner.clone()),
                    ("borrow", [inner]) => Type::borrow(inner.clone()),
                    ("mutborrow", [inner]) | ("mut_borrow", [inner]) => {
                        Type::mut_borrow(inner.clone())
                    }
                    ("channel", [inner]) => Type::channel(inner.clone()),
                    ("map", [k, v]) => Type::map(k.clone(), v.clone()),
                    ("set", [inner]) => Type::set(inner.clone()),
                    _ => syntax.to_type(),
                }
            }
            TypeSyntax::Unknown => Type::Unknown,
        }
    }
    /// Base name of a type, ignoring generic arguments: `List<Float>` → `"List"`.
    pub(crate) fn base_type_name(ty: &Type) -> Option<&'static str> {
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
        self.type_table_id.get(&expr.id).map(|ty| {
            // ADR 0041. Substitute the current specialization's
            // type arguments first (which may make a projection's
            // base concrete), then normalize the result against
            // the impl's associated-type bindings.
            let substituted = ty.substitute(&self.current_subst);
            self.normalize_assoc(&substituted)
        })
    }

    /// ADR 0041. Replace any `Type::Associated` with a concrete base
    /// by the impl's binding. Symbolic bases stay as projections so
    /// a later substitution pass can resolve them. Mirrors the
    /// analyzer's method, reading from `self.assoc_bindings` (which
    /// was forwarded through `build`) instead of the analyzer's
    /// trait registry.
    pub(super) fn normalize_assoc(&self, ty: &Type) -> Type {
        match ty {
            Type::Associated {
                base,
                trait_name,
                assoc_name,
            } => {
                let base_norm = self.normalize_assoc(base);
                if base_norm.contains_unresolved() {
                    return Type::Associated {
                        base: Box::new(base_norm),
                        trait_name: trait_name.clone(),
                        assoc_name: assoc_name.clone(),
                    };
                }
                let target_str = base_norm.to_string();
                if let Some(concrete) = self
                    .assoc_bindings
                    .get(&(trait_name.clone(), target_str))
                    .and_then(|m| m.get(assoc_name))
                    .cloned()
                {
                    return self.normalize_assoc(&concrete);
                }
                Type::Associated {
                    base: Box::new(base_norm),
                    trait_name: trait_name.clone(),
                    assoc_name: assoc_name.clone(),
                }
            }
            Type::List(inner) => Type::list(self.normalize_assoc(inner)),
            Type::Array(inner, n) => Type::array(self.normalize_assoc(inner), *n),
            Type::Option(inner) => Type::option(self.normalize_assoc(inner)),
            Type::Result { ok, error } => {
                Type::result(self.normalize_assoc(ok), self.normalize_assoc(error))
            }
            Type::Map(k, v) => Type::map(self.normalize_assoc(k), self.normalize_assoc(v)),
            Type::Set(inner) => Type::set(self.normalize_assoc(inner)),
            Type::Pointer(inner) => Type::pointer(self.normalize_assoc(inner)),
            Type::Borrow(inner) => Type::borrow(self.normalize_assoc(inner)),
            Type::MutBorrow(inner) => Type::mut_borrow(self.normalize_assoc(inner)),
            Type::Channel(inner) => Type::channel(self.normalize_assoc(inner)),
            Type::Tuple(elems) => {
                Type::tuple(elems.iter().map(|e| self.normalize_assoc(e)).collect())
            }
            Type::Generic { name, args } => {
                Type::generic(name, args.iter().map(|a| self.normalize_assoc(a)).collect())
            }
            Type::Record(name, args) => {
                Type::record(name, args.iter().map(|a| self.normalize_assoc(a)).collect())
            }
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|p| self.normalize_assoc(p)).collect(),
                return_type: Box::new(self.normalize_assoc(return_type)),
            },
            Type::Distinct { id, name, base } => Type::Distinct {
                id: *id,
                name: name.clone(),
                base: Box::new(self.normalize_assoc(base)),
            },
            Type::Subrange {
                id,
                name,
                base,
                low,
                high,
            } => Type::Subrange {
                id: *id,
                name: name.clone(),
                base: Box::new(self.normalize_assoc(base)),
                low: *low,
                high: *high,
            },
            _ => ty.clone(),
        }
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
                scope.insert(name.to_string(), VariableInfo { type_, mutable });
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
            plan: InstantiationPlan::default(),
            record_names: HashSet::new(),
            records: Vec::new(),
            nominal_types: HashMap::new(),
            enum_types: HashMap::new(),
            subrange_types: HashMap::new(),
            const_values: HashMap::new(),
            virtual_calls: HashMap::new(),
            trait_ids: HashMap::new(),
            assoc_bindings: HashMap::new(),
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

    #[test]
    fn resolved_callee_name_returns_fallback_for_non_generic_call() {
        use crate::frontend::ast::{Expr, ExprKind};
        let expr = Expr::new(ExprKind::Int(0, Span::default()));
        let mut builder = make_builder(expr.id, Type::Int, HashMap::new());
        builder.plan = InstantiationPlan::default();
        assert_eq!(builder.resolved_callee_name(&expr, "foo"), "foo");
    }
}

// src/semantics/analyzer/scopes.rs

use super::*;

impl SemanticAnalyzer {
    // cut from mod.rs, change `fn` -> `pub(super) fn`:
    pub(super) fn push_scope(&mut self) {
        let inherited_captures = self.deferred_captures.last().cloned().unwrap_or_default();
        self.deferred_captures.push(inherited_captures);
        self.scopes.push(HashMap::new());
        self.list_lengths.push(HashMap::new());
        self.list_values.push(HashMap::new());
        self.type_params.push(HashMap::new());
        self.type_constraints.push(HashMap::new());
        self.null_bindings.push(HashSet::new());
        let region_name = format!("scope_{}", self.scopes.len());
        self.state.enter_region(region_name);
    }
    pub(super) fn pop_scope(&mut self) {
        let exiting_vars: Vec<String> = self
            .scopes
            .last()
            .map(|s| s.keys().cloned().collect())
            .unwrap_or_default();
        let exiting_region = self.state.region_stack.last().cloned();

        self.deferred_captures.pop();
        self.scopes.pop();
        self.list_lengths.pop();
        self.list_values.pop();
        self.type_params.pop();
        self.type_constraints.pop();
        self.null_bindings.pop();

        if let Some(region) = exiting_region {
            self.state.borrows.retain(|borrower, b_state| {
                // A borrow is released when any of three conditions
                // hold:
                //
                //  1. Its map key names an exiting variable. This is
                //     the shape for mutable borrows, whose key is the
                //     reference binding's name (`y` for `var y :=
                //     &mut x`).
                //  2. Its place's base names an exiting variable. A
                //     shared borrow's key is a synthetic id
                //     (`__shared_p_1_42`), so key-matching never
                //     fires for it — but its place's base is the
                //     borrowed variable (`p`), which is what we need
                //     to compare against the exiting scope.
                //  3. Its lifetime is the region being popped. This
                //     catches borrows that outlive their declaration
                //     scope but are region-scoped.
                let borrower_exits = exiting_vars.contains(borrower);
                let place_base_exits = exiting_vars.contains(&b_state.place.base);
                let lifetime_exits = matches!(
                    &b_state.lifetime,
                    crate::semantics::state::BorrowLifetime::Region(r) if r == &region
                );
                !(borrower_exits || place_base_exits || lifetime_exits)
            });
            for v in &exiting_vars {
                self.state.var_region.remove(v);
                let still_present = self.scopes.iter().any(|s| s.contains_key(v));
                if !still_present {
                    self.state.vars.remove(v);
                }
            }
            // ADR 0045 status: `exit_region` computes the set of
            // borrows that outlive the region being popped, but the
            // result is not yet propagated to the caller as an
            // error. Until it is, this assertion catches cases that
            // *would* be rejected once ADR 0045 is enforced, so we
            // can measure the blast radius without breaking the
            // build. Replace with a `Result` propagation when the
            // enforcement lands.
            let outliving = self.state.exit_region(&region);
            debug_assert!(
                outliving.is_empty(),
                "region escape detected (ADR 0045 not yet enforced): {:?}",
                outliving
            );
        }
    }
    pub(super) fn declare_variable(
        &mut self,
        name: &str,
        type_: Type,
        mutable: bool,
    ) -> Result<()> {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) {
                return Err(CompileError::simple(
                    &format!("Variable '{}' already declared", name),
                    0,
                    0,
                    "",
                    ErrorCode::E0003,
                ));
            }
            scope.insert(name.to_string(), (type_.clone(), mutable));
        }
        // v0.9-D: mirror into unified state
        self.state.declare(name.to_string(), VarState::available());
        Ok(())
    }
    pub(super) fn lookup_variable(&self, name: &str) -> Option<(Type, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some((t, m)) = scope.get(name) {
                return Some((t.clone(), *m));
            }
        }
        None
    }
    pub(super) fn declare_type_param(&mut self, name: &str, type_: Type) {
        if let Some(scope) = self.type_params.last_mut() {
            scope.insert(name.to_string(), type_);
        }
    }
    pub(super) fn lookup_type_param(&self, name: &str) -> Option<Type> {
        for scope in self.type_params.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t.clone());
            }
        }
        None
    }
    pub(super) fn declare_type_constraint(&mut self, type_param: &str, trait_name: &str) {
        if let Some(scope) = self.type_constraints.last_mut() {
            scope
                .entry(type_param.to_string())
                .or_default()
                .push(trait_name.to_string());
        }
    }
    /// Base name of a type, ignoring generic arguments: `List<Float>` → `"List"`.
    pub(crate) fn base_type_name(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Int => Some("Int"),
            Type::Float => Some("Float"),
            Type::String => Some("String"),
            Type::Char => Some("Char"),
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
    pub(super) fn resolve_type(&self, type_: &Type) -> Type {
        match type_ {
            Type::TypeVar(name) => self
                .lookup_type_param(name)
                .unwrap_or_else(|| Type::TypeVar(name.clone())),
            Type::List(inner) => Type::list(self.resolve_type(inner)),
            Type::Option(inner) => Type::option(self.resolve_type(inner)),
            Type::Result { ok, error } => {
                Type::result(self.resolve_type(ok), self.resolve_type(error))
            }
            _ => type_.clone(),
        }
    }
}

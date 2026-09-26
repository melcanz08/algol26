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
                let borrower_exits = exiting_vars.contains(borrower);
                let lifetime_exits = matches!(
                    &b_state.lifetime,
                    crate::semantics::state::BorrowLifetime::Region(r) if r == &region
                );
                !(borrower_exits || lifetime_exits)
            });
            for v in &exiting_vars {
                self.state.var_region.remove(v);
                let still_present = self.scopes.iter().any(|s| s.contains_key(v));
                if !still_present {
                    self.state.vars.remove(v);
                }
            }
            let _outliving = self.state.exit_region(&region);
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

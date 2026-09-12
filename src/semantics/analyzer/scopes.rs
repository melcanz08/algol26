// src/semantics/analyzer/scopes.rs

use super::*;

impl SemanticAnalyzer {
    // cut from mod.rs, change `fn` -> `pub(super) fn`:
    pub(super) fn push_scope(&mut self) {
        self.deferred_captures.push(HashSet::new());
        self.mutable_borrows.push(HashMap::new());
        self.scopes.push(HashMap::new());
        self.moved_vars.push(Vec::new());
        self.borrowed_vars.push(HashSet::new());
        self.mutably_borrowed.push(HashSet::new());
        self.list_lengths.push(HashMap::new());
        self.list_values.push(HashMap::new());
        self.type_params.push(HashMap::new());
        self.type_constraints.push(HashMap::new());
        self.null_bindings.push(HashSet::new());
    }
    pub(super) fn pop_scope(&mut self) {
        self.deferred_captures.pop();
        self.scopes.pop();
        self.moved_vars.pop();
        self.borrowed_vars.pop();
        self.mutably_borrowed.pop();
        self.mutable_borrows.pop();
        self.list_lengths.pop();
        self.list_values.pop();
        self.type_params.pop();
        self.type_constraints.pop();
        self.null_bindings.pop();
    }
    pub(super) fn declare_variable(&mut self, name: &str, type_: Type, mutable: bool) -> Result<()> {
        if let Some(scope) = self.scopes.last_mut() {
            if scope.contains_key(name) {
                return Err(CompileError::simple(
                    &format!("Variable '{}' already declared", name),
                    0, 0, "", ErrorCode::E0003,
                ));
            }
            scope.insert(name.to_string(), (type_, mutable));
        }
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
    pub(super) fn base_type_name(ty: &Type) -> Option<&'static str> {
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
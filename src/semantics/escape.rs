// src/semantics/escape.rs
//
// Tracks whether a reference to a variable may outlive the variable
// itself.
//
// Reading an outer variable from a nested scope is NOT an escape —
// the reference dies with the inner scope. Escape requires the
// caller to signal it explicitly (e.g. `return &x`, storing `&x` in
// a longer-lived location).
//
// This is a scope-level analysis, not a lifetime proof. It cannot
// decide `lifetime(ref) <= lifetime(source)` in general; a full
// lifetime model would need to be threaded through the AST.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct EscapeInfo {
    pub variable: String,
    pub escapes_scope: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EscapeAnalyzer {
    scope_depth: usize,
    max_scope_depth: usize,
    variables: HashMap<String, VariableInfo>,
    active_scopes: Vec<HashSet<String>>,
    escaped: Vec<EscapeInfo>,
}

#[derive(Debug, Clone)]
pub struct VariableInfo {
    decl_depth: usize,
    is_reference: bool,
    escaped: bool,
}

impl EscapeAnalyzer {
    pub fn new() -> Self {
        let mut analyzer = EscapeAnalyzer {
            scope_depth: 0,
            max_scope_depth: 0,
            variables: HashMap::new(),
            active_scopes: Vec::new(),
            escaped: Vec::new(),
        };
        analyzer.active_scopes.push(HashSet::new());
        analyzer
    }

    pub fn enter_scope(&mut self) {
        self.scope_depth += 1;
        if self.scope_depth > self.max_scope_depth {
            self.max_scope_depth = self.scope_depth;
        }
        self.active_scopes.push(HashSet::new());
    }

    pub fn exit_scope(&mut self) {
        if let Some(scoped_vars) = self.active_scopes.pop() {
            for var in scoped_vars {
                // Check if variable escaped its declaring scope
                if let Some(info) = self.variables.get(&var) {
                    if info.decl_depth == self.scope_depth && info.escaped {
                        self.escaped.push(EscapeInfo {
                            variable: var.clone(),
                            escapes_scope: true,
                            reason: Some(format!(
                                "Variable '{}' declared at scope {} escapes its declaring scope",
                                var, info.decl_depth
                            )),
                        });
                    }
                }
                self.variables.remove(&var);
            }
        }
        if self.scope_depth > 0 {
            self.scope_depth -= 1;
        }
    }

    pub fn declare(&mut self, name: &str, is_reference: bool) {
        self.variables.insert(
            name.to_string(),
            VariableInfo {
                decl_depth: self.scope_depth,
                is_reference,
                escaped: false,
            },
        );
        if let Some(current_scope) = self.active_scopes.last_mut() {
            current_scope.insert(name.to_string());
        }
    }

    pub fn reference(&mut self, name: &str, outlives_scope: bool) {
        if let Some(info) = self.variables.get_mut(name) {
            // Reading a variable from a nested scope does NOT constitute
            // escape — the reference dies when the inner scope ends.
            //
            // Escape means the reference may outlive the variable it
            // points to, which the caller signals via `outlives_scope`
            // (e.g. `return &x`, storing `&x` in a longer-lived
            // location, spawning a task that captures `&x`).
            if outlives_scope {
                info.escaped = true;
                self.escaped.push(EscapeInfo {
                    variable: name.to_string(),
                    escapes_scope: true,
                    reason: Some(format!(
                        "Variable '{}' (declared at scope {}) has a reference \
                         that may outlive its declaring scope",
                        name, info.decl_depth
                    )),
                });
            }
        }
    }

    pub fn take_reference(&mut self, name: &str) {
        // Taking a reference does not itself cause escape — the escape
        // occurs only if that reference may outlive the variable.
        if let Some(info) = self.variables.get_mut(name) {
            info.is_reference = true;
        }
    }

    pub fn get_escapes(&self) -> &[EscapeInfo] {
        &self.escaped
    }

    pub fn has_escapes(&self) -> bool {
        !self.escaped.is_empty()
    }

    pub fn clear(&mut self) {
        self.escaped.clear();
        self.variables.clear();
        self.active_scopes.clear();
        self.active_scopes.push(HashSet::new());
        self.scope_depth = 0;
        self.max_scope_depth = 0;
    }

    pub fn get_variable_info(&self, name: &str) -> Option<&VariableInfo> {
        self.variables.get(name)
    }

    pub fn scope_depth(&self) -> usize {
        self.scope_depth
    }
}

impl Default for EscapeAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_declaration_no_escape() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("x", false);
        analyzer.reference("x", false);
        assert!(!analyzer.has_escapes(), "Simple read should not escape");
    }

    #[test]
    fn test_inner_scope_read_is_not_escape() {
        // Reading an outer variable from an inner scope does NOT escape:
        // the reference dies with the inner scope.
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("x", false);
        analyzer.enter_scope();
        analyzer.reference("x", false);
        analyzer.exit_scope();
        assert!(
            !analyzer.has_escapes(),
            "Inner-scope read of outer variable must not count as escape"
        );
    }

    #[test]
    fn test_reference_outliving_scope_is_escape() {
        // A reference explicitly marked as outliving its scope IS an escape.
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("x", false);
        analyzer.enter_scope();
        analyzer.reference("x", true);
        analyzer.exit_scope();
        assert!(
            analyzer.has_escapes(),
            "Reference outliving its scope should be an escape"
        );
    }

    #[test]
    fn test_nested_inner_reads_are_not_escapes() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("outer", false);

        analyzer.enter_scope();
        analyzer.declare("inner", false);

        analyzer.enter_scope();
        analyzer.reference("outer", false);
        analyzer.reference("inner", false);
        analyzer.exit_scope();

        analyzer.exit_scope();

        assert!(
            !analyzer.has_escapes(),
            "Nested inner reads of outer variables must not escape"
        );
    }

    #[test]
    fn test_scope_cleanup() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("x", false);
        analyzer.enter_scope();
        analyzer.declare("y", false);
        analyzer.exit_scope();
        assert!(analyzer.get_variable_info("x").is_some());
        assert!(analyzer.get_variable_info("y").is_none());
    }
}

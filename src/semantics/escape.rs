#![allow(dead_code)]

// Updated Escape Analyzer - HARDENED with proper escape tracking

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
            // Mark as escaped if referenced beyond its declaration scope
            if outlives_scope || info.decl_depth < self.scope_depth {
                info.escaped = true;
                self.escaped.push(EscapeInfo {
                    variable: name.to_string(),
                    escapes_scope: true,
                    reason: Some(format!(
                        "Variable '{}' declared at scope {} referenced at scope {}",
                        name, info.decl_depth, self.scope_depth
                    )),
                });
            }
        }
    }

    pub fn take_reference(&mut self, name: &str) {
        // Taking a reference to a variable may cause it to escape
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
        assert!(!analyzer.has_escapes(), "Variable should not escape");
    }

    #[test]
    fn test_variable_escapes_scope() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("x", false);
        analyzer.enter_scope();
        analyzer.reference("x", false); // Referenced in inner scope
        analyzer.exit_scope();
        assert!(
            analyzer.has_escapes(),
            "Variable should be marked as escaping"
        );
    }

    #[test]
    fn test_reference_causes_escape() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("x", false);
        analyzer.take_reference("x");
        analyzer.enter_scope();
        analyzer.reference("x", true); // Referenced beyond scope
        analyzer.exit_scope();
        assert!(analyzer.has_escapes(), "Reference should cause escape");
    }

    #[test]
    fn test_scope_cleanup() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("x", false);
        analyzer.enter_scope();
        analyzer.declare("y", false);
        analyzer.exit_scope();
        // y should be cleaned up, x should remain
        assert!(analyzer.get_variable_info("x").is_some());
        assert!(analyzer.get_variable_info("y").is_none());
    }

    #[test]
    fn test_nested_scopes() {
        let mut analyzer = EscapeAnalyzer::new();
        analyzer.declare("outer", false);

        analyzer.enter_scope();
        analyzer.declare("inner", false);

        analyzer.enter_scope();
        analyzer.reference("outer", false); // Should mark as escaped
        analyzer.reference("inner", false); // Should mark as escaped
        analyzer.exit_scope();

        analyzer.exit_scope();

        assert!(
            analyzer.has_escapes(),
            "Variables should escape nested scopes"
        );
    }
}

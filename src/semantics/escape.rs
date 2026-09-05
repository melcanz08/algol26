// Updated Escape Analyzer - precise capture and lifetime analysis
#![allow(dead_code)]

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct EscapeInfo {
    pub variable: String,
    pub escapes_scope: bool,
    pub reason: Option<String>,
}

pub struct EscapeAnalyzer {
    scope_depth: usize,
    max_scope_depth: usize,
    variables: HashMap<String, usize>, // variable -> scope depth where declared
    active_scopes: Vec<HashSet<String>>, // tracks bindings declared per scope level
    escaped: Vec<EscapeInfo>,
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
                self.variables.remove(&var);
            }
        }
        if self.scope_depth > 0 {
            self.scope_depth -= 1;
        }
    }

    pub fn declare(&mut self, name: &str) {
        self.variables.insert(name.to_string(), self.scope_depth);
        if let Some(current_scope) = self.active_scopes.last_mut() {
            current_scope.insert(name.to_string());
        }
    }

    pub fn reference(&mut self, name: &str, outlives_scope: bool) {
        if let Some(&decl_depth) = self.variables.get(name) {
            if decl_depth < self.scope_depth || outlives_scope {
                self.escaped.push(EscapeInfo {
                    variable: name.to_string(),
                    escapes_scope: true,
                    reason: Some(format!(
                        "Variable '{}' declared at scope {} captured or referenced beyond stack frame at scope {}",
                        name, decl_depth, self.scope_depth
                    )),
                });
            }
        }
    }

    pub fn get_escapes(&self) -> &[EscapeInfo] {
        &self.escaped
    }
}

impl Default for EscapeAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

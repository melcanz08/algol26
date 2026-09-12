// src/semantics/analyzer/ownership.rs

use super::*;

impl SemanticAnalyzer {
    pub(super) fn release_mutable_borrow(&mut self, reference: &str) {
        // Find the innermost scope that holds `reference`, and release it
        // *only there*. Removing from all scopes could accidentally clear
        // an outer scope's borrow of the same source.
        let scope_idx = self
            .mutable_borrows
            .iter()
            .enumerate()
            .rev()
            .find(|(_, map)| map.contains_key(reference))
            .map(|(i, _)| i);

        let Some(idx) = scope_idx else {
            return;
        };

        // Extract the source before releasing.
        let source = match self.mutable_borrows[idx].remove(reference) {
            Some(src) => src,
            None => return,
        };

        // Remove the source from the same scope's mutably_borrowed set.
        if let Some(set) = self.mutably_borrowed.get_mut(idx) {
            set.remove(&source);
        }
    }    
    pub(super) fn all_moved_vars(&self) -> Vec<String> {
        let mut result = Vec::new();
        for scope in &self.moved_vars {
            for var in scope {
                if !result.contains(var) {
                    result.push(var.clone());
                }
            }
        }
        result
    }
    pub(super) fn is_moved(&self, name: &str) -> bool {
        self.moved_vars
            .iter()
            .any(|scope| scope.iter().any(|v| v == name))
    }
    pub(super) fn mark_moved(&mut self, name: &str) {
        if let Some(scope) = self.moved_vars.last_mut() {
            if !scope.contains(&name.to_string()) {
                scope.push(name.to_string());
            }
        }
    }
    pub(super) fn mark_borrowed(&mut self, name: &str) {
        if let Some(scope) = self.borrowed_vars.last_mut() {
            scope.insert(name.to_string());
        }
    }
    pub(super) fn mark_mutably_borrowed(&mut self, name: &str) {
        if let Some(scope) = self.mutably_borrowed.last_mut() {
            scope.insert(name.to_string());
        }
    }
    pub(super) fn is_mutably_borrowed(&self, name: &str) -> bool {
        self.mutably_borrowed
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
    }
    pub(super) fn register_mutable_borrow(&mut self, reference: &str, source: &str) -> Result<()> {
        // The source must be declared `var` — you cannot take a mutable
        // borrow of an immutable (`val`) binding.
        match self.lookup_variable(source) {
            Some((_, false)) => {
                return Err(CompileError::simple(
                    &format!("Cannot mutably borrow immutable variable '{}'", source),
                    0, 0, "", ErrorCode::E0007,
                ).with_suggestion(&format!(
                    "Declare '{}' with 'var' instead of 'val'", source
                )));
            }
            Some((_, true)) => {} // mutable, ok
            None => {
                // Source not in scope — let the analyzer report the
                // "undefined variable" error elsewhere.
            }
        }
        if self.is_moved(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow moved variable '{}'", source),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("The variable has already been moved"));
        }
        if self.is_mutably_borrowed(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' more than once", source),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if self.is_borrowed(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' while immutably borrowed", source),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Wait for the immutable borrow to end"));
        }
        self.mark_mutably_borrowed(source);
        if let Some(scope) = self.mutable_borrows.last_mut() {
            scope.insert(reference.to_string(), source.to_string());
        }
        Ok(())
    }
    pub(super) fn is_borrowed(&self, name: &str) -> bool {
        self.borrowed_vars.iter().rev().any(|scope| scope.contains(name))
    }
    pub(super) fn check_borrow_rules(&self, name: &str, mutable: bool) -> Result<()> {
        if let Some(scope) = self.deferred_captures.last() {
            if scope.contains(name) {
                return Err(CompileError::simple(
                    &format!("Cannot use '{}' after it was captured by defer", name),
                    0, 0, "", ErrorCode::E0007,
                ).with_suggestion("Deferred statements capture variables at declaration time"));
            }
        }
        if self.is_moved(name) {
            return Err(CompileError::simple(
                &format!("Cannot borrow moved variable '{}'", name),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("The variable has been moved and is no longer available"));
        }
        if mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' more than once", name),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if mutable && self.is_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' while immutably borrowed", name),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Wait for the immutable borrow to end"));
        }
        if !mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot read '{}' while it is mutably borrowed", name),
                0, 0, "", ErrorCode::E0007,
            ).with_suggestion("Wait for the mutable borrow to end before reading"));
        }
        Ok(())
    }
    pub(super) fn collect_deferred_captures(&self, stmt: &Stmt, captured: &mut HashSet<String>) {
        match stmt {
            Stmt::Print { expr } => self.collect_expr_captures(expr, captured),
            Stmt::Assign { name, value } => {
                captured.insert(name.clone());
                self.collect_expr_captures(value, captured);
            }
            Stmt::Expression(expr) => self.collect_expr_captures(expr, captured),
            Stmt::VarDecl { name: _, value, .. } => {
                self.collect_expr_captures(value, captured);
            }
            _ => {}
        }
    }
    pub(super) fn collect_expr_captures(&self, expr: &Expr, captured: &mut HashSet<String>) {
        match expr {
            Expr::Var(name, _) => {
                captured.insert(name.clone());
            }
            Expr::Binary { left, right, .. } => {
                self.collect_expr_captures(left, captured);
                self.collect_expr_captures(right, captured);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_expr_captures(arg, captured);
                }
            }
            Expr::ArrayAccess { array, index } => {
                self.collect_expr_captures(array, captured);
                self.collect_expr_captures(index, captured);
            }
            _ => {}
        }
    }
}
#![allow(dead_code)]
// algol26/src/race.rs
#![allow(unused_imports)]
#![allow(unused_variables)]

// ALGOL26 Race Detection
// Compile-time data race analysis for concurrent code

use crate::frontend::ast::{Expr, FunctionDecl, Stmt};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RaceDetector {
    // Track which variables are accessed in spawned blocks
    spawned_accesses: Vec<HashMap<String, AccessType>>,
    // Track which variables are accessed in main thread
    main_accesses: HashMap<String, AccessType>,
    // Track shared mutable state
    shared_mutable: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccessType {
    Read,
    Write,
    ReadWrite,
}

impl Default for RaceDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl RaceDetector {
    pub fn new() -> Self {
        RaceDetector {
            spawned_accesses: Vec::new(),
            main_accesses: HashMap::new(),
            shared_mutable: Vec::new(),
        }
    }

    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Vec<String> {
        let mut races = Vec::new();

        for func in functions {
            self.analyze_function(func);
        }

        // Check for races - only flag write-write conflicts
        for spawned in &self.spawned_accesses {
            for (var, spawn_access) in spawned {
                if let Some(main_access) = self.main_accesses.get(var) {
                    // Only flag if BOTH are writing
                    let spawn_writes =
                        matches!(spawn_access, AccessType::Write | AccessType::ReadWrite);
                    let main_writes =
                        matches!(main_access, AccessType::Write | AccessType::ReadWrite);

                    if spawn_writes && main_writes {
                        races.push(format!(
                            "Data race detected: variable '{}' written concurrently (spawn: {:?}, main: {:?})",
                            var, spawn_access, main_access
                        ));
                    }
                }
            }
        }

        races
    }

    fn analyze_function(&mut self, func: &FunctionDecl) {
        for stmt in &func.body {
            self.analyze_stmt(stmt, false);
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt, in_spawn: bool) {
        match stmt {
            Stmt::Spawn { body } => {
                let mut spawn_accesses = HashMap::new();
                for s in body {
                    self.collect_accesses(s, &mut spawn_accesses);
                }
                self.spawned_accesses.push(spawn_accesses);
            }
            Stmt::Assign { name, value } => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        accesses.insert(name.clone(), AccessType::Write);
                    }
                } else {
                    self.main_accesses.insert(name.clone(), AccessType::Write);
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::VarDecl { name, value, .. } => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        accesses.insert(name.clone(), AccessType::Write);
                    }
                } else {
                    self.main_accesses.insert(name.clone(), AccessType::Write);
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::Expression(Expr::If {
                condition,
                then_branch,
                else_branch,
            }) => {
                self.analyze_expr(condition, in_spawn);
                let _ = then_branch;
                let _ = else_branch;
            }
            Stmt::Expression(Expr::For {
                var,
                iterable,
                body,
                ..
            }) => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        accesses.insert(var.clone(), AccessType::ReadWrite);
                    }
                } else {
                    self.main_accesses
                        .insert(var.clone(), AccessType::ReadWrite);
                }
                self.analyze_expr(iterable, in_spawn);
                for s in body {
                    self.analyze_stmt(s, in_spawn);
                }
            }
            Stmt::Expression(Expr::While {
                condition, body, ..
            }) => {
                self.analyze_expr(condition, in_spawn);
                for s in body {
                    self.analyze_stmt(s, in_spawn);
                }
            }
            Stmt::Print { expr } => {
                self.analyze_expr(expr, in_spawn);
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    for s in block {
                        self.analyze_stmt(s, true);
                    }
                }
            }
            _ => {}
        }
    }

    fn collect_accesses(&mut self, stmt: &Stmt, accesses: &mut HashMap<String, AccessType>) {
        match stmt {
            Stmt::Assign { name, value } => {
                accesses.insert(name.clone(), AccessType::Write);
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::VarDecl { name, value, .. } => {
                accesses.insert(name.clone(), AccessType::Write);
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::Print { expr } => {
                self.collect_expr_accesses(expr, accesses);
            }
            _ => {}
        }
    }

    fn collect_expr_accesses(&mut self, expr: &Expr, accesses: &mut HashMap<String, AccessType>) {
        match expr {
            Expr::Var(name, _) => {
                accesses.insert(name.clone(), AccessType::Read);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_expr_accesses(left, accesses);
                self.collect_expr_accesses(right, accesses);
            }
            _ => {}
        }
    }

    fn merge_access(existing: &mut AccessType, new: AccessType) {
        *existing = match (&*existing, &new) {
            (AccessType::Read, AccessType::Read) => AccessType::Read,
            (AccessType::Read, AccessType::Write) => AccessType::ReadWrite,
            (AccessType::Write, AccessType::Read) => AccessType::ReadWrite,
            (AccessType::ReadWrite, _) => AccessType::ReadWrite,
            (_, AccessType::ReadWrite) => AccessType::ReadWrite,
            (AccessType::Write, AccessType::Write) => AccessType::Write,
        };
    }

    fn analyze_expr(&mut self, expr: &Expr, in_spawn: bool) {
        match expr {
            Expr::Var(name, _) => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        accesses.entry(name.clone()).or_insert(AccessType::Read);
                    }
                } else {
                    self.main_accesses
                        .entry(name.clone())
                        .or_insert(AccessType::Read);
                }
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left, in_spawn);
                self.analyze_expr(right, in_spawn);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_race() {
        let mut detector = RaceDetector::new();
        let races = detector.analyze(&[]);
        assert!(races.is_empty());
    }
}

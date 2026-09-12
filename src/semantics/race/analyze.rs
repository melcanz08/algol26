// src/semantics/race/analyze.rs
use super::*;

impl RaceDetector {
    pub(super) fn analyze_function(&mut self, func: &FunctionDecl) {
        for stmt in &func.body {
            self.analyze_stmt(stmt, false);
        }
    }
    pub(super) fn analyze_stmt(&mut self, stmt: &Stmt, in_spawn: bool) {
        self.scope_depth += 1;

        match stmt {
            Stmt::Spawn { body } => {
                let mut spawn_accesses = HashMap::new();
                for s in body {
                    self.analyze_stmt_in_collection(s, &mut spawn_accesses);
                }
                self.spawned_accesses.push(spawn_accesses);
            }
            Stmt::Assign { name, value } => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        Self::merge_access_map(accesses, name, AccessType::Write);
                    }
                } else {
                    Self::merge_access_map(&mut self.main_accesses, name, AccessType::Write);
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::VarDecl { name, value, mutable, .. } => {
                // Only `var` bindings participate in race analysis. A
                // `val` is written exactly once, before any concurrent
                // observer could exist, and never reassigned — so it
                // cannot race with anything. Recording it as a write
                // produces false positives on read-only sharing (e.g.
                // `val x := 42; spawn { print(x) }`).
                if *mutable {
                    if in_spawn {
                        if let Some(accesses) = self.spawned_accesses.last_mut() {
                            Self::merge_access_map(accesses, name, AccessType::Write);
                        }
                    } else {
                        Self::merge_access_map(&mut self.main_accesses, name, AccessType::Write);
                    }
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::Expression(Expr::If {
                condition,
                then_branch,
                else_branch,
            }) => {
                self.analyze_expr(condition, in_spawn);
                if let Expr::Block { statements, .. } = then_branch.as_ref() {
                    for s in statements {
                        self.analyze_stmt(s, in_spawn);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let Expr::Block { statements, .. } = else_stmts.as_ref() {
                        for s in statements {
                            self.analyze_stmt(s, in_spawn);
                        }
                    }
                }
            }
            Stmt::Expression(Expr::For {
                var,
                iterable,
                body,
                ..
            }) => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        Self::merge_access_map(accesses, var, AccessType::ReadWrite);
                    }
                } else {
                    Self::merge_access_map(&mut self.main_accesses, var, AccessType::ReadWrite);
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
                    let mut block_accesses = HashMap::new();
                    for s in block {
                        self.analyze_stmt_in_collection(s, &mut block_accesses);
                    }
                    self.spawned_accesses.push(block_accesses);
                }
            }
            Stmt::Expression(expr) => {
                self.analyze_expr(expr, in_spawn);
            }
            _ => {}
        }

        self.scope_depth -= 1;
    }
    pub(super) fn analyze_stmt_in_collection(
        &mut self,
        stmt: &Stmt,
        accesses: &mut HashMap<String, AccessType>,
    ) {
        match stmt {
            Stmt::Assign { name, value } => {
                Self::merge_access_map(accesses, name, AccessType::Write);
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::VarDecl { name, value, mutable, .. } => {
                if *mutable {
                    Self::merge_access_map(accesses, name, AccessType::Write);
                }
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::Print { expr } => {
                self.collect_expr_accesses(expr, accesses);
            }
            Stmt::Expression(Expr::If {
                condition,
                then_branch,
                else_branch,
            }) => {
                self.collect_expr_accesses(condition, accesses);
                if let Expr::Block { statements, .. } = then_branch.as_ref() {
                    for s in statements {
                        self.analyze_stmt_in_collection(s, accesses);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let Expr::Block { statements, .. } = else_stmts.as_ref() {
                        for s in statements {
                            self.analyze_stmt_in_collection(s, accesses);
                        }
                    }
                }
            }
            Stmt::Expression(Expr::While {
                condition, body, ..
            }) => {
                self.collect_expr_accesses(condition, accesses);
                for s in body {
                    self.analyze_stmt_in_collection(s, accesses);
                }
            }
            Stmt::Expression(Expr::For {
                var,
                iterable,
                body,
                ..
            }) => {
                Self::merge_access_map(accesses, var, AccessType::ReadWrite);
                self.collect_expr_accesses(iterable, accesses);
                for s in body {
                    self.analyze_stmt_in_collection(s, accesses);
                }
            }
            _ => {}
        }
    }
    pub(super) fn analyze_expr(&mut self, expr: &Expr, in_spawn: bool) {
        match expr {
            Expr::Var(name, _) => {
                let target = if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        accesses
                    } else {
                        return;
                    }
                } else {
                    &mut self.main_accesses
                };
                Self::merge_access_map(target, name, AccessType::Read);
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left, in_spawn);
                self.analyze_expr(right, in_spawn);
            }
            Expr::Unary { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.analyze_expr(arg, in_spawn);
                }
            }
            Expr::ArrayAccess {
                array: collection,
                index,
                ..
            } => {
                self.analyze_expr(collection, in_spawn);
                self.analyze_expr(index, in_spawn);
            }
            Expr::List(elements) => {
                for elem in elements {
                    self.analyze_expr(elem, in_spawn);
                }
            }
            Expr::Deref { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            Expr::AddrOf { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            _ => {}
        }
    }
}
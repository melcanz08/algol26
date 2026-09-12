// src/semantics/race/collect.rs
use super::*;

impl RaceDetector {
    pub(super) fn collect_declarations(&mut self, func: &FunctionDecl) {
        for stmt in &func.body {
            self.collect_declarations_from_stmt(stmt);
        }
    }
    pub(super) fn collect_declarations_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl { name, mutable, .. } => {
                self.variable_mutability.insert(name.clone(), *mutable);
            }
            Stmt::Expression(Expr::If {
                then_branch,
                else_branch,
                ..
            }) => {
                if let Expr::Block { statements, .. } = then_branch.as_ref() {
                    for s in statements {
                        self.collect_declarations_from_stmt(s);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let Expr::Block { statements, .. } = else_stmts.as_ref() {
                        for s in statements {
                            self.collect_declarations_from_stmt(s);
                        }
                    }
                }
            }
            Stmt::Expression(Expr::While { body, .. }) => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Expression(Expr::For { body, .. }) => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Spawn { body } => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    for s in block {
                        self.collect_declarations_from_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }
    pub(super) fn collect_expr_accesses(&mut self, expr: &Expr, accesses: &mut HashMap<String, AccessType>) {
        match expr {
            Expr::Var(name, _) => {
                Self::merge_access_map(accesses, name, AccessType::Read);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_expr_accesses(left, accesses);
                self.collect_expr_accesses(right, accesses);
            }
            Expr::Unary { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_expr_accesses(arg, accesses);
                }
            }
            Expr::ArrayAccess {
                array: collection,
                index,
                ..
            } => {
                self.collect_expr_accesses(collection, accesses);
                self.collect_expr_accesses(index, accesses);
            }
            Expr::List(elements) => {
                for elem in elements {
                    self.collect_expr_accesses(elem, accesses);
                }
            }
            Expr::Deref { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            Expr::AddrOf { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            _ => {}
        }
    }
    pub(super) fn merge_access_map(map: &mut HashMap<String, AccessType>, key: &str, new_access: AccessType) {
        map.entry(key.to_string())
            .and_modify(|existing| Self::merge_access(existing, new_access.clone()))
            .or_insert(new_access);
    }
    pub(super) fn merge_access(existing: &mut AccessType, new: AccessType) {
        *existing = match (&*existing, &new) {
            (AccessType::Read, AccessType::Read) => AccessType::Read,
            (AccessType::Read, AccessType::Write) => AccessType::ReadWrite,
            (AccessType::Write, AccessType::Read) => AccessType::ReadWrite,
            (AccessType::ReadWrite, _) => AccessType::ReadWrite,
            (_, AccessType::ReadWrite) => AccessType::ReadWrite,
            (AccessType::Write, AccessType::Write) => AccessType::Write,
        };
    }
}
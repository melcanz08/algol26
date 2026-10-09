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
            Stmt::Expression(Expr {
                kind:
                    ExprKind::If {
                        then_branch,
                        else_branch,
                        ..
                    },
                ..
            }) => {
                if let ExprKind::Block { statements, .. } = &then_branch.as_ref().kind {
                    for s in statements {
                        self.collect_declarations_from_stmt(s);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let ExprKind::Block { statements, .. } = &else_stmts.as_ref().kind {
                        for s in statements {
                            self.collect_declarations_from_stmt(s);
                        }
                    }
                }
            }
            Stmt::Expression(Expr {
                kind: ExprKind::While { body, .. },
                ..
            }) => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Expression(Expr {
                kind: ExprKind::For { body, .. },
                ..
            }) => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Spawn { body, .. } => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Parallel { blocks, .. } => {
                for block in blocks {
                    for s in block {
                        self.collect_declarations_from_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn collect_expr_accesses(
        &mut self,
        expr: &Expr,
        accesses: &mut HashMap<String, AccessType>,
    ) {
        match &expr.kind {
            ExprKind::Var(name, _) => {
                Self::merge_access_map(accesses, name, AccessType::Read);
            }
            ExprKind::Binary { left, right, .. } => {
                self.collect_expr_accesses(left, accesses);
                self.collect_expr_accesses(right, accesses);
            }
            ExprKind::Unary { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            ExprKind::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_expr_accesses(arg, accesses);
                }
            }
            ExprKind::ArrayAccess {
                array: collection,
                index,
                ..
            } => {
                self.collect_expr_accesses(collection, accesses);
                self.collect_expr_accesses(index, accesses);
            }
            ExprKind::List(elements, _) => {
                for elem in elements {
                    self.collect_expr_accesses(elem, accesses);
                }
            }
            ExprKind::Deref { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            ExprKind::AddrOf { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            ExprKind::Borrow { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            ExprKind::MutBorrow { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            ExprKind::FieldAccess { object, .. } => {
                self.collect_expr_accesses(object, accesses);
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.collect_expr_accesses(receiver, accesses);
                for arg in args {
                    self.collect_expr_accesses(arg, accesses);
                }
            }
            ExprKind::Some { value, .. }
            | ExprKind::Ok { value, .. }
            | ExprKind::Error { value, .. } => {
                self.collect_expr_accesses(value, accesses);
            }
            ExprKind::RecordLiteral { fields, .. } => {
                for (_, v) in fields {
                    self.collect_expr_accesses(v, accesses);
                }
            }
            ExprKind::MapLiteral { entries, .. } => {
                for (k, v) in entries {
                    self.collect_expr_accesses(k, accesses);
                    self.collect_expr_accesses(v, accesses);
                }
            }
            ExprKind::SetLiteral { elements, .. } => {
                for e in elements {
                    self.collect_expr_accesses(e, accesses);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.collect_expr_accesses(s, accesses);
                }
                if let Some(e) = end {
                    self.collect_expr_accesses(e, accesses);
                }
            }
            // Statement-bodied expression variants. Delegate to
            // `analyze_stmt_in_collection`, which records against the
            // caller's access map directly (as opposed to the global
            // `main_accesses` / `spawned_accesses`).
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for stmt in statements {
                    self.analyze_stmt_in_collection(stmt, accesses);
                }
                if let Some(e) = trailing_expr {
                    self.collect_expr_accesses(e, accesses);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_expr_accesses(condition, accesses);
                self.collect_expr_accesses(then_branch, accesses);
                if let Some(e) = else_branch {
                    self.collect_expr_accesses(e, accesses);
                }
            }
            ExprKind::Match { value, cases, .. } => {
                self.collect_expr_accesses(value, accesses);
                for case in cases {
                    self.collect_expr_accesses(&case.body, accesses);
                }
            }
            ExprKind::TryCatch {
                try_branch,
                catch_branch,
                finally_body,
                ..
            } => {
                self.collect_expr_accesses(try_branch, accesses);
                self.collect_expr_accesses(catch_branch, accesses);
                if let Some(body) = finally_body {
                    for stmt in body {
                        self.analyze_stmt_in_collection(stmt, accesses);
                    }
                }
            }
            ExprKind::For {
                iterable,
                body,
                trailing_expr,
                ..
            } => {
                self.collect_expr_accesses(iterable, accesses);
                for stmt in body {
                    self.analyze_stmt_in_collection(stmt, accesses);
                }
                if let Some(e) = trailing_expr {
                    self.collect_expr_accesses(e, accesses);
                }
            }
            ExprKind::While {
                condition,
                body,
                trailing_expr,
                ..
            } => {
                self.collect_expr_accesses(condition, accesses);
                for stmt in body {
                    self.analyze_stmt_in_collection(stmt, accesses);
                }
                if let Some(e) = trailing_expr {
                    self.collect_expr_accesses(e, accesses);
                }
            }
            // Literal-only variants: no accesses.
            ExprKind::Number(_, _)
            | ExprKind::Int(_, _)
            | ExprKind::String(_, _)
            | ExprKind::Bool(_, _)
            | ExprKind::NullPtr(_)
            | ExprKind::PtrLiteral(_, _)
            | ExprKind::None(_) => {}
        }
    }

    pub(super) fn merge_access_map(
        map: &mut HashMap<String, AccessType>,
        key: &str,
        new_access: AccessType,
    ) {
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

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
            Stmt::Spawn { body, .. } => {
                let mut spawn_accesses = HashMap::new();
                for s in body {
                    self.analyze_stmt_in_collection(s, &mut spawn_accesses);
                }
                self.spawned_accesses.push(spawn_accesses);
            }
            Stmt::Assign { name, value, .. } => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        Self::merge_access_map(accesses, name, AccessType::Write);
                    }
                } else {
                    Self::merge_access_map(&mut self.main_accesses, name, AccessType::Write);
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::FieldAssign { target, value, .. } => {
                // `b.v := 1` writes through the base variable `b`.
                // The detector tracks by name, so this is a Write of
                // `b` — same shape as `Stmt::Assign` for the base
                // variable. Field-level disambiguation (b.v vs b.w)
                // is the same place-based-analysis problem as ADR
                // 0044; deferred.
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        Self::merge_access_map(accesses, target, AccessType::Write);
                    }
                } else {
                    Self::merge_access_map(&mut self.main_accesses, target, AccessType::Write);
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::VarDecl { value, .. } => {
                // A declaration is not a race-relevant access. It
                // happens once, sequentially, before any concurrent
                // observer exists. Recording it as a Write produced a
                // false positive on the common pattern
                //     var s := ...; spawn { print(s) }
                // where the declaration write conflicted with the
                // spawn's read even though the declaration completed
                // before the spawn began. Subsequent reads and
                // assignments are tracked by the `analyze_expr` (Var)
                // and `Stmt::Assign` arms; the declaration itself adds
                // nothing.
                self.analyze_expr(value, in_spawn);
            }
            Stmt::Expression(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then_branch,
                        else_branch,
                        ..
                    },
                ..
            }) => {
                self.analyze_expr(condition, in_spawn);
                if let ExprKind::Block { statements, .. } = &then_branch.as_ref().kind {
                    for s in statements {
                        self.analyze_stmt(s, in_spawn);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let ExprKind::Block { statements, .. } = &else_stmts.as_ref().kind {
                        for s in statements {
                            self.analyze_stmt(s, in_spawn);
                        }
                    }
                }
            }
            Stmt::Expression(Expr {
                kind:
                    ExprKind::For {
                        var,
                        iterable,
                        body,
                        ..
                    },
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
            Stmt::Expression(Expr {
                kind: ExprKind::While {
                    condition, body, ..
                },
                ..
            }) => {
                self.analyze_expr(condition, in_spawn);
                for s in body {
                    self.analyze_stmt(s, in_spawn);
                }
            }
            Stmt::Print { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            Stmt::Parallel { blocks, .. } => {
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
            Stmt::Assign { name, value, .. } => {
                Self::merge_access_map(accesses, name, AccessType::Write);
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::FieldAssign { target, value, .. } => {
                Self::merge_access_map(accesses, target, AccessType::Write);
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::VarDecl { value, .. } => {
                // Declaration is not an access; see the comment in
                // `analyze_stmt`. Only the value expression can
                // reference existing variables.
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::Print { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            Stmt::Expression(Expr {
                kind:
                    ExprKind::If {
                        condition,
                        then_branch,
                        else_branch,
                        ..
                    },
                ..
            }) => {
                self.collect_expr_accesses(condition, accesses);
                if let ExprKind::Block { statements, .. } = &then_branch.as_ref().kind {
                    for s in statements {
                        self.analyze_stmt_in_collection(s, accesses);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let ExprKind::Block { statements, .. } = &else_stmts.as_ref().kind {
                        for s in statements {
                            self.analyze_stmt_in_collection(s, accesses);
                        }
                    }
                }
            }
            Stmt::Expression(Expr {
                kind: ExprKind::While {
                    condition, body, ..
                },
                ..
            }) => {
                self.collect_expr_accesses(condition, accesses);
                for s in body {
                    self.analyze_stmt_in_collection(s, accesses);
                }
            }
            Stmt::Expression(Expr {
                kind:
                    ExprKind::For {
                        var,
                        iterable,
                        body,
                        ..
                    },
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
        match &expr.kind {
            ExprKind::Var(name, _) => {
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
            ExprKind::Binary { left, right, .. } => {
                self.analyze_expr(left, in_spawn);
                self.analyze_expr(right, in_spawn);
            }
            ExprKind::Unary { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            ExprKind::FunctionCall { args, .. } => {
                for arg in args {
                    self.analyze_expr(arg, in_spawn);
                }
            }
            ExprKind::ArrayAccess {
                array: collection,
                index,
                ..
            } => {
                self.analyze_expr(collection, in_spawn);
                self.analyze_expr(index, in_spawn);
            }
            ExprKind::List(elements, _) => {
                for elem in elements {
                    self.analyze_expr(elem, in_spawn);
                }
            }
            ExprKind::Deref { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            ExprKind::AddrOf { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            ExprKind::Borrow { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            ExprKind::MutBorrow { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            ExprKind::FieldAccess { object, .. } => {
                // `b.v` reads through the base variable `b`.
                // Recursing into `object` handles `Variable("b")`
                // (records a Read) and any nested expression.
                // Field-level disambiguation is deferred; see the
                // `Stmt::FieldAssign` arm.
                self.analyze_expr(object, in_spawn);
            }
            ExprKind::MethodCall { receiver, args, .. } => {
                self.analyze_expr(receiver, in_spawn);
                for arg in args {
                    self.analyze_expr(arg, in_spawn);
                }
            }
            ExprKind::Some { value, .. }
            | ExprKind::Ok { value, .. }
            | ExprKind::Error { value, .. } => {
                self.analyze_expr(value, in_spawn);
            }
            ExprKind::RecordLiteral { fields, .. } => {
                for (_, v) in fields {
                    self.analyze_expr(v, in_spawn);
                }
            }
            ExprKind::MapLiteral { entries, .. } => {
                for (k, v) in entries {
                    self.analyze_expr(k, in_spawn);
                    self.analyze_expr(v, in_spawn);
                }
            }
            ExprKind::SetLiteral { elements, .. } => {
                for e in elements {
                    self.analyze_expr(e, in_spawn);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(s) = start {
                    self.analyze_expr(s, in_spawn);
                }
                if let Some(e) = end {
                    self.analyze_expr(e, in_spawn);
                }
            }
            // Statement-bodied expression variants. These normally
            // appear as `Stmt::Expression(Expr { kind: If/For/While
            // .. })` and are dispatched by `analyze_stmt`'s own arms,
            // but nothing prevents them appearing nested inside
            // another expression (e.g. a `Block` used as a value).
            // Recurse into sub-expressions and delegate nested
            // statements back to `analyze_stmt`, which respects the
            // `in_spawn` flag we're carrying.
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for stmt in statements {
                    self.analyze_stmt(stmt, in_spawn);
                }
                if let Some(e) = trailing_expr {
                    self.analyze_expr(e, in_spawn);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.analyze_expr(condition, in_spawn);
                self.analyze_expr(then_branch, in_spawn);
                if let Some(e) = else_branch {
                    self.analyze_expr(e, in_spawn);
                }
            }
            ExprKind::Match { value, cases, .. } => {
                self.analyze_expr(value, in_spawn);
                for case in cases {
                    // Patterns bind names; only the case body can
                    // reference variables.
                    self.analyze_expr(&case.body, in_spawn);
                }
            }
            ExprKind::TryCatch {
                try_branch,
                catch_branch,
                finally_body,
                ..
            } => {
                self.analyze_expr(try_branch, in_spawn);
                self.analyze_expr(catch_branch, in_spawn);
                if let Some(body) = finally_body {
                    for stmt in body {
                        self.analyze_stmt(stmt, in_spawn);
                    }
                }
            }
            ExprKind::For {
                iterable,
                body,
                trailing_expr,
                ..
            } => {
                self.analyze_expr(iterable, in_spawn);
                for stmt in body {
                    self.analyze_stmt(stmt, in_spawn);
                }
                if let Some(e) = trailing_expr {
                    self.analyze_expr(e, in_spawn);
                }
            }
            ExprKind::While {
                condition,
                body,
                trailing_expr,
                ..
            } => {
                self.analyze_expr(condition, in_spawn);
                for stmt in body {
                    self.analyze_stmt(stmt, in_spawn);
                }
                if let Some(e) = trailing_expr {
                    self.analyze_expr(e, in_spawn);
                }
            }
            // Literal-only variants: no sub-expressions, no
            // accesses to record. Listed explicitly so the compiler
            // forces a decision when a new ExprKind is added; the
            // previous `_ => {}` silently ignored FieldAccess,
            // MethodCall, and every composite constructor, which is
            // how the C4_race_via_record hole went unnoticed.
            ExprKind::Number(_, _)
            | ExprKind::Int(_, _)
            | ExprKind::String(_, _)
            | ExprKind::Bool(_, _)
            | ExprKind::NullPtr(_)
            | ExprKind::PtrLiteral(_, _)
            | ExprKind::None(_) => {}
        }
    }
}

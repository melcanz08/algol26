// src/semantics/analyzer/ownership.rs

use super::*;

impl SemanticAnalyzer {
    /// Register a mutable borrow of `source` with `Temporary` lifetime,
    /// for the duration of the current statement. Called by
    /// `Expr::FunctionCall` after analyzing a `&mut x` argument.
    ///
    /// The borrow rules for the argument were already checked by
    /// `Expr::MutBorrow`; this only installs the state entry.
    pub(super) fn register_call_argument_mut_borrow(&mut self, source: &str) {
        let call_id = self.state.fresh_call_id();
        let reference = format!("__tmp_call_{}", call_id.0);
        self.state.borrow(
            reference,
            source.to_string(),
            BorrowKind::Mutable,
            BorrowLifetime::Temporary(call_id),
        );
    }

    /// If `arg` is `&mut <var>`, register the call-argument temporary.
    /// Otherwise do nothing. Shared borrows (`&x`) are not registered
    /// here — `mark_borrowed` already installs a lexical shared borrow
    /// at the `Expr::Borrow` site, which is conservative enough to
    /// reject the cases we care about (`f(&x, &mut x)`).
    pub(super) fn register_call_arg_temporary(&mut self, arg: &Expr) {
        if let ExprKind::MutBorrow { expr, .. } = &arg.kind {
            if let ExprKind::Var(name, _) = &expr.as_ref().kind {
                self.register_call_argument_mut_borrow(name);
            }
        }
    }
    pub(super) fn is_moved(&self, name: &str) -> bool {
        self.state.vars.get(name).is_some_and(|s| s.is_moved())
    }

    pub(super) fn mark_moved(&mut self, name: &str) {
        self.state.move_out(name);
    }

    pub(super) fn mark_borrowed(&mut self, name: &str) {
        // Shared borrows have no declaration site, so give each one a
        // unique synthetic borrower key. Using a per-source key would
        // collapse nested borrows of the same place into one entry and
        // lose the outer borrow when the inner scope exits.
        let borrower = format!("__shared_{}_{}", name, self.state.fresh_node_id().0);
        let lt = if let Some(cur) = self.state.current_region().cloned() {
            BorrowLifetime::Region(cur)
        } else {
            BorrowLifetime::Local(borrower.clone())
        };
        self.state
            .borrow(borrower, name.to_string(), BorrowKind::Shared, lt);
    }

    pub(super) fn is_mutably_borrowed(&self, name: &str) -> bool {
        self.state.is_mutably_borrowed(name)
    }

    pub(super) fn is_borrowed(&self, name: &str) -> bool {
        self.state.is_borrowed(name)
    }

    pub(super) fn release_mutable_borrow(&mut self, reference: &str) {
        self.state.borrows.remove(reference);
    }

    pub(super) fn register_mutable_borrow(&mut self, reference: &str, source: &str) -> Result<()> {
        match self.lookup_variable(source) {
            Some((_, false)) => {
                return Err(CompileError::simple(
                    &format!("Cannot mutably borrow immutable variable '{}'", source),
                    self.current_span.start_line,
                    self.current_span.start_column,
                    "",
                    ErrorCode::E0007,
                )
                .with_suggestion(&format!("Declare '{}' with 'var' instead of 'val'", source)));
            }
            Some((_, true)) => {}
            None => {}
        }
        if self.is_moved(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow moved variable '{}'", source),
                self.current_span.start_line,
                self.current_span.start_column,
                "",
                ErrorCode::E0007,
            )
            .with_suggestion("The variable has already been moved"));
        }
        if self.is_mutably_borrowed(source) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' more than once", source),
                self.current_span.start_line,
                self.current_span.start_column,
                "",
                ErrorCode::E0007,
            )
            .with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if self.is_borrowed(source) {
            return Err(CompileError::simple(
                &format!(
                    "Cannot mutably borrow '{}' while immutably borrowed",
                    source
                ),
                self.current_span.start_line,
                self.current_span.start_column,
                "",
                ErrorCode::E0007,
            )
            .with_suggestion("Wait for the immutable borrow to end"));
        }
        let lt = if let Some(cur) = self.state.current_region().cloned() {
            BorrowLifetime::Region(cur)
        } else {
            BorrowLifetime::Local(reference.to_string())
        };
        self.state.borrow(
            reference.to_string(),
            source.to_string(),
            BorrowKind::Mutable,
            lt,
        );
        Ok(())
    }

    pub(super) fn check_borrow_rules(&self, name: &str, mutable: bool) -> Result<()> {
        // Deferred-capture check unchanged.
        if let Some(scope) = self.deferred_captures.last() {
            if scope.contains(name) {
                return Err(CompileError::simple(
                    &format!("Cannot use '{}' after it was captured by defer", name),
                    self.current_span.start_line,
                    self.current_span.start_column,
                    "",
                    ErrorCode::E0007,
                )
                .with_suggestion("Deferred statements capture variables at declaration time"));
            }
        }
        if self.is_moved(name) {
            return Err(CompileError::simple(
                &format!("Cannot borrow moved variable '{}'", name),
                self.current_span.start_line,
                self.current_span.start_column,
                "",
                ErrorCode::E0007,
            )
            .with_suggestion("The variable has been moved and is no longer available"));
        }
        if mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' more than once", name),
                self.current_span.start_line,
                self.current_span.start_column,
                "",
                ErrorCode::E0007,
            )
            .with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if mutable && self.is_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot mutably borrow '{}' while immutably borrowed", name),
                self.current_span.start_line,
                self.current_span.start_column,
                "",
                ErrorCode::E0007,
            )
            .with_suggestion("Wait for the immutable borrow to end"));
        }
        if !mutable && self.is_mutably_borrowed(name) {
            return Err(CompileError::simple(
                &format!("Cannot read '{}' while it is mutably borrowed", name),
                self.current_span.start_line,
                self.current_span.start_column,
                "",
                ErrorCode::E0007,
            )
            .with_suggestion("Wait for the mutable borrow to end before reading"));
        }
        Ok(())
    }
    pub(super) fn collect_deferred_captures(&self, stmt: &Stmt, captured: &mut HashSet<String>) {
        match stmt {
            Stmt::VarDecl { value, .. } => {
                self.collect_expr_captures(value, captured);
            }
            Stmt::Import { .. } => {}
            Stmt::RegionBlock { body, .. } | Stmt::UnsafeBlock { body, .. } => {
                for s in body {
                    self.collect_deferred_captures(s, captured);
                }
            }
            Stmt::Assign { name, value, .. } => {
                captured.insert(name.clone());
                self.collect_expr_captures(value, captured);
            }
            Stmt::ArrayAssign { index, value, .. } => {
                self.collect_expr_captures(index, captured);
                self.collect_expr_captures(value, captured);
            }
            Stmt::FieldAssign { target, value, .. } => {
                captured.insert(target.clone());
                self.collect_expr_captures(value, captured);
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    self.collect_expr_captures(e, captured);
                }
            }
            Stmt::Print { expr, .. } => {
                self.collect_expr_captures(expr, captured);
            }
            Stmt::Defer { stmt, .. } => {
                self.collect_deferred_captures(stmt, captured);
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Spawn { body, .. } => {
                for s in body {
                    self.collect_deferred_captures(s, captured);
                }
            }
            Stmt::Parallel { blocks, .. } => {
                for block in blocks {
                    for s in block {
                        self.collect_deferred_captures(s, captured);
                    }
                }
            }
            Stmt::ChannelDecl { .. } => {}
            Stmt::Send { value, .. } => {
                self.collect_expr_captures(value, captured);
            }
            Stmt::Receive { .. } => {}
            Stmt::Expression(expr) => {
                self.collect_expr_captures(expr, captured);
            }
        }
    }
    pub(super) fn collect_expr_captures(&self, expr: &Expr, captured: &mut HashSet<String>) {
        match &expr.kind {
            ExprKind::Var(name, _) => {
                captured.insert(name.clone());
            }
            ExprKind::Number(_, _)
            | ExprKind::Int(_, _)
            | ExprKind::String(_, _)
            | ExprKind::Bool(_, _)
            | ExprKind::NullPtr(_)
            | ExprKind::PtrLiteral(_, _)
            | ExprKind::None(_) => {}
            ExprKind::Binary { left, right, .. } => {
                self.collect_expr_captures(left, captured);
                self.collect_expr_captures(right, captured);
            }
            ExprKind::Unary { expr, .. }
            | ExprKind::Deref { expr, .. }
            | ExprKind::AddrOf { expr, .. }
            | ExprKind::Borrow { expr, .. }
            | ExprKind::MutBorrow { expr, .. }
            | ExprKind::Some { value: expr, .. }
            | ExprKind::Ok { value: expr, .. }
            | ExprKind::Error { value: expr, .. }
            | ExprKind::FieldAccess { object: expr, .. } => {
                self.collect_expr_captures(expr, captured);
            }
            ExprKind::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_expr_captures(arg, captured);
                }
            }
            ExprKind::ArrayAccess { array, index, .. } => {
                self.collect_expr_captures(array, captured);
                self.collect_expr_captures(index, captured);
            }
            ExprKind::List(items, _) => {
                for item in items {
                    self.collect_expr_captures(item, captured);
                }
            }
            ExprKind::RecordLiteral { fields, .. } => {
                for (_, v) in fields {
                    self.collect_expr_captures(v, captured);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.collect_expr_captures(condition, captured);
                self.collect_expr_captures(then_branch, captured);
                if let Some(e) = else_branch {
                    self.collect_expr_captures(e, captured);
                }
            }
            ExprKind::Match { value, cases, .. } => {
                self.collect_expr_captures(value, captured);
                for case in cases {
                    self.collect_expr_captures(&case.body, captured);
                }
            }
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for s in statements {
                    self.collect_deferred_captures(s, captured);
                }
                if let Some(e) = trailing_expr {
                    self.collect_expr_captures(e, captured);
                }
            }
            ExprKind::TryCatch {
                try_branch,
                catch_branch,
                finally_body,
                ..
            } => {
                self.collect_expr_captures(try_branch, captured);
                self.collect_expr_captures(catch_branch, captured);
                if let Some(body) = finally_body {
                    for s in body {
                        self.collect_deferred_captures(s, captured);
                    }
                }
            }
            ExprKind::For {
                iterable,
                body,
                trailing_expr,
                ..
            } => {
                self.collect_expr_captures(iterable, captured);
                for s in body {
                    self.collect_deferred_captures(s, captured);
                }
                if let Some(e) = trailing_expr {
                    self.collect_expr_captures(e, captured);
                }
            }
            ExprKind::While {
                condition,
                body,
                trailing_expr,
                ..
            } => {
                self.collect_expr_captures(condition, captured);
                for s in body {
                    self.collect_deferred_captures(s, captured);
                }
                if let Some(e) = trailing_expr {
                    self.collect_expr_captures(e, captured);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(e) = start {
                    self.collect_expr_captures(e, captured);
                }
                if let Some(e) = end {
                    self.collect_expr_captures(e, captured);
                }
            }
        }
    }
}

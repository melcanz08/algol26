// src/semantics/analyzer/ownership.rs

use super::*;

/// ADR 0043. A pending shared-borrow write conflict, resolved
/// once every use of every borrow in the enclosing function is
/// known. `at` is the write's AST position; `place` is the
/// location being written.
pub(super) struct DeferredSharedWrite {
    pub(super) place: crate::semantics::state::Place,
    pub(super) at: ExprId,
    pub(super) span: Span,
}

impl SemanticAnalyzer {
    /// ADR 0043. True iff `place` is the target of a live shared
    /// borrow at AST position `at`. A borrow is live at `at` iff
    /// `created_expr <= at <= last_use`. `last_use` is the
    /// maximum recorded use position; if none is recorded, the
    /// borrow is treated as live through its scope's end.
    ///
    /// Consults `shared_borrow_meta`, not `state.borrows`:
    /// `pop_scope` prunes the borrow map at scope exit, which
    /// would hide borrows declared in inner blocks from the
    /// function-end flush.
    pub(super) fn is_live_shared_borrow_of(
        &self,
        place: &crate::semantics::state::Place,
        at: ExprId,
    ) -> bool {
        self.shared_borrow_meta.iter().any(|(key, (bp, created))| {
            bp.overlaps(place) && *created <= at && at <= self.last_use_of(key)
        })
    }

    fn last_use_of(&self, key: &str) -> ExprId {
        self.shared_borrow_uses
            .get(key)
            .and_then(|uses| uses.iter().copied().max())
            .unwrap_or(LAST_USE_SENTINEL)
    }

    /// ADR 0043. Record a use of the borrow bound to `binding`.
    /// Called from the analyzer's `Var`, `Deref`, and dotted
    /// `FunctionCall` arms. A binding that does not name a borrow
    /// is silently ignored.
    ///
    /// If any loop is open, the use is also recorded at
    /// `LAST_USE_SENTINEL`. On iteration N, a use in the body is
    /// followed by iteration N+1's write, so the borrow is live
    /// through the loop exit on every path that re-enters. A
    /// precise per-iteration interval is ADR 0043's deferred
    /// CFG work; this is the sound approximation.
    pub(super) fn record_borrow_use(&mut self, binding: &str, at: ExprId) {
        let Some(key) = self.binding_to_shared_borrow.get(binding).cloned() else {
            return;
        };
        let uses = self.shared_borrow_uses.entry(key).or_default();
        uses.push(at);
        // A use inside a loop body or a defer body outlives any
        // write that lexically follows it, because the use runs
        // again on the next iteration, or at the enclosing
        // scope's exit. Promote the borrow's last use to
        // LAST_USE_SENTINEL so a write after the use still
        // conflicts.
        if !self.loop_stack.is_empty() || self.defer_depth > 0 {
            uses.push(LAST_USE_SENTINEL);
        }
    }

    /// ADR 0043. Defer a shared-borrow write conflict for
    /// resolution at function end. Called from every write site.
    pub(super) fn defer_shared_borrow_check(
        &mut self,
        place: crate::semantics::state::Place,
        at: ExprId,
        span: Span,
    ) {
        self.deferred_shared_writes
            .push(DeferredSharedWrite { place, at, span });
    }

    /// ADR 0043. Resolve every deferred check. Runs at the end of
    /// `analyze_function`, after all uses are known.
    pub(super) fn flush_shared_borrow_checks(&mut self) -> Result<()> {
        for check in std::mem::take(&mut self.deferred_shared_writes) {
            if self.is_live_shared_borrow_of(&check.place, check.at) {
                return Err(CompileError::at(
                    check.span,
                    &format!(
                        "Cannot write to '{}' while a shared borrow of it is live",
                        check.place
                    ),
                    ErrorCode::E0007,
                )
                .with_suggestion(
                    "Move the write before the borrow, or move the                      borrow's last use before the write",
                ));
            }
        }
        Ok(())
    }

    /// ADR 0043. The eager part of a write-site borrow check:
    /// deferred-capture and mutable-borrow exclusivity. The
    /// shared-borrow case is deferred via
    /// `defer_shared_borrow_check` and resolved by
    /// `flush_shared_borrow_checks`.
    pub(super) fn check_write_borrow_rules(
        &self,
        place: &crate::semantics::state::Place,
    ) -> Result<()> {
        if let Some(scope) = self.deferred_captures.last() {
            if scope.contains(&place.base) {
                return Err(CompileError::at(
                    self.current_span,
                    &format!("Cannot use '{}' after it was captured by defer", place.base),
                    ErrorCode::E0007,
                )
                .with_suggestion("Deferred statements capture variables at declaration time"));
            }
        }
        if self.is_mutably_borrowed(place) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot write to '{}' while it is mutably borrowed", place),
                ErrorCode::E0007,
            )
            .with_suggestion("Wait for the mutable borrow to end"));
        }
        Ok(())
    }
    /// Register a mutable borrow of `source` with `Temporary` lifetime,
    /// for the duration of the current statement. Called by
    /// `Expr::FunctionCall` after analyzing a `&mut x` argument.
    ///
    /// The borrow rules for the argument were already checked by
    /// `Expr::MutBorrow`; this only installs the state entry.
    pub(super) fn register_call_argument_mut_borrow(&mut self, source: &str) {
        let call_id = self.state.fresh_call_id();
        let reference = format!("__tmp_call_{}", call_id.0);
        // ADR 0043. Call-argument temporaries have no AST position
        // of their own — their lifetime is `Temporary(call_id)`
        // and they are purged by `clear_all_temporary_borrows` at
        // the next statement boundary. Pass the `UNASSIGNED`
        // sentinel; the deferred shared-borrow check only consults
        // `shared_borrow_meta`, which this function does not
        // populate, so a `Temporary` mutable borrow is never seen
        // by it. No false positive is possible.
        self.state.borrow(
            reference,
            crate::semantics::state::Place::simple(source),
            BorrowKind::Mutable,
            BorrowLifetime::Temporary(call_id),
            ExprId::UNASSIGNED,
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

    /// True iff `ty` is `Copy`, taking record declarations into account.
    ///
    /// A record is `Copy` iff every field's (substituted) type is `Copy`.
    /// See ADR 0026. Records whose declaration is missing, whose type
    /// arguments don't match the declared arity, or whose fields resolve
    /// to `Unknown`, are conservatively treated as move-only.
    pub(super) fn is_type_copy(&self, ty: &Type) -> bool {
        match ty {
            Type::Record(name, args) => {
                let Some(info) = self.records.get(name) else {
                    return false;
                };
                if info.type_params.len() != args.len() {
                    return false;
                }
                let subst: HashMap<String, Type> = info
                    .type_params
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();
                info.fields
                    .iter()
                    .all(|(_, field_ty)| self.is_type_copy(&field_ty.substitute(&subst)))
            }
            // Copy propagates through these container types. `Option<Copy>`
            // and `Result<Copy, Copy>` are Copy in Rust; tuples of Copy
            // components are Copy.
            Type::Option(inner) => self.is_type_copy(inner),
            Type::Result { ok, error } => self.is_type_copy(ok) && self.is_type_copy(error),
            Type::Tuple(elems) => elems.iter().all(|e| self.is_type_copy(e)),
            // Everything else — including List<T>, Channel<T>, Array<T, N>,
            // Borrow<T>, MutBorrow<T>, Pointer<T> — falls through to the
            // type's own is_copy rule. `List<Float>` is move-only, matching
            // Rust's `Vec<T>`: a heap-allocated container is not Copy
            // regardless of its element type.
            _ => ty.is_copy(),
        }
    }

    pub(super) fn mark_moved(&mut self, name: &str, span: Span) {
        self.state.move_out(name, span);
    }

    /// The source location of the move that consumed `name`, if any.
    /// Returns `None` when the variable was never moved, or was
    /// moved by a path that did not record a span (e.g. a join
    /// where neither branch preserved one).
    pub(super) fn moved_at(&self, name: &str) -> Option<Span> {
        self.state.vars.get(name).and_then(|s| s.moved_at)
    }

    /// ADR 0043. Returns the synthetic borrower key so the
    /// enclosing `Stmt::VarDecl` can bind it to the declaration's
    /// name. `created_expr` is the `ExprId` of the `&x`
    /// expression; the borrow's interval starts there.
    pub(super) fn mark_borrowed(
        &mut self,
        place: &crate::semantics::state::Place,
        created_expr: ExprId,
    ) -> String {
        // Shared borrows have no declaration site, so give each one a
        // unique synthetic borrower key. Using a per-source key would
        // collapse nested borrows of the same place into one entry and
        // lose the outer borrow when the inner scope exits.
        let borrower = format!(
            "__shared_{}_{}_{}",
            place.base,
            place.projections.len(),
            self.state.fresh_node_id().0
        );
        let lt = if let Some(cur) = self.state.current_region().cloned() {
            BorrowLifetime::Region(cur)
        } else {
            BorrowLifetime::Local(borrower.clone())
        };
        self.state.borrow(
            borrower.clone(),
            place.clone(),
            BorrowKind::Shared,
            lt,
            created_expr,
        );
        // ADR 0043. Side table that survives `pop_scope`'s prune
        // of `state.borrows`. The deferred checks at function end
        // need to see borrows declared in inner blocks, which
        // `pop_scope` would otherwise have removed.
        self.shared_borrow_meta
            .insert(borrower.clone(), (place.clone(), created_expr));
        borrower
    }

    pub(super) fn is_mutably_borrowed(&self, place: &crate::semantics::state::Place) -> bool {
        self.state
            .borrows
            .values()
            .any(|b| b.kind == BorrowKind::Mutable && b.place.overlaps(place))
    }

    pub(super) fn is_borrowed(&self, place: &crate::semantics::state::Place) -> bool {
        self.state.borrows.values().any(|b| b.place.overlaps(place))
    }

    /// Remove the mutable borrow held by the reference binding
    /// `reference`. Called from `Stmt::Assign` when a write through
    /// a reference releases that reference's borrow — the key is
    /// the reference's own name (`w` for `var w := &mut x`), not
    /// the borrowed place (`x`).
    ///
    /// Scope-exit release is a separate path: `pop_scope` retains
    /// on the exiting variable set and the borrow's place base.
    /// Collapsing the two would make this call remove every borrow
    /// of `x` on write-through, which is wrong.
    pub(super) fn release_mutable_borrow(&mut self, reference: &str) {
        self.state.borrows.remove(reference);
    }

    /// ADR 0044 stage 2. Install a mutable borrow keyed on
    /// `reference` (the binding name), storing the borrowed `place`
    /// inside the `BorrowState`. Two mutable borrows of disjoint
    /// fields have distinct binding names, so the key distinguishes
    /// them without a synthetic id. Overlap checks use the full
    /// place, so `p.x` and `p.y` are non-overlapping.
    pub(super) fn register_mutable_borrow(
        &mut self,
        reference: &str,
        place: crate::semantics::state::Place,
        created_expr: ExprId,
    ) -> Result<()> {
        let source = &place.base;
        match self.lookup_variable(source) {
            Some((_, false)) => {
                return Err(CompileError::at(
                    self.current_span,
                    &format!("Cannot mutably borrow immutable variable '{}'", source),
                    ErrorCode::E0007,
                )
                .with_suggestion(&format!("Declare '{}' with 'var' instead of 'val'", source)));
            }
            Some((_, true)) => {}
            None => {}
        }
        if self.is_moved(source) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot mutably borrow moved variable '{}'", source),
                ErrorCode::E0007,
            )
            .with_suggestion("The variable has already been moved"));
        }
        // Overlap checks run against the full place; `p.x` and
        // `p.y` are non-overlapping by `Place::overlaps`.
        if self.is_mutably_borrowed(&place) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot mutably borrow '{}' more than once", source),
                ErrorCode::E0007,
            )
            .with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if self.is_borrowed(&place) {
            return Err(CompileError::at(
                self.current_span,
                &format!(
                    "Cannot mutably borrow '{}' while immutably borrowed",
                    source
                ),
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
            place,
            BorrowKind::Mutable,
            lt,
            created_expr,
        );
        Ok(())
    }

    /// ADR 0044. Derive the `Place` an expression refers to, if it
    /// can be determined statically.
    ///
    /// Recognizes:
    ///   Var(name)                    -> Place::simple(name)
    ///   FieldAccess(place, field)    -> place + Field(field)
    ///   ArrayAccess(place, Int lit)  -> place + Index(n)
    ///
    /// Returns `None` for computed indices, method-call receivers,
    /// dereferences of non-places, and anything else the analyzer
    /// cannot resolve to a stable location. Callers that get `None`
    /// should fall back to `Place::simple(base_name_of(expr))` when
    /// a base name exists, or skip place-based tracking entirely.
    ///
    /// This is deliberately conservative: a false negative (missing
    /// a place) degrades to name-based tracking, which over-rejects
    /// but never under-rejects. A false positive would be unsound.
    pub(super) fn place_of_expr(&self, expr: &Expr) -> Option<crate::semantics::state::Place> {
        use crate::semantics::state::{Place, Projection};
        match &expr.kind {
            ExprKind::Var(name, _) => Some(Place::simple(name.clone())),
            ExprKind::FieldAccess { object, field, .. } => {
                let mut base = self.place_of_expr(object)?;
                base.projections.push(Projection::Field(field.clone()));
                Some(base)
            }
            ExprKind::ArrayAccess { array, index, .. } => {
                // Only literal indices produce a stable place. A
                // computed index i would make `xs[i]` and `xs[j]`
                // indistinguishable, so we bail and let the caller
                // use the base name instead.
                let ExprKind::Int(n, _) = &index.as_ref().kind else {
                    return None;
                };
                if *n < 0 {
                    return None;
                }
                let mut base = self.place_of_expr(array)?;
                base.projections.push(Projection::Index(*n as usize));
                Some(base)
            }
            ExprKind::Deref { expr, .. } => {
                // ADR 0056. `*e` is a place: whatever `e` resolves
                // to, projected through `Deref`. The borrow checker
                // sees the syntactic path; referent resolution is
                // the interpreter's / codegen's job.
                let mut base = self.place_of_expr(expr)?;
                base.projections.push(Projection::Deref);
                Some(base)
            }
            _ => None,
        }
    }

    /// ADR 0044 stage 2. The place-aware counterpart of
    /// `check_borrow_rules`. The existing function keys on a bare
    /// name; this one takes a `Place` so a borrow of `p.x` can be
    /// checked against existing borrows on `p`, `p.x`, and `p.y`
    /// (overlap via `Place::overlaps`).
    ///
    /// The deferred-capture check still uses the place's base name
    /// — a deferred statement captures the whole variable, so any
    /// projection of it is equally captured.
    pub(super) fn check_borrow_rules_place(
        &self,
        place: &crate::semantics::state::Place,
        mutable: bool,
    ) -> Result<()> {
        // Deferred capture: a name captured by defer is frozen
        // regardless of which projection of it is being borrowed.
        if let Some(scope) = self.deferred_captures.last() {
            if scope.contains(&place.base) {
                return Err(CompileError::at(
                    self.current_span,
                    &format!("Cannot use '{}' after it was captured by defer", place.base),
                    ErrorCode::E0007,
                )
                .with_suggestion("Deferred statements capture variables at declaration time"));
            }
        }
        // Move check: use the base name; moving a field is not a
        // distinct operation today.
        if self.is_moved(&place.base) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Use of moved variable '{}'", place.base),
                ErrorCode::E0007,
            )
            .with_suggestion(
                "Variable ownership was transferred and cannot be used in this scope",
            ));
        }
        // Overlap check: a second mutable borrow, or a shared
        // borrow of a place currently mutably borrowed, is
        // rejected. The exact set of overlapping borrows comes
        // from the analyzer's state; if the analyzer doesn't yet
        // expose a query for it, fall back to the name-based
        // check (over-rejects on `p.x` / `p.y`, which is safe).
        if mutable && self.is_mutably_borrowed(place) {
            return Err(CompileError::at(
                self.current_span,
                &format!(
                    "Cannot mutably borrow '{}' while it is mutably borrowed",
                    place
                ),
                ErrorCode::E0007,
            )
            .with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if mutable && self.is_borrowed(place) {
            return Err(CompileError::at(
                self.current_span,
                &format!(
                    "Cannot mutably borrow '{}' while it is immutably borrowed",
                    place
                ),
                ErrorCode::E0007,
            )
            .with_suggestion("Wait for the immutable borrow to end"));
        }
        if !mutable && self.is_mutably_borrowed(place) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot borrow '{}' while it is mutably borrowed", place),
                ErrorCode::E0007,
            )
            .with_suggestion("Wait for the mutable borrow to end before borrowing again"));
        }
        Ok(())
    }

    pub(super) fn check_borrow_rules(&self, name: &str, mutable: bool) -> Result<()> {
        // Deferred-capture check unchanged.
        if let Some(scope) = self.deferred_captures.last() {
            if scope.contains(name) {
                return Err(CompileError::at(
                    self.current_span,
                    &format!("Cannot use '{}' after it was captured by defer", name),
                    ErrorCode::E0007,
                )
                .with_suggestion("Deferred statements capture variables at declaration time"));
            }
        }
        let place = crate::semantics::state::Place::simple(name);
        if self.is_moved(name) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot borrow moved variable '{}'", name),
                ErrorCode::E0007,
            )
            .with_suggestion("The variable has been moved and is no longer available"));
        }
        if mutable && self.is_mutably_borrowed(&place) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot mutably borrow '{}' more than once", name),
                ErrorCode::E0007,
            )
            .with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if mutable && self.is_borrowed(&place) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot mutably borrow '{}' while immutably borrowed", name),
                ErrorCode::E0007,
            )
            .with_suggestion("Wait for the immutable borrow to end"));
        }
        if !mutable && self.is_mutably_borrowed(&place) {
            return Err(CompileError::at(
                self.current_span,
                &format!("Cannot read '{}' while it is mutably borrowed", name),
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
            Stmt::DerefAssign { target, value, .. } => {
                self.collect_expr_captures(target, captured);
                self.collect_expr_captures(value, captured);
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
            ExprKind::MethodCall { receiver, args, .. } => {
                self.collect_expr_captures(receiver, captured);
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
            ExprKind::MapLiteral { entries, .. } => {
                for (k, v) in entries {
                    self.collect_expr_captures(k, captured);
                    self.collect_expr_captures(v, captured);
                }
            }
            ExprKind::SetLiteral { elements, .. } => {
                for e in elements {
                    self.collect_expr_captures(e, captured);
                }
            }
        }
    }
}

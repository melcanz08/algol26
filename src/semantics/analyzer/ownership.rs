// src/semantics/analyzer/ownership.rs

use super::*;
impl SemanticAnalyzer {
    pub(super) fn release_mutable_borrow(&mut self, reference: &str) {
        let scope_idx = self.mutable_borrows.iter().enumerate().rev().find(|(_,map)| map.contains_key(reference)).map(|(i,_)| i);
        let Some(idx)=scope_idx else { return; };
        let source = match self.mutable_borrows[idx].remove(reference) { Some(src)=>src, None=>return, };
        if let Some(set)=self.mutably_borrowed.get_mut(idx) { set.remove(&source); }
        // NEW: also release in unified state
        self.state.borrows.remove(reference);
    }
    pub(super) fn all_moved_vars(&self) -> Vec<String> {
        let mut result=Vec::new();
        for scope in &self.moved_vars { for var in scope { if !result.contains(var) { result.push(var.clone()); } } }
        result
    }
    pub(super) fn is_moved(&self, name: &str) -> bool {
        // FIX for conditional moves: only check lexical moved_vars stack
        // state.vars is global and would cause second branch to see move from first branch
        // The proper join happens after if via SemanticState::join
        // We still check state.vars for moves that happened outside conditional (for soundness)
        // but we need to allow moves in sibling branches
        // For now, check stack only - state join will handle post-if state
        // If var is in current or outer moved_vars, it's moved in this path
        self.moved_vars.iter().any(|scope| scope.iter().any(|v| v==name))
    }
    pub(super) fn mark_moved(&mut self, name: &str) {
        if let Some(scope)=self.moved_vars.last_mut() { if !scope.contains(&name.to_string()) { scope.push(name.to_string()); } }
        self.state.move_out(name);
    }
    pub(super) fn mark_borrowed(&mut self, name: &str) {
        if let Some(scope)=self.borrowed_vars.last_mut() { scope.insert(name.to_string()); }
    }
    pub(super) fn mark_mutably_borrowed(&mut self, name: &str) {
        if let Some(scope)=self.mutably_borrowed.last_mut() { scope.insert(name.to_string()); }
    }
    pub(super) fn is_mutably_borrowed(&self, name: &str) -> bool {
        if self.state.is_mutably_borrowed(name) { return true; }
        self.mutably_borrowed.iter().rev().any(|scope| scope.contains(name))
    }
    pub(super) fn register_mutable_borrow(&mut self, reference: &str, source: &str) -> Result<()> {
        match self.lookup_variable(source) {
            Some((_,false))=>{ return Err(CompileError::simple(&format!("Cannot mutably borrow immutable variable '{}'", source), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion(&format!("Declare '{}' with 'var' instead of 'val'", source))); },
            Some((_,true))=>{},
            None=>{},
        }
        if self.is_moved(source) {
            return Err(CompileError::simple(&format!("Cannot mutably borrow moved variable '{}'", source), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("The variable has already been moved"));
        }
        if self.is_mutably_borrowed(source) {
            return Err(CompileError::simple(&format!("Cannot mutably borrow '{}' more than once", source), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("Only one mutable borrow is allowed at a time"));
        }
        if self.is_borrowed(source) {
            return Err(CompileError::simple(&format!("Cannot mutably borrow '{}' while immutably borrowed", source), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("Wait for the immutable borrow to end"));
        }
        self.mark_mutably_borrowed(source);
        if let Some(scope)=self.mutable_borrows.last_mut() { scope.insert(reference.to_string(), source.to_string()); }
        // NEW: mirror into unified state with proper lifetime
        let lt = if let Some(cur)=self.state.current_region().cloned() { crate::semantics::state::BorrowLifetime::Region(cur) } else { crate::semantics::state::BorrowLifetime::Local(reference.to_string()) };
        self.state.borrow(reference.to_string(), source.to_string(), crate::semantics::state::BorrowKind::Mutable, lt);
        Ok(())
    }
    pub(super) fn is_borrowed(&self, name: &str) -> bool {
        if self.state.is_borrowed(name) { return true; }
        self.borrowed_vars.iter().rev().any(|scope| scope.contains(name))
    }
    pub(super) fn check_borrow_rules(&self, name: &str, mutable: bool) -> Result<()> {
        if let Some(scope)=self.deferred_captures.last() { if scope.contains(name) { return Err(CompileError::simple(&format!("Cannot use '{}' after it was captured by defer", name), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("Deferred statements capture variables at declaration time")); } }
        if self.is_moved(name) { return Err(CompileError::simple(&format!("Cannot borrow moved variable '{}'", name), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("The variable has been moved and is no longer available")); }
        if mutable && self.is_mutably_borrowed(name) { return Err(CompileError::simple(&format!("Cannot mutably borrow '{}' more than once", name), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("Only one mutable borrow is allowed at a time")); }
        if mutable && self.is_borrowed(name) { return Err(CompileError::simple(&format!("Cannot mutably borrow '{}' while immutably borrowed", name), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("Wait for the immutable borrow to end")); }
        if !mutable && self.is_mutably_borrowed(name) { return Err(CompileError::simple(&format!("Cannot read '{}' while it is mutably borrowed", name), self.current_span.start_line, self.current_span.start_column,"",ErrorCode::E0007).with_suggestion("Wait for the mutable borrow to end before reading")); }
        Ok(())
    }
    pub(super) fn collect_deferred_captures(&self, stmt: &Stmt, captured: &mut HashSet<String>) {
        match stmt {
            Stmt::VarDecl{value,..}=>{ self.collect_expr_captures(value, captured); },
            Stmt::Import{..}=>{},
            Stmt::RegionBlock{body,..}|Stmt::UnsafeBlock{body,..}=>{ for s in body { self.collect_deferred_captures(s, captured); } },
            Stmt::Assign{name,value,..}=>{ captured.insert(name.clone()); self.collect_expr_captures(value, captured); },
            Stmt::ArrayAssign{index,value,..}=>{ self.collect_expr_captures(index, captured); self.collect_expr_captures(value, captured); },
            Stmt::Return{value,..}=>{ if let Some(e)=value { self.collect_expr_captures(e, captured); } },
            Stmt::Print{expr,..}=>{ self.collect_expr_captures(expr, captured); },
            Stmt::Defer{stmt,..}=>{ self.collect_deferred_captures(stmt, captured); },
            Stmt::Break(_)|Stmt::Continue(_)=>{},
            Stmt::Spawn{body,..}=>{ for s in body { self.collect_deferred_captures(s, captured); } },
            Stmt::Parallel{blocks,..}=>{ for block in blocks { for s in block { self.collect_deferred_captures(s, captured); } } },
            Stmt::ChannelDecl{..}=>{},
            Stmt::Send{value,..}=>{ self.collect_expr_captures(value, captured); },
            Stmt::Receive{..}=>{},
            Stmt::Expression(expr)=>{ self.collect_expr_captures(expr, captured); },
        }
    }
    pub(super) fn collect_expr_captures(&self, expr: &Expr, captured: &mut HashSet<String>) {
        match expr {
            Expr::Var(name,_)=>{ captured.insert(name.clone()); },
            Expr::Number(_,_) | Expr::Int(_,_) | Expr::String(_,_) | Expr::Bool(_,_) | Expr::NullPtr(_) | Expr::PtrLiteral(_,_) | Expr::None(_)=>{},
            Expr::Binary{left,right,..}=>{ self.collect_expr_captures(left, captured); self.collect_expr_captures(right, captured); },
            Expr::Unary{expr,..} | Expr::Deref{expr,..} | Expr::AddrOf{expr,..} | Expr::Borrow{expr,..} | Expr::MutBorrow{expr,..} | Expr::Some{value:expr,..} | Expr::Ok{value:expr,..} | Expr::Error{value:expr,..} | Expr::FieldAccess{object:expr,..}=>{ self.collect_expr_captures(expr, captured); },
            Expr::FunctionCall{args,..}=>{ for arg in args { self.collect_expr_captures(arg, captured); } },
            Expr::ArrayAccess{array,index,..}=>{ self.collect_expr_captures(array, captured); self.collect_expr_captures(index, captured); },
            Expr::List(items,_)=>{ for item in items { self.collect_expr_captures(item, captured); } },
            Expr::If{condition,then_branch,else_branch,..}=>{ self.collect_expr_captures(condition, captured); self.collect_expr_captures(then_branch, captured); if let Some(e)=else_branch { self.collect_expr_captures(e, captured); } },
            Expr::Match{value,cases,..}=>{ self.collect_expr_captures(value, captured); for case in cases { self.collect_expr_captures(&case.body, captured); } },
            Expr::Block{statements,trailing_expr,..}=>{ for s in statements { self.collect_deferred_captures(s, captured); } if let Some(e)=trailing_expr { self.collect_expr_captures(e, captured); } },
            Expr::TryCatch{try_branch,catch_branch,finally_body,..}=>{ self.collect_expr_captures(try_branch, captured); self.collect_expr_captures(catch_branch, captured); if let Some(body)=finally_body { for s in body { self.collect_deferred_captures(s, captured); } } },
            Expr::For{iterable,body,trailing_expr,..}=>{ self.collect_expr_captures(iterable, captured); for s in body { self.collect_deferred_captures(s, captured); } if let Some(e)=trailing_expr { self.collect_expr_captures(e, captured); } },
            Expr::While{condition,body,trailing_expr,..}=>{ self.collect_expr_captures(condition, captured); for s in body { self.collect_deferred_captures(s, captured); } if let Some(e)=trailing_expr { self.collect_expr_captures(e, captured); } },
            Expr::Range{start,end,..}=>{
                if let Some(e)=start { self.collect_expr_captures(e, captured); }
                if let Some(e)=end { self.collect_expr_captures(e, captured); }
            },
        }
    }
}
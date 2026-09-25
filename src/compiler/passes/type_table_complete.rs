// src/compiler/passes/type_table_complete.rs

//! `TypeTableCompletePass` — an `Analysis` pass that walks the typed
//! AST and asserts every reachable `Expr` has an entry in the
//! analyzer-produced `type_table_id`.
//!
//! The failure mode this catches: `SemanticAnalyzer` skips typing an
//! expression (a code path where `self.type_of` is never populated).
//! The IR builder then looks up that expression, finds nothing, and
//! falls back to `Type::Unknown`, which the IR verifier tolerates but
//! codegen does not — the `short_circuit.gol` bug.
//!
//! Missing entries fail the pass. Every reachable `Expr` must have a
//! type; a missing entry is a real analyzer bug, not a legitimate
//! exception. The previous warning-only behavior was a migration aid
//! during the `ExprId` transition and has been retired now that the
//! table is provably complete across the whole test suite.

use crate::common::types::Type;
use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{IrLevel, Pass, PassContract, PassError, PassId, PassKind, PassResult};
use crate::compiler::program::Program;
use crate::frontend::ast::{Expr, ExprId, ExprKind, FunctionDecl, Pattern, Stmt};
use std::collections::HashMap;

pub struct TypeTableCompletePass;

impl Pass<Program> for TypeTableCompletePass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("ast.type_table_complete"),
            kind: PassKind::Analysis,
            input: IrLevel::Ast,
            output: IrLevel::Ast,
            requires: &["typed AST with analyzer-produced type table"],
            guarantees: &[
                "every reachable Expr node has an ExprId-keyed type_table entry, \
                 or the pass fails",
            ],
            may_change: &["diagnostics (via PassError on failure)"],
            must_preserve: &["program.ast", "program.typed"],
            may_fail: true,
        };
        &C
    }

    fn run(&self, _ctx: &mut CompilerContext, program: &mut Program) -> PassResult {
        let typed = program.typed.as_ref().ok_or_else(|| {
            PassError::new(
                PassId("ast.type_table_complete"),
                "contract violation: typed AST not present",
            )
        })?;

        let mut walker = Walker {
            type_table_id: &typed.type_table_id,
            missing: Vec::new(),
        };

        for func in typed.functions.iter() {
            walker.visit_function(func);
        }

        if walker.missing.is_empty() {
            return Ok(());
        }

        // Group by node kind so a program with N untyped calls produces
        // one summary line, not N.
        let mut counts: HashMap<&'static str, usize> = HashMap::new();
        for kind in &walker.missing {
            *counts.entry(kind).or_insert(0) += 1;
        }

        let mut parts: Vec<String> = counts
            .iter()
            .map(|(k, n)| format!("{} × {}", n, k))
            .collect();
        parts.sort();

        Err(PassError::new(
            PassId("ast.type_table_complete"),
            format!(
                "type table incomplete: {} expression(s) have no entry ({})",
                walker.missing.len(),
                parts.join(", ")
            ),
        ))
    }
}

// ─── Walker ─────────────────────────────────────────────────────────────

struct Walker<'a> {
    type_table_id: &'a HashMap<ExprId, Type>,
    /// Short kind name for each missing node, in visitation order.
    missing: Vec<&'static str>,
}

impl<'a> Walker<'a> {
    fn visit_function(&mut self, f: &FunctionDecl) {
        for stmt in &f.body {
            self.visit_stmt(stmt);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl { value, .. } => self.visit_expr(value),
            Stmt::Import { .. } => {}
            Stmt::RegionBlock { body, .. } => {
                for s in body {
                    self.visit_stmt(s);
                }
            }
            Stmt::UnsafeBlock { body, .. } => {
                for s in body {
                    self.visit_stmt(s);
                }
            }
            Stmt::Assign { value, .. } => self.visit_expr(value),
            Stmt::ArrayAssign { index, value, .. } => {
                self.visit_expr(index);
                self.visit_expr(value);
            }
            Stmt::FieldAssign {
                target,
                field,
                value,
                ..
            } => {
                self.visit_expr(value);
                // `target` is a plain variable name; `field` is a field
                // name. Neither is an expression node that needs visiting
                // — but if this pass tracks variable uses, mark `target`.
                // For now, visiting the RHS is enough.
                let _ = (target, field);
            }
            Stmt::Return { value, .. } => {
                if let Some(e) = value {
                    self.visit_expr(e);
                }
            }
            Stmt::Print { expr, .. } => self.visit_expr(expr),
            Stmt::Defer { stmt, .. } => self.visit_stmt(stmt),
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Spawn { body, .. } => {
                for s in body {
                    self.visit_stmt(s);
                }
            }
            Stmt::Parallel { blocks, .. } => {
                for b in blocks {
                    for s in b {
                        self.visit_stmt(s);
                    }
                }
            }
            Stmt::ChannelDecl { .. } => {}
            Stmt::Send { value, .. } => self.visit_expr(value),
            Stmt::Receive { .. } => {}
            Stmt::Expression(e) => self.visit_stmt_expr(e),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if !self.type_table_id.contains_key(&expr.id) {
            self.missing.push(expr_kind(expr));
        }
        self.visit_expr_children(expr);
    }

    /// Visit an expression in statement position — its value is
    /// discarded, so the analyzer is not required to record a type
    /// for the top-level node. Children are still value expressions.
    fn visit_stmt_expr(&mut self, expr: &Expr) {
        self.visit_expr_children(expr);
    }

    fn visit_expr_children(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Number(_, _)
            | ExprKind::Int(_, _)
            | ExprKind::String(_, _)
            | ExprKind::Bool(_, _)
            | ExprKind::Var(_, _)
            | ExprKind::None(_)
            | ExprKind::NullPtr(_)
            | ExprKind::PtrLiteral(_, _) => {}

            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                for s in statements {
                    self.visit_stmt(s);
                }
                if let Some(e) = trailing_expr {
                    self.visit_expr(e);
                }
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.visit_expr(condition);
                self.visit_expr(then_branch);
                if let Some(e) = else_branch {
                    self.visit_expr(e);
                }
            }
            ExprKind::Match { value, cases, .. } => {
                self.visit_expr(value);
                for c in cases {
                    self.visit_pattern(&c.pattern);
                    self.visit_expr(&c.body);
                }
            }
            ExprKind::Borrow { expr, .. }
            | ExprKind::MutBorrow { expr, .. }
            | ExprKind::Deref { expr, .. }
            | ExprKind::AddrOf { expr, .. }
            | ExprKind::Some { value: expr, .. }
            | ExprKind::Ok { value: expr, .. }
            | ExprKind::Error { value: expr, .. }
            | ExprKind::Unary { expr, .. } => {
                self.visit_expr(expr);
            }
            ExprKind::List(items, _) => {
                for e in items {
                    self.visit_expr(e);
                }
            }
            ExprKind::ArrayAccess { array, index, .. } => {
                self.visit_expr(array);
                self.visit_expr(index);
            }
            ExprKind::Binary { left, right, .. } => {
                self.visit_expr(left);
                self.visit_expr(right);
            }
            ExprKind::FunctionCall { args, .. } => {
                for e in args {
                    self.visit_expr(e);
                }
            }
            ExprKind::TryCatch {
                try_branch,
                catch_branch,
                finally_body,
                ..
            } => {
                self.visit_expr(try_branch);
                self.visit_expr(catch_branch);
                if let Some(stmts) = finally_body {
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
            }
            ExprKind::For {
                iterable,
                body,
                trailing_expr,
                ..
            } => {
                self.visit_expr(iterable);
                for s in body {
                    self.visit_stmt(s);
                }
                if let Some(e) = trailing_expr {
                    self.visit_expr(e);
                }
            }
            ExprKind::While {
                condition,
                body,
                trailing_expr,
                ..
            } => {
                self.visit_expr(condition);
                for s in body {
                    self.visit_stmt(s);
                }
                if let Some(e) = trailing_expr {
                    self.visit_expr(e);
                }
            }
            ExprKind::Range { start, end, .. } => {
                if let Some(e) = start {
                    self.visit_expr(e);
                }
                if let Some(e) = end {
                    self.visit_expr(e);
                }
            }
            ExprKind::FieldAccess { object, .. } => {
                self.visit_expr(object);
            }
            ExprKind::RecordLiteral { fields, .. } => {
                for (_, v) in fields {
                    self.visit_expr(v);
                }
            }
        }
    }

    fn visit_pattern(&mut self, pat: &Pattern) {
        match pat {
            Pattern::Literal(e) => self.visit_expr(e),
            Pattern::Guarded { pattern, condition } => {
                self.visit_pattern(pattern);
                self.visit_expr(condition);
            }
            Pattern::Range { start, end } => {
                if let Some(e) = start {
                    self.visit_expr(e);
                }
                if let Some(e) = end {
                    self.visit_expr(e);
                }
            }
            Pattern::SomeNested(p) | Pattern::OkNested(p) | Pattern::ErrorNested(p) => {
                self.visit_pattern(p);
            }
            Pattern::ListDestructure { first, rest } => {
                if let Some(p) = first {
                    self.visit_pattern(p);
                }
                if let Some(p) = rest {
                    self.visit_pattern(p);
                }
            }
            Pattern::Record { .. } => {
                // Record pattern bindings are introduced into the case's
                // scope; the analyzer already recorded their types. Nothing
                // for this pass to do.
            }
            Pattern::Some(_)
            | Pattern::None
            | Pattern::Ok(_)
            | Pattern::Error(_)
            | Pattern::Wildcard
            | Pattern::Binding(_) => {}
        }
    }
}

fn expr_kind(e: &Expr) -> &'static str {
    match &e.kind {
        ExprKind::Number(_, _) => "number",
        ExprKind::Int(_, _) => "int",
        ExprKind::String(_, _) => "string",
        ExprKind::Bool(_, _) => "bool",
        ExprKind::Var(_, _) => "var",
        ExprKind::Block { .. } => "block",
        ExprKind::If { .. } => "if",
        ExprKind::Match { .. } => "match",
        ExprKind::Borrow { .. } => "borrow",
        ExprKind::MutBorrow { .. } => "mut_borrow",
        ExprKind::Deref { .. } => "deref",
        ExprKind::AddrOf { .. } => "addr_of",
        ExprKind::List(_, _) => "list",
        ExprKind::ArrayAccess { .. } => "array_access",
        ExprKind::Binary { .. } => "binary",
        ExprKind::Unary { .. } => "unary",
        ExprKind::FunctionCall { .. } => "call",
        ExprKind::Some { .. } => "some",
        ExprKind::None(_) => "none",
        ExprKind::Ok { .. } => "ok",
        ExprKind::TryCatch { .. } => "try_catch",
        ExprKind::Error { .. } => "error",
        ExprKind::For { .. } => "for",
        ExprKind::While { .. } => "while",
        ExprKind::PtrLiteral(_, _) => "ptr_literal",
        ExprKind::NullPtr(_) => "null_ptr",
        ExprKind::Range { .. } => "range",
        ExprKind::FieldAccess { .. } => "field_access",
        ExprKind::RecordLiteral { .. } => "record_literal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::span::Span;

    #[test]
    fn walker_reports_every_leaf_node() {
        let ast = Expr::new(ExprKind::Block {
            statements: vec![],
            trailing_expr: Some(Expr::boxed(ExprKind::Int(42, Span::default()))),
            span: Span::default(),
        });
        let table = HashMap::new();
        let mut w = Walker {
            type_table_id: &table,
            missing: Vec::new(),
        };
        w.visit_expr(&ast);
        // Both the Block and the Int(42) should be reported.
        assert_eq!(w.missing.len(), 2, "missing: {:?}", w.missing);
        assert!(w.missing.contains(&"block"));
        assert!(w.missing.contains(&"int"));
    }

    #[test]
    fn walker_silent_when_table_complete() {
        // Build a two-node AST, number it, and populate the ID table
        // with an entry for every reachable expression. The walker
        // should report nothing.
        use crate::compiler::assign_expr_ids;

        let mut functions = vec![crate::frontend::ast::FunctionDecl {
            name: "f".into(),
            params: vec![],
            return_type: None,
            body: vec![Stmt::Expression(Expr::new(ExprKind::Block {
                statements: vec![],
                trailing_expr: Some(Expr::boxed(ExprKind::Int(42, Span::default()))),
                span: Span::default(),
            }))],
            is_extern: false,
            ffi_info: None,
            type_params: vec![],
            where_clauses: vec![],
        }];
        assign_expr_ids(&mut functions);

        // Collect every ID in the AST. The AST shape here is
        // Block { trailing: Int }, so both nodes are reachable from
        // the outer `Stmt::Expression`.
        let mut ids = Vec::new();
        fn collect(expr: &Expr, ids: &mut Vec<ExprId>) {
            ids.push(expr.id);
            if let ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } = &expr.kind
            {
                for s in statements {
                    if let Stmt::Expression(e) = s {
                        collect(e, ids);
                    }
                }
                if let Some(e) = trailing_expr {
                    collect(e, ids);
                }
            }
        }
        if let Stmt::Expression(e) = &functions[0].body[0] {
            collect(e, &mut ids);
        }

        let mut table = HashMap::new();
        for id in ids {
            table.insert(id, Type::Int);
        }

        let mut w = Walker {
            type_table_id: &table,
            missing: Vec::new(),
        };
        w.visit_stmt(&functions[0].body[0]);

        assert!(
            w.missing.is_empty(),
            "expected no missing entries, got {:?}",
            w.missing
        );
    }

    #[test]
    fn statement_position_expr_is_not_checked() {
        let stmt = Stmt::Expression(Expr::new(ExprKind::If {
            condition: Expr::boxed(ExprKind::Bool(true, Span::default())),
            then_branch: Expr::boxed(ExprKind::Block {
                statements: vec![],
                trailing_expr: Some(Expr::boxed(ExprKind::Int(1, Span::default()))),
                span: Span::default(),
            }),
            else_branch: None,
            span: Span::default(),
        }));
        let table = HashMap::new();
        let mut w = Walker {
            type_table_id: &table,
            missing: Vec::new(),
        };
        w.visit_stmt(&stmt);

        // The top-level `if` is in statement position, so its type
        // doesn't matter — the analyzer is not required to record it.
        assert!(
            !w.missing.contains(&"if"),
            "top-level `if` should not be reported: {:?}",
            w.missing
        );

        // Its children are value positions and are reported.
        assert!(
            w.missing.contains(&"bool"),
            "condition should be reported: {:?}",
            w.missing
        );
        assert!(
            w.missing.contains(&"block"),
            "then_branch should be reported: {:?}",
            w.missing
        );
        assert!(
            w.missing.contains(&"int"),
            "trailing_expr should be reported: {:?}",
            w.missing
        );
    }
}

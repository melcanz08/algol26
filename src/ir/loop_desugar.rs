// src/ir/loop_desugar.rs

use crate::common::span::Span;
use crate::frontend::ast::{BinOp, Expr, FunctionDecl, Stmt};
use std::collections::HashMap;

pub fn desugar_loops(functions: &mut [FunctionDecl]) {
    for func in functions.iter_mut() {
        let mut env: HashMap<String, Vec<Expr>> = HashMap::new();
        func.body = desugar_stmts(std::mem::take(&mut func.body), &mut env);
    }
}

/// Desugar a block of statements whose declared variables must not
/// escape the block. Saves and restores the environment around the
/// recursive call.
///
/// Used for every nested scope: `if`/`match` branches, block
/// expressions, `for`/`while` bodies, `region`/`unsafe`/`spawn`/
/// `parallel` blocks. Without this, a variable shadowed inside the
/// nested scope would overwrite the outer binding in `env` and be
/// seen by subsequent loops at the outer level.
fn desugar_scoped_stmts(stmts: Vec<Stmt>, env: &mut HashMap<String, Vec<Expr>>) -> Vec<Stmt> {
    let saved = env.clone();
    let result = desugar_stmts(stmts, env);

    // Names present before the scope.
    let before_keys: std::collections::HashSet<String> = saved.keys().cloned().collect();
    // Names present after the scope.
    let after_keys: std::collections::HashSet<String> = env.keys().cloned().collect();

    // Names newly added inside the scope are local declarations; they
    // must not leak out.
    for name in after_keys.difference(&before_keys) {
        env.remove(name);
    }

    // Names that existed before and disappeared inside the scope were
    // removed by `ArrayAssign`'s handler, which calls `env.remove`.
    // That is a mutation to the outer binding, not a shadow; do not
    // restore them.
    let removed_inside: std::collections::HashSet<String> =
        before_keys.difference(&after_keys).cloned().collect();

    // Every surviving name is restored to its pre-scope value. For a
    // name the scope did not touch, this is a no-op assignment of the
    // same value. For a name shadowed by an inner `var`, this puts
    // the outer value back. Names removed inside the scope stay
    // removed.
    //
    // Before this change, the whole env was restored unconditionally,
    // which reinstated a stale list literal after any array mutation
    // inside a nested scope. `examples/sales_report.gol` hit this: the
    // `while` loop mutates `revenues` via `revenues[i] := ...`, the
    // mutation was reverted on scope exit, and the subsequent
    // `for rev in revenues` unrolled over the pre-mutation zeros.
    for (name, saved_val) in &saved {
        if !removed_inside.contains(name) {
            env.insert(name.clone(), saved_val.clone());
        }
    }
    result
}

fn desugar_stmts(stmts: Vec<Stmt>, env: &mut HashMap<String, Vec<Expr>>) -> Vec<Stmt> {
    let mut result = Vec::new();

    for stmt in stmts {
        match stmt {
            Stmt::VarDecl {
                name,
                value,
                type_annotation,
                mutable,
                span,
            } => {
                let desugared_value = desugar_expr(value, env);
                // Track the new list value. A non-list initializer
                // invalidates any prior list binding for this name.
                if let Expr::List(elements, _) = &desugared_value {
                    env.insert(name.clone(), elements.clone());
                } else {
                    env.remove(&name);
                }
                result.push(Stmt::VarDecl {
                    name,
                    value: desugared_value,
                    type_annotation,
                    mutable,
                    span,
                });
            }
            Stmt::Assign { name, value, span } => {
                // Reassignment invalidates the environment entry for
                // `name`. If the new value is a known list literal,
                // re-record it; otherwise remove the stale entry so
                // subsequent loops don't unroll over the old list.
                let desugared_value = desugar_expr(value, env);
                if let Expr::List(elements, _) = &desugared_value {
                    env.insert(name.clone(), elements.clone());
                } else {
                    env.remove(&name);
                }
                result.push(Stmt::Assign {
                    name,
                    value: desugared_value,
                    span,
                });
            }
            Stmt::ArrayAssign {
                array,
                index,
                value,
                span,
            } => {
                // Mutating a list invalidates any known-list binding.
                // We do not track element-wise updates.
                env.remove(&array);
                let desugared_index = desugar_expr(index, env);
                let desugared_value = desugar_expr(value, env);
                result.push(Stmt::ArrayAssign {
                    array,
                    index: desugared_index,
                    value: desugared_value,
                    span,
                });
            }
            Stmt::Expression(Expr::For {
                var,
                iterable,
                body,
                trailing_expr,
                span,
            }) => {
                let resolved_iterable = resolve_iterable(&iterable, env);

                if let Expr::List(elements, _) = &resolved_iterable {
                    // Unroll if known list and no complex control flow
                    if !has_complex_cf(&body) && trailing_expr.is_none() {
                        for elem in elements {
                            let substituted = substitute_var_literal(&body, &var, elem);
                            let folded = fold_constant_ifs(substituted);
                            let mut should_break = false;

                            for s in folded {
                                match s {
                                    Stmt::Break(_) => {
                                        should_break = true;
                                        break;
                                    }
                                    Stmt::Continue(_) => {
                                        break; // Continue to next iteration
                                    }
                                    _ => {
                                        // Each unrolled iteration is its
                                        // own scope: variables declared
                                        // inside the body must not leak
                                        // into sibling iterations or
                                        // past the loop.
                                        let inner = desugar_scoped_stmts(vec![s], env);
                                        result.extend(inner);
                                    }
                                }
                            }

                            if should_break {
                                break;
                            }
                        }
                    } else {
                        // Keep loop with resolved iterable. The body
                        // is a fresh scope.
                        let desugared_body = desugar_scoped_stmts(body, env);
                        result.push(Stmt::Expression(Expr::For {
                            var,
                            iterable: Box::new(resolved_iterable),
                            body: desugared_body,
                            trailing_expr,
                            span,
                        }));
                    }
                } else {
                    let desugared_body = desugar_scoped_stmts(body, env);
                    result.push(Stmt::Expression(Expr::For {
                        var,
                        iterable: Box::new(resolved_iterable),
                        body: desugared_body,
                        trailing_expr,
                        span,
                    }));
                }
            }
            Stmt::Expression(Expr::While {
                condition,
                body,
                trailing_expr,
                span,
            }) => {
                let desugared_body = desugar_scoped_stmts(body, env);
                result.push(Stmt::Expression(Expr::While {
                    condition,
                    body: desugared_body,
                    trailing_expr,
                    span,
                }));
            }
            Stmt::Expression(expr) => {
                let desugared = desugar_expr(expr, env);
                result.push(Stmt::Expression(desugared));
            }
            Stmt::Print { expr, span } => {
                result.push(Stmt::Print {
                    expr: desugar_expr(expr, env),
                    span,
                });
            }
            Stmt::RegionBlock { name, body, span } => {
                let desugared_body = desugar_scoped_stmts(body, env);
                result.push(Stmt::RegionBlock {
                    name,
                    body: desugared_body,
                    span,
                });
            }
            Stmt::UnsafeBlock { body, span } => {
                let desugared_body = desugar_scoped_stmts(body, env);
                result.push(Stmt::UnsafeBlock {
                    body: desugared_body,
                    span,
                });
            }
            Stmt::Spawn { body, span } => {
                let desugared_body = desugar_scoped_stmts(body, env);
                result.push(Stmt::Spawn {
                    body: desugared_body,
                    span,
                });
            }
            Stmt::Parallel { blocks, span } => {
                let desugared_blocks: Vec<Vec<Stmt>> = blocks
                    .into_iter()
                    .map(|b| desugar_scoped_stmts(b, env))
                    .collect();
                result.push(Stmt::Parallel {
                    blocks: desugared_blocks,
                    span,
                });
            }
            other => {
                result.push(other);
            }
        }
    }
    result
}

fn desugar_expr(expr: Expr, env: &mut HashMap<String, Vec<Expr>>) -> Expr {
    match expr {
        Expr::For {
            var,
            iterable,
            body,
            trailing_expr,
            span,
        } => {
            let resolved = resolve_iterable(&iterable, env);
            if let Expr::List(elements, _) = &resolved {
                if !has_complex_cf(&body) {
                    if let Some(te) = trailing_expr.as_ref() {
                        if let Some(last) = elements.last() {
                            return substitute_expr_literal(te, &var, last);
                        }
                    }
                }
            }
            let desugared_body = desugar_scoped_stmts(body, env);
            let desugared_trailing = trailing_expr.map(|te| Box::new(desugar_expr(*te, env)));
            Expr::For {
                var,
                iterable: Box::new(resolved),
                body: desugared_body,
                trailing_expr: desugared_trailing,
                span,
            }
        }
        Expr::While {
            condition,
            body,
            trailing_expr,
            span,
        } => {
            let desugared_body = desugar_scoped_stmts(body, env);
            let desugared_trailing = trailing_expr.map(|te| Box::new(desugar_expr(*te, env)));
            Expr::While {
                condition,
                body: desugared_body,
                trailing_expr: desugared_trailing,
                span,
            }
        }
        Expr::Block {
            statements,
            trailing_expr,
            span,
        } => {
            // The block's own variables are visible to its trailing
            // expression, so we desugar both inside the same scope.
            let saved = env.clone();
            let desugared_statements = desugar_stmts(statements, env);
            let desugared_trailing = trailing_expr.map(|te| Box::new(desugar_expr(*te, env)));
            *env = saved;
            Expr::Block {
                statements: desugared_statements,
                trailing_expr: desugared_trailing,
                span,
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => Expr::If {
            condition: Box::new(desugar_expr(*condition, env)),
            then_branch: Box::new(desugar_expr(*then_branch, env)),
            else_branch: else_branch.map(|e| Box::new(desugar_expr(*e, env))),
            span,
        },
        Expr::Binary {
            left,
            op,
            right,
            span,
        } => Expr::Binary {
            left: Box::new(desugar_expr(*left, env)),
            op,
            right: Box::new(desugar_expr(*right, env)),
            span,
        },
        other => other,
    }
}

fn resolve_iterable(iterable: &Expr, env: &HashMap<String, Vec<Expr>>) -> Expr {
    match iterable {
        Expr::Var(vname, _) => {
            if let Some(elems) = env.get(vname) {
                Expr::List(elems.clone(), Span::default())
            } else {
                iterable.clone()
            }
        }
        _ => iterable.clone(),
    }
}

fn has_complex_cf(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_has_complex_cf)
}

fn stmt_has_complex_cf(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return { .. } | Stmt::Defer { .. } => true,
        // `var y := x` (with y != x) is a potential move of x. If the
        // loop is unrolled, the move appears once per iteration in the
        // enclosing scope — but that scope has no notion of iteration,
        // so the analyzer's loop-aware move check never sees it.
        //
        // Treat such declarations as "complex enough to prevent
        // unrolling." The check is conservative: `var y := 42.0` and
        // `var y := x` where x is Copy both stay un-unrolled too. That
        // costs an unrolling opportunity but preserves the analyzer's
        // ability to reject moves-in-loops correctly.
        Stmt::VarDecl {
            name,
            value: Expr::Var(src, _),
            ..
        } if name != src => true,
        Stmt::Expression(expr) => expr_has_complex_cf(expr),
        Stmt::Spawn { body, .. }
        | Stmt::RegionBlock { body, .. }
        | Stmt::UnsafeBlock { body, .. } => body.iter().any(stmt_has_complex_cf),
        Stmt::Parallel { blocks, .. } => blocks.iter().any(|b| b.iter().any(stmt_has_complex_cf)),
        _ => false,
    }
}

fn expr_has_complex_cf(expr: &Expr) -> bool {
    match expr {
        Expr::Block {
            statements,
            trailing_expr,
            ..
        } => {
            statements.iter().any(stmt_has_complex_cf)
                || trailing_expr
                    .as_ref()
                    .is_some_and(|e| expr_has_complex_cf(e))
        }
        Expr::If { .. } => true,
        Expr::Match { cases, .. } => cases.iter().any(|c| expr_has_complex_cf(&c.body)),
        Expr::TryCatch {
            try_branch,
            catch_branch,
            ..
        } => expr_has_complex_cf(try_branch) || expr_has_complex_cf(catch_branch),
        Expr::For {
            body,
            trailing_expr,
            ..
        }
        | Expr::While {
            body,
            trailing_expr,
            ..
        } => {
            body.iter().any(stmt_has_complex_cf)
                || trailing_expr
                    .as_ref()
                    .is_some_and(|e| expr_has_complex_cf(e))
        }
        _ => false,
    }
}

// FIXED: Actually folds constant ifs
fn fold_constant_ifs(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let mut result = Vec::new();

    for stmt in stmts {
        match stmt {
            Stmt::Expression(Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            }) => {
                if let Some(cond_val) = eval_const_expr(&condition) {
                    if cond_val {
                        if let Expr::Block { statements, .. } = then_branch.as_ref() {
                            for s in statements {
                                if matches!(s, Stmt::Break(_)) {
                                    return result;
                                }
                                result.push(s.clone());
                            }
                        }
                    } else if let Some(else_br) = else_branch {
                        if let Expr::Block { statements, .. } = else_br.as_ref() {
                            for s in statements {
                                if matches!(s, Stmt::Break(_)) {
                                    return result;
                                }
                                result.push(s.clone());
                            }
                        }
                    }
                } else {
                    result.push(Stmt::Expression(Expr::If {
                        condition,
                        then_branch,
                        else_branch,
                        span,
                    }));
                }
            }
            _ => result.push(stmt),
        }
    }

    result
}

fn eval_const_expr(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::Bool(b, _) => Some(*b),
        Expr::Binary {
            left, op, right, ..
        } => {
            let l = eval_const_num(left)?;
            let r = eval_const_num(right)?;
            match op {
                BinOp::Greater => Some(l > r),
                BinOp::Less => Some(l < r),
                BinOp::GreaterEqual => Some(l >= r),
                BinOp::LessEqual => Some(l <= r),
                BinOp::Equal => Some(l == r),
                BinOp::NotEqual => Some(l != r),
                _ => None,
            }
        }
        _ => None,
    }
}

fn eval_const_num(expr: &Expr) -> Option<f64> {
    match expr {
        Expr::Number(n, _) => Some(*n),
        Expr::Int(i, _) => Some(*i as f64),
        _ => None,
    }
}

fn substitute_var_literal(stmts: &[Stmt], old_name: &str, literal: &Expr) -> Vec<Stmt> {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::Assign { name, value, span } => Stmt::Assign {
                name: name.clone(),
                value: substitute_expr_literal(value, old_name, literal),
                span: *span,
            },
            Stmt::Print { expr, span } => Stmt::Print {
                expr: substitute_expr_literal(expr, old_name, literal),
                span: *span,
            },
            Stmt::Expression(expr) => {
                Stmt::Expression(substitute_expr_literal(expr, old_name, literal))
            }
            Stmt::VarDecl {
                name,
                value,
                type_annotation,
                mutable,
                span,
            } => Stmt::VarDecl {
                name: name.clone(),
                value: substitute_expr_literal(value, old_name, literal),
                type_annotation: type_annotation.clone(),
                mutable: *mutable,
                span: *span,
            },
            Stmt::ArrayAssign {
                array,
                index,
                value,
                span,
            } => Stmt::ArrayAssign {
                array: array.clone(),
                index: substitute_expr_literal(index, old_name, literal),
                value: substitute_expr_literal(value, old_name, literal),
                span: *span,
            },
            Stmt::Return { value, span } => Stmt::Return {
                value: value
                    .as_ref()
                    .map(|v| substitute_expr_literal(v, old_name, literal)),
                span: *span,
            },
            Stmt::RegionBlock { name, body, span } => Stmt::RegionBlock {
                name: name.clone(),
                body: substitute_var_literal(body, old_name, literal),
                span: *span,
            },
            Stmt::UnsafeBlock { body, span } => Stmt::UnsafeBlock {
                body: substitute_var_literal(body, old_name, literal),
                span: *span,
            },
            Stmt::Spawn { body, span } => Stmt::Spawn {
                body: substitute_var_literal(body, old_name, literal),
                span: *span,
            },
            Stmt::Parallel { blocks, span } => Stmt::Parallel {
                blocks: blocks
                    .iter()
                    .map(|b| substitute_var_literal(b, old_name, literal))
                    .collect(),
                span: *span,
            },
            Stmt::Send {
                channel,
                value,
                span,
            } => Stmt::Send {
                channel: channel.clone(),
                value: substitute_expr_literal(value, old_name, literal),
                span: *span,
            },
            Stmt::Defer { stmt, span } => {
                let substituted =
                    substitute_var_literal(std::slice::from_ref(stmt), old_name, literal);
                let new_stmt = substituted
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| (**stmt).clone());
                Stmt::Defer {
                    stmt: Box::new(new_stmt),
                    span: *span,
                }
            }
            _ => stmt.clone(),
        })
        .collect()
}

fn substitute_expr_literal(expr: &Expr, old_name: &str, literal: &Expr) -> Expr {
    match expr {
        Expr::Var(name, _) if name == old_name => literal.clone(),
        Expr::Var(name, span) => Expr::Var(name.clone(), *span),
        Expr::Binary {
            left,
            op,
            right,
            span,
        } => Expr::Binary {
            left: Box::new(substitute_expr_literal(left, old_name, literal)),
            op: op.clone(),
            right: Box::new(substitute_expr_literal(right, old_name, literal)),
            span: *span,
        },
        Expr::If {
            condition,
            then_branch,
            else_branch,
            span,
        } => Expr::If {
            condition: Box::new(substitute_expr_literal(condition, old_name, literal)),
            then_branch: Box::new(substitute_expr_literal(then_branch, old_name, literal)),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(substitute_expr_literal(e, old_name, literal))),
            span: *span,
        },
        Expr::Block {
            statements,
            trailing_expr,
            span,
        } => {
            let new_stmts = substitute_var_literal(statements, old_name, literal);
            Expr::Block {
                statements: new_stmts,
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(substitute_expr_literal(e, old_name, literal))),
                span: *span,
            }
        }
        Expr::FunctionCall { name, args, span } => Expr::FunctionCall {
            name: name.clone(),
            args: args
                .iter()
                .map(|a| substitute_expr_literal(a, old_name, literal))
                .collect(),
            span: *span,
        },
        _ => expr.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    fn desugar(src: &str) -> Vec<FunctionDecl> {
        let lexer = Lexer::new(src.to_string()).unwrap();
        let mut parser = Parser::new(lexer.tokens);
        let program = parser.parse_program().unwrap();
        let mut funcs = program.functions;
        desugar_loops(&mut funcs);
        funcs
    }

    fn still_has_for(funcs: &[FunctionDecl]) -> bool {
        funcs[0]
            .body
            .iter()
            .any(|s| matches!(s, Stmt::Expression(Expr::For { .. })))
    }

    #[test]
    fn loop_with_var_decl_from_outer_var_is_not_unrolled() {
        let src = "\
procedure main
    var x := [1.0, 2.0]
    for a in [1.0] do
        var y := x
";
        let funcs = desugar(src);
        assert!(
            still_has_for(&funcs),
            "loop whose body contains `var y := x` must not be unrolled"
        );
    }

    #[test]
    fn loop_without_move_is_unrolled() {
        let src = "\
procedure main
    for a in [1.0, 2.0] do
        print(a)
";
        let funcs = desugar(src);
        assert!(
            !still_has_for(&funcs),
            "loop without a move-shaped body should still be unrolled"
        );
    }

    #[test]
    fn nested_move_loop_is_not_unrolled() {
        let src = "\
procedure main
    var x := [1.0, 2.0]
    for a in [1.0] do
        for b in [1.0] do
            var y := x
";
        let funcs = desugar(src);
        assert!(
            still_has_for(&funcs),
            "outer loop containing a nested loop with a move must not be unrolled"
        );
    }

    #[test]
    fn shadowed_variable_does_not_corrupt_outer_env() {
        // Inner block shadows `x` with a 3-element list. The outer
        // `for a in x` at the top level must still see the original
        // 2-element list. Before the fix, the inner VarDecl overwrote
        // `env["x"]` and the outer loop unrolled over the wrong list.
        let src = "\
procedure main
    var x := [1.0, 2.0]
    if true
        var x := [10.0, 20.0, 30.0]
        for a in x
            print(a)
    for a in x
        print(a)
";
        let funcs = desugar(src);

        // The outer loop unrolls to `print(1.0); print(2.0)`. The
        // last top-level statement must be `Print(2.0)`.
        let last = funcs[0]
            .body
            .last()
            .expect("desugared body should not be empty");

        match last {
            Stmt::Print {
                expr: Expr::Number(f, _),
                ..
            } => {
                assert_eq!(
                    *f, 2.0,
                    "outer loop unrolled with the inner (shadowed) list"
                );
            }
            other => panic!("expected final Print(2.0), got {:?}", other),
        }
    }

    #[test]
    fn reassignment_invalidates_stale_env_entry() {
        // `x` is reassigned to a 3-element list before the loop.
        // The loop must unroll over the new value, not the initial one.
        let src = "\
procedure main
    var x := [1.0, 2.0]
    x := [3.0, 4.0, 5.0]
    for a in x
        print(a)
";
        let funcs = desugar(src);

        let last = funcs[0]
            .body
            .last()
            .expect("desugared body should not be empty");

        match last {
            Stmt::Print {
                expr: Expr::Number(f, _),
                ..
            } => {
                assert_eq!(*f, 5.0, "loop unrolled with stale pre-reassignment list");
            }
            other => panic!("expected final Print(5.0), got {:?}", other),
        }
    }

    #[test]
    fn substitution_recurses_into_region_block() {
        // The loop variable `i` must be substituted inside a nested
        // `region` block. Before the fix, `substitute_var_literal`
        // fell through to `_ => clone()` for `Stmt::RegionBlock`, so
        // `i` remained unsubstituted and the unrolled body referenced
        // an undeclared variable.
        let src = "\
procedure main
    for i in [1.0, 2.0]
        region r
            print(i)
";
        let funcs = desugar(src);
        let body = &funcs[0].body;

        // After unrolling, the body should be two RegionBlocks, each
        // containing a Print. The second RegionBlock's Print must be
        // `Print(2.0)` — a literal, not a `Var("i")`.
        assert_eq!(body.len(), 2, "expected two unrolled RegionBlocks");

        match body.last().unwrap() {
            Stmt::RegionBlock { body: inner, .. } => match inner.last() {
                Some(Stmt::Print {
                    expr: Expr::Number(f, _),
                    ..
                }) => {
                    assert_eq!(*f, 2.0, "loop var not substituted in region block");
                }
                other => panic!("expected Print(2.0) in region body, got {:?}", other),
            },
            other => panic!("expected RegionBlock, got {:?}", other),
        }
    }
}

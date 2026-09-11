// src/ir/loop_desugar.rs

use crate::frontend::ast::{BinOp, Expr, FunctionDecl, Stmt};
use std::collections::HashMap;

pub fn desugar_loops(functions: &mut [FunctionDecl]) {
    for func in functions.iter_mut() {
        let mut env: HashMap<String, Vec<Expr>> = HashMap::new();
        func.body = desugar_stmts(std::mem::take(&mut func.body), &mut env);
    }
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
                if let Expr::List(elements) = &desugared_value {
                    env.insert(name.clone(), elements.clone());
                }
                result.push(Stmt::VarDecl {
                    name,
                    value: desugared_value,
                    type_annotation,
                    mutable,
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

                if let Expr::List(elements) = &resolved_iterable {
                    // Unroll if known list and no complex control flow
                    if !has_complex_cf(&body) && trailing_expr.is_none() {
                        for elem in elements {
                            let substituted = substitute_var_literal(&body, &var, elem);
                            let folded = fold_constant_ifs(substituted);
                            let mut should_break = false;

                            for s in folded {
                                match s {
                                    Stmt::Break => {
                                        should_break = true;
                                        break;
                                    }
                                    Stmt::Continue => {
                                        break; // Continue to next iteration
                                    }
                                    _ => {
                                        let inner = desugar_stmts(vec![s], env);
                                        result.extend(inner);
                                    }
                                }
                            }

                            if should_break {
                                break;
                            }
                        }
                    } else {
                        // Keep loop with resolved iterable
                        let desugared_body = desugar_stmts(body, env);
                        result.push(Stmt::Expression(Expr::For {
                            var,
                            iterable: Box::new(resolved_iterable),
                            body: desugared_body,
                            trailing_expr,
                            span,
                        }));
                    }
                } else {
                    let desugared_body = desugar_stmts(body, env);
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
                let desugared_body = desugar_stmts(body, env);
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
            Stmt::Assign { name, value } => {
                let desugared = desugar_expr(value, env);
                result.push(Stmt::Assign {
                    name,
                    value: desugared,
                });
            }
            Stmt::Print { expr } => {
                result.push(Stmt::Print {
                    expr: desugar_expr(expr, env),
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
            if let Expr::List(elements) = &resolved {
                if !has_complex_cf(&body) {
                    if let Some(te) = trailing_expr.as_ref() {
                        if let Some(last) = elements.last() {
                            return substitute_expr_literal(te, &var, last);
                        }
                    }
                }
            }
            let desugared_body = desugar_stmts(body, env);
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
            let desugared_body = desugar_stmts(body, env);
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
        } => {
            let desugared_stmts = desugar_stmts(statements, env);
            let desugared_trailing = trailing_expr.map(|te| Box::new(desugar_expr(*te, env)));
            Expr::Block {
                statements: desugared_stmts,
                trailing_expr: desugared_trailing,
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => Expr::If {
            condition: Box::new(desugar_expr(*condition, env)),
            then_branch: Box::new(desugar_expr(*then_branch, env)),
            else_branch: else_branch.map(|e| Box::new(desugar_expr(*e, env))),
        },
        Expr::Binary { left, op, right } => Expr::Binary {
            left: Box::new(desugar_expr(*left, env)),
            op,
            right: Box::new(desugar_expr(*right, env)),
        },
        other => other,
    }
}

fn resolve_iterable(iterable: &Expr, env: &HashMap<String, Vec<Expr>>) -> Expr {
    match iterable {
        Expr::Var(vname, _) => {
            if let Some(elems) = env.get(vname) {
                Expr::List(elems.clone())
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
        Stmt::Break | Stmt::Continue | Stmt::Return { .. } | Stmt::Defer { .. } => true,
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
        Stmt::VarDecl { name, value: Expr::Var(src, _), .. } if name != src => true,
        Stmt::Expression(expr) => expr_has_complex_cf(expr),
        Stmt::Spawn { body } | Stmt::RegionBlock { body, .. } | Stmt::UnsafeBlock { body } => {
            body.iter().any(stmt_has_complex_cf)
        }
        Stmt::Parallel { blocks } => blocks.iter().any(|b| b.iter().any(stmt_has_complex_cf)),
        _ => false,
    }
}

fn expr_has_complex_cf(expr: &Expr) -> bool {
    match expr {
        Expr::Block {
            statements,
            trailing_expr,
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
            }) => {
                if let Some(cond_val) = eval_const_expr(&condition) {
                    if cond_val {
                        if let Expr::Block { statements, .. } = then_branch.as_ref() {
                            for s in statements {
                                if matches!(s, Stmt::Break) {
                                    return result;
                                }
                                result.push(s.clone());
                            }
                        }
                    } else if let Some(else_br) = else_branch {
                        if let Expr::Block { statements, .. } = else_br.as_ref() {
                            for s in statements {
                                if matches!(s, Stmt::Break) {
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
        Expr::Bool(b) => Some(*b),
        Expr::Binary { left, op, right } => {
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
        Expr::Number(n) => Some(*n),
        Expr::Int(i) => Some(*i as f64),
        _ => None,
    }
}

fn substitute_var_literal(stmts: &[Stmt], old_name: &str, literal: &Expr) -> Vec<Stmt> {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::Assign { name, value } => Stmt::Assign {
                name: name.clone(),
                value: substitute_expr_literal(value, old_name, literal),
            },
            Stmt::Print { expr } => Stmt::Print {
                expr: substitute_expr_literal(expr, old_name, literal),
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
            _ => stmt.clone(),
        })
        .collect()
}

fn substitute_expr_literal(expr: &Expr, old_name: &str, literal: &Expr) -> Expr {
    match expr {
        Expr::Var(name, _) if name == old_name => literal.clone(),
        Expr::Var(name, span) => Expr::Var(name.clone(), *span),
        Expr::Binary { left, op, right } => Expr::Binary {
            left: Box::new(substitute_expr_literal(left, old_name, literal)),
            op: op.clone(),
            right: Box::new(substitute_expr_literal(right, old_name, literal)),
        },
        Expr::If {
            condition,
            then_branch,
            else_branch,
        } => Expr::If {
            condition: Box::new(substitute_expr_literal(condition, old_name, literal)),
            then_branch: Box::new(substitute_expr_literal(then_branch, old_name, literal)),
            else_branch: else_branch
                .as_ref()
                .map(|e| Box::new(substitute_expr_literal(e, old_name, literal))),
        },
        Expr::Block {
            statements,
            trailing_expr,
        } => {
            let new_stmts = substitute_var_literal(statements, old_name, literal);
            Expr::Block {
                statements: new_stmts,
                trailing_expr: trailing_expr
                    .as_ref()
                    .map(|e| Box::new(substitute_expr_literal(e, old_name, literal))),
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
}

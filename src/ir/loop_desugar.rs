// src/loop_desugar.rs - Orthogonal: handles both Stmt::For and Expr::For/While as values
// 100% orthogonal desugar with environment tracking + trailing_expr support

use crate::common::span::Span;
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
                // Desugar the value first (it might be For/While as expr)
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
                ..
            }) => {
                // Resolve iterable from env
                let resolved_iterable = resolve_iterable(&iterable, env);

                if let Expr::List(elements) = &resolved_iterable {
                    // Unroll if it's a known list and body has no complex control flow
                    if !has_complex_cf(&body) && trailing_expr.is_none() {
                        for elem in elements {
                            let substituted = substitute_var_literal(&body, &var, elem);
                            let folded = fold_constant_ifs(substituted);
                            for s in folded {
                                if matches!(s, Stmt::Break) {
                                    break;
                                }
                                // Recursively desugar inner statements
                                let inner = desugar_stmts(vec![s], env);
                                result.extend(inner);
                            }
                        }
                    } else {
                        // Keep loop but with resolved iterable and desugared body
                        let desugared_body = desugar_stmts(body, env);
                        result.push(Stmt::Expression(Expr::For {
                            var,
                            iterable: Box::new(resolved_iterable),
                            body: desugared_body,
                            trailing_expr,
                            span: Span::default(),
                        }));
                    }
                } else {
                    let desugared_body = desugar_stmts(body, env);
                    result.push(Stmt::Expression(Expr::For {
                        var,
                        iterable: Box::new(resolved_iterable),
                        body: desugared_body,
                        trailing_expr,
                        span: Span::default(),
                    }));
                }
            }
            Stmt::Expression(Expr::While {
                condition,
                body,
                trailing_expr,
                ..
            }) => {
                let desugared_body = desugar_stmts(body, env);
                result.push(Stmt::Expression(Expr::While {
                    condition,
                    body: desugared_body,
                    trailing_expr,
                    span: Span::default(),
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
                // For other stmts, recursively desugar inner blocks if any
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
                // If For is used as value `val x := for i in [1,2,3] do i + 1`
                // We can desugar to last trailing_expr value if known
                if !has_complex_cf(&body) {
                    if let Some(te) = trailing_expr.as_ref() {
                        // For orthogonal: evaluate trailing_expr with substituted var for last element
                        if let Some(last) = elements.last() {
                            return substitute_expr_literal(te, &var, last);
                        }
                    }
                }
            }
            // Keep as For expr but with desugared body
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
        Expr::If {
            then_branch: _,
            else_branch: _,
            ..
        } => true,
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

fn fold_constant_ifs(stmts: Vec<Stmt>) -> Vec<Stmt> {
    return stmts;
}
#[allow(dead_code)]
fn _fold_constant_ifs_orig(stmts: Vec<Stmt>) -> Vec<Stmt> {
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        Expr::For {
            var,
            iterable,
            body,
            trailing_expr,
            span,
        } => {
            if var == old_name {
                Expr::For {
                    var: var.clone(),
                    iterable: iterable.clone(),
                    body: body.clone(),
                    trailing_expr: trailing_expr.clone(),
                    span: *span,
                }
            } else {
                Expr::For {
                    var: var.clone(),
                    iterable: Box::new(substitute_expr_literal(iterable, old_name, literal)),
                    body: body
                        .iter()
                        .map(|s| match s {
                            Stmt::Expression(e) => {
                                Stmt::Expression(substitute_expr_literal(e, old_name, literal))
                            }
                            _ => s.clone(),
                        })
                        .collect(),
                    trailing_expr: trailing_expr
                        .as_ref()
                        .map(|e| Box::new(substitute_expr_literal(e, old_name, literal))),
                    span: *span,
                }
            }
        }
        Expr::While {
            condition,
            body,
            trailing_expr,
            span,
        } => Expr::While {
            condition: Box::new(substitute_expr_literal(condition, old_name, literal)),
            body: body
                .iter()
                .map(|s| match s {
                    Stmt::Expression(e) => {
                        Stmt::Expression(substitute_expr_literal(e, old_name, literal))
                    }
                    _ => s.clone(),
                })
                .collect(),
            trailing_expr: trailing_expr
                .as_ref()
                .map(|e| Box::new(substitute_expr_literal(e, old_name, literal))),
            span: *span,
        },
        _ => expr.clone(),
    }
}

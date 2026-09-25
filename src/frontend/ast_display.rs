// src/frontend/ast_display.rs
//
// Human-readable source-shaped rendering of AST nodes.
// Companion to `src/ir/semantic_ir/display.rs`. Used by
// `algol26 inspect --ast`.
//
// The goal is the same as the IR module's: make the tree readable
// without requiring the reader to know the Rust `Debug` format of
// every variant.

// `write!` into a `String` cannot fail. Every `.unwrap()` below is
// safe by construction.
#![allow(clippy::unwrap_used)]

use super::ast::{
    BinOp, Expr, ExprKind, FunctionDecl, ImplBlock, Pattern, Stmt, TraitDecl, TraitMethod,
    TypeSyntax, UnaryOp,
};
use std::fmt::Write;

const INDENT: &str = "  ";

fn indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str(INDENT);
    }
}

fn end_line(out: &mut String) {
    if !out.ends_with('\n') {
        out.push('\n');
    }
}

/// Render a parsed program — its functions, traits, and impls — as
/// source-shaped text.
pub fn format_program(
    functions: &[FunctionDecl],
    traits: &[TraitDecl],
    impls: &[ImplBlock],
) -> String {
    let mut out = String::new();
    for (i, f) in functions.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        format_function(&mut out, 0, f);
    }
    for t in traits {
        out.push('\n');
        format_trait(&mut out, 0, t);
    }
    for im in impls {
        out.push('\n');
        format_impl(&mut out, 0, im);
    }
    out
}

fn format_function(out: &mut String, level: usize, func: &FunctionDecl) {
    indent(out, level);
    if func.is_extern {
        out.push_str("extern ");
        if let Some(info) = &func.ffi_info {
            if let Some(abi) = &info.abi {
                write!(out, "\"{}\" ", abi).unwrap();
            }
        }
    }
    out.push_str("function ");
    out.push_str(&func.name);
    if !func.type_params.is_empty() {
        write!(out, "<{}>", func.type_params.join(", ")).unwrap();
    }
    out.push('(');
    for (i, (name, ty)) in func.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(name);
        if let Some(t) = ty {
            out.push_str(": ");
            format_type(out, t);
        }
    }
    out.push(')');
    if let Some(ret) = &func.return_type {
        out.push_str(" -> ");
        format_type(out, ret);
    }
    if !func.where_clauses.is_empty() {
        out.push_str(" where ");
        for (i, wc) in func.where_clauses.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            write!(out, "{}: {}", wc.type_param, wc.trait_name).unwrap();
        }
    }
    end_line(out);

    if func.is_extern {
        return;
    }
    for stmt in &func.body {
        format_stmt(out, level + 1, stmt);
    }
}

fn format_trait(out: &mut String, level: usize, tr: &TraitDecl) {
    indent(out, level);
    write!(out, "trait {}", tr.name).unwrap();
    end_line(out);
    for m in &tr.methods {
        format_trait_method(out, level + 1, m);
    }
}

fn format_trait_method(out: &mut String, level: usize, m: &TraitMethod) {
    indent(out, level);
    write!(out, "function {}(", m.name).unwrap();
    for (i, (name, ty)) in m.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(name);
        if let Some(t) = ty {
            out.push_str(": ");
            format_type(out, t);
        }
    }
    out.push(')');
    if let Some(ret) = &m.return_type {
        out.push_str(" -> ");
        format_type(out, ret);
    }
    end_line(out);
}

fn format_impl(out: &mut String, level: usize, im: &ImplBlock) {
    indent(out, level);
    write!(out, "impl {} for {}", im.trait_name, im.target_type).unwrap();
    end_line(out);
    for m in &im.methods {
        format_function(out, level + 1, m);
    }
}

fn format_type(out: &mut String, ty: &TypeSyntax) {
    match ty {
        TypeSyntax::Named(name) => out.push_str(name),
        TypeSyntax::Generic { name, args } => {
            out.push_str(name);
            out.push('<');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_type(out, a);
            }
            out.push('>');
        }
        TypeSyntax::Unknown => out.push('_'),
    }
}

fn format_stmt(out: &mut String, level: usize, stmt: &Stmt) {
    match stmt {
        Stmt::VarDecl {
            name,
            value,
            type_annotation,
            mutable,
            ..
        } => {
            indent(out, level);
            out.push_str(if *mutable { "var " } else { "val " });
            out.push_str(name);
            if let Some(t) = type_annotation {
                out.push_str(": ");
                format_type(out, t);
            }
            out.push_str(" := ");
            format_expr(out, level, value);
            end_line(out);
        }
        Stmt::Assign { name, value, .. } => {
            indent(out, level);
            write!(out, "{} := ", name).unwrap();
            format_expr(out, level, value);
            end_line(out);
        }
        Stmt::ArrayAssign {
            array,
            index,
            value,
            ..
        } => {
            indent(out, level);
            write!(out, "{}[", array).unwrap();
            format_expr(out, level, index);
            out.push_str("] := ");
            format_expr(out, level, value);
            end_line(out);
        }
        Stmt::FieldAssign {
            target,
            field,
            value,
            ..
        } => {
            let _ = write!(out, "{}.{} := ", target, field);
            format_expr(out, level, value);
        }
        Stmt::Return { value, .. } => {
            indent(out, level);
            out.push_str("return");
            if let Some(e) = value {
                out.push(' ');
                format_expr(out, level, e);
            }
            end_line(out);
        }
        Stmt::Print { expr, .. } => {
            indent(out, level);
            out.push_str("print(");
            format_expr(out, level, expr);
            out.push(')');
            end_line(out);
        }
        Stmt::Break(_) => {
            indent(out, level);
            out.push_str("break");
            end_line(out);
        }
        Stmt::Continue(_) => {
            indent(out, level);
            out.push_str("continue");
            end_line(out);
        }
        Stmt::Defer { stmt, .. } => {
            indent(out, level);
            out.push_str("defer");
            end_line(out);
            format_stmt(out, level + 1, stmt);
        }
        Stmt::Expression(expr) => {
            // Bare block in statement position is transparent — its
            // statements render at the same level, no extra indent.
            if let ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } = &expr.kind
            {
                for s in statements {
                    format_stmt(out, level, s);
                }
                if let Some(e) = trailing_expr {
                    indent(out, level);
                    format_expr(out, level, e);
                    end_line(out);
                }
            } else {
                indent(out, level);
                format_expr(out, level, expr);
                end_line(out);
            }
        }
        Stmt::RegionBlock { name, body, .. } => {
            indent(out, level);
            write!(out, "region {}", name).unwrap();
            end_line(out);
            for s in body {
                format_stmt(out, level + 1, s);
            }
        }
        Stmt::UnsafeBlock { body, .. } => {
            indent(out, level);
            out.push_str("unsafe");
            end_line(out);
            for s in body {
                format_stmt(out, level + 1, s);
            }
        }
        Stmt::Import { path, .. } => {
            indent(out, level);
            write!(out, "import {}", path).unwrap();
            end_line(out);
        }
        Stmt::Spawn { body, .. } => {
            indent(out, level);
            out.push_str("spawn");
            end_line(out);
            for s in body {
                format_stmt(out, level + 1, s);
            }
        }
        Stmt::Parallel { blocks, .. } => {
            indent(out, level);
            out.push_str("parallel");
            end_line(out);
            for (i, block) in blocks.iter().enumerate() {
                if i > 0 {
                    indent(out, level);
                    out.push_str("and");
                    end_line(out);
                }
                for s in block {
                    format_stmt(out, level + 1, s);
                }
            }
        }
        Stmt::ChannelDecl { name, .. } => {
            indent(out, level);
            write!(out, "channel {}", name).unwrap();
            end_line(out);
        }
        Stmt::Send { channel, value, .. } => {
            indent(out, level);
            write!(out, "send {} ", channel).unwrap();
            format_expr(out, level, value);
            end_line(out);
        }
        Stmt::Receive {
            channel, target, ..
        } => {
            indent(out, level);
            write!(out, "receive {} into {}", channel, target).unwrap();
            end_line(out);
        }
    }
}

fn format_expr(out: &mut String, level: usize, expr: &Expr) {
    match &expr.kind {
        ExprKind::Int(i, _) => write!(out, "{}", i).unwrap(),
        ExprKind::Number(f, _) => write!(out, "{}", f).unwrap(),
        ExprKind::String(s, _) => write!(out, "{:?}", s).unwrap(),
        ExprKind::Bool(b, _) => write!(out, "{}", b).unwrap(),
        ExprKind::NullPtr(_) => out.push_str("null"),
        ExprKind::PtrLiteral(p, _) => write!(out, "0x{:x}", p).unwrap(),
        ExprKind::Var(name, _) => out.push_str(name),
        ExprKind::Borrow { expr, .. } => {
            out.push('&');
            format_expr(out, level, expr);
        }
        ExprKind::MutBorrow { expr, .. } => {
            out.push_str("&mut ");
            format_expr(out, level, expr);
        }
        ExprKind::Deref { expr, .. } => {
            out.push('*');
            format_expr(out, level, expr);
        }
        ExprKind::AddrOf { expr, .. } => {
            out.push_str("addr_of(");
            format_expr(out, level, expr);
            out.push(')');
        }
        ExprKind::Some { value, .. } => {
            out.push_str("Some(");
            format_expr(out, level, value);
            out.push(')');
        }
        ExprKind::None(_) => out.push_str("None"),
        ExprKind::Ok { value, .. } => {
            out.push_str("Ok(");
            format_expr(out, level, value);
            out.push(')');
        }
        ExprKind::Error { value, .. } => {
            out.push_str("Error(");
            format_expr(out, level, value);
            out.push(')');
        }
        ExprKind::List(items, _) => {
            out.push('[');
            for (i, e) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, level, e);
            }
            out.push(']');
        }
        ExprKind::ArrayAccess { array, index, .. } => {
            format_expr(out, level, array);
            out.push('[');
            format_expr(out, level, index);
            out.push(']');
        }
        ExprKind::Binary {
            left, op, right, ..
        } => {
            format_expr(out, level, left);
            write!(out, " {} ", binop_str(op)).unwrap();
            format_expr(out, level, right);
        }
        ExprKind::Unary { op, expr, .. } => {
            out.push_str(unop_str(op));
            format_expr(out, level, expr);
        }
        ExprKind::FunctionCall { name, args, .. } => {
            write!(out, "{}(", name).unwrap();
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                format_expr(out, level, a);
            }
            out.push(')');
        }
        ExprKind::Block {
            statements,
            trailing_expr,
            ..
        } => {
            for s in statements {
                format_stmt(out, level + 1, s);
            }
            if let Some(e) = trailing_expr {
                indent(out, level + 1);
                format_expr(out, level + 1, e);
                end_line(out);
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            out.push_str("if ");
            format_expr(out, level, condition);
            end_line(out);
            format_expr_block(out, level + 1, then_branch);
            if let Some(eb) = else_branch {
                indent(out, level);
                out.push_str("else");
                end_line(out);
                format_expr_block(out, level + 1, eb);
            }
        }
        ExprKind::Match { value, cases, .. } => {
            out.push_str("match ");
            format_expr(out, level, value);
            end_line(out);
            for c in cases {
                indent(out, level);
                out.push_str("case ");
                format_pattern(out, &c.pattern);
                end_line(out);
                format_expr_block(out, level + 1, &c.body);
            }
        }
        ExprKind::TryCatch {
            try_branch,
            catch_var,
            catch_branch,
            finally_body,
            ..
        } => {
            out.push_str("try");
            end_line(out);
            format_expr_block(out, level + 1, try_branch);
            indent(out, level);
            out.push_str("catch");
            if let Some(v) = catch_var {
                write!(out, " {}", v).unwrap();
            }
            end_line(out);
            format_expr_block(out, level + 1, catch_branch);
            if let Some(finally) = finally_body {
                indent(out, level);
                out.push_str("finally");
                end_line(out);
                for s in finally {
                    format_stmt(out, level + 1, s);
                }
            }
        }
        ExprKind::For {
            var,
            iterable,
            body,
            trailing_expr,
            ..
        } => {
            write!(out, "for {} in ", var).unwrap();
            format_expr(out, level, iterable);
            end_line(out);
            for s in body {
                format_stmt(out, level + 1, s);
            }
            if let Some(te) = trailing_expr {
                indent(out, level + 1);
                format_expr(out, level + 1, te);
                end_line(out);
            }
        }
        ExprKind::While {
            condition,
            body,
            trailing_expr,
            ..
        } => {
            out.push_str("while ");
            format_expr(out, level, condition);
            end_line(out);
            for s in body {
                format_stmt(out, level + 1, s);
            }
            if let Some(te) = trailing_expr {
                indent(out, level + 1);
                format_expr(out, level + 1, te);
                end_line(out);
            }
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
            ..
        } => {
            if let Some(s) = start {
                format_expr(out, level, s);
            }
            out.push_str(if *inclusive { "..=" } else { ".." });
            if let Some(e) = end {
                format_expr(out, level, e);
            }
        }
        ExprKind::FieldAccess { object, field, .. } => {
            format_expr(out, level, object);
            out.push('.');
            out.push_str(field);
        }
        ExprKind::RecordLiteral { name, fields, .. } => {
            let _ = write!(out, "{} {{ ", name);
            for (i, (fname, fval)) in fields.iter().enumerate() {
                if i > 0 {
                    let _ = write!(out, ", ");
                }
                let _ = write!(out, "{}: ", fname);
                format_expr(out, level, fval);
            }
            let _ = write!(out, " }}");
        }
    }
}

/// Render an expression as a block — used where the grammar requires
/// block syntax (then/else branches, match arms, try/catch bodies).
fn format_expr_block(out: &mut String, level: usize, expr: &Expr) {
    if let ExprKind::Block {
        statements,
        trailing_expr,
        ..
    } = &expr.kind
    {
        for s in statements {
            format_stmt(out, level, s);
        }
        if let Some(e) = trailing_expr {
            indent(out, level);
            format_expr(out, level, e);
            end_line(out);
        }
    } else {
        indent(out, level);
        format_expr(out, level, expr);
        end_line(out);
    }
}

fn format_pattern(out: &mut String, pat: &Pattern) {
    match pat {
        Pattern::Some(v) => write!(out, "Some({})", v).unwrap(),
        Pattern::None => out.push_str("None"),
        Pattern::Ok(v) => write!(out, "Ok({})", v).unwrap(),
        Pattern::Error(v) => write!(out, "Error({})", v).unwrap(),
        Pattern::Wildcard => out.push('_'),
        Pattern::Binding(v) => out.push_str(v),
        Pattern::Literal(e) => format_expr(out, 0, e),
        Pattern::SomeNested(p) => {
            out.push_str("Some(");
            format_pattern(out, p);
            out.push(')');
        }
        Pattern::OkNested(p) => {
            out.push_str("Ok(");
            format_pattern(out, p);
            out.push(')');
        }
        Pattern::ErrorNested(p) => {
            out.push_str("Error(");
            format_pattern(out, p);
            out.push(')');
        }
        Pattern::Guarded { pattern, condition } => {
            format_pattern(out, pattern);
            out.push_str(" if ");
            format_expr(out, 0, condition);
        }
        Pattern::ListDestructure { first, rest } => {
            out.push('[');
            if let Some(f) = first {
                format_pattern(out, f);
            }
            if let Some(r) = rest {
                if first.is_some() {
                    out.push_str(", ");
                }
                out.push_str("..");
                format_pattern(out, r);
            }
            out.push(']');
        }
        Pattern::Record { name, bindings } => {
            let _ = write!(out, "{} {{ {} }}", name, bindings.join(", "));
        }
        Pattern::Range { start, end } => {
            if let Some(s) = start {
                format_expr(out, 0, s);
            }
            out.push_str("..");
            if let Some(e) = end {
                format_expr(out, 0, e);
            }
        }
    }
}

fn binop_str(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Subtract => "-",
        BinOp::Multiply => "*",
        BinOp::Divide => "/",
        BinOp::Greater => ">",
        BinOp::Less => "<",
        BinOp::GreaterEqual => ">=",
        BinOp::LessEqual => "<=",
        BinOp::Equal => "==",
        BinOp::NotEqual => "!=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

fn unop_str(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Negate => "-",
        UnaryOp::Not => "not ",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::span::Span;

    fn mk_fn(name: &str, body: Vec<Stmt>) -> FunctionDecl {
        FunctionDecl {
            name: name.into(),
            params: vec![],
            return_type: None,
            body,
            is_extern: false,
            ffi_info: None,
            type_params: vec![],
            where_clauses: vec![],
        }
    }

    #[test]
    fn simple_function() {
        let f = mk_fn(
            "main",
            vec![Stmt::Print {
                expr: Expr::new(ExprKind::String("hello".into(), Span::default())),
                span: Span::default(),
            }],
        );
        let out = format_program(&[f], &[], &[]);
        assert!(out.contains("function main()"), "got:\n{}", out);
        assert!(out.contains("print(\"hello\")"), "got:\n{}", out);
    }

    #[test]
    fn var_decl_and_assign() {
        let f = mk_fn(
            "f",
            vec![
                Stmt::VarDecl {
                    name: "y".into(),
                    value: Expr::new(ExprKind::Int(5, Span::default())),
                    type_annotation: None,
                    mutable: true,
                    span: Span::default(),
                },
                Stmt::Assign {
                    name: "y".into(),
                    value: Expr::new(ExprKind::Int(6, Span::default())),
                    span: Span::default(),
                },
            ],
        );
        let out = format_program(&[f], &[], &[]);
        assert!(out.contains("var y := 5"), "got:\n{}", out);
        assert!(out.contains("y := 6"), "got:\n{}", out);
    }

    #[test]
    fn if_else() {
        let then_block = Expr::new(ExprKind::Block {
            statements: vec![Stmt::Print {
                expr: Expr::new(ExprKind::String("a".into(), Span::default())),
                span: Span::default(),
            }],
            trailing_expr: None,
            span: Span::default(),
        });
        let else_block = Expr::new(ExprKind::Block {
            statements: vec![Stmt::Print {
                expr: Expr::new(ExprKind::String("b".into(), Span::default())),
                span: Span::default(),
            }],
            trailing_expr: None,
            span: Span::default(),
        });
        let f = mk_fn(
            "main",
            vec![Stmt::Expression(Expr::new(ExprKind::If {
                condition: Expr::boxed(ExprKind::Bool(true, Span::default())),
                then_branch: Box::new(then_block),
                else_branch: Some(Box::new(else_block)),
                span: Span::default(),
            }))],
        );
        let out = format_program(&[f], &[], &[]);
        assert!(out.contains("if true"), "got:\n{}", out);
        assert!(out.contains("else"), "got:\n{}", out);
    }

    #[test]
    fn trait_and_impl() {
        let tr = TraitDecl {
            name: "Display".into(),
            methods: vec![TraitMethod {
                name: "show".into(),
                params: vec![],
                return_type: Some(TypeSyntax::Named("String".into())),
            }],
        };
        let im = ImplBlock {
            trait_name: "Display".into(),
            target_type: "Int".into(),
            methods: vec![],
        };
        let out = format_program(&[], &[tr], &[im]);
        assert!(out.contains("trait Display"), "got:\n{}", out);
        assert!(out.contains("function show() -> String"), "got:\n{}", out);
        assert!(out.contains("impl Display for Int"), "got:\n{}", out);
    }
}

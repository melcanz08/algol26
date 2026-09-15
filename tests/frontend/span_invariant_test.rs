// tests/frontend/span_invariant_test.rs
//
// Verify that every AST node produced by the parser carries a real
// span (non-zero start_line). This is the foundational invariant that
// PR-4 established; if it ever regresses, diagnostics silently become
// useless again.

use algol26::frontend::ast::{Expr, FunctionDecl, Stmt};
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;

fn parse(source: &str) -> Vec<FunctionDecl> {
    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    parser.parse_program().expect("parse").functions
}

fn check_expr_spans(expr: &Expr, path: &str) {
    let span = expr.span();
    assert!(
        span.start_line > 0,
        "{}: expression has zero start_line (kind: {})",
        path,
        expr_kind(expr),
    );

    match expr {
        Expr::Block { statements, trailing_expr, .. } => {
            for (i, s) in statements.iter().enumerate() {
                check_stmt_spans(s, &format!("{}/block[{}]", path, i));
            }
            if let Some(e) = trailing_expr {
                check_expr_spans(e, &format!("{}/block.trailing", path));
            }
        }
        Expr::If { condition, then_branch, else_branch, .. } => {
            check_expr_spans(condition, &format!("{}/if.cond", path));
            check_expr_spans(then_branch, &format!("{}/if.then", path));
            if let Some(e) = else_branch {
                check_expr_spans(e, &format!("{}/if.else", path));
            }
        }
        Expr::Binary { left, right, .. } => {
            check_expr_spans(left, &format!("{}/bin.left", path));
            check_expr_spans(right, &format!("{}/bin.right", path));
        }
        Expr::Unary { expr, .. } => {
            check_expr_spans(expr, &format!("{}/unary", path));
        }
        Expr::List(items, _) => {
            for (i, e) in items.iter().enumerate() {
                check_expr_spans(e, &format!("{}/list[{}]", path, i));
            }
        }
        Expr::FunctionCall { args, .. } => {
            for (i, e) in args.iter().enumerate() {
                check_expr_spans(e, &format!("{}/call.arg[{}]", path, i));
            }
        }
        Expr::ArrayAccess { array, index, .. } => {
            check_expr_spans(array, &format!("{}/access.array", path));
            check_expr_spans(index, &format!("{}/access.index", path));
        }
        Expr::Match { value, cases, .. } => {
            check_expr_spans(value, &format!("{}/match.value", path));
            for (i, c) in cases.iter().enumerate() {
                check_expr_spans(&c.body, &format!("{}/match.case[{}]", path, i));
            }
        }
        Expr::For { iterable, trailing_expr, .. } => {
            check_expr_spans(iterable, &format!("{}/for.iter", path));
            if let Some(e) = trailing_expr {
                check_expr_spans(e, &format!("{}/for.trailing", path));
            }
        }
        Expr::While { condition, trailing_expr, .. } => {
            check_expr_spans(condition, &format!("{}/while.cond", path));
            if let Some(e) = trailing_expr {
                check_expr_spans(e, &format!("{}/while.trailing", path));
            }
        }
        Expr::Borrow { expr, .. }
        | Expr::MutBorrow { expr, .. }
        | Expr::Deref { expr, .. }
        | Expr::AddrOf { expr, .. }
        | Expr::Some { value: expr, .. }
        | Expr::Ok { value: expr, .. }
        | Expr::Error { value: expr, .. } => {
            check_expr_spans(expr, &format!("{}/wrap", path));
        }
        Expr::TryCatch { try_branch, catch_branch, .. } => {
            check_expr_spans(try_branch, &format!("{}/try.branch", path));
            check_expr_spans(catch_branch, &format!("{}/try.catch", path));
        }
        _ => {}
    }
}

fn check_stmt_spans(stmt: &Stmt, path: &str) {
    let span = stmt.span();
    assert!(
        span.start_line > 0,
        "{}: statement has zero start_line",
        path,
    );

    match stmt {
        Stmt::VarDecl { value, .. }
        | Stmt::Assign { value, .. }
        | Stmt::Print { expr: value, .. }
        | Stmt::Return { value: Some(value), .. } => {
            check_expr_spans(value, &format!("{}/value", path));
        }
        Stmt::ArrayAssign { index, value, .. } => {
            check_expr_spans(index, &format!("{}/index", path));
            check_expr_spans(value, &format!("{}/value", path));
        }
        Stmt::Expression(e) => {
            check_expr_spans(e, &format!("{}/expr", path));
        }
        Stmt::Defer { stmt, .. } => {
            check_stmt_spans(stmt, &format!("{}/defer", path));
        }
        Stmt::Spawn { body, .. }
        | Stmt::RegionBlock { body, .. }
        | Stmt::UnsafeBlock { body, .. } => {
            for (i, s) in body.iter().enumerate() {
                check_stmt_spans(s, &format!("{}/body[{}]", path, i));
            }
        }
        Stmt::Parallel { blocks, .. } => {
            for (i, blk) in blocks.iter().enumerate() {
                for (j, s) in blk.iter().enumerate() {
                    check_stmt_spans(s, &format!("{}/block[{}][{}]", path, i, j));
                }
            }
        }
        _ => {}
    }
}

fn expr_kind(e: &Expr) -> &'static str {
    match e {
        Expr::Number(..) => "Number",
        Expr::Int(..) => "Int",
        Expr::String(..) => "String",
        Expr::Bool(..) => "Bool",
        Expr::Var(..) => "Var",
        Expr::Block { .. } => "Block",
        Expr::If { .. } => "If",
        Expr::Match { .. } => "Match",
        Expr::Borrow { .. } => "Borrow",
        Expr::MutBorrow { .. } => "MutBorrow",
        Expr::Deref { .. } => "Deref",
        Expr::AddrOf { .. } => "AddrOf",
        Expr::List(..) => "List",
        Expr::ArrayAccess { .. } => "ArrayAccess",
        Expr::Binary { .. } => "Binary",
        Expr::Unary { .. } => "Unary",
        Expr::FunctionCall { .. } => "FunctionCall",
        Expr::Some { .. } => "Some",
        Expr::None(..) => "None",
        Expr::Ok { .. } => "Ok",
        Expr::Error { .. } => "Error",
        Expr::TryCatch { .. } => "TryCatch",
        Expr::For { .. } => "For",
        Expr::While { .. } => "While",
        Expr::PtrLiteral(..) => "PtrLiteral",
        Expr::NullPtr(..) => "NullPtr",
        Expr::Range { .. } => "Range",
        Expr::FieldAccess { .. } => "FieldAccess",
    }
}

#[test]
fn every_ast_node_has_a_real_span() {
    let source = r#"
function add(x: Float, y: Float) -> Float
    return x + y

procedure main
    var a := 5.0
    var b := 10.0
    val sum := add(a, b)
    if sum > 10.0
        print(sum)
    else
        print(0.0)

    var items := [1.0, 2.0, 3.0]
    for item in items
        print(item)

    val maybe := Some(42.0)
    val unwrapped := match maybe
        case Some(v)
            v
        case None
            0.0

    print(unwrapped)
"#;

    let functions = parse(source);
    for func in &functions {
        for (i, stmt) in func.body.iter().enumerate() {
            check_stmt_spans(stmt, &format!("{}.body[{}]", func.name, i));
        }
    }
}
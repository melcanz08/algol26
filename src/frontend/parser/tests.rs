// src/frontend/parser/tests.rs

use super::*;
use crate::frontend::lexer::Lexer;

fn parse_source(source: &str) -> Result<Vec<FunctionDecl>> {
    let lexer = Lexer::new(source.to_string())?;
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program()?;
    Ok(program.functions)
}

#[test]
fn test_parse_simple_function() {
    let source = "function main() -> Float\n    return 42.0";
    let functions = parse_source(source).expect("parse error");
    assert_eq!(functions.len(), 1);
    assert_eq!(functions[0].name, "main");
    assert_eq!(
        functions[0].return_type,
        Some(TypeSyntax::Named("Float".to_string()))
    );
    assert_eq!(functions[0].body.len(), 1);
}

#[test]
fn test_parse_var_decl() {
    let source = "function main()\n    var x := 5\n    val y := 10";
    let functions = parse_source(source).expect("parse error");
    assert_eq!(functions[0].body.len(), 2);
}

#[test]
fn test_parse_if_else() {
    let source = "function main()\n    if x > 5\n        print x\n    else\n        print 0";
    let functions = parse_source(source).expect("parse error");
    assert_eq!(functions[0].body.len(), 1);
}

#[test]
fn test_parse_array_access() {
    let source = "function main()\n    var x := arr[0]";
    let functions = parse_source(source).expect("parse error");
    match &functions[0].body[0] {
        Stmt::VarDecl { value, .. } => match value {
            Expr::ArrayAccess { .. } => {}
            _ => panic!("Expected ArrayAccess"),
        },
        _ => panic!("Expected VarDecl"),
    }
}

#[test]
fn test_parse_function_call() {
    let source = "function main()\n    print add(1, 2)";
    let functions = parse_source(source).expect("parse error");
    match &functions[0].body[0] {
        Stmt::Print { expr } => match expr {
            Expr::FunctionCall { name, args, .. } => {
                assert_eq!(name, "add");
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected FunctionCall"),
        },
        _ => panic!("Expected Print"),
    }
}

#[test]
fn test_parse_for_as_expr() {
    let source = "function main()\n    val x := for i in [1,2,3] do i + 1";
    let functions = parse_source(source).expect("parse error");
    match &functions[0].body[0] {
        Stmt::VarDecl { value, .. } => match value {
            Expr::For {
                var, trailing_expr, ..
            } => {
                assert_eq!(var, "i");
                assert!(trailing_expr.is_some());
            }
            _ => panic!("Expected For expr"),
        },
        _ => panic!("Expected VarDecl"),
    }
}

#[test]
fn test_parse_method_call() {
    let source = "function main()\n    list.append(3)";
    let functions = parse_source(source).expect("parse error");
    match &functions[0].body[0] {
        Stmt::Expression(Expr::FunctionCall { name, args, .. }) => {
            assert_eq!(name, "list.append");
            assert_eq!(args.len(), 1);
        }
        _ => panic!("Expected FunctionCall with dotted name"),
    }
}

#[test]
fn test_parse_range() {
    let source = "function main()\n    val r := 1..5";
    let functions = parse_source(source).expect("parse error");
    match &functions[0].body[0] {
        Stmt::VarDecl { value, .. } => match value {
            Expr::Range { start, end, inclusive } => {
                assert!(!inclusive);
                assert!(start.is_some());
                assert!(end.is_some());
            }
            _ => panic!("Expected Range"),
        },
        _ => panic!("Expected VarDecl"),
    }
}
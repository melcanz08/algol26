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
        Stmt::VarDecl { value, .. } => match &value.kind {
            ExprKind::ArrayAccess { .. } => {}
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
        Stmt::Print { expr, .. } => match &expr.kind {
            ExprKind::FunctionCall { name, args, .. } => {
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
        Stmt::VarDecl { value, .. } => match &value.kind {
            ExprKind::For {
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
        Stmt::Expression(e) => match &e.kind {
            ExprKind::FunctionCall { name, args, .. } => {
                assert_eq!(name, "list.append");
                assert_eq!(args.len(), 1);
            }
            other => panic!("Expected FunctionCall with dotted name, got {:?}", other),
        },
        _ => panic!("Expected FunctionCall with dotted name"),
    }
}

#[test]
fn test_parse_range() {
    let source = "function main()\n    val r := 1..5";
    let functions = parse_source(source).expect("parse error");
    match &functions[0].body[0] {
        Stmt::VarDecl { value, .. } => match &value.kind {
            ExprKind::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                assert!(!inclusive);
                assert!(start.is_some());
                assert!(end.is_some());
            }
            _ => panic!("Expected Range"),
        },
        _ => panic!("Expected VarDecl"),
    }
}

#[test]
fn test_negation_produces_unary_negate() {
    use crate::frontend::ast::UnaryOp;

    let source = "function main() -> Float\n    return -5.0";
    let functions = parse_source(source).expect("parse error");
    match &functions[0].body[0] {
        Stmt::Return {
            value: Some(expr), ..
        } => match &expr.kind {
            ExprKind::Unary {
                op: UnaryOp::Negate,
                ..
            } => {}
            other => panic!("expected Unary::Negate, got {:?}", other),
        },
        other => panic!("expected Return, got {:?}", other),
    }
}

#[test]
fn test_match_expression_in_value_position() {
    let source = "\
function main() -> Float
    val x := match 1
        case 1
            42.0
        case _
            0.0
    return x";
    let functions = parse_source(source).expect("parse error");

    match &functions[0].body[0] {
        Stmt::VarDecl { value, .. } => match &value.kind {
            ExprKind::Match { cases, .. } => {
                assert_eq!(cases.len(), 2);
                for case in cases {
                    match &case.body.kind {
                        ExprKind::Block { trailing_expr, .. } => {
                            assert!(
                                trailing_expr.is_some(),
                                "match arm lost its trailing expression"
                            );
                        }
                        other => panic!("expected Block, got {:?}", other),
                    }
                }
            }
            other => panic!("expected Expr::Match, got {:?}", other),
        },
        other => panic!("expected VarDecl, got {:?}", other),
    }
}

#[test]
fn test_garbage_pattern_is_rejected() {
    // A pattern that doesn't parse must be an error, not a silent
    // fallback to Wildcard. Otherwise a typo like `case + 42` compiles
    // and silently behaves as `case _`.
    //
    // Note: we use `+` (a real lexer token that `parse_pattern` doesn't
    // handle) rather than a character like `@`, which the lexer rejects
    // before the parser runs.
    let source = "\
function main() -> Float
    val x := match 1
        case + 42
            1.0
        case _
            0.0
    return x";
    let err = parse_source(source).expect_err("garbage pattern should fail to parse");
    let msg = err.to_string();
    assert!(
        msg.contains("Unexpected token in pattern"),
        "expected pattern-error message, got: {}",
        msg
    );
}

#[test]
fn test_mixed_parens_on_some_is_rejected() {
    // `Some(5` (missing close paren) must error, not silently accept
    // the value as if the paren were optional.
    let source = "\
function main() -> Float
    val x := Some(5
    return 0.0";
    let err = parse_source(source).expect_err("unbalanced parens should fail");
    // Either an "Expected ')'" from expect_token or an error further
    // downstream. The exact message depends on which token follows.
    let _ = err.to_string(); // ensure it's a CompileError
}

#[test]
fn test_do_at_statement_position_is_rejected() {
    // `do` after a complete statement must be an error, not a silent
    // no-op. Previously `parse_stmt` consumed the `do` and parsed the
    // next statement, which hid typos.
    let source = "\
function main() -> Float
    do val x := 5
    return 0.0";
    let err = parse_source(source).expect_err("`do` at statement position should fail");
    let msg = err.to_string();
    assert!(
        msg.contains("Unexpected expression"),
        "expected 'Unexpected expression' error, got: {}",
        msg
    );
}

#[test]
fn ampersand_produces_borrow_not_addrof() {
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    let src = "\
procedure main
    val x := 5.0
    val p := &x
";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let body = &program.functions[0].body;
    let has_borrow = body.iter().any(|s| {
        matches!(
            s,
            Stmt::VarDecl { value, .. } if matches!(&value.kind, ExprKind::Borrow { .. })
        )
    });
    let has_addrof = body.iter().any(|s| {
        matches!(
            s,
            Stmt::VarDecl { value, .. } if matches!(&value.kind, ExprKind::AddrOf { .. })
        )
    });
    assert!(has_borrow, "&x should parse as Expr::Borrow");
    assert!(!has_addrof, "&x should NOT parse as Expr::AddrOf");
}

#[test]
fn parses_record_decl_and_literal() {
    let src = "rec Point\n    x: Int\n    y: Int\n\nprocedure main\n    val p := Point { x: 1, y: 2 }\n    print(p.x)\n";
    let toks = crate::frontend::lexer::Lexer::new(src.to_string())
        .unwrap()
        .tokens;
    let mut p = Parser::new(toks);
    let program = p.parse_program().unwrap();
    assert_eq!(program.records.len(), 1);
    assert_eq!(program.records[0].name, "Point");
    assert_eq!(program.records[0].fields.len(), 2);
    assert_eq!(program.records[0].fields[0].0, "x");
    assert_eq!(program.records[0].fields[1].0, "y");
}

#[test]
fn record_declaration_parses() {
    let source = "rec Point\n    x: Int\n    y: Int\n";
    let lexer = crate::frontend::lexer::Lexer::new(source.to_string()).unwrap();
    let mut parser = crate::frontend::parser::Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    assert_eq!(program.records.len(), 1, "one record expected");
    assert_eq!(program.records[0].name, "Point");
    assert_eq!(program.records[0].type_params.len(), 0);
    assert_eq!(program.records[0].fields.len(), 2);
    assert_eq!(program.records[0].fields[0].0, "x");
    assert_eq!(program.records[0].fields[1].0, "y");
}

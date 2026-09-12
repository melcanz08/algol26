// src/semantics/analyzer/tests.rs

use super::*;
use crate::frontend::lexer::Lexer;
use crate::frontend::parser::Parser;

#[cfg(test)]
fn analyze(source: &str) -> Result<()> {
    let lexer = Lexer::new(source.to_string())?;
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program()?;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_spans(
        &program.functions,
        &program.traits,
        &program.impls,
        &std::collections::HashMap::new(),
    )
}

#[test]
fn test_mut_borrow_released_at_scope_exit() {
    // Borrow in the inner block should be released before the second
    // borrow at the outer scope.
    let source = "\
procedure main
    var x := 5.0
    region r
        var y := &mut x
    var z := &mut x
";
    analyze(source).expect("scope-exit release should allow re-borrow");
}

#[test]
fn test_double_mut_borrow_same_scope_fails() {
    let source = "\
procedure main
    var x := 5.0
    var y := &mut x
    var z := &mut x
";
    let result = analyze(source);
    assert!(result.is_err(), "double mut-borrow should fail");
}

#[test]
fn test_literal_null_deref_rejected() {
    let source = "\
procedure main
    val x := *null
";
    assert!(analyze(source).is_err());
}

#[test]
fn test_known_null_binding_deref_rejected() {
    let source = "\
procedure main
    val p := null
    val x := *p
";
    assert!(analyze(source).is_err());
}

#[test]
fn test_null_as_value_accepted() {
    let source = "\
procedure main
    val p := null
    if p == null then
        print(\"ok\")
";
    assert!(analyze(source).is_ok());
}

#[test]
fn test_if_branch_void_mismatch_rejected() {
    let source = "\
procedure main
    val x := if true
        print(\"done\")
    else
        42.0
";
    assert!(
        analyze(source).is_err(),
        "if with one Void and one value branch should be rejected"
    );
}

#[test]
fn test_if_both_branches_void_accepted() {
    let source = "\
procedure main
    if true
        print(\"a\")
    else
        print(\"b\")
";
    assert!(
        analyze(source).is_ok(),
        "if with both branches Void should be accepted"
    );
}

#[test]
fn test_if_both_branches_produce_value_accepted() {
    let source = "\
procedure main
    val x := if true
        1.0
    else
        2.0
    print(x)
";
    assert!(
        analyze(source).is_ok(),
        "if with both branches producing values should be accepted"
    );
}

#[test]
fn test_match_arm_void_mismatch_rejected() {
    let source = "\
procedure main
    val x := match 1
        case 1
            42.0
        case 2
            print(\"done\")
";
    assert!(
        analyze(source).is_err(),
        "match with mixed Void and value arms should be rejected"
    );
}
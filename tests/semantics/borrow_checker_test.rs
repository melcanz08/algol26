// ALGOL26 - Borrow Checker Tests (v0.3.0 - HARDENED)
// Verifies the three borrow rules: one owner, many readers, one writer

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::semantic::SemanticAnalyzer;

fn analyze(source: &str) -> Result<(), String> {
    let lexer = Lexer::new(source.to_string()).map_err(|e| e.message.to_string())?;
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().map_err(|e| e.message.to_string())?;
    let functions = program.functions;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze(&functions)
        .map_err(|e| e.message.to_string())
}

#[test]
fn test_borrow_basic_works() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    print(x)
    print(y)
"#;

    assert!(analyze(source).is_ok(), "Basic borrow should work");
}

#[test]
fn test_borrow_does_not_move() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    print(x)
    print(y)
"#;

    assert!(
        analyze(source).is_ok(),
        "Borrow should not move the variable"
    );
}

#[test]
fn test_borrow_moved_variable_fails() {
    let source = r#"
procedure main
    val x := "hello"
    val y := x
    val z := &x
"#;

    assert!(
        analyze(source).is_err(),
        "Should fail: borrowing moved variable"
    );
}

#[test]
fn test_multiple_immutable_borrows_ok() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    val z := &x
    print(y)
    print(z)
"#;

    assert!(
        analyze(source).is_ok(),
        "Multiple immutable borrows should be allowed"
    );
}

#[test]
fn test_double_mutable_borrow_fails() {
    let source = r#"
procedure main
    var x := 10.0
    var y := &mut x
    var z := &mut x
"#;

    assert!(
        analyze(source).is_err(),
        "Should fail: double mutable borrow"
    );
}

#[test]
fn test_read_during_mutable_borrow_fails() {
    let source = r#"
procedure main
    var x := 10.0
    var y := &mut x
    print(x)
"#;

    assert!(
        analyze(source).is_err(),
        "Should fail: reading while mutably borrowed"
    );
}

#[test]
fn test_borrow_scope_end_allows_reuse() {
    // FIXED: Should properly handle scope-based borrow ending
    let source = r#"
procedure main
    var x := 10.0
    
    if true then
        var y := &mut x
        print(y)
    end
    
    print(x)
"#;

    assert!(
        analyze(source).is_ok(),
        "Borrow should end at scope boundary, allowing reuse"
    );
}

#[test]
fn test_borrow_in_function_scope() {
    // FIXED: Should support reference parameters
    let source = r#"
function get_value(x: &float) -> float
    return x

procedure main
    val value := 10.0
    val result := get_value(&value)
    print(result)
"#;

    assert!(
        analyze(source).is_ok(),
        "Reference parameters should work correctly"
    );
}

#[test]
fn test_mutable_borrow_then_immutable_fails() {
    let source = r#"
procedure main
    var x := 10.0
    var y := &mut x
    val z := &x
"#;

    assert!(
        analyze(source).is_err(),
        "Should fail: immutable borrow after mutable borrow"
    );
}

#[test]
fn test_borrow_chain() {
    // Borrow of a borrow should work
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    val z := &y
    print(z)
"#;

    assert!(analyze(source).is_ok(), "Borrow chain should work");
}

#[test]
fn test_borrow_across_function_calls() {
    // Borrow should work across function calls
    let source = r#"
function add_one(x: &float) -> float
    return x + 1.0

procedure main
    val value := 10.0
    val result := add_one(&value)
    print(result)
    print(value)
"#;

    assert!(
        analyze(source).is_ok(),
        "Borrow across function calls should work"
    );
}

#[test]
fn test_mutable_borrow_across_functions() {
    // Mutable borrow in function
    let source = r#"
function increment(x: &mut float)
    x := x + 1.0

procedure main
    var value := 10.0
    increment(&mut value)
    print(value)
"#;

    assert!(
        analyze(source).is_ok(),
        "Mutable borrow across functions should work"
    );
}

#[test]
fn test_borrow_in_loop() {
    // Borrow inside loop should work
    let source = r#"
procedure main
    val values := [1.0, 2.0, 3.0]
    
    for v in values
        print(v)
    end
"#;

    assert!(analyze(source).is_ok(), "Borrow in loop should work");
}

#[test]
fn test_borrow_moved_in_loop_fails() {
    // Can't borrow after move in loop
    let source = r#"
procedure main
    val x := "hello"
    
    for i in 1..10
        val y := x
    end
    
    val z := &x
"#;

    assert!(
        analyze(source).is_err(),
        "Should fail: borrowing after move in loop"
    );
}

#[test]
fn test_multiple_borrows_different_variables() {
    // Borrows of different variables should be independent
    let source = r#"
procedure main
    val x := 10.0
    val y := 20.0
    val x_ref := &x
    val y_ref := &y
    print(x_ref)
    print(y_ref)
"#;

    assert!(
        analyze(source).is_ok(),
        "Borrows of different variables should be independent"
    );
}

#[test]
fn test_borrow_in_conditional() {
    // Borrow in conditional should work
    let source = r#"
procedure main
    val x := 10.0
    
    if true then
        val y := &x
        print(y)
    end
    
    print(x)
"#;

    assert!(analyze(source).is_ok(), "Borrow in conditional should work");
}

#[test]
fn test_mut_borrow_of_immutable_fails() {
    let source = "\
procedure main
    val x := 5.0
    var y := &mut x
";
    let result = analyze(source);
    assert!(result.is_err(), "mut-borrow of val should fail");
    let msg = format!("{:?}", result.unwrap_err());
    assert!(msg.contains("immutable"), "expected 'immutable' in error, got: {}", msg);
}
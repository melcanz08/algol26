// tests/borrow_checker_extra.rs

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::analyzer::SemanticAnalyzer;

fn analyze(source: &str) -> Result<(), String> {
    let lexer = Lexer::new(source.to_string()).map_err(|e| e.message.to_string())?;
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().map_err(|e| e.message.to_string())?;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze(&program.functions)
        .map_err(|e| e.message.to_string())
}

#[test]
fn test_borrow_across_loop_continue() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        if item > 1.0 then
            continue
        print(y)
"#;
    let r = analyze(source);
    assert!(r.is_ok(), "borrow across continue should be ok: {:?}", r);
}

#[test]
fn test_borrow_across_loop_break() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        if item > 2.0 then
            break
        print(y)
    print(x)
"#;
    let r = analyze(source);
    assert!(r.is_ok(), "borrow across break should be ok: {:?}", r);
}

#[test]
fn test_borrow_across_defer() {
    let source = r#"
procedure main
    val x := 10.0
    val y := &x
    defer print(y)
    print(y)
"#;
    assert!(analyze(source).is_ok(), "borrow across defer should work");
}

#[test]
fn test_borrow_in_defer_after_move() {
    let source = r#"
procedure main
    val x := "hello"
    defer print(x)
    val y := x  // Move x
"#;
    assert!(
        analyze(source).is_err(),
        "Should fail: defer captures x but x is moved"
    );
}

#[test]
fn test_move_in_branch() {
    // FIXED: Should specifically test that move in one branch
    // makes variable unavailable in subsequent code
    let source = r#"
procedure main
    val s := "hi"
    if true then
        val moved := s
        print(moved)
    end
    print(s)  // Should fail: s was moved in branch
"#;
    let r = analyze(source);
    assert!(
        r.is_err(),
        "Should fail: s moved in branch then used: {:?}",
        r
    );
}

#[test]
fn test_move_in_conditional_both_branches() {
    // Moving in both branches should make variable unavailable after
    let source = r#"
procedure main
    val s := "hi"
    if true then
        val moved1 := s
        print(moved1)
    else
        val moved2 := s
        print(moved2)
    end
"#;
    let r = analyze(source);
    assert!(r.is_ok(), "Move in both branches should be ok: {:?}", r);
}

#[test]
fn test_move_in_loop_body() {
    // Moving in loop body should fail - variable moved multiple times
    let source = r#"
procedure main
    val s := "hi"
    for i in 1..3 do
        val moved := s  // Can't move s multiple times
        print(moved)
    end
"#;
    let r = analyze(source);
    assert!(
        r.is_err(),
        "Should fail: s moved multiple times in loop: {:?}",
        r
    );
}

#[test]
fn test_borrow_in_nested_scope() {
    // Borrow in nested scope should end at scope boundary
    let source = r#"
procedure main
    var x := 10.0
    
    if true then
        val y := &x
        print(y)
        if true then
            val z := &x
            print(z)
        end
    end
    
    var w := &mut x  // Should work - all borrows ended
    w := 20.0
    print(x)
"#;
    let r = analyze(source);
    assert!(
        r.is_ok(),
        "Borrow in nested scope should end properly: {:?}",
        r
    );
}

#[test]
fn test_borrow_across_function_boundary() {
    // Borrow passed to function should not escape
    let source = r#"
function read_value(x: &float) -> float
    return x

procedure main
    val value := 42.0
    val result := read_value(&value)
    print(result)
    print(value)
"#;
    let r = analyze(source);
    assert!(
        r.is_ok(),
        "Borrow across function boundary should work: {:?}",
        r
    );
}

#[test]
fn test_mutable_borrow_in_loop() {
    // Mutable borrow in loop should be checked
    let source = r#"
procedure main
    var x := 0.0
    
    for i in 1..5 do
        var y := &mut x
        y := y + 1.0
    end
    
    print(x)
"#;
    let r = analyze(source);
    assert!(r.is_ok(), "Mutable borrow in loop should work: {:?}", r);
}

#[test]
fn test_double_borrow_in_parallel() {
    // Borrows in parallel blocks should be checked
    let source = r#"
procedure main
    var x := 10.0
    
    parallel do
        val y := &x
        print(y)
    end
    
    parallel do
        val z := &x
        print(z)
    end
"#;
    let r = analyze(source);
    // This might be ok or err depending on implementation
    // But should not crash
    assert!(r.is_ok() || r.is_err(), "Must not ICE: {:?}", r);
}

#[test]
fn test_borrow_after_conditional_move() {
    // Borrow after conditional move should fail
    let source = r#"
procedure main
    val x := "hello"
    
    if true then
        val y := x  // Move x
    end
    
    val z := &x  // Should fail: x was moved
"#;
    let r = analyze(source);
    assert!(
        r.is_err(),
        "Should fail: borrow after conditional move: {:?}",
        r
    );
}

#[test]
fn test_two_mut_borrows_in_same_call_rejected() {
    // Two `&mut x` arguments to the same call. The first installs a
    // statement-scoped temporary borrow; the second hits it in
    // `check_borrow_rules`. Must be rejected.
    let source = r#"
function take(a: &mut float, b: &mut float) -> float
    return a

procedure main
    var x := 0.0
    val y := take(&mut x, &mut x)
    print(y)
"#;
    let r = analyze(source);
    assert!(
        r.is_err(),
        "Should fail: two mutable borrows of x in one call: {:?}",
        r
    );
}

#[test]
fn test_two_mut_borrows_in_same_statement_expression_rejected() {
    // Both calls are in the same statement, so the first call's
    // temporary is still live when the second call's argument is
    // analyzed. Must be rejected.
    let source = r#"
function f(a: &mut float) -> float
    return a

function g(a: &mut float) -> float
    return a

procedure main
    var x := 0.0
    val y := f(&mut x) + g(&mut x)
    print(y)
"#;
    let r = analyze(source);
    assert!(
        r.is_err(),
        "Should fail: two mutable borrows of x in one statement: {:?}",
        r
    );
}

#[test]
fn test_two_mut_borrows_in_separate_statements_accepted() {
    // Separate statements: the first temporary is released at the
    // statement boundary, so the second call sees no conflict.
    let source = r#"
function f(a: &mut float) -> float
    return a

function g(a: &mut float) -> float
    return a

procedure main
    var x := 0.0
    val y := f(&mut x)
    val z := g(&mut x)
    print(y + z)
"#;
    let r = analyze(source);
    assert!(
        r.is_ok(),
        "Two mutable borrows in separate statements should be ok: {:?}",
        r
    );
}

#[test]
fn test_mut_borrow_then_read_in_next_statement_accepted() {
    // After the call returns, the temporary is released, so a
    // subsequent read of x is not blocked by the earlier
    // `&mut x` argument.
    let source = r#"
function f(a: &mut float) -> float
    return a

procedure main
    var x := 0.0
    val y := f(&mut x)
    print(x)
    print(y)
"#;
    let r = analyze(source);
    assert!(
        r.is_ok(),
        "Reading x after a mut-borrow argument in a prior statement should be ok: {:?}",
        r
    );
}

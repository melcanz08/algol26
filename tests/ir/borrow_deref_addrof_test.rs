// tests/borrow_deref_addrof_test.rs - FIXED
use algol26::common::diagnostics::Result;
use algol26::compiler::Compiler;

fn compile_and_check(source: &str) -> Result<()> {
    let mut compiler = Compiler::new();
    compiler.compile(source, "test.gol", "test_output", true, false)
}

#[test]
fn test_borrow_expression() {
    let source = r#"
procedure main
    val x := 5.0
    val y := &x
    print(y)
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "Borrow expression should compile"
    );
}

#[test]
fn test_deref_expression() {
    let source = r#"
procedure main
    val x := 5.0
    val y := &x
    val z := *y
    print(z)
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "Deref expression should compile"
    );
}

#[test]
fn test_addrof_expression() {
    let source = r#"
procedure main
    val x := 5.0
    val y := &x
    print(y)
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "AddrOf expression should compile"
    );
}

#[test]
fn test_double_borrow_fails() {
    let source = r#"
procedure main
    var x := 5.0
    var y := &mut x
    var z := &mut x
"#;

    assert!(
        compile_and_check(source).is_err(),
        "Double mutable borrow should fail"
    );
}

#[test]
fn test_borrow_immutable_fails() {
    let source = r#"
procedure main
    val x := 5.0
    var y := &mut x
"#;

    assert!(
        compile_and_check(source).is_err(),
        "Cannot mutably borrow immutable variable"
    );
}

#[test]
fn test_deref_non_pointer_fails() {
    let source = r#"
procedure main
    val x := 5.0
    val y := *x
"#;

    assert!(
        compile_and_check(source).is_err(),
        "Cannot dereference non-pointer"
    );
}

use algol26::common::diagnostics::Result;
use algol26::compiler::Compiler;

fn compile_and_check(source: &str) -> Result<()> {
    let mut compiler = Compiler::new();
    compiler.compile(source, "test.gol", "test_output", true, false)
}

#[test]
fn test_borrow_expression() {
    let source = r#"
function main() -> Void
    val x := 5
    val y := &x
    print y
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "Borrow expression should compile"
    );
}

#[test]
fn test_deref_expression() {
    let source = r#"
function main() -> Void
    val x := 5
    val y := &x
    val z := *y
    print z
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "Deref expression should compile"
    );
}

#[test]
fn test_addrof_expression() {
    let source = r#"
function main() -> Void
    val x := 5
    val y := &x
    print y
"#;

    assert!(
        compile_and_check(source).is_ok(),
        "AddrOf expression should compile"
    );
}

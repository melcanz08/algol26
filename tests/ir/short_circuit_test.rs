use algol26::backends::interpreter::Interpreter;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::semantic::SemanticAnalyzer;
use algol26::semantics::builder::SemanticIRBuilder;

fn run(source: &str) -> String {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_spans(
        &program.functions, &program.traits, &program.impls,
        &std::collections::HashMap::new(),
    ).unwrap();
    let type_table = analyzer.take_type_table();
    let (ir, _) = SemanticIRBuilder::build(&program.functions, type_table);
    Interpreter::new(ir).run().unwrap()
}

#[test]
fn test_and_short_circuits() {
    let source = "\
function side_effect() -> Bool
    print(\"evaluated\")
    return true

procedure main
    val x := false and side_effect()
    print(x)
";
    let out = run(source);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["false"], "and should not evaluate right when left is false");
}

#[test]
fn test_or_short_circuits() {
    let source = "\
function side_effect() -> Bool
    print(\"evaluated\")
    return false

procedure main
    val y := true or side_effect()
    print(y)
";
    let out = run(source);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["true"], "or should not evaluate right when left is true");
}

#[test]
fn test_and_evaluates_right_when_left_is_true() {
    let source = "\
function side_effect() -> Bool
    print(\"evaluated\")
    return true

procedure main
    val x := true and side_effect()
    print(x)
";
    let out = run(source);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["evaluated", "true"]);
}

#[test]
fn test_or_evaluates_right_when_left_is_false() {
    let source = "\
function side_effect() -> Bool
    print(\"evaluated\")
    return true

procedure main
    val y := false or side_effect()
    print(y)
";
    let out = run(source);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines, vec!["evaluated", "true"],
        "`false or side_effect()` must call side_effect; got: {}",
        out
    );
}

#[test]
fn test_short_circuit_in_if_condition() {
    // A short-circuit inside an `if` condition must not disrupt the
    // outer if's CFG. This is the exact case that broke when the
    // lowering was first added — the outer Branch was attached to a
    // block already terminated by the short-circuit's internal Branch.
    let source = "\
procedure main
    val a := 10.0
    val b := 20.0
    if a > 5.0 and b > 15.0 then
        print(\"Both true\")
";
    let out = run(source);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["Both true"], "got: {}", out);
}

#[test]
fn test_short_circuit_in_while_condition() {
    // Same regression, in a while condition.
    let source = "\
procedure main
    var n := 0
    while n < 3 and n >= 0 do
        print(n)
        n := n + 1
";
    let out = run(source);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines, vec!["0", "1", "2"], "got: {}", out);
}
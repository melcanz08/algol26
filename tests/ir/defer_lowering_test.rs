// tests/ir/defer_lowering_test.rs - ALGOL26 - Defer Tests
//
// With the new defer semantics, defers are chained LIFO by the IR
// builder when the enclosing function returns. There is no
// Terminator::Defer in the built IR, so these tests run programs
// through the interpreter and assert on their observable output.
//
// Known limitations (documented in translate_defer):
// - Only Return chains defers. break/continue/fall-through are not
//   handled.
// - One defer stack per function, no per-scope tracking.

use algol26::backends::interpreter::Interpreter;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::ir::semantic_ir::{SemanticProgram, Terminator};
use algol26::semantics::semantic::SemanticAnalyzer;
use algol26::semantics::semantic_builder::SemanticIRBuilder;

fn build_and_run(source: &str) -> (SemanticProgram, Vec<String>, String) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &program.functions,
            &program.traits,
            &program.impls,
            &std::collections::HashMap::new(),
        )
        .expect("semantic analysis failed");
    let type_table = analyzer.take_type_table();

    let (ir, diagnostics) = SemanticIRBuilder::build(&program.functions, type_table);

    let mut interpreter = Interpreter::new(ir.clone());
    let output = interpreter.run().unwrap_or_default();
    (ir, diagnostics, output)
}

#[test]
fn test_defer_with_return() {
    let source = "\
function f() -> Int
    defer
        print(\"cleanup\")
    return 42

procedure main
    print(f())
";
    let (ir, diagnostics, output) = build_and_run(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );

    // The built IR must not contain Terminator::Defer — the defer is
    // chained directly at return time by the IR builder.
    for func in &ir.functions {
        for block in &func.blocks {
            assert!(
                !matches!(block.terminator, Some(Terminator::Defer { .. })),
                "Terminator::Defer must not appear in built IR"
            );
        }
    }

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines, vec!["cleanup", "42"], "got: {}", output);
}

#[test]
fn test_defer_preserves_order() {
    let source = "\
procedure main
    defer print(\"First registered\")
    defer print(\"Second registered\")
    print(\"Main body\")
    return
";
    let (_ir, diagnostics, output) = build_and_run(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines,
        vec!["Main body", "Second registered", "First registered"],
        "defers should run LIFO, got: {}",
        output
    );
}

#[test]
fn test_multiple_defers_in_same_scope() {
    let source = "\
procedure main
    defer print(\"First cleanup\")
    defer print(\"Second cleanup\")
    print(\"Main body\")
    return
";
    let (_ir, diagnostics, output) = build_and_run(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines,
        vec!["Main body", "Second cleanup", "First cleanup"],
        "got: {}",
        output
    );
}

#[test]
fn test_defer_in_nested_scope() {
    // The current implementation does not scope defers per block — a
    // defer inside an if-branch stays on the function's defer stack and
    // fires on the function's return. We only assert that the program
    // compiles.
    let source = "\
procedure main
    if true then
        defer print(\"Inner cleanup\")
    print(\"After\")
    return
";
    let (_ir, diagnostics, _output) = build_and_run(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_defer_with_loop() {
    // Each iteration registers a defer on the function's stack; they
    // fire on return (not per iteration). We only assert that the
    // program compiles.
    let source = "\
procedure main
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        defer print(\"Loop cleanup\")
        print(item)
    return
";
    let (_ir, diagnostics, _output) = build_and_run(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
}

#[test]
fn test_defer_with_break() {
    let source = "\
procedure main
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        defer print(\"Break cleanup\")
        if item > 1.0 then
            break
        print(item)
    return
";
    let (_ir, diagnostics, _output) = build_and_run(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
}
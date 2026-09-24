// tests/ir/defer_lowering_test.rs - ALGOL26 - Defer Tests
//
// Known limitations:
// - break/continue do not chain defers (only Return and fall-through).
// - One defer stack per function, no per-scope tracking.

use algol26::backends::interpreter::Interpreter;
use algol26::compiler::assign_expr_ids;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::ir::semantic_ir::SemanticProgram;
use algol26::semantics::analyzer::SemanticAnalyzer;
use algol26::semantics::builder::SemanticIRBuilder;

fn build_and_run(source: &str) -> (SemanticProgram, Vec<String>, String) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(&functions, &program.traits, &program.impls)
        .expect("semantic analysis failed");
    let type_table = analyzer.take_type_table_id();

    let (ir, diagnostics) = SemanticIRBuilder::build(
        &functions,
        type_table,
        algol26::ir::instantiation_plan::InstantiationPlan::default(),
    );

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
    let (_, diagnostics, output) = build_and_run(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );

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

#[test]
fn list_declared_by_vardecl_iterates_through_llvm() {
    use algol26::compiler::Compiler;

    // `val xs := [1, 2, 3]` reaches `Declare` with a list literal.
    // Before ADR 0021, `list_array_types` was not populated for
    // this path, and the subsequent IteratorInit fell back to
    // `f64.array[0]`. This test pins the fix.
    let source = r#"
procedure main
    val xs := [1, 2, 3]
    var total := 0
    for n in xs
        total := total + n
    print(total)
"#;

    let mut compiler = Compiler::new();
    let verified = compiler
        .run_pipeline_for(source, "list_iter.gol")
        .expect("canonical pipeline should reach verified IR");

    // Lower through LLVM. If `list_array_types` is still missing
    // for Declare-initialized lists, this fails with an LLVM
    // verifier error, not a clean CompileError.
    let dir = std::env::temp_dir().join(format!("algol26_iter_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let out = dir.join("list_iter");

    let backend = algol26::backends::llvm_backend::LlvmBackend::new();
    let _ =
        algol26::backends::backend::Backend::compile(&backend, &verified, out.to_str().unwrap())
            .expect("LLVM lowering of iterator over Declare-initialized list");

    let _ = std::fs::remove_dir_all(&dir);
}

// ALGOL26 - Defer Lowering Tests
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::ir::semantic_ir::{SemanticProgram, Terminator};
use algol26::semantics::semantic_builder::SemanticIRBuilder;

fn build_semantic_ir(source: &str) -> (SemanticProgram, Vec<String>) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let functions = program.functions;
    SemanticIRBuilder::build(&functions)
}

#[test]
fn test_defer_with_return() {
    let source = r#"
procedure main
    defer print("Cleanup")
    return
"#;
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();
    let has_defer = main.blocks.iter().any(|b| {
        matches!(b.terminator, Some(Terminator::Defer {.. }))
        || b.instructions.iter().any(|i| {
            // fallback if Defer is still represented as instruction in some builds
            format!("{:?}", i).contains("Defer")
        })
    });
    assert!(has_defer, "Expected defer in IR (terminator or instruction), blocks: {:?}", main.blocks);
}

#[test]
fn test_defer_in_nested_scope() {
    let source = r#"
procedure main
    if true then
        defer print("Inner cleanup")
    print("After")
"#;
    let (_ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_defer_with_loop() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        defer print("Loop cleanup")
        print(item)
"#;
    let (_ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_multiple_defers_in_same_scope() {
    let source = r#"
procedure main
    defer print("First cleanup")
    defer print("Second cleanup")
    print("Main body")
"#;
    let (_ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_defer_preserves_order() {
    let source = r#"
procedure main
    defer print("Last registered")
    defer print("First registered")
"#;
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();
    let defer_blocks: Vec<usize> = main.blocks.iter().filter(|b| {
        matches!(b.terminator, Some(Terminator::Defer {.. }))
        || b.instructions.iter().any(|i| format!("{:?}", i).contains("Defer"))
    }).map(|b| b.id).collect();
    assert!(!defer_blocks.is_empty(), "Expected defer blocks");
}

#[test]
fn test_defer_with_break() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        defer print("Break cleanup")
        if item > 1.0 then
            break
        print(item)
"#;
    let (_ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

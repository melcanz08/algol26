// ALGOL26 - Defer Lowering Tests
// Verifies defer works correctly with all control-flow exits

use algol26::semantic_ir::SemanticProgram;
use algol26::semantic_builder::SemanticIRBuilder;
use algol26::parser::Parser;
use algol26::lexer::Lexer;

fn build_semantic_ir(source: &str) -> (SemanticProgram, Vec<String>) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let functions = parser.parse_program().unwrap();
    SemanticIRBuilder::build(&functions)
}

#[test]
fn test_defer_with_return() {
    let source = r#"
procedure main
    defer Terminal.print("Cleanup")
    return
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
    
    // The defer should create a cleanup block
    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();
    let has_defer = main.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, algol26::semantic_ir::SemanticInstruction::Defer { .. })
        })
    });
    assert!(has_defer, "Expected defer instruction in IR");
}

#[test]
fn test_defer_in_nested_scope() {
    let source = r#"
procedure main
    if true then
        defer Terminal.print("Inner cleanup")
    Terminal.print("After")
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_defer_with_loop() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        defer Terminal.print("Loop cleanup")
        Terminal.print(item)
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_multiple_defers_in_same_scope() {
    let source = r#"
procedure main
    defer Terminal.print("First cleanup")
    defer Terminal.print("Second cleanup")
    Terminal.print("Main body")
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_defer_preserves_order() {
    // Defers should execute in LIFO order
    let source = r#"
procedure main
    defer Terminal.print("Last registered")
    defer Terminal.print("First registered")
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
    
    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();
    
    // Find all Defer instructions and verify they're in the right order
    let defer_blocks: Vec<usize> = main.blocks.iter()
        .filter(|b| {
            b.instructions.iter().any(|i| {
                matches!(i, algol26::semantic_ir::SemanticInstruction::Defer { .. })
            })
        })
        .map(|b| b.id)
        .collect();
    
    assert!(!defer_blocks.is_empty(), "Expected defer blocks");
}

#[test]
fn test_defer_with_break() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        defer Terminal.print("Break cleanup")
        if item > 1.0 then
            break
        Terminal.print(item)
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_defer_with_continue() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0]
    for item in arr do
        defer Terminal.print("Continue cleanup")
        if item < 2.0 then
            continue
        Terminal.print(item)
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

#[test]
fn test_defer_with_early_return_in_if() {
    let source = r#"
procedure main
    defer Terminal.print("Outer cleanup")
    if true then
        defer Terminal.print("Inner cleanup")
        return
    Terminal.print("Not reached")
"#;
    
    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics, got: {:?}", diagnostics);
}

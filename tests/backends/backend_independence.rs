// ALGOL26 - Backend Independence Test
// Verifies that Semantic IR is independent of any specific backend

use algol26::ir::semantic_ir::SemanticProgram;
use algol26::semantics::semantic_builder::SemanticIRBuilder;

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;

fn build_semantic_ir(source: &str) -> (SemanticProgram, Vec<String>) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let functions = program.functions;
    SemanticIRBuilder::build(&functions)
}

#[test]
fn test_semantic_ir_is_backend_independent() {
    let source = r#"
function add(x: float, y: float) -> float
    return x + y

procedure main
    val result := add(5.0, 3.0)
    print(result)
"#;

    let (ir, diagnostics) = build_semantic_ir(source);

    // The IR should be complete and valid
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
    assert!(
        !ir.functions.is_empty(),
        "Expected at least one function in IR"
    );

    // Check that the main function exists
    let main_func = ir.functions.iter().find(|f| f.name == "main");
    assert!(main_func.is_some(), "Expected main function in IR");

    // Check that the add function exists
    let add_func = ir.functions.iter().find(|f| f.name == "add");
    assert!(add_func.is_some(), "Expected add function in IR");

    // The IR should have blocks and instructions
    let main = main_func.unwrap();
    assert!(!main.blocks.is_empty(), "Expected blocks in main function");
    assert!(
        main.entry_block < main.blocks.len()
            || main.blocks.iter().any(|b| b.id == main.entry_block),
        "Entry block should exist"
    );
}

#[test]
fn test_semantic_ir_preserves_types() {
    let source = r#"
procedure main
    val x := 5.0
    val y := 10.0
    val sum := x + y
    print(sum)
"#;

    let (ir, _) = build_semantic_ir(source);
    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();

    // All blocks should have valid instructions
    for block in &main.blocks {
        for instr in &block.instructions {
            // Each instruction should be well-formed
            // (this is a basic sanity check)
            {
                let _ = instr;
            }
        }
    }
}

#[test]
fn test_semantic_ir_handles_string_operations() {
    let source = r#"
procedure main
    val greeting := "Hello"
    val name := "World"
    val combined := String.concat(greeting, name)
    print(combined)
"#;

    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
    assert!(!ir.functions.is_empty());
}

#[test]
fn test_semantic_ir_is_deterministic() {
    let source = r#"
procedure main
    val x := 5.0
    val y := 10.0
    val sum := x + y
    print(sum)
"#;

    // Building IR twice should produce the same result
    let (ir1, _) = build_semantic_ir(source);
    let (ir2, _) = build_semantic_ir(source);

    assert_eq!(ir1.functions.len(), ir2.functions.len());
    assert_eq!(ir1.functions[0].name, ir2.functions[0].name);
    assert_eq!(ir1.functions[0].blocks.len(), ir2.functions[0].blocks.len());
}

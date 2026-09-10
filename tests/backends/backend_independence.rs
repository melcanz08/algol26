// tests/backends/backend_independence.rs - HARDENED
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::ir::semantic_ir::SemanticProgram;
use algol26::semantics::semantic_builder::SemanticIRBuilder;

fn build_semantic_ir(source: &str) -> (SemanticProgram, Vec<String>) {
    use algol26::semantics::semantic::SemanticAnalyzer;

    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let functions = program.functions;

    // ─── UNIFY TYPES ─── run the analyzer so the IR builder has real types.
    let mut analyzer = SemanticAnalyzer::new();
    let span_map = std::collections::HashMap::new();
    analyzer
        .analyze_with_spans(&functions, &program.traits, &program.impls, &span_map)
        .expect("semantic analysis failed");

    let type_table = analyzer.take_type_table();
    SemanticIRBuilder::build(&functions, type_table)
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

    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );
    assert!(
        !ir.functions.is_empty(),
        "Expected at least one function in IR"
    );

    // Check functions
    let main_func = ir.functions.iter().find(|f| f.name == "main");
    assert!(main_func.is_some(), "Expected main function in IR");

    let add_func = ir.functions.iter().find(|f| f.name == "add");
    assert!(add_func.is_some(), "Expected add function in IR");

    // Verify main function structure
    let main = main_func.unwrap();
    assert!(!main.blocks.is_empty(), "Expected blocks in main function");
    assert!(
        main.blocks.iter().any(|b| b.id == main.entry_block),
        "Entry block should exist"
    );

    // Verify all blocks have terminators
    for block in &main.blocks {
        assert!(
            block.terminator.is_some(),
            "Block {} should have terminator",
            block.id
        );
    }

    // Verify add function
    let add = add_func.unwrap();
    assert_eq!(add.return_type, algol26::common::types::Type::Float);
    assert_eq!(add.params.len(), 2);
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

    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(diagnostics.is_empty(), "Expected no diagnostics");

    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();

    // Check that instructions have proper types
    let mut has_print = false;
    for block in &main.blocks {
        for instr in &block.instructions {
            match instr {
                algol26::ir::semantic_ir::Instruction::Print { value } => {
                    has_print = true;
                    // Value should have a valid type
                    assert!(value.type_of() != algol26::common::types::Type::Unknown);
                }
                algol26::ir::semantic_ir::Instruction::Declare { type_, .. } => {
                    assert!(*type_ != algol26::common::types::Type::Unknown);
                }
                _ => {}
            }
        }
    }

    assert!(has_print, "Expected print instruction");
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

    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();

    // Should have a call to String.concat
    let has_concat = main.blocks.iter().any(|b| {
        b.instructions.iter().any(|i| {
            matches!(i, algol26::ir::semantic_ir::Instruction::Call { func: function, .. }
                if function == "String.concat")
        })
    });

    assert!(has_concat, "Expected String.concat call");
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

    let (ir1, diags1) = build_semantic_ir(source);
    let (ir2, diags2) = build_semantic_ir(source);

    assert_eq!(diags1, diags2, "Diagnostics should be deterministic");
    assert_eq!(ir1.functions.len(), ir2.functions.len());
    assert_eq!(ir1.functions[0].name, ir2.functions[0].name);
    assert_eq!(ir1.functions[0].blocks.len(), ir2.functions[0].blocks.len());
    assert_eq!(ir1.functions[0].entry_block, ir2.functions[0].entry_block);
}

#[test]
fn test_semantic_ir_handles_control_flow() {
    let source = r#"
procedure main
    val x := 10.0

    if x > 5.0 then
        print("Greater")
    else
        print("Less")
    end
"#;

    let (ir, diagnostics) = build_semantic_ir(source);
    assert!(
        diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        diagnostics
    );

    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();

    // Should have branch terminator
    let has_branch = main.blocks.iter().any(|b| {
        matches!(
            b.terminator,
            Some(algol26::ir::semantic_ir::Terminator::Branch { .. })
        )
    });

    assert!(has_branch, "Expected branch terminator for if/else");
}

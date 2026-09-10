// ALGOL26 - Architecture Tests
use algol26::common::types::Type;
use algol26::semantics::flow_analyzer::FlowAnalyzer;

#[test]
fn test_flow_analyzer_termination() {
    use algol26::ir::semantic_ir::{SemanticBlock, Terminator};

    // Block without terminator is not terminated
    let block = SemanticBlock {
        id: 0,
        instructions: Vec::new(),
        terminator: None,
    };
    assert!(!FlowAnalyzer::is_terminated(&block));

    // Block with Return is terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![],
        terminator: Some(Terminator::Return {
            value: None,
            type_: Type::Void,
        }),
    };
    assert!(FlowAnalyzer::is_terminated(&block));

    // Block with Jump is terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![],
        terminator: Some(Terminator::Jump { block: 1 }),
    };
    assert!(FlowAnalyzer::is_terminated(&block));

    // Block with Branch is terminated
    let block = SemanticBlock {
        id: 0,
        instructions: vec![],
        terminator: Some(Terminator::Branch {
            condition: algol26::ir::semantic_ir::TypedIRValue::Bool(true),
            then_block: 1,
            else_block: 2,
        }),
    };
    assert!(FlowAnalyzer::is_terminated(&block));
}

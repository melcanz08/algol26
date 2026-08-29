use algol26::optimizer::Optimizer;
use algol26::semantic_ir::{SemanticProgram, SemanticFunction, SemanticBlock, SemanticInstruction, TypedIRValue, SemanticBinOp};
use algol26::semantic_type::SemanticType;

#[test]
fn test_constant_folding() {
    let mut optimizer = Optimizer::new();
    let mut program = SemanticProgram::new();
    
    let entry = 0;
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: SemanticType::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![SemanticInstruction::Declare {
                name: "x".to_string(),
                mutable: false,
                type_: SemanticType::Float,
                value: TypedIRValue::BinaryOp {
                    op: SemanticBinOp::Add,
                    left: Box::new(TypedIRValue::Float(5.0)),
                    right: Box::new(TypedIRValue::Float(3.0)),
                    result_type: SemanticType::Float,
                },
            }],
        }],
        entry_block: entry,
    };
    
    program.functions.push(func);
    
    optimizer.optimize(&mut program);
    
    let func = &program.functions[0];
    let block = &func.blocks[0];
    
    // After folding, the BinaryOp should be replaced with a constant Float
    match &block.instructions[0] {
        SemanticInstruction::Declare { value, .. } => {
            match value {
                TypedIRValue::Float(f) => assert_eq!(*f, 8.0),
                TypedIRValue::Int(i) => assert_eq!(*i, 8),
                _ => panic!("Expected constant Float(8.0) after folding"),
            }
        }
        _ => panic!("Expected Declare instruction"),
    }
    assert_eq!(optimizer.stats.folded_constants, 1);
}

#[test]
fn test_dead_code_elimination() {
    let mut optimizer = Optimizer::new();
    let mut program = SemanticProgram::new();
    
    let entry = 0;
    let unreachable = 1;
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: SemanticType::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![SemanticInstruction::Return { value: None, type_: SemanticType::Void }],
            },
            SemanticBlock {
                id: unreachable,
                instructions: vec![],
            },
        ],
        entry_block: entry,
    };
    
    program.functions.push(func);
    
    optimizer.optimize(&mut program);
    
    assert_eq!(program.functions[0].blocks.len(), 1);
    assert_eq!(optimizer.stats.removed_blocks, 1);
}

use algol26::ir::semantic_ir::*;
use algol26::ir::optimizer::Optimizer;
use algol26::common::types::Type;

#[test]
fn test_constant_folding() {
    let mut optimizer = Optimizer::new();
    let mut program = SemanticProgram::new();
    let entry = 0;
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Int,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![
                    SemanticInstruction::Declare {
                        name: "x".to_string(),
                        mutable: false,
                        type_: Type::Int,
                        value: TypedIRValue::Int(2),
                    }
                ],
                terminator: Some(Terminator::Return {
                    value: Some(TypedIRValue::Variable("x".to_string(), Type::Int)),
                    type_: Type::Int,
                }),
            }
        ],
        entry_block: entry,
        is_extern: false,
    };
    program.functions.push(func);
    optimizer.optimize(&mut program);
    assert_eq!(program.functions[0].blocks.len(), 1);
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
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            },
            SemanticBlock {
                id: unreachable,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            },
        ],
        entry_block: entry,
        is_extern: false,
    };

    program.functions.push(func);
    optimizer.optimize(&mut program);
    assert_eq!(program.functions[0].blocks.len(), 1);
}
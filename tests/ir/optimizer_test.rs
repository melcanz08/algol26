// tests/ir/optimizer_test.rs - HARDENED
use algol26::common::types::Type;
use algol26::ir::optimizer::Optimizer;
use algol26::ir::semantic_ir::*;

#[test]
fn test_constant_folding() {
    let mut optimizer = Optimizer::new();
    let mut program = SemanticProgram::new();
    let entry = 0;

    // Create: val x := 2 + 3 (should fold to 5)
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Int,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![Instruction::Declare {
                name: "x".to_string(),
                mutable: false,
                type_: Type::Int,
                value: TypedIRValue::BinaryOp {
                    op: SemanticBinOp::Add,
                    left: Box::new(TypedIRValue::Int(2)),
                    right: Box::new(TypedIRValue::Int(3)),
                    result_type: Type::Int,
                },
            }],
            terminator: Some(Terminator::Return {
                value: Some(TypedIRValue::Variable("x".to_string(), Type::Int)),
                type_: Type::Int,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    };

    program.functions.push(func);
    optimizer.optimize(&mut program);

    // Check that constant was folded
    if let Instruction::Declare { value, .. } = &program.functions[0].blocks[0].instructions[0] {
        assert_eq!(
            *value,
            TypedIRValue::Int(5),
            "Constant should be folded to 5"
        );
    } else {
        panic!("Expected Declare instruction");
    }

    // Check stats
    assert!(optimizer.stats().folded_constants > 0);
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

    // Check unreachable block removed
    assert_eq!(program.functions[0].blocks.len(), 1);
    assert_eq!(program.functions[0].blocks[0].id, entry);
    assert!(optimizer.stats().removed_blocks > 0);
}

#[test]
fn test_division_by_zero_not_folded() {
    let mut optimizer = Optimizer::new();
    let mut program = SemanticProgram::new();
    let entry = 0;

    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Float,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![Instruction::Declare {
                name: "x".to_string(),
                mutable: false,
                type_: Type::Float,
                value: TypedIRValue::BinaryOp {
                    op: SemanticBinOp::Divide,
                    left: Box::new(TypedIRValue::Float(1.0)),
                    right: Box::new(TypedIRValue::Float(0.0)),
                    result_type: Type::Float,
                },
            }],
            terminator: Some(Terminator::Return {
                value: Some(TypedIRValue::Variable("x".to_string(), Type::Float)),
                type_: Type::Float,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    };

    program.functions.push(func);
    optimizer.optimize(&mut program);

    // Division by zero should NOT be folded
    if let Instruction::Declare { value, .. } = &program.functions[0].blocks[0].instructions[0] {
        assert!(
            matches!(value, TypedIRValue::BinaryOp { .. }),
            "Division by zero should not be folded"
        );
    }
}

#[test]
fn test_branch_simplification() {
    let mut optimizer = Optimizer::new();
    let mut program = SemanticProgram::new();
    let entry = 0;
    let then_block = 1;
    let else_block = 2;

    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Branch {
                    condition: TypedIRValue::Bool(true),
                    then_block,
                    else_block,
                }),
            },
            SemanticBlock {
                id: then_block,
                instructions: vec![],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            },
            SemanticBlock {
                id: else_block,
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

    // Branch should be simplified to jump
    assert!(matches!(
        program.functions[0].blocks[0].terminator,
        Some(Terminator::Jump { block: _then_block })
    ));
    assert!(optimizer.stats().simplified_branches > 0);
}

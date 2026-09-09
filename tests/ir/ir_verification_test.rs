// src/ir/ir_verification_test.rs - EXPANDED
use algol26::common::types::Type;
use algol26::ir::semantic_ir::{
    SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
};

#[test]
fn test_valid_program_passes_verification() {
    let mut program = SemanticProgram::new();
    let entry_id = program.new_block_id();
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry_id,
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry_id,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_ok());
}

#[test]
fn test_duplicate_block_id_fails() {
    let mut program = SemanticProgram::new();
    let block_id = program.new_block_id();
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: block_id,
                instructions: vec![],
                terminator: None,
            },
            SemanticBlock {
                id: block_id,
                instructions: vec![],
                terminator: None,
            },
        ],
        entry_block: block_id,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_err());
}

#[test]
fn test_missing_entry_block_fails() {
    let mut program = SemanticProgram::new();
    let block_id = program.new_block_id();
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: block_id,
            instructions: vec![],
            terminator: None,
        }],
        entry_block: 999,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_err());
}

#[test]
fn test_invalid_jump_target_fails() {
    let mut program = SemanticProgram::new();
    let entry_id = program.new_block_id();
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry_id,
            instructions: vec![],
            terminator: Some(Terminator::Jump { block: 999 }),
        }],
        entry_block: entry_id,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_err());
}

#[test]
fn test_missing_terminator_fails() {
    let mut program = SemanticProgram::new();
    let entry_id = program.new_block_id();
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry_id,
            instructions: vec![],
            terminator: None,
        }],
        entry_block: entry_id,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_err());
}

#[test]
fn test_unreachable_block_fails() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let unreachable = program.new_block_id();
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
    assert!(program.verify().is_err());
}

#[test]
fn test_duplicate_function_names_fail() {
    let mut program = SemanticProgram::new();

    // First function
    let entry1 = program.new_block_id();
    let func1 = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry1,
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry1,
        is_extern: false,
    };

    // Second function with same name
    let entry2 = program.new_block_id();
    let func2 = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry2,
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry2,
        is_extern: false,
    };

    program.functions.push(func1);
    program.functions.push(func2);
    assert!(program.verify().is_err());
}

#[test]
fn test_missing_return_in_non_void_function() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let func = SemanticFunction {
        name: "get_value".to_string(),
        params: vec![],
        return_type: Type::Int,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![],
            terminator: Some(Terminator::Jump { block: entry }), // Infinite loop
        }],
        entry_block: entry,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_err());
}

#[test]
fn test_return_type_mismatch() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let func = SemanticFunction {
        name: "get_value".to_string(),
        params: vec![],
        return_type: Type::Int,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: Some(TypedIRValue::String("wrong".to_string())),
                type_: Type::String,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_err());
}

#[test]
fn test_branch_condition_type() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let then_block = program.new_block_id();
    let else_block = program.new_block_id();
    let func = SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![],
            terminator: Some(Terminator::Branch {
                condition: TypedIRValue::Int(42), // Should be Bool!
                then_block,
                else_block,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    };
    program.functions.push(func);
    assert!(program.verify().is_err());
}

use algol26::common::types::Type;
use algol26::ir::semantic_ir::{
    SemanticBlock, SemanticFunction, SemanticProgram, Terminator,
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
            SemanticBlock { id: block_id, instructions: vec![], terminator: None },
            SemanticBlock { id: block_id, instructions: vec![], terminator: None },
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
        blocks: vec![SemanticBlock { id: block_id, instructions: vec![], terminator: None }],
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

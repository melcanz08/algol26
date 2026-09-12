// src/ir/verifier/tests.rs

use super::*;
use crate::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram, Terminator};

fn single_block_program(
    instructions: Vec<Instruction>,
    terminator: Option<Terminator>,
) -> SemanticProgram {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions,
            terminator: Some(terminator.unwrap_or(Terminator::Return {
                value: None,
                type_: Type::Void,
            })),
        }],
        entry_block: entry,
        is_extern: false,
    });
    program
}
#[test]
fn test_missing_return_detected() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Int,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![],
            terminator: Some(Terminator::Jump { block: entry }),
        }],
        entry_block: entry,
        is_extern: false,
    });
    assert!(verify(&program).is_err());
}
#[test]
fn test_return_type_mismatch() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
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
    });
    assert!(verify(&program).is_err());
}

// ─── Stage 1: instruction-level checks ───
#[test]
fn verifier_rejects_declare_type_mismatch() {
    let program = single_block_program(
        vec![Instruction::Declare {
            name: "x".to_string(),
            mutable: true,
            type_: Type::Int,
            value: TypedIRValue::String("hello".to_string()),
        }],
        None,
    );
    let result = verify(&program);
    assert!(result.is_err(), "expected type mismatch, got: {:?}", result);
}
#[test]
fn verifier_rejects_assign_to_immutable() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "x".to_string(),
                mutable: false,
                type_: Type::Int,
                value: TypedIRValue::Int(0),
            },
            Instruction::Assign {
                target: "x".to_string(),
                value: TypedIRValue::Int(1),
            },
        ],
        None,
    );
    assert!(verify(&program).is_err());
}
#[test]
fn verifier_rejects_undefined_variable_read() {
    let program = single_block_program(
        vec![Instruction::Print {
            value: TypedIRValue::Variable("missing".to_string(), Type::Int),
        }],
        None,
    );
    assert!(verify(&program).is_err());
}
#[test]
fn verifier_rejects_binary_op_type_mismatch() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "x".to_string(),
                mutable: true,
                type_: Type::Int,
                value: TypedIRValue::Int(0),
            },
            Instruction::Declare {
                name: "y".to_string(),
                mutable: true,
                type_: Type::Int,
                value: TypedIRValue::BinaryOp {
                    op: SemanticBinOp::Add,
                    left: Box::new(TypedIRValue::Variable("x".into(), Type::Int)),
                    right: Box::new(TypedIRValue::String("oops".into())),
                    result_type: Type::Int,
                },
            },
        ],
        None,
    );
    assert!(verify(&program).is_err());
}
#[test]
fn verifier_accepts_valid_declare() {
    let program = single_block_program(
        vec![Instruction::Declare {
            name: "x".to_string(),
            mutable: true,
            type_: Type::Float,
            value: TypedIRValue::Float(42.0),
        }],
        None,
    );
    assert!(verify(&program).is_ok());
}
#[test]
fn verifier_accepts_int_to_float_coercion() {
    let program = single_block_program(
        vec![Instruction::Declare {
            name: "x".to_string(),
            mutable: true,
            type_: Type::Float,
            value: TypedIRValue::Int(5),
        }],
        None,
    );
    assert!(verify(&program).is_ok());
}
    // ─── Stage 2: instruction-level checks ───
#[test]
fn verifier_rejects_array_assign_float_index() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "xs".to_string(),
                mutable: true,
                type_: Type::list(Type::Int),
                value: TypedIRValue::List(
                    vec![TypedIRValue::Int(1), TypedIRValue::Int(2)],
                    Type::Int,
                ),
            },
            Instruction::ArrayAssign {
                array: Box::new(TypedIRValue::Variable("xs".into(), Type::list(Type::Int))),
                index: Box::new(TypedIRValue::Float(1.5)),
                value: TypedIRValue::Int(99),
            },
        ],
        None,
    );
    assert!(verify(&program).is_err(), "float index must be rejected");
}
#[test]
fn verifier_rejects_array_assign_value_type_mismatch() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "xs".to_string(),
                mutable: true,
                type_: Type::list(Type::Int),
                value: TypedIRValue::List(
                    vec![TypedIRValue::Int(1)],
                    Type::Int,
                ),
            },
            Instruction::ArrayAssign {
                array: Box::new(TypedIRValue::Variable("xs".into(), Type::list(Type::Int))),
                index: Box::new(TypedIRValue::Int(0)),
                value: TypedIRValue::String("oops".into()),
            },
        ],
        None,
    );
    assert!(
        verify(&program).is_err(),
        "value whose type does not match the element type must be rejected"
    );
}
#[test]
fn verifier_rejects_iterator_init_on_non_list() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "n".to_string(),
                mutable: true,
                type_: Type::Int,
                value: TypedIRValue::Int(0),
            },
            Instruction::IteratorInit {
                iterator: "__it".to_string(),
                iterable: TypedIRValue::Variable("n".into(), Type::Int),
            },
        ],
        None,
    );
    assert!(verify(&program).is_err(), "iterator over Int must be rejected");
}
#[test]
fn verifier_rejects_receive_on_non_channel() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "n".to_string(),
                mutable: true,
                type_: Type::Int,
                value: TypedIRValue::Int(0),
            },
            Instruction::Receive {
                channel: "n".to_string(),
                target: "y".to_string(),
            },
        ],
        None,
    );
    assert!(verify(&program).is_err(), "receive on Int must be rejected");
}
#[test]
fn verifier_rejects_free_on_non_pointer() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "n".to_string(),
                mutable: true,
                type_: Type::Int,
                value: TypedIRValue::Int(0),
            },
            Instruction::Free {
                ptr: TypedIRValue::Variable("n".into(), Type::Int),
            },
        ],
        None,
    );
    assert!(verify(&program).is_err(), "free on Int must be rejected");
}
#[test]
fn verifier_rejects_illegal_cast() {
    let program = single_block_program(
        vec![Instruction::Declare {
            name: "x".to_string(),
            mutable: true,
            type_: Type::Int,
            value: TypedIRValue::Cast {
                value: Box::new(TypedIRValue::String("hello".into())),
                target_type: Type::Int,
            },
        }],
        None,
    );
    assert!(
        verify(&program).is_err(),
        "String -> Int cast must be rejected"
    );
}
#[test]
fn verifier_accepts_channel_send_receive() {
    let program = single_block_program(
        vec![
            Instruction::ChannelDecl {
                name: "ch".to_string(),
                type_: Type::channel(Type::Int),
            },
            Instruction::Send {
                channel: "ch".to_string(),
                value: TypedIRValue::Int(42),
            },
            Instruction::Receive {
                channel: "ch".to_string(),
                target: "got".to_string(),
            },
        ],
        None,
    );
    assert!(verify(&program).is_ok());
}
#[test]
fn verifier_accepts_iterator_over_list() {
    let program = single_block_program(
        vec![
            Instruction::Declare {
                name: "xs".to_string(),
                mutable: true,
                type_: Type::list(Type::Int),
                value: TypedIRValue::List(
                    vec![TypedIRValue::Int(1), TypedIRValue::Int(2)],
                    Type::Int,
                ),
            },
            Instruction::IteratorInit {
                iterator: "__it".to_string(),
                iterable: TypedIRValue::Variable("xs".into(), Type::list(Type::Int)),
            },
        ],
        None,
    );
    assert!(verify(&program).is_ok());
}

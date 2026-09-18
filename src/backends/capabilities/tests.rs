// src/backends/capabilities/tests.rs

use super::*;
use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticBlock, SemanticFunction, Terminator, TypedIRValue,
};

#[cfg(test)]
fn program_with(instr: Instruction, term: Terminator) -> SemanticProgram {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![instr],
            terminator: Some(term),
        }],
        entry_block: entry,
        is_extern: false,
    });
    program
}

fn simple_return() -> Terminator {
    Terminator::Return {
        value: None,
        type_: Type::Void,
    }
}

#[test]
fn llvm_accepts_plain_program() {
    let program = program_with(
        Instruction::Print {
            value: TypedIRValue::Int(1),
        },
        simple_return(),
    );
    assert!(check_backend(&program, &BackendCapabilities::llvm()).is_ok());
}

#[test]
fn llvm_rejects_result_values() {
    let program = program_with(
        Instruction::Declare {
            name: "r".to_string(),
            mutable: false,
            type_: Type::result(Type::Int, Type::String),
            value: TypedIRValue::Ok {
                value: Box::new(TypedIRValue::Int(42)),
                result_type: Type::result(Type::Int, Type::String),
            },
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
    assert!(err.message.contains("Result"));
    assert!(err.message.contains("interpreter"));
}

#[test]
fn llvm_rejects_option_values() {
    let program = program_with(
        Instruction::Declare {
            name: "m".to_string(),
            mutable: false,
            type_: Type::option(Type::Int),
            value: TypedIRValue::Some(Box::new(TypedIRValue::Int(1))),
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
    assert!(err.message.contains("Option"), "{}", err.message);
}

#[test]
fn interpreter_accepts_option_values() {
    let program = program_with(
        Instruction::Declare {
            name: "m".to_string(),
            mutable: false,
            type_: Type::option(Type::Int),
            value: TypedIRValue::Some(Box::new(TypedIRValue::Int(1))),
        },
        simple_return(),
    );
    assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());
}

#[test]
fn llvm_accepts_raw_memory() {
    // Step 5 wiring: alloc/free lower to malloc/free.
    let program = program_with(
        Instruction::Allocate {
            target: "p".to_string(),
            size: TypedIRValue::Int(8),
            type_: Type::pointer(Type::Unknown),
        },
        simple_return(),
    );
    assert!(check_backend(&program, &BackendCapabilities::llvm()).is_ok());
}

#[test]
fn interpreter_accepts_raw_memory() {
    // The interpreter has a simulated heap for alloc/free
    // (added in Step 2 wiring). Refusal is only for LLVM.
    let program = program_with(
        Instruction::Free {
            ptr: TypedIRValue::NullPtr,
        },
        simple_return(),
    );
    assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());
}

#[test]
fn interpreter_accepts_result_values() {
    let program = program_with(
        Instruction::Declare {
            name: "r".to_string(),
            mutable: false,
            type_: Type::result(Type::Int, Type::String),
            value: TypedIRValue::Ok {
                value: Box::new(TypedIRValue::Int(42)),
                result_type: Type::result(Type::Int, Type::String),
            },
        },
        simple_return(),
    );
    assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());
}

#[test]
fn llvm_rejects_spawn() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let spawned = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Spawn {
                    entry_block: spawned,
                }),
            },
            SemanticBlock {
                id: spawned,
                instructions: vec![],
                terminator: Some(simple_return()),
            },
        ],
        entry_block: entry,
        is_extern: false,
    });
    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
    assert!(err.message.contains("spawn"));
}

#[test]
fn llvm_accepts_ffi() {
    // Ffi is a supported feature for LLVM. Pin the positive case
    // so a future change to BackendCapabilities::llvm() cannot
    // silently route FFI-using programs to the interpreter.
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "puts".to_string(),
        params: vec![("s".to_string(), Type::String)],
        return_type: Type::Int,
        blocks: vec![],
        entry_block: 0,
        is_extern: true,
    });
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![Instruction::Call {
                func: "puts".to_string(),
                args: vec![TypedIRValue::String("hi".to_string())],
                result: None,
            }],
            terminator: Some(simple_return()),
        }],
        entry_block: entry,
        is_extern: false,
    });
    assert!(check_backend(&program, &BackendCapabilities::llvm()).is_ok());
}

#[test]
fn wasm_rejects_result_values() {
    // WASM supports nothing today. Result must be refused.
    let program = program_with(
        Instruction::Declare {
            name: "r".to_string(),
            mutable: false,
            type_: Type::result(Type::Int, Type::String),
            value: TypedIRValue::Ok {
                value: Box::new(TypedIRValue::Int(42)),
                result_type: Type::result(Type::Int, Type::String),
            },
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(err.message.contains("Result"), "{}", err.message);
}

#[test]
fn wasm_rejects_channels() {
    let program = program_with(
        Instruction::ChannelDecl {
            name: "ch".to_string(),
            type_: Type::channel(Type::Int),
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(err.message.contains("channels"));
}

#[test]
fn interpreter_rejects_ffi() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    // Simulate an extern function and a call to it.
    program.functions.push(SemanticFunction {
        name: "puts".to_string(),
        params: vec![("s".to_string(), Type::String)],
        return_type: Type::Int,
        blocks: vec![],
        entry_block: 0,
        is_extern: true,
    });
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![Instruction::Print {
                value: TypedIRValue::Call {
                    function: "puts".to_string(),
                    args: vec![TypedIRValue::String("hi".to_string())],
                    return_type: Type::Int,
                },
            }],
            terminator: Some(simple_return()),
        }],
        entry_block: entry,
        is_extern: false,
    });
    let err = check_backend(&program, &BackendCapabilities::interpreter()).unwrap_err();
    assert!(err.message.contains("foreign"));
}
#[test]
fn llvm_rejects_list_print() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![
                Instruction::Declare {
                    name: "xs".to_string(),
                    mutable: false,
                    type_: Type::list(Type::Float),
                    value: TypedIRValue::List(vec![TypedIRValue::Float(1.0)], Type::Float),
                },
                Instruction::Print {
                    value: TypedIRValue::Variable("xs".to_string(), Type::list(Type::Float)),
                },
            ],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    });
    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
    assert!(
        err.message.contains("printing a list"),
        "expected list-print diagnostic, got: {}",
        err.message
    );
}

#[test]
fn interpreter_accepts_list_print() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![Instruction::Print {
                value: TypedIRValue::List(vec![TypedIRValue::Float(1.0)], Type::Float),
            }],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    });
    assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());
}

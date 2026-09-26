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

#[test]
fn llvm_rejects_channels() {
    let program = program_with(
        Instruction::ChannelDecl {
            name: "ch".to_string(),
            type_: Type::channel(Type::Int),
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
    assert!(
        err.message.contains("channels"),
        "expected channel diagnostic, got: {}",
        err.message
    );
}

#[test]
fn wasm_rejects_ffi() {
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
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(
        err.message.contains("foreign"),
        "expected FFI diagnostic, got: {}",
        err.message
    );
}

#[test]
fn wasm_rejects_option_values() {
    let program = program_with(
        Instruction::Declare {
            name: "m".to_string(),
            mutable: false,
            type_: Type::option(Type::Int),
            value: TypedIRValue::Some(Box::new(TypedIRValue::Int(1))),
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(
        err.message.contains("Option"),
        "expected Option diagnostic, got: {}",
        err.message
    );
}

#[test]
fn llvm_rejects_parallel() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let a = program.new_block_id();
    let b = program.new_block_id();
    let join = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Fork {
                    blocks: vec![a, b],
                    join_block: join,
                }),
            },
            SemanticBlock {
                id: a,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: join }),
            },
            SemanticBlock {
                id: b,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: join }),
            },
            SemanticBlock {
                id: join,
                instructions: vec![],
                terminator: Some(simple_return()),
            },
        ],
        entry_block: entry,
        is_extern: false,
    });
    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
    assert!(
        err.message.contains("parallel"),
        "expected parallel diagnostic, got: {}",
        err.message
    );
}

#[test]
fn wasm_rejects_parallel() {
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let a = program.new_block_id();
    let b = program.new_block_id();
    let join = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Fork {
                    blocks: vec![a, b],
                    join_block: join,
                }),
            },
            SemanticBlock {
                id: a,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: join }),
            },
            SemanticBlock {
                id: b,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: join }),
            },
            SemanticBlock {
                id: join,
                instructions: vec![],
                terminator: Some(simple_return()),
            },
        ],
        entry_block: entry,
        is_extern: false,
    });
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(
        err.message.contains("parallel"),
        "expected parallel diagnostic, got: {}",
        err.message
    );
}

#[test]
fn wasm_rejects_spawn() {
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
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(
        err.message.contains("spawn"),
        "expected spawn diagnostic, got: {}",
        err.message
    );
}

#[test]
fn interpreter_rejects_references() {
    let program = program_with(
        Instruction::Declare {
            name: "r".to_string(),
            mutable: false,
            type_: Type::borrow(Type::Int),
            value: TypedIRValue::BorrowShared {
                expr: Box::new(TypedIRValue::Variable("x".to_string(), Type::Int)),
                target_type: Type::borrow(Type::Int),
            },
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::interpreter()).unwrap_err();
    assert!(
        err.message.contains("reference"),
        "expected reference diagnostic, got: {}",
        err.message
    );
}

#[test]
fn llvm_accepts_references() {
    let program = program_with(
        Instruction::Declare {
            name: "r".to_string(),
            mutable: false,
            type_: Type::borrow(Type::Int),
            value: TypedIRValue::BorrowShared {
                expr: Box::new(TypedIRValue::Variable("x".to_string(), Type::Int)),
                target_type: Type::borrow(Type::Int),
            },
        },
        simple_return(),
    );
    assert!(
        check_backend(&program, &BackendCapabilities::llvm()).is_ok(),
        "LLVM has codegen arms for reference operations",
    );
}

#[test]
fn wasm_rejects_references() {
    let program = program_with(
        Instruction::Declare {
            name: "r".to_string(),
            mutable: false,
            type_: Type::borrow(Type::Int),
            value: TypedIRValue::BorrowShared {
                expr: Box::new(TypedIRValue::Variable("x".to_string(), Type::Int)),
                target_type: Type::borrow(Type::Int),
            },
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(
        err.message.contains("reference"),
        "expected reference diagnostic, got: {}",
        err.message
    );
}

#[test]
fn reference_parameter_requires_capability() {
    // A function parameter of reference type requires the
    // capability even when the body contains no reference
    // operation — the caller produces the reference.
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "use_ref".to_string(),
        params: vec![("r".to_string(), Type::borrow(Type::Int))],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![],
            terminator: Some(simple_return()),
        }],
        entry_block: entry,
        is_extern: false,
    });
    let err = check_backend(&program, &BackendCapabilities::interpreter()).unwrap_err();
    assert!(
        err.message.contains("reference"),
        "expected reference diagnostic for &T parameter, got: {}",
        err.message
    );
}

#[test]
fn interpreter_rejects_channels() {
    let program = program_with(
        Instruction::ChannelDecl {
            name: "ch".to_string(),
            type_: Type::channel(Type::Int),
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::interpreter()).unwrap_err();
    assert!(
        err.message.contains("channels"),
        "expected channel diagnostic, got: {}",
        err.message
    );
}

#[test]
fn interpreter_accepts_args() {
    let program = program_with(
        Instruction::Declare {
            name: "xs".to_string(),
            mutable: false,
            type_: Type::list(Type::String),
            value: TypedIRValue::Call {
                function: "args".to_string(),
                args: vec![],
                return_type: Type::list(Type::String),
            },
        },
        simple_return(),
    );
    assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());
}

#[test]
fn llvm_rejects_args() {
    let program = program_with(
        Instruction::Declare {
            name: "xs".to_string(),
            mutable: false,
            type_: Type::list(Type::String),
            value: TypedIRValue::Call {
                function: "args".to_string(),
                args: vec![],
                return_type: Type::list(Type::String),
            },
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
    assert!(
        err.message.contains("command-line"),
        "expected args diagnostic, got: {}",
        err.message
    );
}

#[test]
fn wasm_rejects_args() {
    let program = program_with(
        Instruction::Declare {
            name: "xs".to_string(),
            mutable: false,
            type_: Type::list(Type::String),
            value: TypedIRValue::Call {
                function: "args".to_string(),
                args: vec![],
                return_type: Type::list(Type::String),
            },
        },
        simple_return(),
    );
    let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
    assert!(
        err.message.contains("command-line"),
        "expected args diagnostic, got: {}",
        err.message
    );
}

/// Build verified-semantic IR from source, stopping short of
/// running the backend. Used to feed `check_backend` directly.
fn build_ir(source: &str) -> crate::ir::semantic_ir::SemanticProgram {
    use crate::compiler::assign_expr_ids;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    use crate::ir::instantiation_plan::InstantiationPlan;
    use crate::semantics::analyzer::SemanticAnalyzer;
    use crate::semantics::builder::SemanticIRBuilder;

    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect("parse");
    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &functions,
            &program.traits,
            &program.impls,
            &program.records,
        )
        .expect("analyze");

    let type_table = analyzer.take_type_table_id();
    let instantiations = analyzer.take_instantiations();
    let mut plan = InstantiationPlan::from_instantiations(&instantiations);
    plan.close(&functions);

    let (semantic_program, _diags) = SemanticIRBuilder::build(&functions, type_table, plan);
    semantic_program
}

const RECORD_SOURCE: &str = r#"
rec Point
    x: Int
    y: Int

procedure main
    val p := Point { x: 1, y: 2 }
    print(p.x)
"#;

#[test]
fn interpreter_accepts_records() {
    let program = build_ir(RECORD_SOURCE);
    let result = super::check_backend(&program, &super::BackendCapabilities::interpreter());
    assert!(
        result.is_ok(),
        "interpreter should accept records, got: {:?}",
        result.err()
    );
}

#[test]
fn llvm_rejects_records() {
    let program = build_ir(RECORD_SOURCE);
    let result = super::check_backend(&program, &super::BackendCapabilities::llvm());
    let err = result.expect_err("LLVM should refuse records");
    let msg = format!("{}", err);
    assert!(
        msg.contains("record"),
        "expected diagnostic mentioning records, got: {}",
        msg
    );
}

#[test]
fn wasm_rejects_records() {
    let program = build_ir(RECORD_SOURCE);
    let result = super::check_backend(&program, &super::BackendCapabilities::wasm());
    assert!(result.is_err(), "WASM should refuse records");
}

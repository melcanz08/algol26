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
                value: TypedIRValue::List(vec![TypedIRValue::Int(1)], Type::Int),
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
    assert!(
        verify(&program).is_err(),
        "iterator over Int must be rejected"
    );
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

#[test]
fn some_of_undefined_variable_is_rejected() {
    // `Some(Var("undefined_var", Int))` must fail verification because
    // the inner variable reference is checked. Before the fix, the
    // catch-all arm returned the value's self-claimed type without
    // recursing into the inner value, so the missing variable was
    // silently accepted.
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{
        Instruction, SemanticBlock, SemanticFunction, SemanticProgram, Terminator, TypedIRValue,
    };

    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();

    let func = SemanticFunction {
        name: "f".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![Instruction::Declare {
                name: "wrapped".to_string(),
                mutable: false,
                type_: Type::option(Type::Int),
                value: TypedIRValue::Some(Box::new(TypedIRValue::Variable(
                    "undefined_var".to_string(),
                    Type::Int,
                ))),
            }],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    };
    program.functions.push(func);

    let result = crate::ir::verifier::verify(&program);
    assert!(
        result.is_err(),
        "verifier accepted `Some(Var(undefined_var))`"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("undefined_var"),
        "expected an error mentioning the missing variable, got: {}",
        msg
    );
}

#[test]
fn float_arg_does_not_satisfy_int_param() {
    use crate::common::types::Type;
    assert!(
        !super::value::types_compatible_for_call(&Type::Float, &Type::Int),
        "verifier must reject Float arguments for Int parameters"
    );
    assert!(
        super::value::types_compatible_for_call(&Type::Int, &Type::Float),
        "verifier must accept Int arguments for Float parameters"
    );
    assert!(
        super::value::types_compatible_for_call(&Type::Int, &Type::Int),
        "same-type Int arguments must be accepted"
    );
    assert!(
        super::value::types_compatible_for_call(&Type::Float, &Type::Float),
        "same-type Float arguments must be accepted"
    );
}

#[test]
fn builtin_signatures_match_analyzer_table() {
    // The analyzer (`SemanticAnalyzer::register_builtin_functions`) and
    // the IR verifier (`verifier::builtins::builtin_signatures`) each
    // maintain a built-in signature table. They must agree on names,
    // param counts, param types, and return types. If they diverge,
    // a call the analyzer accepts will be rejected at verify time with
    // "Call to undefined function" or a param-count mismatch — the
    // failure surfaces far from the actual change.
    //
    // This test pins the full expected table. Adding a new built-in
    // requires updating three places: the analyzer's registration, the
    // verifier's `builtin_signatures`, and this test.
    use crate::common::types::Type;

    let sigs = super::builtins::builtin_signatures();

    let expected: Vec<(&str, Vec<Type>, Type)> = vec![
        // Math
        ("Math.sqrt", vec![Type::Float], Type::Float),
        ("Math.pow", vec![Type::Float, Type::Float], Type::Float),
        ("Math.sin", vec![Type::Float], Type::Float),
        ("Math.cos", vec![Type::Float], Type::Float),
        ("Math.abs", vec![Type::Float], Type::Float),
        ("Math.floor", vec![Type::Float], Type::Float),
        ("Math.ceil", vec![Type::Float], Type::Float),
        ("Math.exp", vec![Type::Float], Type::Float),
        ("Math.log", vec![Type::Float], Type::Float),
        ("Math.tan", vec![Type::Float], Type::Float),
        // String
        ("String.length", vec![Type::String], Type::Int),
        (
            "String.concat",
            vec![Type::String, Type::String],
            Type::String,
        ),
        (
            "String.substring",
            vec![Type::String, Type::Int, Type::Int],
            Type::String,
        ),
        ("String.to_upper", vec![Type::String], Type::String),
        ("String.to_lower", vec![Type::String], Type::String),
        // File
        ("File.read", vec![Type::String], Type::String),
        ("File.write", vec![Type::String, Type::String], Type::Int),
        ("File.append", vec![Type::String, Type::String], Type::Int),
        // List
        ("List.length", vec![Type::list(Type::Unknown)], Type::Int),
        ("List.sum", vec![Type::list(Type::Unknown)], Type::Float),
        ("List.max", vec![Type::list(Type::Unknown)], Type::Float),
        ("List.min", vec![Type::list(Type::Unknown)], Type::Float),
        // Raw memory
        ("alloc", vec![Type::Int], Type::pointer(Type::Unknown)),
        ("free", vec![Type::pointer(Type::Unknown)], Type::Void),
    ];

    for (name, expected_params, expected_ret) in &expected {
        let sig = sigs.get(*name).unwrap_or_else(|| {
            panic!(
                "builtin `{}` is in the expected table but missing from \
                 `builtin_signatures()` — add it to `verifier/builtins.rs`",
                name
            )
        });

        assert_eq!(
            sig.params.len(),
            expected_params.len(),
            "builtin `{}`: param count mismatch (expected {}, got {})",
            name,
            expected_params.len(),
            sig.params.len()
        );

        for (i, (expected_ty, (_, actual_ty))) in
            expected_params.iter().zip(&sig.params).enumerate()
        {
            assert_eq!(
                actual_ty, expected_ty,
                "builtin `{}`: param {} type mismatch (expected {:?}, got {:?})",
                name, i, expected_ty, actual_ty
            );
        }

        assert_eq!(
            sig.return_type, *expected_ret,
            "builtin `{}`: return type mismatch (expected {:?}, got {:?})",
            name, expected_ret, sig.return_type
        );
    }

    // Catch the reverse direction: a built-in in the verifier's table
    // that isn't in the expected list — probably a new built-in whose
    // analyzer registration didn't land, or a leftover from a removal.
    for name in sigs.keys() {
        assert!(
            expected.iter().any(|(n, _, _)| *n == name.as_str()),
            "verifier's signature table has `{}` but the expected list \
             does not — update this test if the built-in is intentional",
            name
        );
    }
}

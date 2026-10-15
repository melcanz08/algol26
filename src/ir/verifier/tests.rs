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
            Instruction::ReceiveChannel {
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
            Instruction::SendChannel {
                channel: "ch".to_string(),
                value: TypedIRValue::Int(42),
            },
            Instruction::ReceiveChannel {
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
fn verifier_builtin_table_matches_canonical() {
    // The verifier's built-in signature table is now derived from
    // `crate::builtins::builtins()`. This test pins that the
    // derivation is correct: every entry in the canonical table
    // appears in the verifier's table with matching param types,
    // param names, and return type, and there are no extras.
    //
    // If someone changes the shape of `FunctionSignature` or the
    // conversion in `verifier/builtins.rs`, this test fails with a
    // specific builtin name and field.
    let canonical = crate::builtins::builtins();
    let verifier = super::builtins::builtin_signatures();

    for sig in &canonical {
        let v = verifier.get(sig.name).unwrap_or_else(|| {
            panic!(
                "builtin `{}` in crate::builtins but not in verifier table — \
                 update `verifier::builtins::builtin_signatures`",
                sig.name
            )
        });

        assert_eq!(
            v.params.len(),
            sig.params.len(),
            "builtin `{}`: param count mismatch (canonical {}, verifier {})",
            sig.name,
            sig.params.len(),
            v.params.len()
        );

        for (i, ((canonical_name, canonical_ty), (verifier_name, verifier_ty))) in
            sig.params.iter().zip(&v.params).enumerate()
        {
            assert_eq!(
                verifier_name, canonical_name,
                "builtin `{}`: param {} name mismatch (canonical {}, verifier {})",
                sig.name, i, canonical_name, verifier_name
            );
            assert_eq!(
                verifier_ty, canonical_ty,
                "builtin `{}`: param {} type mismatch (canonical {:?}, verifier {:?})",
                sig.name, i, canonical_ty, verifier_ty
            );
        }

        assert_eq!(
            v.return_type, sig.return_type,
            "builtin `{}`: return type mismatch (canonical {:?}, verifier {:?})",
            sig.name, sig.return_type, v.return_type
        );
    }

    // Reverse direction: any name in the verifier's table that isn't
    // in the canonical table is stale. This catches a leftover
    // entry from a builtin that was removed.
    for name in verifier.keys() {
        assert!(
            canonical.iter().any(|s| s.name == name),
            "verifier table has `{}` but crate::builtins does not — \
             remove it from `verifier::builtins::builtin_signatures`",
            name
        );
    }
}

#[test]
fn addrof_on_non_place_is_rejected() {
    // Construct a program where AddrOf wraps an arithmetic
    // expression. The verifier must reject it — the operand has
    // no stable storage.
    use crate::common::types::Type;
    use crate::ir::semantic_ir::{
        Instruction, SemanticBinOp, SemanticBlock, SemanticFunction, SemanticProgram, Terminator,
        TypedIRValue,
    };

    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let func = SemanticFunction {
        name: "f".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![Instruction::Print {
                value: TypedIRValue::AddrOf {
                    expr: Box::new(TypedIRValue::BinaryOp {
                        op: SemanticBinOp::Add,
                        left: Box::new(TypedIRValue::Int(1)),
                        right: Box::new(TypedIRValue::Int(2)),
                        result_type: Type::Int,
                    }),
                    target_type: Type::pointer(Type::Int),
                },
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
        "AddrOf on a non-place should be rejected, got Ok"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("AddrOf requires a place"),
        "expected 'requires a place' error, got: {}",
        msg
    );
}

#[test]
fn addrof_on_variable_is_accepted() {
    // Positive case: AddrOf(Variable) is a valid place.
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
            instructions: vec![
                Instruction::Declare {
                    name: "x".to_string(),
                    mutable: true,
                    type_: Type::Int,
                    value: TypedIRValue::Int(5),
                },
                Instruction::Print {
                    value: TypedIRValue::AddrOf {
                        expr: Box::new(TypedIRValue::Variable("x".to_string(), Type::Int)),
                        target_type: Type::pointer(Type::Int),
                    },
                },
            ],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    };
    program.functions.push(func);

    assert!(crate::ir::verifier::verify(&program).is_ok());
}

// ─── Worklist verifier: join-point env handling ──────────────────

fn branch_program(declare_x_in_then: bool) -> SemanticProgram {
    // Shared skeleton for the two join tests. When
    // `declare_x_in_then` is true, the `then` branch declares
    // `x: Int`; the `else` branch always jumps straight to the
    // join. The join block reads `x` in both cases — which is
    // legal only when `x` dominates the branch, which it does not
    // in either variant.
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let then_bb = program.new_block_id();
    let else_bb = program.new_block_id();
    let join_bb = program.new_block_id();

    let then_instrs = if declare_x_in_then {
        vec![Instruction::Declare {
            name: "x".to_string(),
            mutable: false,
            type_: Type::Int,
            value: TypedIRValue::Int(1),
        }]
    } else {
        vec![]
    };

    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![],
                terminator: Some(Terminator::Branch {
                    condition: TypedIRValue::Bool(true),
                    then_block: then_bb,
                    else_block: else_bb,
                }),
            },
            SemanticBlock {
                id: then_bb,
                instructions: then_instrs,
                terminator: Some(Terminator::Jump { block: join_bb }),
            },
            SemanticBlock {
                id: else_bb,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: join_bb }),
            },
            SemanticBlock {
                id: join_bb,
                instructions: vec![Instruction::Print {
                    value: TypedIRValue::Variable("x".to_string(), Type::Int),
                }],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            },
        ],
        entry_block: entry,
        is_extern: false,
    });

    program
}

#[test]
fn verifier_rejects_use_of_branch_local_variable() {
    // `x` is declared only in the `then` branch; the join block
    // reads it. Under the old DFS-first-visited-wins verifier, if
    // `then` happened to be walked first, the join would see `x`
    // in scope and pass — a false negative that let a
    // not-actually-in-scope name through. The worklist verifier
    // joins predecessor envs, drops `x` (present in only one side),
    // and rejects the join block's read.
    let program = branch_program(true);
    assert!(
        verify(&program).is_err(),
        "verifier should reject use of `x` after a branch that may not declare it"
    );
}

#[test]
fn verifier_accepts_use_of_dominating_variable_at_join() {
    // Same shape, but `x` is declared in the *entry* block (dominates
    // both branches). Both predecessors carry it in their envs, so
    // the join keeps it, and the join block's read is fine.
    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    let then_bb = program.new_block_id();
    let else_bb = program.new_block_id();
    let join_bb = program.new_block_id();

    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![
            SemanticBlock {
                id: entry,
                instructions: vec![Instruction::Declare {
                    name: "x".to_string(),
                    mutable: false,
                    type_: Type::Int,
                    value: TypedIRValue::Int(1),
                }],
                terminator: Some(Terminator::Branch {
                    condition: TypedIRValue::Bool(true),
                    then_block: then_bb,
                    else_block: else_bb,
                }),
            },
            SemanticBlock {
                id: then_bb,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: join_bb }),
            },
            SemanticBlock {
                id: else_bb,
                instructions: vec![],
                terminator: Some(Terminator::Jump { block: join_bb }),
            },
            SemanticBlock {
                id: join_bb,
                instructions: vec![Instruction::Print {
                    value: TypedIRValue::Variable("x".to_string(), Type::Int),
                }],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            },
        ],
        entry_block: entry,
        is_extern: false,
    });

    assert!(
        verify(&program).is_ok(),
        "dominating-variable join should verify; got: {:?}",
        verify(&program)
    );
}

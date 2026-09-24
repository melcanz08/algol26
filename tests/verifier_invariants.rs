// ADR 0014 — executable-IR invariant tests.
//
// Only the TypeVar-leakage invariant is tested here. Call-target
// resolution is `SemanticProgram::verify`'s responsibility and has
// its own tests in `tests/ir_tests.rs`.

use algol26::common::types::Type;
use algol26::ir::instantiation_plan::InstantiationPlan;
use algol26::ir::semantic_ir::{
    SemanticBlock, SemanticFunction, SemanticInstruction, SemanticProgram, Terminator, TypedIRValue,
};
use algol26::ir::verifier::invariants::{check_invariants, InvariantError};

fn trivial_function(name: &str) -> SemanticFunction {
    SemanticFunction {
        name: name.to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: 0,
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: 0,
        is_extern: false,
    }
}

fn program_with(functions: Vec<SemanticFunction>) -> SemanticProgram {
    let mut p = SemanticProgram::new();
    p.functions = functions;
    p
}

#[test]
fn type_var_in_parameter_is_rejected() {
    let mut f = trivial_function("identity");
    f.params = vec![("x".to_string(), Type::TypeVar("T".to_string()))];

    let program = program_with(vec![f]);
    let plan = InstantiationPlan::default();

    let err = check_invariants(&program, &plan).unwrap_err();
    assert!(
        err.iter()
            .any(|e| matches!(e, InvariantError::TypeVarInExecutableIr { .. })),
        "expected TypeVarInExecutableIr, got {:?}",
        err,
    );
}

#[test]
fn type_var_in_return_type_is_rejected() {
    let mut f = trivial_function("identity");
    f.return_type = Type::TypeVar("T".to_string());

    let program = program_with(vec![f]);
    let plan = InstantiationPlan::default();

    let err = check_invariants(&program, &plan).unwrap_err();
    assert!(err
        .iter()
        .any(|e| matches!(e, InvariantError::TypeVarInExecutableIr { .. })));
}

#[test]
fn type_var_in_declare_value_is_rejected() {
    let mut f = trivial_function("main");
    f.blocks[0].instructions.push(SemanticInstruction::Declare {
        name: "x".to_string(),
        mutable: false,
        type_: Type::Int,
        value: TypedIRValue::Variable("y".to_string(), Type::TypeVar("T".to_string())),
    });

    let program = program_with(vec![f]);
    let plan = InstantiationPlan::default();

    let err = check_invariants(&program, &plan).unwrap_err();
    assert!(err
        .iter()
        .any(|e| matches!(e, InvariantError::TypeVarInExecutableIr { .. })));
}

#[test]
fn type_var_in_nested_list_is_rejected() {
    let mut f = trivial_function("main");
    f.blocks[0].instructions.push(SemanticInstruction::Declare {
        name: "x".to_string(),
        mutable: false,
        type_: Type::Int,
        value: TypedIRValue::List(vec![], Type::TypeVar("T".to_string())),
    });

    let program = program_with(vec![f]);
    let plan = InstantiationPlan::default();

    let err = check_invariants(&program, &plan).unwrap_err();
    assert!(err
        .iter()
        .any(|e| matches!(e, InvariantError::TypeVarInExecutableIr { .. })));
}

#[test]
fn well_formed_program_passes() {
    let mut main_fn = trivial_function("main");
    main_fn.blocks[0]
        .instructions
        .push(SemanticInstruction::Call {
            func: "helper".to_string(),
            args: vec![],
            result: None,
        });

    let program = program_with(vec![main_fn, trivial_function("helper")]);
    let plan = InstantiationPlan::default();

    check_invariants(&program, &plan).expect("well-formed program should pass");
}

#[test]
fn generic_specialization_passes() {
    let mut main_fn = trivial_function("main");
    main_fn.blocks[0]
        .instructions
        .push(SemanticInstruction::Call {
            func: "identity_Int".to_string(),
            args: vec![],
            result: None,
        });

    let program = program_with(vec![main_fn, trivial_function("identity_Int")]);
    let plan = InstantiationPlan::default();

    check_invariants(&program, &plan).expect("specialized program should pass");
}

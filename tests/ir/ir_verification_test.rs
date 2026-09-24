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

#[test]
fn generic_call_emits_specialized_function_and_rewrites_callee() {
    use algol26::common::types::Type;
    use algol26::compiler::Compiler;

    let source = r#"
function identity<T>(x: T) -> T
    return x

procedure main
    val v := 1.0
    val p := &v
    val q := identity(p)
    print(q)
"#;

    let mut compiler = Compiler::new();
    let ir = compiler
        .build_semantic_ir_for(source, "generic_rewrite.gol")
        .expect("generic program failed to lower");

    // Exactly one specialization exists, and its name is mangled.
    let names: Vec<&str> = ir.functions.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.contains(&"identity_Borrow_Float"),
        "expected specialized function; got {:?}",
        names,
    );
    assert!(
        !names.contains(&"identity"),
        "generic template leaked into executable IR: {:?}",
        names,
    );

    // The call site references the mangled name.
    let main = ir.functions.iter().find(|f| f.name == "main").unwrap();
    let calls: Vec<&str> = main
        .blocks
        .iter()
        .flat_map(|b| b.instructions.iter())
        .filter_map(|i| match i {
            algol26::ir::semantic_ir::SemanticInstruction::Call { func, .. } => Some(func.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        calls.contains(&"identity_Borrow_Float"),
        "main did not call the specialized name; calls = {:?}",
        calls,
    );
    assert!(
        !calls.contains(&"identity"),
        "main called the template name; calls = {:?}",
        calls,
    );

    // The specialized function's signature is concrete.
    let spec = ir
        .functions
        .iter()
        .find(|f| f.name == "identity_Borrow_Float")
        .unwrap();
    assert_eq!(spec.params[0].1, Type::borrow(Type::Float));
    assert_eq!(spec.return_type, Type::borrow(Type::Float));
}

#[test]
fn transitive_generic_call_closure_emits_both_specializations() {
    use algol26::compiler::Compiler;

    let source = r#"
function outer<T>(x: T) -> T
    return inner(x)

function inner<T>(x: T) -> T
    return x

procedure main
    val q := outer(42)
    print(q)
"#;

    let mut compiler = Compiler::new();
    let ir = compiler
        .build_semantic_ir_for(source, "closure.gol")
        .expect("transitive generic program failed to lower");

    let names: Vec<&str> = ir.functions.iter().map(|f| f.name.as_str()).collect();

    // Both specializations must be emitted. `outer_Int` comes from
    // the concrete call in main; `inner_Int` only exists because
    // close() walked outer's body under T -> Int.
    assert!(names.contains(&"outer_Int"), "names: {:?}", names);
    assert!(names.contains(&"inner_Int"), "names: {:?}", names);

    // Neither template reaches executable IR.
    assert!(!names.contains(&"outer"), "template leaked: {:?}", names);
    assert!(!names.contains(&"inner"), "template leaked: {:?}", names);
}

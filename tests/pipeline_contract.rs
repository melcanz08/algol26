use algol26::ir::cfg::{Cfg, CfgBlock, CfgInstruction, DataflowEngine, OwnershipTransfer};
use algol26::semantics::state::SemanticState;

#[test]
fn dataflow_engine_wired() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock {
        id: 0,
        instructions: vec![
            CfgInstruction::Declare { name: "x".into() },
            CfgInstruction::Assign { name: "x".into() },
        ],
    });
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(
        !result.has_errors(),
        "valid program should pass dataflow: {:?}",
        result.diagnostics
    );
    assert!(result.visited.contains(&0));
}

#[test]
fn dataflow_enforces_move_semantics() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock {
        id: 0,
        instructions: vec![
            CfgInstruction::Declare { name: "x".into() },
            CfgInstruction::Move { name: "x".into() },
            CfgInstruction::Use { name: "x".into() },
        ],
    });
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(result.has_errors(), "use after move must be detected");
}

#[test]
fn dataflow_enforces_region_outlives() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock {
        id: 0,
        instructions: vec![
            CfgInstruction::RegionEnter {
                name: "outer".into(),
            },
            CfgInstruction::Declare { name: "r".into() },
            CfgInstruction::RegionEnter {
                name: "inner".into(),
            },
            CfgInstruction::Declare { name: "x".into() },
            CfgInstruction::Borrow {
                borrower: "r".into(),
                place: "x".into(),
                mutable: false,
            },
            CfgInstruction::RegionExit {
                name: "inner".into(),
            },
        ],
    });
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(
        result.has_errors(),
        "borrow outliving region must be rejected: {:?}",
        result.diagnostics
    );
}

#[test]
fn unsupported_ir_must_be_compiler_error() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock {
        id: 0,
        instructions: vec![CfgInstruction::Unsupported { op: "Deref".into() }],
    });
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(
        result.has_errors(),
        "unsupported IR must be compiler error, not silent fallback"
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("E-UNSUPPORTED-001")),
        "must have E-UNSUPPORTED-001, got {:?}",
        result.diagnostics
    );
}

#[test]
fn transform_preserves_verification() {
    use algol26::common::types::Type;
    use algol26::ir::optimizer::Optimizer;
    use algol26::ir::semantic_ir::{
        SemanticBlock, SemanticFunction, SemanticProgram, Terminator,
    };
    use algol26::ir::verified_ir::VerifiedIR;

    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    });

    let verified = VerifiedIR::new(program).expect("empty program verifies");

    let mut optimizer = Optimizer::new();
    let optimized = verified
        .mutate(|program| optimizer.optimize(program))
        .expect("optimizer preserves verification");

    assert!(
        optimized.verify().is_ok(),
        "optimizer output must pass the verifier"
    );
}

#[test]
fn mutate_rejects_invalid_ir() {
    use algol26::common::types::Type;
    use algol26::ir::semantic_ir::{
        SemanticBlock, SemanticFunction, SemanticProgram, Terminator,
    };
    use algol26::ir::verified_ir::VerifiedIR;

    let mut program = SemanticProgram::new();
    let entry = program.new_block_id();
    program.functions.push(SemanticFunction {
        name: "main".to_string(),
        params: vec![],
        return_type: Type::Void,
        blocks: vec![SemanticBlock {
            id: entry,
            instructions: vec![],
            terminator: Some(Terminator::Return {
                value: None,
                type_: Type::Void,
            }),
        }],
        entry_block: entry,
        is_extern: false,
    });

    let verified = VerifiedIR::new(program).expect("program verifies");

    // Break the IR inside the mutation: append a block with no
    // terminator. The verifier should reject this.
    let result = verified.mutate(|program| {
        let bad = program.new_block_id();
        program.functions[0].blocks.push(SemanticBlock {
            id: bad,
            instructions: vec![],
            terminator: None,
        });
    });

    assert!(
        result.is_err(),
        "mutate must reject IR that fails verification"
    );
}
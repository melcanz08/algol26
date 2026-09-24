use algol26::ir::cfg::{Cfg, CfgBlock, CfgInstruction, DataflowEngine, OwnershipTransfer};

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
    let result = engine.run(&cfg);
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
    let result = engine.run(&cfg);
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
    let result = engine.run(&cfg);
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
    let result = engine.run(&cfg);
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
    use algol26::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram, Terminator};
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
    use algol26::ir::semantic_ir::{SemanticBlock, SemanticFunction, SemanticProgram, Terminator};
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

// ─── ADR 0016: per-function CFG ─────────────────────────────────────
//
// These tests exercise the pipeline path (source → analyzer → IR →
// build_cfgs_from_semantic_program → run_all), unlike the CFG-shape
// tests above which construct CFGs directly. They verify that every
// non-extern function is analyzed, that parameters have entry state,
// and that the worklist's revisit behavior does not duplicate
// diagnostics.

#[test]
fn every_non_extern_function_has_a_cfg_and_is_visited() {
    use algol26::compiler::Compiler;
    use algol26::ir::cfg::{build_cfgs_from_semantic_program, DataflowEngine, OwnershipTransfer};

    let source = r#"
procedure helper(x: Int)
    print(x)

procedure main
    helper(1)
"#;

    let mut compiler = Compiler::new();
    let sem = compiler
        .build_semantic_ir_for(source, "test.gol")
        .expect("source failed to build");
    let cfgs = build_cfgs_from_semantic_program(&sem);

    let names: Vec<&str> = cfgs.iter().map(|f| f.name.as_str()).collect();
    assert!(names.contains(&"helper"), "cfgs: {:?}", names);
    assert!(names.contains(&"main"), "cfgs: {:?}", names);

    let total_blocks: usize = cfgs.iter().map(|f| f.cfg.blocks.len()).sum();
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run_all(&cfgs);

    // Before ADR 0016, the flattened CFG started at block 0 and only
    // reached main's blocks. After, every block in every function is
    // visited.
    assert_eq!(
        result.visited.len(),
        total_blocks,
        "expected every block visited; total={} visited={:?}",
        total_blocks,
        result.visited,
    );
}

#[test]
fn parameter_is_initialized_at_entry() {
    use algol26::compiler::Compiler;
    use algol26::ir::cfg::{build_cfgs_from_semantic_program, DataflowEngine, OwnershipTransfer};

    let source = r#"
procedure takes_arg(x: Int)
    print(x)

procedure main
    takes_arg(1)
"#;

    let mut compiler = Compiler::new();
    let sem = compiler
        .build_semantic_ir_for(source, "test.gol")
        .expect("source failed to build");
    let cfgs = build_cfgs_from_semantic_program(&sem);
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run_all(&cfgs);

    let init_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("E-INIT"))
        .collect();
    assert!(
        init_diags.is_empty(),
        "parameter use should not trigger E-INIT: {:?}",
        init_diags,
    );
}

#[test]
fn run_all_deduplicates_diagnostics() {
    use algol26::ir::cfg::{
        Cfg, CfgBlock, CfgInstruction, DataflowEngine, FunctionCfg, OwnershipTransfer,
    };

    // Block 1 contains Move(x) then Use(x), producing a
    // use-of-moved diagnostic. Block 2 has a back edge to block 1,
    // so the worklist revisits block 1 once its incoming state
    // changes. Without dedup, the diagnostic appears twice.
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock {
        id: 0,
        instructions: vec![CfgInstruction::Declare { name: "x".into() }],
    });
    cfg.add_block(CfgBlock {
        id: 1,
        instructions: vec![
            CfgInstruction::Move { name: "x".into() },
            CfgInstruction::Use { name: "x".into() },
        ],
    });
    cfg.add_block(CfgBlock {
        id: 2,
        instructions: vec![],
    });
    cfg.add_edge(0, 1);
    cfg.add_edge(1, 2);
    cfg.add_edge(2, 1);

    let func = FunctionCfg {
        name: "loop_test".into(),
        cfg,
        params: vec![],
    };
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run_all(&[func]);

    let move_diags: Vec<_> = result
        .diagnostics
        .iter()
        .filter(|d| d.message.contains("E-MOVE-001"))
        .collect();
    assert_eq!(
        move_diags.len(),
        1,
        "expected exactly one E-MOVE-001 after dedup, got {}: {:?}",
        move_diags.len(),
        move_diags,
    );
}

#[test]
fn well_formed_multi_function_program_passes() {
    use algol26::compiler::Compiler;
    use algol26::ir::cfg::{build_cfgs_from_semantic_program, DataflowEngine, OwnershipTransfer};

    let source = r#"
function helper(x: Int) -> Int
    return x + 1

procedure main
    val y := helper(1)
    print(y)
"#;

    let mut compiler = Compiler::new();
    let sem = compiler
        .build_semantic_ir_for(source, "test.gol")
        .expect("source failed to build");
    let cfgs = build_cfgs_from_semantic_program(&sem);
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run_all(&cfgs);

    assert!(
        !result.has_errors(),
        "well-formed program produced dataflow errors: {:?}",
        result.diagnostics,
    );
}

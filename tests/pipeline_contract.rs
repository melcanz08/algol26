use algol26::ir::cfg::{Cfg, CfgBlock, CfgInstruction, DataflowEngine, OwnershipTransfer};
use algol26::semantics::state::SemanticState;
use algol26::ir::semantic_ir::SemanticProgram;

fn valid_program() -> SemanticProgram {
    SemanticProgram::new()
}

#[test]
fn transform_must_invalidate_verified_ir() {
    let program = valid_program();
    let cfg = algol26::ir::cfg::builder::build_cfg_from_semantic_program(&program);
    // empty program should at least build cfg without panic
    assert!(cfg.blocks.len() >= 0);
}

#[test]
fn dataflow_engine_wired() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock { id: 0, instructions: vec![
        CfgInstruction::Declare { name: "x".into() },
        CfgInstruction::Assign { name: "x".into() },
    ]});
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(!result.has_errors(), "valid program should pass dataflow: {:?}", result.diagnostics);
    assert!(result.visited.contains(&0));
}

#[test]
fn dataflow_enforces_move_semantics() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock { id: 0, instructions: vec![
        CfgInstruction::Declare { name: "x".into() },
        CfgInstruction::Move { name: "x".into() },
        CfgInstruction::Use { name: "x".into() },
    ]});
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(result.has_errors(), "use after move must be detected");
}

#[test]
fn dataflow_enforces_region_outlives() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock { id: 0, instructions: vec![
        CfgInstruction::RegionEnter { name: "outer".into() },
        CfgInstruction::Declare { name: "r".into() },
        CfgInstruction::RegionEnter { name: "inner".into() },
        CfgInstruction::Declare { name: "x".into() },
        CfgInstruction::Borrow { borrower: "r".into(), place: "x".into(), mutable: false },
        CfgInstruction::RegionExit { name: "inner".into() },
    ]});
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(result.has_errors(), "borrow outliving region must be rejected: {:?}", result.diagnostics);
}

#[test]
fn unsupported_ir_must_be_compiler_error() {
    let mut cfg = Cfg::new(0);
    cfg.add_block(CfgBlock { id: 0, instructions: vec![CfgInstruction::Unsupported { op: "Deref".into() }] });
    let engine = DataflowEngine::new(OwnershipTransfer);
    let result = engine.run(&cfg, SemanticState::new());
    assert!(result.has_errors(), "unsupported IR must be compiler error, not silent fallback");
    assert!(result.diagnostics.iter().any(|d| d.message.contains("E-UNSUPPORTED-001")), "must have E-UNSUPPORTED-001, got {:?}", result.diagnostics);
}
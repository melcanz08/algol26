// tests/compiler_pipeline_equiv.rs

//! Equivalence oracle: `VerifyIrPass` must agree with the direct
//! `SemanticProgram::verify()` call on every conformance input.
//!
//! `compile()` now routes through the pass; the direct call is kept
//! here as an independent oracle. If this test ever fails, the pass
//! is not a faithful adapter and something in the pipeline is wrong.
use algol26::compiler::context::{CompilerConfig, CompilerContext};
use algol26::compiler::pass::{Pass, PassKind};
use algol26::compiler::passes::verify_ir::VerifyIrPass;
use algol26::compiler::program::Program;
use algol26::compiler::Compiler;
use algol26::compiler::passes::optimize::OptimizePass;
use algol26::compiler::passes::build_ir::BuildSemanticIRPass;
use algol26::compiler::program::AstPayload;
use algol26::ir::semantic_ir::SemanticProgram;
use algol26::ir::optimizer::Optimizer;

#[test]
fn verify_pass_agrees_with_direct_call_on_conformance_valid() {
    let dir = std::path::Path::new("tests/conformance/valid");
    let mut checked = 0usize;
    let mut agreed_ok = 0usize;
    let mut agreed_err = 0usize;

    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gol") {
            continue;
        }

        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        let mut compiler = Compiler::default();
        let sem = match compiler.build_semantic_ir_for(&source, &filename) {
            Ok(s) => s,
            // Frontend rejections are not what this test is about; the
            // verifier never runs on them in `compile()` either.
            Err(_) => continue,
        };

        // Path A: the old way.
        let old = sem.verify();

        // Path B: through the pass.
        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new(&source, &filename);
        program.semantic_ir = Some(sem);
        let pass = VerifyIrPass;
        assert_eq!(pass.contract().kind, PassKind::Verification);
        let new = pass.run(&mut ctx, &mut program);

        assert_eq!(
            old.is_ok(),
            new.is_ok(),
            "verify disagreement on {}: old={:?} new={:?}",
            filename,
            old.as_ref().err(),
            new.as_ref().err().map(|e| &e.message),
        );

        if old.is_ok() {
            agreed_ok += 1;
            assert_eq!(ctx.error_count(), 0, "spurious error on {}", filename);
        } else {
            agreed_err += 1;
            assert_eq!(ctx.error_count(), 1, "wrong error count on {}", filename);
        }
        checked += 1;
    }

    assert!(checked > 0, "no conformance files were exercised");
    eprintln!(
        "verify equivalence: {} files checked, {} ok, {} err",
        checked, agreed_ok, agreed_err
    );
}

#[test]
fn optimize_pass_produces_identical_ir_to_direct_call() {
    use algol26::compiler::pipeline::Pipeline;
    use algol26::compiler::scheduler::Scheduler;

    let dir = std::path::Path::new("tests/conformance/valid");
    let mut checked = 0usize;

    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gol") {
            continue;
        }

        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        // Build IR twice. `build_semantic_ir_for` is deterministic
        // (see backends_tests::test_semantic_ir_is_deterministic),
        // so the two inputs are byte-identical.
        let mut compiler = Compiler::default();
        let mut ir_a = match compiler.build_semantic_ir_for(&source, &filename) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut compiler = Compiler::default();
        let ir_b = compiler.build_semantic_ir_for(&source, &filename).unwrap();

        // Path A: direct.
        let mut opt = Optimizer::new();
        opt.optimize(&mut ir_a);

        // Path B: through the pass.
        let pipeline = Pipeline::builder()
            .add(OptimizePass)
            .add(VerifyIrPass)
            .build()
            .unwrap();
        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.semantic_ir = Some(ir_b);
        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);
        assert!(
            outcome.succeeded(),
            "optimize pass failed on {}: {:?}",
            filename,
            outcome.failure
        );
        let ir_b = program.semantic_ir.take().unwrap();

        assert_eq!(
            format!("{:?}", ir_a),
            format!("{:?}", ir_b),
            "optimizer output diverges on {}",
            filename
        );

        checked += 1;
    }

    assert!(checked > 0, "no files exercised");
    eprintln!("optimize equivalence: {} files checked", checked);
}

#[test]
fn build_ir_pass_produces_identical_ir_to_direct_call() {
    use algol26::compiler::pipeline::Pipeline;
    use algol26::compiler::scheduler::Scheduler;

    let dir = std::path::Path::new("tests/conformance/valid");
    let mut checked = 0usize;

    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gol") {
            continue;
        }

        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        // Build the typed AST once via the existing helper — this is
        // the same input both paths consume.
        let mut compiler = Compiler::default();
        let typed = match compiler.type_check_source_for(&source, &filename) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Path A: direct call to the extracted free function.
        let direct = match algol26::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table.clone(),
        ) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Path B: through the pass.
        let pipeline = Pipeline::builder()
            .add(BuildSemanticIRPass)
            .build()
            .unwrap();
        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.ast = Some(AstPayload {
            functions: typed.functions.clone(),
            type_table: typed.type_table.clone(),
        });
        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);
        assert!(
            outcome.succeeded(),
            "build_ir pass failed on {}: {:?}",
            filename,
            outcome.failure
        );
        let via_pass: SemanticProgram = program.semantic_ir.take().unwrap();

        assert_eq!(
            format!("{:?}", direct),
            format!("{:?}", via_pass),
            "IR diverges on {}",
            filename
        );
        checked += 1;
    }

    assert!(checked > 0, "no files exercised");
    eprintln!("build_ir equivalence: {} files checked", checked);
}
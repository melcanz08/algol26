// tests/compiler_pipeline_equiv.rs

//! Equivalence oracle: `VerifyIrPass` must agree with the direct
//! `SemanticProgram::verify()` call on every conformance input.
//!
//! `compile()` now routes through the pass; the direct call is kept
//! here as an independent oracle. If this test ever fails, the pass
//! is not a faithful adapter and something in the pipeline is wrong.

use algol26::compiler::context::{CompilerConfig, CompilerContext};
use algol26::compiler::pass::{Pass, PassKind};
use algol26::compiler::passes::build_ir::BuildSemanticIRPass;
use algol26::compiler::passes::optimize::OptimizePass;
use algol26::compiler::passes::verify_ir::{ReVerifyPass, VerifyIrPass};
use algol26::compiler::program::{IrState, Program};
use algol26::compiler::Compiler;
use algol26::ir::optimizer::Optimizer;
use algol26::ir::semantic_ir::SemanticProgram;
use std::path::{Path, PathBuf};

/// Collect every `.gol` file under `dir`, recursing one level.
///
/// Fixtures are grouped by feature (Tier 4.1 restructure), so `dir`
/// contains subdirectories like `arithmetic/`, `strings/`, `defer/`,
/// each holding `.gol` files. Flat files directly under `dir` are
/// also accepted for backwards compatibility.
fn conformance_gol_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Ok(sub_entries) = std::fs::read_dir(&path) {
                for sub in sub_entries.flatten() {
                    let sub_path = sub.path();
                    if sub_path.extension().and_then(|s| s.to_str()) == Some("gol") {
                        out.push(sub_path);
                    }
                }
            }
        } else if path.extension().and_then(|s| s.to_str()) == Some("gol") {
            out.push(path);
        }
    }
    out.sort();
    out
}

#[test]
fn verify_pass_agrees_with_direct_call_on_conformance_valid() {
    let dir = std::path::Path::new("tests/conformance/valid");
    let mut checked = 0usize;
    let mut agreed_ok = 0usize;
    let mut agreed_err = 0usize;

    for path in conformance_gol_files(dir) {
        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        let mut compiler = Compiler;
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
        program.ir = IrState::Built(sem);
        let pass = VerifyIrPass;
        assert_eq!(pass.contract().kind, PassKind::Lowering);
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

    for path in conformance_gol_files(dir) {
        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        // Build IR twice. `build_semantic_ir_for` is deterministic
        // (see backends_tests::test_semantic_ir_is_deterministic),
        // so the two inputs are byte-identical.
        let mut compiler = Compiler;
        let mut ir_a = match compiler.build_semantic_ir_for(&source, &filename) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut compiler = Compiler;
        let ir_b = compiler.build_semantic_ir_for(&source, &filename).unwrap();

        // Path A: direct.
        let mut opt = Optimizer::new();
        opt.optimize(&mut ir_a);

        // Path B: through the pass.
        let pipeline = Pipeline::builder()
            .add(VerifyIrPass)
            .add(OptimizePass)
            .add(ReVerifyPass)
            .build()
            .unwrap();
        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.ir = IrState::Built(ir_b);
        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);
        assert!(
            outcome.succeeded(),
            "optimize pass failed on {}: {:?}",
            filename,
            outcome.failure
        );
        let ir_b = match program.ir {
            IrState::Verified(v) => v.program().clone(),
            _ => panic!("optimize pipeline did not produce verified IR"),
        };
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

    for path in conformance_gol_files(dir) {
        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        // Build the typed AST once via the existing helper — this is
        // the same input both paths consume.
        let mut compiler = Compiler;
        let typed = match compiler.type_check_source_for(&source, &filename) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Path A: direct call to the extracted free function.
        let direct = match algol26::compiler::build_semantic_ir_program(
            &typed.functions,
            typed.type_table_id.clone(),
            typed.plan.clone(),
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
        program.typed = Some(typed.clone());
        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);
        assert!(
            outcome.succeeded(),
            "build_ir pass failed on {}: {:?}",
            filename,
            outcome.failure
        );
        let via_pass: SemanticProgram = match program.ir {
            IrState::Built(p) => p,
            _ => panic!("build_ir pass did not produce unverified IR"),
        };

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

/// Canonical string form of a `TypedProgram` for comparison.
///
/// `TypedProgram::type_table_id` is a `HashMap`, whose `Debug` output
/// depends on internal iteration order. Sort entries by key so two
/// tables with the same contents compare equal.
fn canonical_typed(typed: &algol26::compiler::TypedProgram) -> String {
    let mut entries: Vec<(algol26::frontend::ast::ExprId, String)> = typed
        .type_table_id
        .iter()
        .map(|(k, v)| (*k, format!("{:?}", v)))
        .collect();
    entries.sort();
    format!(
        "functions={:?}\ntype_info={:?}\ntype_table={:?}",
        typed.functions, typed.type_info, entries
    )
}

#[test]
fn type_check_pass_agrees_with_direct_call() {
    use algol26::compiler::pass::{Pass, PassKind};
    use algol26::compiler::passes::type_check::TypeCheckPass;
    use algol26::compiler::program::AstPayload;
    use std::rc::Rc;

    let dir = std::path::Path::new("tests/conformance/valid");
    let mut checked = 0usize;

    for path in conformance_gol_files(dir) {
        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        // Run the frontend *once*. Both paths below consume this same
        // `parsed` value.
        let mut compiler = Compiler;
        let parsed = match compiler.parse_source_for(&source, &filename) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Path A: the free function directly.
        let direct = match algol26::compiler::type_check_program(
            &parsed.functions,
            &parsed.traits,
            &parsed.impls,
            &parsed.records,
        ) {
            Ok(t) => t,
            Err(_) => continue,
        };

        // Path B: through the pass, same input.
        let pass = TypeCheckPass;
        assert_eq!(pass.contract().kind, PassKind::Annotation);

        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new(&source, &filename);
        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            records: parsed.records.clone(),
        });
        pass.run(&mut ctx, &mut program)
            .unwrap_or_else(|e| panic!("type_check pass failed on {}: {:?}", filename, e));
        let via_pass = program.typed.take().unwrap();

        assert_eq!(
            canonical_typed(&direct),
            canonical_typed(&via_pass),
            "typed AST diverges on {}",
            filename
        );

        checked += 1;
    }

    assert!(checked > 0, "no files exercised");
    eprintln!("type_check equivalence: {} files checked", checked);
}

#[test]
fn type_table_complete_passes_on_conformance_suite() {
    let dir = std::path::Path::new("tests/conformance/valid");
    let mut checked = 0usize;

    for path in conformance_gol_files(dir) {
        let source = std::fs::read_to_string(&path).unwrap();
        let filename = path.file_name().unwrap().to_string_lossy().to_string();

        let mut compiler = Compiler;
        let typed = match compiler.type_check_source_for(&source, &filename) {
            Ok(t) => t,
            Err(_) => continue,
        };
        compiler
            .run_type_table_complete_pass_public(typed)
            .unwrap_or_else(|e| panic!("type_table incomplete on {}: {:?}", filename, e));
        checked += 1;
    }

    assert!(checked > 0, "no files exercised");
    eprintln!("type_table_complete: {} files checked", checked);
}

#[test]
fn run_pipeline_for_reaches_verified_state() {
    use algol26::compiler::Compiler;

    let source = r#"
procedure main
    print(42)
"#;

    let mut c = Compiler::new();
    let verified = c
        .run_pipeline_for(source, "pipeline_test.gol")
        .expect("canonical pipeline should reach verified IR");

    // The VerifiedIR type is the proof — arriving here means every
    // pass in the chain ran and verification succeeded.
    assert!(verified.function_count() >= 1);
}

#[test]
fn run_pipeline_for_rejects_invalid_program() {
    use algol26::compiler::Compiler;

    let source = r#"
procedure main
    val x := undefined_variable
"#;

    let mut c = Compiler::new();
    let result = c.run_pipeline_for(source, "pipeline_bad.gol");
    assert!(
        result.is_err(),
        "canonical pipeline should reject invalid input before producing VerifiedIR",
    );
}

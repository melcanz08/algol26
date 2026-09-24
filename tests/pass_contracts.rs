// tests/pass_contracts.rs
//
// Cross-cutting assertions over every registered pass.
//
// The pipeline and scheduler enforce the *structural* portion of
// each pass's contract (kind, input, output) at build and run
// time. These tests cover the *informational* portion: that
// every pass declares non-empty metadata, and that the
// declarations are internally consistent.
//
// See docs/pass-contracts.md for the full contract model.

use algol26::compiler::pass::{PassContract, PassId, PassKind};
use algol26::compiler::passes::build_ir::BuildSemanticIRPass;
use algol26::compiler::passes::optimize::OptimizePass;
use algol26::compiler::passes::type_check::TypeCheckPass;
use algol26::compiler::passes::type_table_complete::TypeTableCompletePass;
use algol26::compiler::passes::verify_ir::{ReVerifyPass, VerifyIrPass};
use algol26::compiler::program::Program;
use algol26::compiler::registry::PassRegistry;

fn registry() -> PassRegistry<Program> {
    let mut reg: PassRegistry<Program> = PassRegistry::new();
    reg.register(TypeCheckPass);
    reg.register(TypeTableCompletePass);
    reg.register(BuildSemanticIRPass);
    reg.register(OptimizePass);
    reg.register(VerifyIrPass);
    reg.register(ReVerifyPass);
    reg
}

fn contracts() -> Vec<PassContract> {
    registry().contracts().cloned().collect()
}

#[test]
fn every_pass_declares_requires() {
    for c in contracts() {
        assert!(
            !c.requires.is_empty(),
            "pass `{}` declares no `requires` — every pass must state its \
             preconditions, even if only to name the input representation",
            c.id
        );
    }
}

#[test]
fn every_pass_declares_guarantees() {
    for c in contracts() {
        assert!(
            !c.guarantees.is_empty(),
            "pass `{}` declares no `guarantees` — every pass must state what \
             it establishes on success",
            c.id
        );
    }
}

#[test]
fn every_pass_declares_may_change() {
    for c in contracts() {
        assert!(
            !c.may_change.is_empty(),
            "pass `{}` declares no `may_change` — even a read-only pass \
             writes to `diagnostics`; declare it",
            c.id
        );
    }
}

#[test]
fn every_pass_declares_must_preserve() {
    for c in contracts() {
        assert!(
            !c.must_preserve.is_empty(),
            "pass `{}` declares no `must_preserve` — every pass must state \
             what downstream passes can rely on",
            c.id
        );
    }
}

#[test]
fn lowering_passes_advance_level() {
    for c in contracts() {
        if c.kind == PassKind::Lowering {
            assert!(
                c.output > c.input,
                "Lowering pass `{}` declares output `{}` <= input `{}`; \
                 lowering must advance the IR level",
                c.id,
                c.output,
                c.input
            );
        }
    }
}

#[test]
fn non_lowering_passes_do_not_change_level() {
    for c in contracts() {
        if c.kind != PassKind::Lowering {
            assert_eq!(
                c.input, c.output,
                "pass `{}` has kind {:?} but changes level `{}` -> `{}`; \
                 only Lowering may change level",
                c.id, c.kind, c.input, c.output
            );
        }
    }
}

#[test]
fn canonical_pipeline_builds() {
    // The order that `main.rs` uses. If this fails, the pass
    // contracts are inconsistent with each other — a real bug in
    // the contract declarations, not in this test.
    let ids = [
        PassId("ast.type_check"),
        PassId("ast.type_table_complete"),
        PassId("ir.build"),
        PassId("ir.verify"),
        PassId("ir.optimize"),
        PassId("ir.reverify"),
    ];
    registry()
        .build_pipeline(&ids)
        .expect("canonical pipeline should build — check per-pass contracts");
}

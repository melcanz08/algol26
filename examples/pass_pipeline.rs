// examples/pass_pipeline.rs

use algol26::compiler::{
    capabilities::{BackendKind, Feature},
    context::{CompilerConfig, CompilerContext, OptLevel},
    pass::{IrLevel, Pass, PassContract, PassId, PassKind, PassResult},
    pipeline::Pipeline,
    scheduler::Scheduler,
};

/// Placeholder compilation unit. Replace fields as you migrate.
pub struct Program {
    pub source: String,
    pub ast_ready: bool,
    pub semantic_ir_ready: bool,
}

// ---- Pass 1: frontend -> AST ----
struct ParsePass;

impl Pass<Program> for ParsePass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("frontend.parse"),
            kind: PassKind::Lowering,
            input: IrLevel::Source,
            output: IrLevel::Ast,
            requires: &["utf-8 source"],
            guarantees: &["syntactically well-formed AST"],
            may_change: &["program.ast"],
            must_preserve: &["source spans"],
            may_fail: true,
        };
        &C
    }
    fn run(&self, _ctx: &mut CompilerContext, p: &mut Program) -> PassResult {
        println!("  parsing {} bytes", p.source.len());
        p.ast_ready = true;
        Ok(())
    }
}

// ---- Pass 2: AST -> Semantic IR ----
struct SemanticLowerPass;

impl Pass<Program> for SemanticLowerPass {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("semantics.lower"),
            kind: PassKind::Lowering,
            input: IrLevel::Ast,
            output: IrLevel::SemanticIr,
            requires: &["AST built"],
            guarantees: &["semantic IR built", "symbols resolved"],
            may_change: &["program.semantic_ir"],
            must_preserve: &["source spans"],
            may_fail: true,
        };
        &C
    }
    fn run(&self, _ctx: &mut CompilerContext, p: &mut Program) -> PassResult {
        assert!(p.ast_ready, "contract violated: AST not built");
        println!("  lowering AST -> Semantic IR");
        p.semantic_ir_ready = true;
        Ok(())
    }
}

// ---- Pass 3: an analysis ----
struct OwnershipAnalysis;

impl Pass<Program> for OwnershipAnalysis {
    fn contract(&self) -> &PassContract {
        static C: PassContract = PassContract {
            id: PassId("semantics.ownership"),
            kind: PassKind::Analysis,
            input: IrLevel::SemanticIr,
            output: IrLevel::SemanticIr,
            requires: &["semantic IR built"],
            guarantees: &["ownership state computed"],
            may_change: &[],
            must_preserve: &["program"],
            may_fail: true,
        };
        &C
    }
    fn run(&self, _ctx: &mut CompilerContext, _p: &mut Program) -> PassResult {
        println!("  analyzing ownership");
        Ok(())
    }
}

fn main() {
    let config = CompilerConfig {
        target: BackendKind::Interpreter,
        opt_level: OptLevel::O0,
        ..Default::default()
    };
    let mut ctx = CompilerContext::new(config);

    // Sanity: capability matrix
    println!("Capability matrix:\n{}", ctx.capabilities.render_table());
    assert!(ctx.capabilities.supports(Feature::Ownership, BackendKind::Llvm));
    assert!(!ctx.capabilities.supports(Feature::Spawn, BackendKind::Llvm));

    // Build the pipeline. The builder validates contract chaining.
    let pipeline = Pipeline::builder()
        .add(ParsePass)
        .add(SemanticLowerPass)
        .add(OwnershipAnalysis)
        .build()
        .expect("pipeline contract chain must be valid");

    let mut program = Program {
        source: "fn main() { print(42) }".into(),
        ast_ready: false,
        semantic_ir_ready: false,
    };

    let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);
    assert!(outcome.succeeded(), "pipeline failed: {:?}", outcome.failure);
    println!("pipeline: {}/{} stages, {:?}", outcome.completed, outcome.total,
             outcome.timings.iter().map(|t| (t.pass, t.duration)).collect::<Vec<_>>());
}
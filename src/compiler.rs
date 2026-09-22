#![allow(dead_code)]
#![allow(unused_variables)]

// src/compiler.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::compiler::context::{CompilerConfig, CompilerContext};
use crate::compiler::program::{AstPayload, Program};
use crate::frontend::ast::Stmt;
use crate::frontend::ast::{ImplBlock, TraitDecl, TypeSyntax};
use crate::frontend::lexer::Lexer;
use crate::frontend::module_loader::ModuleLoader;
use crate::frontend::parser::Parser;
use crate::ir::monomorphize::Monomorphizer;
use crate::ir::semantic_ir::SemanticProgram;
use crate::ir::verified_ir::VerifiedIR;
use crate::semantics::analyzer::SemanticAnalyzer;
use crate::semantics::race::RaceDetector;
use std::rc::Rc;

// Pass infrastructure (Phase 1)
pub mod capabilities;
pub mod context;
pub mod pass;
pub mod passes;
pub mod pipeline;
pub mod program;
pub mod registry;
pub mod scheduler;

pub struct Compiler;

pub struct LexedProgram {
    pub tokens: Vec<crate::frontend::lexer::SpannedToken>,
}

pub struct ParsedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub type_info: TypeInfo,
    pub type_table: std::collections::HashMap<usize, crate::common::types::Type>,
}

#[derive(Debug, Default, Clone)]
pub struct TypeInfo {
    pub total_functions: usize,
    pub total_variables: usize,
    pub types_checked: bool,
}

/// Per-pass durations returned by `run_optimize_pass`.
#[derive(Debug, Clone, Copy)]
struct OptimizeTimings {
    optimize: std::time::Duration,
    verify: std::time::Duration,
}

/// TODO: orphaned after removing `SemanticIROptimized`. Either
/// delete or reintroduce via the optimizer's return value.
#[derive(Debug, Default)]
pub struct OptimizationReport {
    pub passes_run: Vec<String>,
    pub instructions_removed: usize,
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Runs the semantic analyzer and race detector, producing a typed AST.
///
/// Returns `CompileError` (with its original `ErrorCode`) on failure
/// so the pass wrapper can round-trip it through `PassError::cause`.
///
/// **Invariant:** `functions` is shared by `Rc::clone`, not cloned
/// deeply. The analyzer populates the type table keyed by the
/// addresses it visits; those addresses must survive into the
/// returned `TypedProgram`. See `docs/compiler/type-table-addressing.md`.
pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    span_map: &std::collections::HashMap<usize, (usize, usize)>,
) -> Result<TypedProgram> {
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_traits(functions, traits, impls, span_map)?;

    let mut race_detector = RaceDetector::new();
    let races = race_detector.analyze(functions);
    if let Some(race) = races.into_iter().next() {
        return Err(CompileError::simple(&race, 0, 0, "", ErrorCode::E0007));
    }

    let type_table = analyzer.take_type_table();

    Ok(TypedProgram {
        functions: Rc::clone(functions),
        type_info: TypeInfo {
            total_functions: functions.len(),
            total_variables: 0,
            types_checked: true,
        },
        type_table,
    })
}

/// Build a `SemanticProgram` from the typed AST.
///
/// This is the actual lowering step. Called by
/// `BuildSemanticIRPass`; there is no second implementation to keep
/// in sync.
pub fn build_semantic_ir_program(
    functions: &[crate::frontend::ast::FunctionDecl],
    type_table: std::collections::HashMap<usize, crate::common::types::Type>,
) -> Result<crate::ir::semantic_ir::SemanticProgram> {
    use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode};
    use crate::semantics::builder::SemanticIRBuilder;

    let (program, diagnostics) = SemanticIRBuilder::build(functions, type_table);

    if !diagnostics.is_empty() {
        for diag in &diagnostics {
            Diagnostic::Warning(diag.to_string()).display();
        }
        return Err(CompileError::simple(
            "Semantic IR construction failed",
            0,
            0,
            "",
            ErrorCode::E0002,
        ));
    }
    Ok(program)
}

impl Compiler {
    pub fn new() -> Self {
        Compiler
    }

    /// Frontend through IR construction, returning unverified IR.
    /// Used by `inspect --ir`.
    pub fn build_semantic_ir_for(
        &mut self,
        source: &str,
        filename: &str,
    ) -> Result<SemanticProgram> {
        let typed = self.type_check_source_for(source, filename)?;

        let mut program = Program::new(source, filename);
        program.typed = Some(typed);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        self.run_build_ir_pass(&mut program, &mut ctx)?;

        Ok(program
            .semantic_ir
            .take()
            .expect("build pass left IR in place"))
    }

    /// Frontend through monomorphization, stopping before type
    /// checking. Used by equivalence tests that need the exact
    /// allocation the analyzer will see.
    pub fn parse_source_for(&mut self, source: &str, filename: &str) -> Result<ParsedProgram> {
        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);
        Ok(self.monomorphize(&parsed))
    }

    /// Lex a source string. Used by `algol26 inspect --tokens`.
    pub fn lex_source_for(&self, source: &str) -> Result<LexedProgram> {
        self.lex(source)
    }

    /// Frontend through type checking. Used by `inspect --type-table`.
    pub fn type_check_source_for(&mut self, source: &str, filename: &str) -> Result<TypedProgram> {
        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);
        let parsed = self.monomorphize(&parsed);

        let mut program = Program::new(source, filename);
        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            span_map: std::collections::HashMap::new(),
        });
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        self.run_type_check_pass(&mut program, &mut ctx)?;

        Ok(program
            .typed
            .take()
            .expect("type_check pass left typed in place"))
    }

    /// Runs the IR verifier through the pass pipeline. On success,
    /// `program.verified` is set to true and `program.semantic_ir`
    /// still holds the verified IR.
    fn run_verify_pass(
        &self,
        program: &mut Program,
        ctx: &mut CompilerContext,
        context_label: &str,
    ) -> Result<()> {
        use crate::compiler::passes::verify_ir::VerifyIrPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(VerifyIrPass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let outcome = Scheduler::default().run(&pipeline, ctx, program);

        if let Some(err) = outcome.failure {
            return Err(CompileError::simple(
                &format!("IR verification failed {}: {}", context_label, err.message),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        program.verified = true;
        Ok(())
    }

    /// Runs the optimizer, followed by a verifier, through the
    /// scheduler. The scheduler refuses a `Transform` pass that is
    /// not immediately followed by a `Verification`, so the pipeline
    /// shape is enforced at run time, not by convention.
    fn run_optimize_pass(
        &self,
        program: &mut Program,
        ctx: &mut CompilerContext,
    ) -> Result<OptimizeTimings> {
        use crate::compiler::pass::PassId;
        use crate::compiler::passes::optimize::OptimizePass;
        use crate::compiler::passes::verify_ir::VerifyIrPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(OptimizePass)
            .add(VerifyIrPass)
            .build()
            .expect("optimize + verify is a valid chain — see pass-contracts.md");

        let outcome = Scheduler::default().run(&pipeline, ctx, program);

        if let Some(err) = outcome.failure {
            return Err(CompileError::simple(
                &format!("IR optimization failed: {}", err.message),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        let optimize_dur = outcome
            .timings
            .iter()
            .find(|t| t.pass == PassId("ir.optimize"))
            .map(|t| t.duration)
            .unwrap_or(std::time::Duration::ZERO);
        let verify_dur = outcome
            .timings
            .iter()
            .find(|t| t.pass == PassId("ir.verify"))
            .map(|t| t.duration)
            .unwrap_or(std::time::Duration::ZERO);

        // The pipeline ends with `VerifyIrPass`, so the program is
        // verified after this returns.
        program.verified = true;

        Ok(OptimizeTimings {
            optimize: optimize_dur,
            verify: verify_dur,
        })
    }

    /// Runs `BuildSemanticIRPass` through the scheduler. On success,
    /// `program.semantic_ir` holds the fresh unverified IR and
    /// `program.verified` is cleared.
    fn run_build_ir_pass(&self, program: &mut Program, ctx: &mut CompilerContext) -> Result<()> {
        use crate::compiler::passes::build_ir::BuildSemanticIRPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(BuildSemanticIRPass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let outcome = Scheduler::default().run(&pipeline, ctx, program);

        if let Some(err) = outcome.failure {
            return Err(CompileError::simple(
                &err.message,
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        program.verified = false;
        Ok(())
    }

    /// Compile through semantic IR, verify, then run through the
    /// interpreter. Used for programs that exercise IR features the
    /// LLVM backend does not lower (Result, try/catch).
    pub fn run_interpreter(&mut self, source: &str, filename: &str) -> Result<()> {
        use crate::backends::backend::Backend;
        use crate::backends::interpreter_backend::InterpreterBackend;

        let mut program = Program::new(source, filename);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);
        let parsed = self.monomorphize(&parsed);

        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            span_map: std::collections::HashMap::new(),
        });

        self.run_type_check_pass(&mut program, &mut ctx)?;
        self.run_build_ir_pass(&mut program, &mut ctx)?;
        self.run_verify_pass(&mut program, &mut ctx, "before interpreter lowering")?;

        let verified = VerifiedIR::from_verify_pass(
            program
                .semantic_ir
                .take()
                .expect("pipeline left IR in place"),
        );

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::interpreter(),
        )?;

        let backend = InterpreterBackend::new();
        backend.compile(&verified, "")?;
        let output = backend.get_output();
        if !output.trim().is_empty() {
            print!("{}", output);
        }
        Ok(())
    }

    /// Runs `TypeCheckPass` through the scheduler. On success,
    /// `program.typed` holds the analyzer output and the addressing
    /// invariant is asserted.
    fn run_type_check_pass(&self, program: &mut Program, ctx: &mut CompilerContext) -> Result<()> {
        use crate::compiler::passes::type_check::TypeCheckPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(TypeCheckPass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let outcome = Scheduler::default().run(&pipeline, ctx, program);

        if let Some(err) = outcome.failure {
            // Preserve the original CompileError if the pass wrapped
            // one (so a race-detection E0007 does not become E0002).
            if let Some(cause) = err.cause {
                return Err(*cause);
            }
            return Err(CompileError::simple(
                &err.message,
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        program.assert_addressing_invariant();
        Ok(())
    }

    /// Runs `TypeTableCompletePass`. Analysis-only: emits warnings,
    /// never fails. Returns the number of warnings produced in this
    /// invocation.
    fn run_type_table_complete_pass(
        &self,
        program: &mut Program,
        ctx: &mut CompilerContext,
    ) -> Result<usize> {
        use crate::compiler::passes::type_table_complete::TypeTableCompletePass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(TypeTableCompletePass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let start = ctx.diagnostics.len();
        let _outcome = Scheduler::default().run(&pipeline, ctx, program);

        // Render only this pass's diagnostics, not earlier ones.
        for d in &ctx.diagnostics[start..] {
            d.display();
        }

        Ok(ctx.diagnostics[start..]
            .iter()
            .filter(|d| matches!(d, crate::common::diagnostics::Diagnostic::Warning(_)))
            .count())
    }

    /// Public wrapper for `inspect --type-table`.
    pub fn run_type_table_complete_pass_public(&self, typed: TypedProgram) -> Result<usize> {
        let mut program = Program::new("", "");
        program.typed = Some(typed);
        let mut ctx = CompilerContext::new(CompilerConfig::default());
        self.run_type_table_complete_pass(&mut program, &mut ctx)
    }

    pub fn compile(
        &mut self,
        source: &str,
        filename: &str,
        output_name: &str,
        emit_llvm: bool,
        run_after_compile: bool,
        timing: bool,
    ) -> Result<()> {
        use std::time::Instant;

        let total_start = Instant::now();

        // One Program and one CompilerContext for the whole pipeline.
        // Passes read and write `program` in place; diagnostics
        // accumulate across phases.
        let mut program = Program::new(source, filename);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        // Phase 1: LEX
        let phase_start = Instant::now();
        let lexed = self.lex(source)?;
        let lex_time = phase_start.elapsed();

        // Phase 2: PARSE
        let phase_start = Instant::now();
        let parsed = self.parse(lexed)?;
        let parse_time = phase_start.elapsed();

        // Phase 3: PROCESS IMPORTS
        let phase_start = Instant::now();
        let parsed = self.process_imports(&parsed, filename)?;
        let imports_time = phase_start.elapsed();

        // Phase 4: DESUGAR
        let phase_start = Instant::now();
        let parsed = self.desugar(&parsed);
        let desugar_time = phase_start.elapsed();

        // Phase 5: EXPAND IMPL METHODS
        let phase_start = Instant::now();
        let parsed = self.expand_impl_methods(&parsed);
        let expand_time = phase_start.elapsed();

        // Phase 6: MONOMORPHIZE
        let phase_start = Instant::now();
        let parsed = self.monomorphize(&parsed);
        let mono_time = phase_start.elapsed();

        // Hand the parsed AST to the pipeline. Rc::clone keeps the
        // same allocation the analyzer will key its type table
        // against — see the addressing invariant in `program.rs`.
        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            span_map: std::collections::HashMap::new(),
        });

        // Phase 7: TYPE CHECK
        let phase_start = Instant::now();
        self.run_type_check_pass(&mut program, &mut ctx)?;
        let type_check_time = phase_start.elapsed();

        // Phase 7.5: TYPE TABLE COMPLETENESS
        let phase_start = Instant::now();
        let _warnings = self.run_type_table_complete_pass(&mut program, &mut ctx)?;
        let type_table_check_time = phase_start.elapsed();

        // Phase 8: BUILD SEMANTIC IR
        let phase_start = Instant::now();
        self.run_build_ir_pass(&mut program, &mut ctx)?;
        let ir_build_time = phase_start.elapsed();

        // Phase 9: VERIFY IR (pre-optimization)
        let phase_start = Instant::now();
        self.run_verify_pass(&mut program, &mut ctx, "after construction")?;
        let verify_pre_time = phase_start.elapsed();

        // Phase 10: OPTIMIZE. The pipeline includes a following
        // VerifyIrPass; the scheduler enforces that ordering.
        let timings = self.run_optimize_pass(&mut program, &mut ctx)?;

        // Phase 11: LOWER TO BACKEND
        let phase_start = Instant::now();
        let verified = VerifiedIR::from_verify_pass(
            program
                .semantic_ir
                .take()
                .expect("optimize pass left IR in place"),
        );
        self.lower_to_llvm(
            &verified,
            filename,
            output_name,
            emit_llvm,
            run_after_compile,
        )?;
        let lower_time = phase_start.elapsed();

        let total_time = total_start.elapsed();

        if timing || total_time.as_secs() > 1 {
            eprintln!("[Timing] Total: {:.2}s", total_time.as_secs_f64());
            eprintln!("  Lex:        {:.4}s", lex_time.as_secs_f64());
            eprintln!("  Parse:      {:.4}s", parse_time.as_secs_f64());
            eprintln!("  Imports:    {:.4}s", imports_time.as_secs_f64());
            eprintln!("  Desugar:    {:.4}s", desugar_time.as_secs_f64());
            eprintln!("  Expand:     {:.4}s", expand_time.as_secs_f64());
            eprintln!("  Mono:       {:.4}s", mono_time.as_secs_f64());
            eprintln!("  TypeCheck:  {:.4}s", type_check_time.as_secs_f64());
            eprintln!("  IR Build:   {:.4}s", ir_build_time.as_secs_f64());
            eprintln!("  Verify(1):  {:.4}s", verify_pre_time.as_secs_f64());
            eprintln!("  Optimize:   {:.4}s", timings.optimize.as_secs_f64());
            eprintln!("  Verify(2):  {:.4}s", timings.verify.as_secs_f64());
            eprintln!("  Lower:      {:.4}s", lower_time.as_secs_f64());
            eprintln!("  TypeTblChk: {:.4}s", type_table_check_time.as_secs_f64());
        }

        Ok(())
    }

    fn expand_impl_methods(&self, parsed: &ParsedProgram) -> ParsedProgram {
        let mut all_functions = (*parsed.functions).clone();

        for impl_block in &parsed.impls {
            let type_name = impl_block.target_type.clone();
            for method in &impl_block.methods {
                let mut renamed_method = method.clone();
                // Rename "compare" to "Int_compare"
                renamed_method.name = format!("{}_{}", type_name, method.name);
                // Add self parameter (the receiver) as first param
                renamed_method.params.insert(
                    0,
                    (
                        "self".to_string(),
                        Some(TypeSyntax::Named(type_name.clone())),
                    ),
                );
                all_functions.push(renamed_method);
            }
        }

        ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        }
    }

    fn monomorphize(&self, parsed: &ParsedProgram) -> ParsedProgram {
        let mut monomorphizer = Monomorphizer::new();
        monomorphizer.collect_instantiations(&parsed.functions);
        let specialized_functions = monomorphizer.monomorphize(&parsed.functions);

        ParsedProgram {
            functions: Rc::new(specialized_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        }
    }

    fn lex(&self, source: &str) -> Result<LexedProgram> {
        let lexer = Lexer::new(source.to_string())?;
        Ok(LexedProgram {
            tokens: lexer.tokens,
        })
    }

    fn desugar(&self, parsed: &ParsedProgram) -> ParsedProgram {
        let mut functions = (*parsed.functions).clone();
        crate::ir::loop_desugar::desugar_loops(&mut functions);
        ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        }
    }

    fn parse(&self, lexed: LexedProgram) -> Result<ParsedProgram> {
        let mut parser = Parser::new(lexed.tokens);
        let program = parser.parse_program()?;
        Ok(ParsedProgram {
            functions: Rc::new(program.functions),
            traits: program.traits,
            impls: program.impls,
        })
    }

    fn process_imports(&self, parsed: &ParsedProgram, current_file: &str) -> Result<ParsedProgram> {
        let mut loader = ModuleLoader::new();
        let mut all_functions = (*parsed.functions).clone();

        for func in parsed.functions.iter() {
            for stmt in &func.body {
                if let Stmt::Import { path, .. } = stmt {
                    let resolved = loader.resolve_import(path, current_file)?;
                    let source = loader.load_file(&resolved)?;

                    if !source.is_empty() {
                        // Parse the imported file
                        let lexer = Lexer::new(source.clone())?;
                        let mut parser = Parser::new(lexer.tokens);
                        let imported_program = parser.parse_program()?;

                        // Add imported functions (skip any functions
                        // that already exist by name).
                        for imported in imported_program.functions {
                            if !all_functions.iter().any(|f| f.name == imported.name) {
                                all_functions.push(imported);
                            }
                        }
                    }

                    loader.end_import();
                }
            }
        }

        Ok(ParsedProgram {
            functions: Rc::new(all_functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        })
    }

    fn lower_to_llvm(
        &self,
        verified: &VerifiedIR,
        _filename: &str,
        output_name: &str,
        emit_llvm: bool,
        run_after_compile: bool,
    ) -> Result<()> {
        use crate::backends::backend::{Backend, BackendOutput};
        use crate::backends::llvm_backend::LlvmBackend;

        // Delegate LLVM emission to the backend trait. The same
        // code path is exercised by `tests/backends/`, so
        // `module.verify()` and the capability check both run on
        // the production path.
        let backend = LlvmBackend::new();
        let ir_path = match backend.compile(verified, output_name)? {
            BackendOutput::LlvmIr { path } => path,
            other => {
                return Err(CompileError::simple(
                    &format!("LlvmBackend returned unexpected output: {:?}", other),
                    0,
                    0,
                    "",
                    ErrorCode::E0009,
                ));
            }
        };

        if emit_llvm {
            return Ok(());
        }

        // FFI libraries requested by extern declarations are
        // forwarded to clang as -l flags.
        let libraries = &verified.program().ffi_libraries;
        let output_path = crate::toolchain::link_llvm_ir(&ir_path, output_name, libraries)?;
        println!("[Successfully compiled to {}]", output_path.display());

        if run_after_compile {
            crate::toolchain::run_binary(&output_path)?;
        }

        Ok(())
    }

    pub fn compile_to_wasm(
        &mut self,
        source: &str,
        filename: &str,
        output_name: &str,
    ) -> Result<()> {
        use crate::backends::backend::Backend;
        use crate::backends::wasm_backend::WasmBackend;

        let mut program = Program::new(source, filename);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        // Frontend phases. Note: unlike `compile()` and
        // `run_interpreter()`, this path does not call
        // `monomorphize`. That is a pre-existing inconsistency,
        // not a deliberate choice.
        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);

        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
            span_map: std::collections::HashMap::new(),
        });

        self.run_type_check_pass(&mut program, &mut ctx)?;
        self.run_build_ir_pass(&mut program, &mut ctx)?;
        self.run_verify_pass(&mut program, &mut ctx, "before WASM lowering")?;

        let verified = VerifiedIR::from_verify_pass(
            program
                .semantic_ir
                .take()
                .expect("pipeline left IR in place"),
        );

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::wasm(),
        )?;

        let backend = WasmBackend::new();
        backend.compile(&verified, output_name)?;

        Ok(())
    }
}

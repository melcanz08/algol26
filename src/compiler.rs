#![allow(dead_code)]
#![allow(unused_variables)]

// src/compiler.rs updates for Semantic IR & Defer Lowering Integration

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::frontend::ast::Stmt;
use crate::frontend::ast::{ImplBlock, TraitDecl, TypeSyntax};
use crate::frontend::lexer::Lexer;
use crate::frontend::module_loader::ModuleLoader;
use crate::frontend::parser::Parser;
use crate::ir::monomorphize::Monomorphizer;
use crate::ir::semantic_ir::SemanticProgram;
use crate::ir::verified_ir::VerifiedIR;
use crate::backends::llvm_codegen::IRCodeGen;
use crate::semantics::race::RaceDetector;
use crate::semantics::analyzer::SemanticAnalyzer;
use inkwell::context::Context;
use std::path::PathBuf;
use std::rc::Rc;

// Pass infrastructure (Phase 1)
pub mod capabilities;
pub mod context;
pub mod pass;
pub mod pipeline;
pub mod registry;
pub mod scheduler;
pub mod passes;
pub mod program;

pub struct Compiler;

pub struct LexedProgram {
    pub tokens: Vec<crate::frontend::lexer::Token>,
    pub positions: Vec<(usize, usize)>,
}

pub struct ParsedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub span_map: std::collections::HashMap<usize, (usize, usize)>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}

pub struct TypedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub type_info: TypeInfo,
    pub type_table: std::collections::HashMap<usize, crate::common::types::Type>,
}

pub struct SemanticIROptimized {
    pub program: SemanticProgram,
    pub optimization_report: OptimizationReport,
}

#[derive(Debug, Default)]
pub struct TypeInfo {
    pub total_functions: usize,
    pub total_variables: usize,
    pub types_checked: bool,
}

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

/// Build a `SemanticProgram` from the typed AST.
///
/// This is the actual lowering step. `Compiler::build_semantic_ir`
/// delegates to it and `BuildSemanticIRPass` calls it; the two paths
/// share one implementation so the equivalence test is meaningful.
///
/// Returns `Err` on any diagnostic produced by the builder. Diagnostics
/// are printed to stderr here for parity with the pre-pass behavior;
/// once the compiler routes diagnostics through `CompilerContext`,
/// this becomes a `Vec<Diagnostic>` in the error.
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

    /// Runs the frontend + semantics phases up to and including IR
    /// construction, returning the unverified `SemanticProgram`.
    ///
    /// This is the extracted prefix of `compile()`. It exists so the
    /// pass pipeline and the equivalence test share exactly one
    /// implementation of the frontend. When `compile()` is eventually
    /// migrated to drive the pipeline, this method goes away — the
    /// pipeline will be the implementation.
    pub fn build_semantic_ir_for(
        &mut self,
        source: &str,
        filename: &str,
    ) -> Result<SemanticProgram> {
        let typed = self.type_check_source_for(source, filename)?;
        self.run_build_ir_pass(typed.functions, typed.type_table)
    }

    pub fn type_check_source_for(
        &mut self,
        source: &str,
        filename: &str,
    ) -> Result<TypedProgram> {
        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);
        let parsed = self.monomorphize(&parsed);
        self.type_check(&parsed)
    }

    /// Runs `VerifyIrPass` on the given IR, returning it unchanged on
    /// success and a `CompileError` on failure.
    ///
    /// Takes ownership because `Program` owns `Option<SemanticProgram>`;
    /// the pass does not mutate the IR, so the value round-trips out
    /// intact. When Transform passes migrate, this signature stays
    /// correct — the pipeline mutates in place and hands the value back.
    fn run_verify_pass(
        &self,
        semantic_ir: crate::ir::semantic_ir::SemanticProgram,
        context_label: &str,
    ) -> Result<crate::ir::semantic_ir::SemanticProgram> {
        use crate::compiler::context::{CompilerConfig, CompilerContext};
        use crate::compiler::passes::verify_ir::VerifyIrPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::program::Program;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(VerifyIrPass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.semantic_ir = Some(semantic_ir);

        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);

        if let Some(err) = outcome.failure {
            return Err(CompileError::simple(
                &format!("IR verification failed {}: {}", context_label, err.message),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        Ok(program
            .semantic_ir
            .take()
            .expect("verification pass left IR in place"))
    }

    /// Runs `OptimizePass` followed by `VerifyIrPass`.
    ///
    /// The two are combined because the scheduler enforces the
    /// transform-must-be-verified discipline locally: the pipeline is
    /// `[Transform, Verification]` at the same IR level, so the
    /// `require_verification_after_transforms` rule is satisfied
    /// without having to disable it for this call site.
    ///
    /// On failure, the error carries the "after optimization" wording
    /// that Phase 12 of `compile()` used to produce directly.
    fn run_optimize_pass(
        &self,
        semantic_ir: crate::ir::semantic_ir::SemanticProgram,
    ) -> Result<crate::ir::semantic_ir::SemanticProgram> {
        use crate::compiler::context::{CompilerConfig, CompilerContext};
        use crate::compiler::passes::optimize::OptimizePass;
        use crate::compiler::passes::verify_ir::VerifyIrPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::program::Program;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(OptimizePass)
            .add(VerifyIrPass)
            .build()
            .expect("optimize-then-verify is a valid pass chain");

        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.semantic_ir = Some(semantic_ir);

        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);

        if let Some(err) = outcome.failure {
            return Err(CompileError::simple(
                &format!("IR verification failed after optimization: {}", err.message),
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        Ok(program
            .semantic_ir
            .take()
            .expect("optimize+verify pipeline left IR in place"))
    }

        /// Runs `BuildSemanticIRPass` on the given typed AST.
    ///
    /// Shape matches `run_verify_pass` / `run_optimize_pass`:
    /// construct the pipeline, load inputs into a transient `Program`,
    /// run, and extract the output. The `Program` is throw-away here
    /// because the driver (`Compiler::compile`) still threads IR
    /// values by value; once the driver itself moves to a persistent
    /// `Program`, these helpers collapse into pass invocations.
    fn run_build_ir_pass(
        &self,
        functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
        type_table: std::collections::HashMap<usize, crate::common::types::Type>,
    ) -> Result<crate::ir::semantic_ir::SemanticProgram> {
        use crate::compiler::context::{CompilerConfig, CompilerContext};
        use crate::compiler::passes::build_ir::BuildSemanticIRPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::program::{AstPayload, Program};
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(BuildSemanticIRPass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.ast = Some(AstPayload { functions, type_table });

        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);

        if let Some(err) = outcome.failure {
            return Err(CompileError::simple(&err.message, 0, 0, "", ErrorCode::E0002));
        }

        Ok(program
            .semantic_ir
            .take()
            .expect("build pass left IR in place"))
    }

    pub fn compile(
        &mut self,
        source: &str,
        filename: &str,
        output_name: &str,
        emit_llvm: bool,
        run_after_compile: bool,
    ) -> Result<()> {
        use std::time::Instant;

        let total_start = Instant::now();

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

        // Phase 7: TYPE CHECK
        let phase_start = Instant::now();
        let typed = self.type_check(&parsed)?;
        let type_check_time = phase_start.elapsed();

        // Phase 8: SAFETY CHECK
        let phase_start = Instant::now();
        let safety_time = phase_start.elapsed();

        // Phase 9: BUILD SEMANTIC IR
        let phase_start = Instant::now();
        let mut semantic_ir = self.build_semantic_ir(&parsed.functions, typed.type_table.clone())?;
        let ir_build_time = phase_start.elapsed();

        // Phase 10: VERIFY IR (pre-optimization)
        let phase_start = Instant::now();
        semantic_ir = self.run_verify_pass(semantic_ir, "after construction")?;
        let verify_pre_time = phase_start.elapsed();

        // Phase 11 + 12: OPTIMIZE, then VERIFY (post-optimization)
        let phase_start = Instant::now();
        semantic_ir = self.run_optimize_pass(semantic_ir)?;
        let optimize_time = phase_start.elapsed();
        let verify_post_time = std::time::Duration::ZERO;

        let optimized_semantic_ir = self.optimize_semantic_ir(semantic_ir)?;

        // Phase 13: LOWER TO BACKEND
        let phase_start = Instant::now();
        self.lower_to_llvm(
            &optimized_semantic_ir,
            filename,
            output_name,
            emit_llvm,
            run_after_compile,
        )?;
        let lower_time = phase_start.elapsed();

        let total_time = total_start.elapsed();

        // Print timing summary (only if compile takes > 1 second)
        if total_time.as_secs() > 1 {
            eprintln!("[Timing] Total: {:.2}s", total_time.as_secs_f64());
            eprintln!("  Lex:        {:.4}s", lex_time.as_secs_f64());
            eprintln!("  Parse:      {:.4}s", parse_time.as_secs_f64());
            eprintln!("  Imports:    {:.4}s", imports_time.as_secs_f64());
            eprintln!("  Desugar:    {:.4}s", desugar_time.as_secs_f64());
            eprintln!("  Expand:     {:.4}s", expand_time.as_secs_f64());
            eprintln!("  Mono:       {:.4}s", mono_time.as_secs_f64());
            eprintln!("  TypeCheck:  {:.4}s", type_check_time.as_secs_f64());
            eprintln!("  Safety:     {:.4}s", safety_time.as_secs_f64());
            eprintln!("  IR Build:   {:.4}s", ir_build_time.as_secs_f64());
            eprintln!("  Verify(1):  {:.4}s", verify_pre_time.as_secs_f64());
            eprintln!("  Optimize:   {:.4}s", optimize_time.as_secs_f64());
            eprintln!("  Verify(2):  {:.4}s", verify_post_time.as_secs_f64());
            eprintln!("  Lower:      {:.4}s", lower_time.as_secs_f64());
        }

        Ok(())
    }

    /// Compile through semantic IR, verify, then execute via the interpreter.
    /// Skips LLVM and WASM codegen — used for programs that exercise IR
    /// features the LLVM backend doesn't yet lower (Result, try/catch).
    pub fn run_interpreter(
        &mut self,
        source: &str,
        filename: &str,
    ) -> Result<()> {
        use crate::backends::backend::Backend;
        use crate::backends::interpreter_backend::InterpreterBackend;

        // Phases 1–8 (same as compile).
        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);
        let typed = self.type_check(&parsed)?;

        // Phase 9–10: IR + verification.
        let semantic_ir =
            self.build_semantic_ir(&parsed.functions, typed.type_table.clone())?;
        semantic_ir.verify().map_err(|e| {
            CompileError::simple(
                &format!("IR verification failed: {}", e),
                0, 0, "", ErrorCode::E0002,
            )
        })?;

        crate::backends::capabilities::check_backend(
            &semantic_ir,
            &crate::backends::capabilities::BackendCapabilities::interpreter(),
        )?;

        // Phase 13: Interpreter backend.
        let verified = VerifiedIR::new(semantic_ir)?;
        let backend = InterpreterBackend::new();
        backend.compile(&verified, "")?;
        let output = backend.get_output();
        if !output.trim().is_empty() {
            print!("{}", output);
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
            span_map: parsed.span_map.clone(),
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
            span_map: parsed.span_map.clone(),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        }
    }

    fn lex(&self, source: &str) -> Result<LexedProgram> {
        let lexer = Lexer::new(source.to_string())?;
        Ok(LexedProgram {
            tokens: lexer.tokens,
            positions: lexer.positions,
        })
    }

    fn desugar(&self, parsed: &ParsedProgram) -> ParsedProgram {
        let mut functions = (*parsed.functions).clone();
        crate::ir::loop_desugar::desugar_loops(&mut functions);
        ParsedProgram {
            functions: Rc::new(functions),
            span_map: parsed.span_map.clone(),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        }
    }

    fn parse(&self, lexed: LexedProgram) -> Result<ParsedProgram> {
        let mut parser = Parser::new_with_positions(lexed.tokens, lexed.positions);
        let program = parser.parse_program()?;
        let span_map = parser.get_span_map().clone();
        Ok(ParsedProgram {
            functions: Rc::new(program.functions),
            span_map,
            traits: program.traits,
            impls: program.impls,
        })
    }

    fn process_imports(&self, parsed: &ParsedProgram, current_file: &str) -> Result<ParsedProgram> {
        let mut loader = ModuleLoader::new();
        let mut all_functions = (*parsed.functions).clone();

        for func in parsed.functions.iter() {
            for stmt in &func.body {
                if let Stmt::Import { path } = stmt {
                    let resolved = loader.resolve_import(&path, current_file)?;
                    let source = loader.load_file(&resolved)?;

                    if !source.is_empty() {
                        // Parse the imported file
                        let lexer = Lexer::new(source.clone()).map_err(|e| {
                            e.display();
                            CompileError::simple(
                                "Lexing failed in import",
                                0,
                                0,
                                "",
                                ErrorCode::E0001,
                            )
                        })?;
                        let mut parser = Parser::new_with_positions(lexer.tokens, lexer.positions);
                        let imported_program = parser.parse_program().map_err(|e| {
                            e.display();
                            CompileError::simple(
                                "Parsing failed in import",
                                0,
                                0,
                                "",
                                ErrorCode::E0001,
                            )
                        })?;

                        // Add imported functions (skip any functions that already exist)
                        let imported_funcs = imported_program.functions;
                        for imported in imported_funcs {
                            if !all_functions.iter().any(|f| f.name == imported.name) {
                                all_functions.push(imported);
                            }
                        }
                        // TODO: merge span maps from imports
                    }

                    loader.end_import();
                }
            }
        }

        Ok(ParsedProgram {
            functions: Rc::new(all_functions),
            span_map: std::collections::HashMap::new(),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        })
    }

    fn type_check(&self, parsed: &ParsedProgram) -> Result<TypedProgram> {
        let mut analyzer = SemanticAnalyzer::new();
        analyzer
            .analyze_with_traits(
                &parsed.functions,
                &parsed.traits,
                &parsed.impls,
                &parsed.span_map,
            )
            .map_err(|e| {
                e.display();
                CompileError::simple("Type checking failed", 0, 0, "", ErrorCode::E0002)
            })?;

        let mut race_detector = RaceDetector::new();
        let races = race_detector.analyze(&parsed.functions);
        if let Some(race) = races.into_iter().next() {
            return Err(CompileError::simple(&race, 0, 0, "", ErrorCode::E0007));
        }

        // ─── UNIFY TYPES ─── extract the table produced by the analyzer.
        let type_table = analyzer.take_type_table();

        Ok(TypedProgram {
            functions: Rc::clone(&parsed.functions),   // refcount bump, same allocation
            type_info: TypeInfo {
                total_functions: parsed.functions.len(),
                total_variables: 0,
                types_checked: true,
            },
            type_table,
        })
    }

    fn build_semantic_ir(
        &self,
        functions: &[crate::frontend::ast::FunctionDecl],
        type_table: std::collections::HashMap<usize, crate::common::types::Type>,
    ) -> Result<SemanticProgram> {
        build_semantic_ir_program(functions, type_table)
    }

    fn optimize_semantic_ir(&self, ir: SemanticProgram) -> Result<SemanticIROptimized> {
        Ok(SemanticIROptimized {
            program: ir,
            optimization_report: OptimizationReport {
                passes_run: vec!["cfg_verification".to_string()],
                instructions_removed: 0,
            },
        })
    }

    fn lower_to_llvm(
        &self,
        optimized: &SemanticIROptimized,
        filename: &str,
        output_name: &str,
        emit_llvm: bool,
        run_after_compile: bool,
    ) -> Result<()> {
        crate::backends::capabilities::check_backend(
            &optimized.program,
            &crate::backends::capabilities::BackendCapabilities::llvm(),
        )?;

        let context = Context::create();
        let mut codegen = IRCodeGen::new(&context, "algol26_module");

        codegen.compile(&optimized.program).map_err(|e| {
            e.display();
            CompileError::simple("Code generation failed", 0, 0, "", ErrorCode::E0002)
        })?;

        let ir_path = PathBuf::from(output_name).with_extension("ll");
        codegen.module.print_to_file(&ir_path).map_err(|e| {
            let err = CompileError::simple(
                &format!("Failed to emit LLVM IR: {}", e),
                0, 0, "", ErrorCode::E0001,
            );
            err.display();
            err
        })?;

        println!("[Generated LLVM IR: {}]", ir_path.display());

        if emit_llvm {
            return Ok(());
        }

        let output_path = crate::toolchain::link_llvm_ir(&ir_path, output_name)?;
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

        // Phases 1-8: Same as compile()
        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);
        let typed = self.type_check(&parsed)?;

        // Use the *original* `parsed.functions` slice — the analyzer recorded
        // type info keyed by these exact nodes, so addresses line up.
        let semantic_ir = self.build_semantic_ir(&parsed.functions, typed.type_table.clone())?;

        // Phase 10: Verify IR
        semantic_ir.verify().map_err(|e| {
            CompileError::simple(
                &format!("IR verification failed: {}", e),
                0,
                0,
                "",
                ErrorCode::E0002,
            )
        })?;

        crate::backends::capabilities::check_backend(
            &semantic_ir,
            &crate::backends::capabilities::BackendCapabilities::wasm(),
        )?;

        // Phase 13: Lower to WASM backend
        let backend = WasmBackend::new();
        backend.compile(&VerifiedIR::new(semantic_ir.clone())?, output_name)?;

        Ok(())
    }
}
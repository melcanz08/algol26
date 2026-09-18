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
    pub span_map: std::collections::HashMap<usize, (usize, usize)>,
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
/// TODO: OptimizationReport is orphaned after removing
///SemanticIROptimized; either delete or reintroduce via
///the optimizer's return value
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
/// returned `TypedProgram`. See
/// `docs/compiler/type-table-addressing.md`.
pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
    span_map: &std::collections::HashMap<usize, (usize, usize)>,
) -> Result<TypedProgram> {
    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_traits(functions, traits, impls, span_map)
        .map_err(|e| {
            e.display();
            CompileError::simple("Type checking failed", 0, 0, "", ErrorCode::E0002)
        })?;

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
        self.run_build_ir_pass(typed)
    }

    /// Runs the frontend up to and including monomorphization, stopping
    /// before type checking. Used by equivalence tests that need the
    /// same `ParsedProgram` the analyzer would see.
    ///
    /// The returned `ParsedProgram::functions` is the *final* allocation
    /// before type checking — the one the analyzer will key its type
    /// table against.
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

    pub fn type_check_source_for(&mut self, source: &str, filename: &str) -> Result<TypedProgram> {
        let lexed = self.lex(source)?;
        let parsed = self.parse(lexed)?;
        let parsed = self.process_imports(&parsed, filename)?;
        let parsed = self.desugar(&parsed);
        let parsed = self.expand_impl_methods(&parsed);
        let parsed = self.monomorphize(&parsed);
        self.type_check(&parsed)
    }

    /// Runs `VerifyIrPass` on the given IR and returns a `VerifiedIR`.
    ///
    /// The return type is the safety property: after this call, the
    /// IR is known to have passed verification, and the only way to
    /// reach the optimizer or a backend is to hold a `VerifiedIR`.
    fn run_verify_pass(
        &self,
        semantic_ir: crate::ir::semantic_ir::SemanticProgram,
        context_label: &str,
    ) -> Result<VerifiedIR> {
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

        let verified = program
            .semantic_ir
            .take()
            .expect("verification pass left IR in place");

        // `VerifyIrPass` just succeeded on this exact program. Wrap
        // without re-running the verifier.
        Ok(VerifiedIR::from_verify_pass(verified))
    }

    /// Runs the IR optimizer on a `VerifiedIR`, re-verifies the
    /// result, and returns a fresh `VerifiedIR`.
    ///
    /// The signature is the point: unverified IR cannot enter the
    /// optimizer (only a `VerifiedIR` can be passed), and unverified
    /// IR cannot leave it (the `mutate` call inside re-runs the
    /// verifier before returning). Both properties are enforced by
    /// the type system, not by convention.
    fn run_optimize_pass(&self, verified: VerifiedIR) -> Result<VerifiedIR> {
        use crate::ir::optimizer::Optimizer;

        let mut optimizer = Optimizer::new();
        verified
            .mutate(|program| optimizer.optimize(program))
            .map_err(|e| {
                CompileError::simple(
                    &format!("IR verification failed after optimization: {}", e),
                    0,
                    0,
                    "",
                    ErrorCode::E0002,
                )
            })
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
        typed: TypedProgram,
    ) -> Result<crate::ir::semantic_ir::SemanticProgram> {
        use crate::compiler::context::{CompilerConfig, CompilerContext};
        use crate::compiler::passes::build_ir::BuildSemanticIRPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::program::Program;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(BuildSemanticIRPass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.typed = Some(typed);

        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);

        if let Some(err) = outcome.failure {
            return Err(CompileError::simple(
                &err.message,
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        Ok(program
            .semantic_ir
            .take()
            .expect("build pass left IR in place"))
    }

    /// Compile through semantic IR, verify, then execute via the interpreter.
    /// Skips LLVM and WASM codegen — used for programs that exercise IR
    /// features the LLVM backend doesn't yet lower (Result, try/catch).
    pub fn run_interpreter(&mut self, source: &str, filename: &str) -> Result<()> {
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
        let semantic_ir = self.build_semantic_ir(&parsed.functions, typed.type_table.clone())?;
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

    /// Runs `TypeCheckPass` on the given parsed program.
    ///
    /// On failure, propagates the original `CompileError` (with its
    /// `ErrorCode`) rather than reconstructing it — so a race
    /// detection failure still surfaces as `E0007`.
    fn run_type_check_pass(
        &self,
        functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
        traits: Vec<TraitDecl>,
        impls: Vec<ImplBlock>,
        span_map: std::collections::HashMap<usize, (usize, usize)>,
    ) -> Result<TypedProgram> {
        use crate::compiler::context::{CompilerConfig, CompilerContext};
        use crate::compiler::passes::type_check::TypeCheckPass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::program::{AstPayload, Program};
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(TypeCheckPass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.ast = Some(AstPayload {
            functions,
            traits,
            impls,
            span_map,
        });

        let outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);

        if let Some(err) = outcome.failure {
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

        Ok(program
            .typed
            .take()
            .expect("type_check pass left typed in place"))
    }

    /// Runs `TypeTableCompletePass` on the typed AST.
    ///
    /// `Analysis` kind: reads `program.typed`, produces diagnostics,
    /// never fails. Returns the number of warnings emitted so callers
    /// can surface it (e.g. `inspect` or `--verbose`).
    fn run_type_table_complete_pass(&self, typed: TypedProgram) -> Result<usize> {
        use crate::compiler::context::{CompilerConfig, CompilerContext};
        use crate::compiler::passes::type_table_complete::TypeTableCompletePass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::program::Program;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(TypeTableCompletePass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let mut ctx = CompilerContext::new(CompilerConfig::default());
        let mut program = Program::new("", "");
        program.typed = Some(typed);

        let _outcome = Scheduler::default().run(&pipeline, &mut ctx, &mut program);

        for d in ctx.diagnostics.iter() {
            d.display();
        }

        Ok(ctx.warning_count())
    }

    /// Public wrapper for `inspect --type-table`.
    ///
    /// `run_type_table_complete_pass` is private so only the driver
    /// uses it; the CLI goes through this. Returns the number of
    /// warnings the pass emitted (0 means the table is complete).
    pub fn run_type_table_complete_pass_public(&self, typed: TypedProgram) -> Result<usize> {
        self.run_type_table_complete_pass(typed)
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

        // Phase 7.5: TYPE TABLE COMPLETENESS
        let phase_start = Instant::now();
        let _warnings = self.run_type_table_complete_pass(typed.clone())?;
        let type_table_check_time = phase_start.elapsed();

        // Phase 8: SAFETY CHECK
        let phase_start = Instant::now();
        let safety_time = phase_start.elapsed();

        // Phase 9: BUILD SEMANTIC IR
        let phase_start = Instant::now();
        let semantic_ir = self.build_semantic_ir(&parsed.functions, typed.type_table.clone())?;
        let ir_build_time = phase_start.elapsed();

        // Phase 10: VERIFY IR (pre-optimization) → VerifiedIR
        let phase_start = Instant::now();
        let verified_pre = self.run_verify_pass(semantic_ir, "after construction")?;
        let verify_pre_time = phase_start.elapsed();

        // Phase 11 + 12: OPTIMIZE inside the verified wrapper → VerifiedIR
        let phase_start = Instant::now();
        let verified_post = self.run_optimize_pass(verified_pre)?;
        let optimize_time = phase_start.elapsed();
        let verify_post_time = std::time::Duration::ZERO;

        // Phase 13: LOWER TO BACKEND
        let phase_start = Instant::now();
        self.lower_to_llvm(
            &verified_post,
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
        let mut parser = Parser::new(lexed.tokens);
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
                if let Stmt::Import { path, .. } = stmt {
                    let resolved = loader.resolve_import(path, current_file)?;
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
                        let mut parser = Parser::new(lexer.tokens);
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
        self.run_type_check_pass(
            Rc::clone(&parsed.functions),
            parsed.traits.clone(),
            parsed.impls.clone(),
            parsed.span_map.clone(),
        )
    }

    fn build_semantic_ir(
        &self,
        functions: &[crate::frontend::ast::FunctionDecl],
        type_table: std::collections::HashMap<usize, crate::common::types::Type>,
    ) -> Result<SemanticProgram> {
        build_semantic_ir_program(functions, type_table)
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
        // the production path. Before PR-13e this function
        // reimplemented the codegen inline and never called
        // `verify()`.
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

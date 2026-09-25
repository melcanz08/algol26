// src/compiler.rs

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::compiler::context::{CompilerConfig, CompilerContext};
use crate::compiler::program::{AstPayload, IrState, Program};
use crate::frontend::ast::{
    Expr, ExprId, ExprKind, FunctionDecl, ImplBlock, Stmt, TraitDecl, TypeSyntax,
};
use crate::frontend::lexer::Lexer;
use crate::frontend::module_loader::ModuleLoader;
use crate::frontend::parser::Parser;
use crate::ir::instantiation_plan::InstantiationPlan;
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

/// Output of the canonical frontend normalization.
///
/// Every entry point that reaches semantic analysis must produce its
/// `ParsedProgram` through `prepare_frontend`. Adding a phase to the
/// frontend means adding it here, once.
pub struct FrontendPrep {
    pub parsed: ParsedProgram,
    pub timings: FrontendTimings,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FrontendTimings {
    pub lex: std::time::Duration,
    pub parse: std::time::Duration,
    pub imports: std::time::Duration,
    pub desugar: std::time::Duration,
    pub expand: std::time::Duration,
}

#[derive(Debug, Clone)]
pub struct TypedProgram {
    pub functions: Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    pub type_info: TypeInfo,
    pub type_table_id:
        std::collections::HashMap<crate::frontend::ast::ExprId, crate::common::types::Type>,
    /// The generic specialization plan derived from the analyzer's
    /// instantiation records. Consumed by the IR builder in later
    /// stages of ADR 0013; unused by consumers today.
    pub plan: InstantiationPlan,
}

#[derive(Debug, Default, Clone)]
pub struct TypeInfo {
    pub total_functions: usize,
    pub total_variables: usize,
    pub types_checked: bool,
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

pub fn assert_all_numbered(functions: &[FunctionDecl]) -> bool {
    fn walk_expr(e: &Expr) -> bool {
        if !e.id.is_assigned() {
            return false;
        }
        match &e.kind {
            ExprKind::Block {
                statements,
                trailing_expr,
                ..
            } => {
                statements.iter().all(walk_stmt)
                    && trailing_expr.as_ref().map_or(true, |x| walk_expr(x))
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                walk_expr(condition)
                    && walk_expr(then_branch)
                    && else_branch.as_ref().map_or(true, |x| walk_expr(x))
            }
            ExprKind::Match { value, cases, .. } => {
                walk_expr(value) && cases.iter().all(|c| walk_expr(&c.body))
            }
            ExprKind::Borrow { expr, .. }
            | ExprKind::MutBorrow { expr, .. }
            | ExprKind::Deref { expr, .. }
            | ExprKind::AddrOf { expr, .. }
            | ExprKind::Unary { expr, .. } => walk_expr(expr),
            ExprKind::Some { value, .. }
            | ExprKind::Ok { value, .. }
            | ExprKind::Error { value, .. } => walk_expr(value),
            ExprKind::List(items, _) => items.iter().all(walk_expr),
            ExprKind::ArrayAccess { array, index, .. } => walk_expr(array) && walk_expr(index),
            ExprKind::Binary { left, right, .. } => walk_expr(left) && walk_expr(right),
            ExprKind::FunctionCall { args, .. } => args.iter().all(walk_expr),
            ExprKind::TryCatch {
                try_branch,
                catch_branch,
                finally_body,
                ..
            } => {
                walk_expr(try_branch)
                    && walk_expr(catch_branch)
                    && finally_body
                        .as_ref()
                        .map_or(true, |b| b.iter().all(walk_stmt))
            }
            ExprKind::For {
                iterable,
                body,
                trailing_expr,
                ..
            }
            | ExprKind::While {
                condition: iterable,
                body,
                trailing_expr,
                ..
            } => {
                walk_expr(iterable)
                    && body.iter().all(walk_stmt)
                    && trailing_expr.as_ref().map_or(true, |x| walk_expr(x))
            }
            ExprKind::Range { start, end, .. } => {
                start.as_ref().map_or(true, |x| walk_expr(x))
                    && end.as_ref().map_or(true, |x| walk_expr(x))
            }
            ExprKind::FieldAccess { object, .. } => walk_expr(object),
            _ => true,
        }
    }

    fn walk_stmt(s: &Stmt) -> bool {
        match s {
            Stmt::VarDecl { value, .. } | Stmt::Assign { value, .. } => walk_expr(value),
            Stmt::ArrayAssign { index, value, .. } => walk_expr(index) && walk_expr(value),
            Stmt::Return { value: Some(e), .. } => walk_expr(e),
            Stmt::Print { expr, .. } => walk_expr(expr),
            Stmt::Defer { stmt, .. } => walk_stmt(stmt),
            Stmt::Spawn { body, .. }
            | Stmt::RegionBlock { body, .. }
            | Stmt::UnsafeBlock { body, .. } => body.iter().all(walk_stmt),
            Stmt::Parallel { blocks, .. } => blocks.iter().all(|b| b.iter().all(walk_stmt)),
            Stmt::Send { value, .. } => walk_expr(value),
            Stmt::Expression(e) => walk_expr(e),
            _ => true,
        }
    }

    functions.iter().all(|f| f.body.iter().all(walk_stmt))
}

/// Runs the semantic analyzer and race detector, producing a typed AST.
///
/// Returns `CompileError` (with its original `ErrorCode`) on failure
/// so the pass wrapper can round-trip it through `PassError::cause`.
pub fn type_check_program(
    functions: &Rc<Vec<crate::frontend::ast::FunctionDecl>>,
    traits: &[TraitDecl],
    impls: &[ImplBlock],
) -> Result<TypedProgram> {
    let mut analyzer = SemanticAnalyzer::new();
    debug_assert!(
        assert_all_numbered(functions),
        "type_check_program reached with an UNASSIGNED ExprId — \
         some AST construction path bypassed prepare_frontend"
    );
    analyzer.analyze_with_traits(functions, traits, impls)?;

    let mut race_detector = RaceDetector::new();
    let races = race_detector.analyze(functions);
    if let Some(race) = races.into_iter().next() {
        return Err(CompileError::simple(&race, 0, 0, "", ErrorCode::E0007));
    }

    let type_table_id = analyzer.take_type_table_id();
    let instantiations = analyzer.take_instantiations();
    let mut plan = InstantiationPlan::from_instantiations(&instantiations);
    plan.close(functions);

    Ok(TypedProgram {
        functions: Rc::clone(functions),
        type_info: TypeInfo {
            total_functions: functions.len(),
            total_variables: 0,
            types_checked: true,
        },
        type_table_id,
        plan,
    })
}

/// Build a `SemanticProgram` from the typed AST.
///
/// This is the actual lowering step. Called by
/// `BuildSemanticIRPass`; there is no second implementation to keep
/// in sync.
pub fn build_semantic_ir_program(
    functions: &[crate::frontend::ast::FunctionDecl],
    type_table_id: std::collections::HashMap<
        crate::frontend::ast::ExprId,
        crate::common::types::Type,
    >,
    plan: crate::ir::instantiation_plan::InstantiationPlan,
) -> Result<crate::ir::semantic_ir::SemanticProgram> {
    use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode};
    use crate::semantics::builder::SemanticIRBuilder;

    let (program, diagnostics) = SemanticIRBuilder::build(functions, type_table_id, plan);
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

    /// ADR 0018. One canonical pipeline. Every compilation entry
    /// point runs this sequence to completion:
    ///
    ///     type_check -> type_table_complete -> build_ir
    ///         -> verify -> optimize -> reverify
    ///
    /// Returns the verified IR by value along with the scheduler's
    /// per-stage timings. Callers do target-specific work after
    /// this returns; no caller adds or removes a pass.
    fn run_pipeline(
        &self,
        program: &mut Program,
        ctx: &mut CompilerContext,
    ) -> Result<(VerifiedIR, crate::compiler::scheduler::ScheduleOutcome)> {
        use crate::compiler::passes::build_ir::BuildSemanticIRPass;
        use crate::compiler::passes::optimize::OptimizePass;
        use crate::compiler::passes::type_check::TypeCheckPass;
        use crate::compiler::passes::type_table_complete::TypeTableCompletePass;
        use crate::compiler::passes::verify_ir::{ReVerifyPass, VerifyIrPass};
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(TypeCheckPass)
            .add(TypeTableCompletePass)
            .add(BuildSemanticIRPass)
            .add(VerifyIrPass)
            .add(OptimizePass)
            .add(ReVerifyPass)
            .build()
            .expect("canonical pipeline must build — see ADR 0018");

        let mut outcome = Scheduler::default().run(&pipeline, ctx, program);

        if let Some(err) = outcome.failure.take() {
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

        let verified = match std::mem::replace(&mut program.ir, IrState::Absent) {
            IrState::Verified(v) => v,
            other => {
                program.ir = other;
                return Err(CompileError::simple(
                    "internal: canonical pipeline did not reach verified IR",
                    0,
                    0,
                    "",
                    ErrorCode::E0009,
                ));
            }
        };

        Ok((verified, outcome))
    }

    /// Public wrapper for tests and for callers that want the
    /// verified IR without going through a backend.
    pub fn run_pipeline_for(&mut self, source: &str, filename: &str) -> Result<VerifiedIR> {
        let prep = self.prepare_frontend(source, filename)?;
        let mut program = Program::new(source, filename);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;
        Ok(verified)
    }

    /// The canonical frontend normalization sequence:
    ///
    ///   lex → parse → imports → desugar → expand impls → assign ExprIds
    ///
    /// Returns the resulting AST along with per-phase timings.
    /// Callers that don't surface timings ignore the second field.
    ///
    fn prepare_frontend(&self, source: &str, filename: &str) -> Result<FrontendPrep> {
        use std::time::Instant;

        let start = Instant::now();
        let lexed = self.lex(source)?;
        let lex = start.elapsed();

        let start = Instant::now();
        let parsed = self.parse(lexed)?;
        let parse = start.elapsed();

        let start = Instant::now();
        let parsed = self.process_imports(&parsed, filename)?;
        let imports = start.elapsed();

        let start = Instant::now();
        let parsed = self.desugar(&parsed);
        let desugar = start.elapsed();

        let start = Instant::now();
        let parsed = self.expand_impl_methods(&parsed);
        let expand = start.elapsed();

        // Assign stable ExprId to every node. After this point no AST
        // transformation may construct new Expr nodes; `type_check_program`
        // enforces this.
        let mut functions = (*parsed.functions).clone();
        assign_expr_ids(&mut functions);
        let parsed = ParsedProgram {
            functions: Rc::new(functions),
            traits: parsed.traits,
            impls: parsed.impls,
        };

        Ok(FrontendPrep {
            parsed,
            timings: FrontendTimings {
                lex,
                parse,
                imports,
                desugar,
                expand,
            },
        })
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

        match std::mem::replace(&mut program.ir, IrState::Absent) {
            IrState::Built(p) => Ok(p),
            _ => Err(CompileError::simple(
                "internal: build_ir pass did not produce unverified IR",
                0,
                0,
                "",
                ErrorCode::E0009,
            )),
        }
    }

    pub fn parse_source_for(&mut self, source: &str, filename: &str) -> Result<ParsedProgram> {
        Ok(self.prepare_frontend(source, filename)?.parsed)
    }

    /// Lex a source string. Used by `algol26 inspect --tokens`.
    pub fn lex_source_for(&self, source: &str) -> Result<LexedProgram> {
        self.lex(source)
    }

    /// Frontend through type checking. Used by `inspect --type-table`.
    pub fn type_check_source_for(&mut self, source: &str, filename: &str) -> Result<TypedProgram> {
        let prep = self.prepare_frontend(source, filename)?;
        let parsed = prep.parsed;

        let mut program = Program::new(source, filename);
        program.ast = Some(AstPayload {
            functions: Rc::clone(&parsed.functions),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        });
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        self.run_type_check_pass(&mut program, &mut ctx)?;

        Ok(program
            .typed
            .take()
            .expect("type_check pass left typed in place"))
    }

    /// Runs `BuildSemanticIRPass` through the scheduler. On success,
    /// `program.ir` holds `IrState::Built` with fresh unverified IR.
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
        Ok(())
    }

    pub fn run_interpreter(&mut self, source: &str, filename: &str) -> Result<()> {
        self.run_interpreter_with_args(source, filename, Vec::new())
    }

    /// ADR 0023. Run the program through the interpreter with an
    /// explicit list of program arguments. `args()` inside the
    /// interpreted program returns this list. `run_interpreter`
    /// calls this with an empty list.
    pub fn run_interpreter_with_args(
        &mut self,
        source: &str,
        filename: &str,
        program_args: Vec<String>,
    ) -> Result<()> {
        use crate::backends::backend::Backend;
        use crate::backends::interpreter_backend::InterpreterBackend;

        let prep = self.prepare_frontend(source, filename)?;
        let mut program = Program::new(source, filename);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::interpreter(),
        )?;

        let backend = InterpreterBackend::with_args(program_args);
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
        Ok(())
    }

    /// Runs `TypeTableCompletePass`. Fails if any reachable expression
    /// is missing from the ExprId-keyed type table.
    fn run_type_table_complete_pass(
        &self,
        program: &mut Program,
        ctx: &mut CompilerContext,
    ) -> Result<()> {
        use crate::compiler::passes::type_table_complete::TypeTableCompletePass;
        use crate::compiler::pipeline::Pipeline;
        use crate::compiler::scheduler::Scheduler;

        let pipeline = Pipeline::builder()
            .add(TypeTableCompletePass)
            .build()
            .expect("single-pass pipeline is trivially valid");

        let outcome = Scheduler::default().run(&pipeline, ctx, program);

        if let Some(err) = outcome.failure {
            // Preserve a wrapped CompileError if the pass produced one;
            // otherwise surface the pass message as E0002.
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

        Ok(())
    }

    /// Public wrapper for `inspect --type-table`.
    pub fn run_type_table_complete_pass_public(&self, typed: TypedProgram) -> Result<()> {
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
        use crate::compiler::pass::PassId;
        use std::time::Instant;

        let total_start = Instant::now();

        let mut program = Program::new(source, filename);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        let prep = self.prepare_frontend(source, filename)?;
        let lex_time = prep.timings.lex;
        let parse_time = prep.timings.parse;
        let imports_time = prep.timings.imports;
        let desugar_time = prep.timings.desugar;
        let expand_time = prep.timings.expand;

        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
        });

        // ADR 0018: one canonical pipeline. All passes run to
        // completion; the target-specific work is below.
        let pipeline_start = Instant::now();
        let (verified, outcome) = self.run_pipeline(&mut program, &mut ctx)?;
        let pipeline_time = pipeline_start.elapsed();

        // Target-specific: LLVM lowering + link + optional run.
        let phase_start = Instant::now();
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
            let find = |id: &'static str| {
                outcome
                    .timings
                    .iter()
                    .find(|t| t.pass == PassId(id))
                    .map(|t| t.duration)
                    .unwrap_or(std::time::Duration::ZERO)
            };
            eprintln!("[Timing] Total: {:.2}s", total_time.as_secs_f64());
            eprintln!("  Lex:        {:.4}s", lex_time.as_secs_f64());
            eprintln!("  Parse:      {:.4}s", parse_time.as_secs_f64());
            eprintln!("  Imports:    {:.4}s", imports_time.as_secs_f64());
            eprintln!("  Desugar:    {:.4}s", desugar_time.as_secs_f64());
            eprintln!("  Expand:     {:.4}s", expand_time.as_secs_f64());
            eprintln!("  TypeCheck:  {:.4}s", find("ast.type_check").as_secs_f64());
            eprintln!(
                "  TypeTblChk: {:.4}s",
                find("ast.type_table_complete").as_secs_f64()
            );
            eprintln!("  IR Build:   {:.4}s", find("ir.build").as_secs_f64());
            eprintln!("  Verify(1):  {:.4}s", find("ir.verify").as_secs_f64());
            eprintln!("  Optimize:   {:.4}s", find("ir.optimize").as_secs_f64());
            eprintln!("  Verify(2):  {:.4}s", find("ir.reverify").as_secs_f64());
            eprintln!("  Pipeline:   {:.4}s", pipeline_time.as_secs_f64());
            eprintln!("  Lower:      {:.4}s", lower_time.as_secs_f64());
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

        let prep = self.prepare_frontend(source, filename)?;
        let mut program = Program::new(source, filename);
        let mut ctx = CompilerContext::new(CompilerConfig::default());

        program.ast = Some(AstPayload {
            functions: Rc::clone(&prep.parsed.functions),
            traits: prep.parsed.traits.clone(),
            impls: prep.parsed.impls.clone(),
        });

        let (verified, _outcome) = self.run_pipeline(&mut program, &mut ctx)?;

        crate::backends::capabilities::check_backend(
            verified.program(),
            &crate::backends::capabilities::BackendCapabilities::wasm(),
        )?;

        let backend = WasmBackend::new();
        backend.compile(&verified, output_name)?;

        Ok(())
    }
}

// ─── ExprId numbering ─────────────────────────────────────────────────

/// Assign a unique `ExprId` to every expression node, in preorder,
/// after the last AST transformation. Called by `prepare_frontend`
/// once, after impl expansion, before the AST is handed to semantic
/// analysis.
pub fn assign_expr_ids(functions: &mut [FunctionDecl]) {
    let mut next = 0u32;
    for func in functions.iter_mut() {
        number_stmts(&mut func.body, &mut next);
    }
}

fn number_stmts(stmts: &mut [Stmt], next: &mut u32) {
    for stmt in stmts.iter_mut() {
        number_stmt(stmt, next);
    }
}

fn number_stmt(stmt: &mut Stmt, next: &mut u32) {
    match stmt {
        Stmt::VarDecl { value, .. } => number_expr(value, next),
        Stmt::Assign { value, .. } => number_expr(value, next),
        Stmt::ArrayAssign { index, value, .. } => {
            number_expr(index, next);
            number_expr(value, next);
        }
        Stmt::Return { value: Some(e), .. } => number_expr(e, next),
        Stmt::Print { expr, .. } => number_expr(expr, next),
        Stmt::Defer { stmt, .. } => number_stmt(stmt, next),
        Stmt::Spawn { body, .. }
        | Stmt::RegionBlock { body, .. }
        | Stmt::UnsafeBlock { body, .. } => number_stmts(body, next),
        Stmt::Parallel { blocks, .. } => {
            for b in blocks {
                number_stmts(b, next);
            }
        }
        Stmt::Send { value, .. } => number_expr(value, next),
        Stmt::Expression(e) => number_expr(e, next),
        _ => {}
    }
}

fn number_expr(expr: &mut Expr, next: &mut u32) {
    expr.id = ExprId(*next);
    *next += 1;
    match &mut expr.kind {
        ExprKind::Block {
            statements,
            trailing_expr,
            ..
        } => {
            number_stmts(statements, next);
            if let Some(e) = trailing_expr {
                number_expr(e, next);
            }
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            number_expr(condition, next);
            number_expr(then_branch, next);
            if let Some(e) = else_branch {
                number_expr(e, next);
            }
        }
        ExprKind::Match { value, cases, .. } => {
            number_expr(value, next);
            for c in cases {
                number_expr(&mut c.body, next);
            }
        }
        ExprKind::Borrow { expr, .. }
        | ExprKind::MutBorrow { expr, .. }
        | ExprKind::Deref { expr, .. }
        | ExprKind::AddrOf { expr, .. }
        | ExprKind::Unary { expr, .. } => number_expr(expr, next),
        ExprKind::Some { value, .. }
        | ExprKind::Ok { value, .. }
        | ExprKind::Error { value, .. } => number_expr(value, next),
        ExprKind::List(items, _) => {
            for e in items {
                number_expr(e, next);
            }
        }
        ExprKind::ArrayAccess { array, index, .. } => {
            number_expr(array, next);
            number_expr(index, next);
        }
        ExprKind::Binary { left, right, .. } => {
            number_expr(left, next);
            number_expr(right, next);
        }
        ExprKind::FunctionCall { args, .. } => {
            for e in args {
                number_expr(e, next);
            }
        }
        ExprKind::TryCatch {
            try_branch,
            catch_branch,
            finally_body,
            ..
        } => {
            number_expr(try_branch, next);
            number_expr(catch_branch, next);
            if let Some(body) = finally_body {
                number_stmts(body, next);
            }
        }
        ExprKind::For {
            iterable,
            body,
            trailing_expr,
            ..
        } => {
            number_expr(iterable, next);
            number_stmts(body, next);
            if let Some(e) = trailing_expr {
                number_expr(e, next);
            }
        }
        ExprKind::While {
            condition,
            body,
            trailing_expr,
            ..
        } => {
            number_expr(condition, next);
            number_stmts(body, next);
            if let Some(e) = trailing_expr {
                number_expr(e, next);
            }
        }
        ExprKind::Range { start, end, .. } => {
            if let Some(e) = start {
                number_expr(e, next);
            }
            if let Some(e) = end {
                number_expr(e, next);
            }
        }
        ExprKind::FieldAccess { object, .. } => number_expr(object, next),
        ExprKind::Number(..)
        | ExprKind::Int(..)
        | ExprKind::String(..)
        | ExprKind::Bool(..)
        | ExprKind::NullPtr(..)
        | ExprKind::PtrLiteral(..)
        | ExprKind::Var(..)
        | ExprKind::None(..) => {}
    }
}

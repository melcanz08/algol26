// src/compiler.rs updates for Semantic IR & Defer Lowering Integration

#![allow(dead_code)]
#![allow(unused_variables)]

use crate::backends::ir_codegen::IRCodeGen;
use crate::common::diagnostics::{CompileError, Diagnostic, ErrorCode, Result};
use crate::frontend::ast::Stmt;
use crate::frontend::ast::{ImplBlock, TraitDecl, TypeSyntax};
use crate::frontend::lexer::Lexer;
use crate::frontend::module_loader::ModuleLoader;
use crate::frontend::parser::Parser;
use crate::ir::defer_lowering::DeferLoweringPass;
use crate::ir::monomorphize::Monomorphizer;
use crate::ir::optimizer::Optimizer;
use crate::ir::semantic_ir::SemanticProgram;
use crate::ir::verified_ir::VerifiedIR;
use crate::semantics::race::RaceDetector;
use crate::semantics::semantic::SemanticAnalyzer;
use crate::semantics::semantic_builder::SemanticIRBuilder;
use inkwell::context::Context;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Compiler;

pub struct LexedProgram {
    pub tokens: Vec<crate::frontend::lexer::Token>,
    pub positions: Vec<(usize, usize)>,
}

pub struct ParsedProgram {
    pub functions: Vec<crate::frontend::ast::FunctionDecl>,
    pub span_map: std::collections::HashMap<usize, (usize, usize)>,
    pub traits: Vec<TraitDecl>,
    pub impls: Vec<ImplBlock>,
}

pub struct TypedProgram {
    pub functions: Vec<crate::frontend::ast::FunctionDecl>,
    pub type_info: TypeInfo,
}

pub struct SafeProgram {
    pub functions: Vec<crate::frontend::ast::FunctionDecl>,
    pub safety_report: SafetyReport,
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
pub struct SafetyReport {
    pub bounds_checked: bool,
    pub immutability_checked: bool,
    pub ownership_checked: bool,
    pub issues: Vec<String>,
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

impl Compiler {
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
        let safe = self.safety_check(&typed)?;

        // Phase 9: Build Semantic IR
        let semantic_ir = self.build_semantic_ir(&safe)?;

        // Phase 10: Verify IR
        semantic_ir.verify().map_err(|e| {
            CompileError::new(
                &format!("IR verification failed: {}", e),
                0,
                0,
                "",
                ErrorCode::E0002,
            )
        })?;

        // Phase 13: Lower to WASM backend
        let backend = WasmBackend::new();
        backend.compile(&VerifiedIR::new(semantic_ir.clone())?, output_name)?;

        Ok(())
    }

    pub fn new() -> Self {
        Compiler
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
        let safe = self.safety_check(&typed)?;
        let safety_time = phase_start.elapsed();

        // Phase 9: BUILD SEMANTIC IR
        let phase_start = Instant::now();
        let mut semantic_ir = self.build_semantic_ir(&safe)?;
        let ir_build_time = phase_start.elapsed();

        // Phase 10: VERIFY IR (pre-optimization)
        let phase_start = Instant::now();
        semantic_ir.verify().map_err(|e| {
            CompileError::new(
                &format!("IR verification failed after construction: {}", e),
                0,
                0,
                "",
                ErrorCode::E0002,
            )
        })?;
        let verify_pre_time = phase_start.elapsed();

        // Phase 11: OPTIMIZE
        let phase_start = Instant::now();
        let mut optimizer = Optimizer::new();
        optimizer.optimize(&mut semantic_ir);
        let optimize_time = phase_start.elapsed();

        // Phase 12: VERIFY IR (post-optimization)
        let phase_start = Instant::now();
        semantic_ir.verify().map_err(|e| {
            CompileError::new(
                &format!("IR verification failed after optimization: {}", e),
                0,
                0,
                "",
                ErrorCode::E0002,
            )
        })?;
        let verify_post_time = phase_start.elapsed();

        let optimized_semantic_ir = self.optimize_semantic_ir(semantic_ir)?;

        // Phase 13: LOWER TO BACKEND
        let phase_start = Instant::now();
        self.lower_to_llvm(
            &optimized_semantic_ir,
            &safe,
            filename,
            output_name,
            emit_llvm,
            run_after_compile,
        )?;
        let lower_time = phase_start.elapsed();

        let total_time = total_start.elapsed();

        // Print timing summary (only if compile takes > 1 second)
        if total_time.as_secs() > 1 {
            println!("[Timing] Total: {:.2}s", total_time.as_secs_f64());
            println!("  Lex:        {:.4}s", lex_time.as_secs_f64());
            println!("  Parse:      {:.4}s", parse_time.as_secs_f64());
            println!("  Imports:    {:.4}s", imports_time.as_secs_f64());
            println!("  Desugar:    {:.4}s", desugar_time.as_secs_f64());
            println!("  Expand:     {:.4}s", expand_time.as_secs_f64());
            println!("  Mono:       {:.4}s", mono_time.as_secs_f64());
            println!("  TypeCheck:  {:.4}s", type_check_time.as_secs_f64());
            println!("  Safety:     {:.4}s", safety_time.as_secs_f64());
            println!("  IR Build:   {:.4}s", ir_build_time.as_secs_f64());
            println!("  Verify(1):  {:.4}s", verify_pre_time.as_secs_f64());
            println!("  Optimize:   {:.4}s", optimize_time.as_secs_f64());
            println!("  Verify(2):  {:.4}s", verify_post_time.as_secs_f64());
            println!("  Lower:      {:.4}s", lower_time.as_secs_f64());
        }

        Ok(())
    }
    fn expand_impl_methods(&self, parsed: &ParsedProgram) -> ParsedProgram {
        let mut all_functions = parsed.functions.clone();

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
            functions: all_functions,
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
            functions: specialized_functions,
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
        let mut functions = parsed.functions.clone();
        crate::ir::loop_desugar::desugar_loops(&mut functions);
        ParsedProgram {
            functions,
            span_map: parsed.span_map.clone(),
            traits: parsed.traits.clone(),
            impls: parsed.impls.clone(),
        }
    }

    fn parse(&self, lexed: LexedProgram) -> Result<ParsedProgram> {
        let mut parser = Parser::new_with_positions(lexed.tokens, lexed.positions);
        let program = parser.parse_program()?;
        let (functions, traits, impls) = (program.functions, program.traits, program.impls);
        let span_map = parser.get_span_map().clone();
        Ok(ParsedProgram {
            functions,
            span_map,
            traits,
            impls,
        })
    }

    fn process_imports(&self, parsed: &ParsedProgram, current_file: &str) -> Result<ParsedProgram> {
        let mut loader = ModuleLoader::new();
        let mut all_functions = parsed.functions.clone();

        for func in &parsed.functions {
            for stmt in &func.body {
                if let Stmt::Import { path } = stmt {
                    let resolved = loader.resolve_import(path, current_file)?;
                    let source = loader.load_file(&resolved)?;

                    if !source.is_empty() {
                        // Parse the imported file
                        let lexer = Lexer::new(source.clone()).map_err(|e| {
                            e.display();
                            CompileError::new("Lexing failed in import", 0, 0, "", ErrorCode::E0001)
                        })?;
                        let mut parser = Parser::new_with_positions(lexer.tokens, lexer.positions);
                        let imported_program = parser.parse_program().map_err(|e| {
                            e.display();
                            CompileError::new(
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

                    loader.pop_import();
                }
            }
        }

        Ok(ParsedProgram {
            functions: all_functions,
            span_map: std::collections::HashMap::new(),
            traits: parsed.traits.clone(), // FIXED: Preserve traits
            impls: parsed.impls.clone(),   // FIXED: Preserve impls
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
                CompileError::new("Type checking failed", 0, 0, "", ErrorCode::E0002)
            })?;

        let mut race_detector = RaceDetector::new();
        let races = race_detector.analyze(&parsed.functions);
        if !races.is_empty() {
            for race in races {
                Diagnostic::Warning(race).display();
            }
        }

        Ok(TypedProgram {
            functions: parsed.functions.clone(),
            type_info: TypeInfo {
                total_functions: parsed.functions.len(),
                total_variables: 0,
                types_checked: true,
            },
        })
    }

    fn safety_check(&self, typed: &TypedProgram) -> Result<SafeProgram> {
        // These checks are enforced by SemanticAnalyzer during type_check
        // The SafetyReport reflects what was actually verified
        let report = SafetyReport {
            bounds_checked: true,       // Enforced in SemanticAnalyzer::analyze_expr
            immutability_checked: true, // Enforced in SemanticAnalyzer::analyze_stmt
            ownership_checked: true,    // Enforced via move semantics tracking
            issues: Vec::new(),
        };

        Ok(SafeProgram {
            functions: typed.functions.clone(),
            safety_report: report,
        })
    }

    fn build_semantic_ir(&self, safe: &SafeProgram) -> Result<SemanticProgram> {
        let (mut program, diagnostics) = SemanticIRBuilder::build(&safe.functions);
        if !diagnostics.is_empty() {
            for diag in &diagnostics {
                Diagnostic::Warning(diag.to_string()).display();
            }
            return Err(CompileError::new(
                "Semantic IR construction failed",
                0,
                0,
                "",
                ErrorCode::E0002,
            ));
        }

        // Execute explicit Defer Lowering Pass on the Semantic IR
        let defer_pass = DeferLoweringPass::new();
        let _ = defer_pass.lower(&mut program);

        Ok(program)
    }

    fn optimize_semantic_ir(&self, ir: SemanticProgram) -> Result<SemanticIROptimized> {
        Ok(SemanticIROptimized {
            program: ir,
            optimization_report: OptimizationReport {
                passes_run: vec!["defer_lowering".to_string(), "cfg_verification".to_string()],
                instructions_removed: 0,
            },
        })
    }

    fn lower_to_llvm(
        &self,
        optimized: &SemanticIROptimized,
        _safe_program: &SafeProgram,
        filename: &str,
        output_name: &str,
        emit_llvm: bool,
        run_after_compile: bool,
    ) -> Result<()> {
        let context = Context::create();
        let mut codegen = IRCodeGen::new(&context, "algol26_module");

        codegen.compile(&optimized.program).map_err(|e| {
            e.display();
            CompileError::new("Code generation failed", 0, 0, "", ErrorCode::E0002)
        })?;

        let ir_path = PathBuf::from(output_name).with_extension("ll");
        codegen.module.print_to_file(&ir_path).map_err(|e| {
            let err = CompileError::new(
                &format!("Failed to emit LLVM IR: {}", e),
                0,
                0,
                "",
                ErrorCode::E0001,
            );
            err.display();
            err
        })?;

        println!("[Generated LLVM IR: {}]", ir_path.display());

        if emit_llvm {
            return Ok(());
        }

        let output_path = if Path::new(output_name).is_absolute() {
            PathBuf::from(output_name)
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(output_name)
        };

        let output = Command::new("clang")
            .arg(&ir_path)
            .arg("-o")
            .arg(&output_path)
            .arg("-O2")
            .arg("-lm")
            .arg("-lpthread")
            .output()
            .map_err(|e| {
                let err = CompileError::new(
                    &format!("Failed to run clang: {}", e),
                    0,
                    0,
                    "",
                    ErrorCode::E0001,
                );
                err.display();
                err
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let err = CompileError::new(
                &format!("Linking failed: {}", stderr),
                0,
                0,
                "",
                ErrorCode::E0001,
            );
            err.display();
            return Err(err);
        }

        println!("[Successfully compiled to {}]", output_path.display());

        if run_after_compile {
            let status = Command::new(&output_path).status().map_err(|e| {
                let err =
                    CompileError::new(&format!("Failed to run: {}", e), 0, 0, "", ErrorCode::E0001);
                err.display();
                err
            })?;
            let _ = status;
        }

        Ok(())
    }
}

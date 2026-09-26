// src/backends/interpreter/tests.rs
//
// End-to-end interpreter tests for `rec` records (ADR 0024).

use super::Interpreter;
use crate::compiler::assign_expr_ids;
use crate::frontend::lexer::Lexer;
use crate::frontend::parser::Parser;
use crate::ir::instantiation_plan::InstantiationPlan;
use crate::ir::semantic_ir::SemanticProgram;
use crate::semantics::analyzer::SemanticAnalyzer;
use crate::semantics::builder::SemanticIRBuilder;

/// Run `source` through the full pipeline (lex → parse → analyze →
/// IR → verify → interpret) and return the interpreter's stdout.
fn run_source(source: &str) -> String {
    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect("parse");
    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &functions,
            &program.traits,
            &program.impls,
            &program.records,
        )
        .expect("analyze");

    let type_table = analyzer.take_type_table_id();
    let instantiations = analyzer.take_instantiations();
    let mut plan = InstantiationPlan::from_instantiations(&instantiations);
    plan.close(&functions);

    let (semantic_program, diagnostics): (SemanticProgram, Vec<String>) =
        SemanticIRBuilder::build(&functions, type_table, plan);
    assert!(
        diagnostics.is_empty(),
        "IR build produced diagnostics: {:?}",
        diagnostics
    );

    crate::ir::verifier::verify(&semantic_program).expect("verify");

    let mut interp = Interpreter::new(semantic_program);
    interp.run().expect("interpret")
}

#[test]
fn field_access_reads_value() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    val p := Point { x: 1, y: 2 }
    print(p.x)
    print(p.y)
"#,
    );
    assert_eq!(output, "1\n2");
}

#[test]
fn field_access_assigns_when_var() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    var p := Point { x: 1, y: 2 }
    p.x := 99
    print(p.x)
    print(p.y)
"#,
    );
    assert_eq!(output, "99\n2");
}

#[test]
fn record_pattern_match_destructures() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    val p := Point { x: 1, y: 2 }
    match p
        case Point { x, y }
            print(x + y)
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn record_in_list() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    val pts := [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]
    print(pts[1].x)
"#,
    );
    assert_eq!(output, "3");
}

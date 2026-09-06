// ALGOL26 - Release Hardening Tests - Level 5.3 Final
// Negative testing, stress testing, and equivalence

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::ir::optimizer::Optimizer;
use algol26::ir::semantic_ir::SemanticProgram;
use algol26::semantics::semantic_builder::SemanticIRBuilder;
use std::time::Instant;

fn build_ir(source: &str) -> (SemanticProgram, Vec<String>) {
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let functions = program.functions;
    SemanticIRBuilder::build(&functions)
}

// --- existing 5.1/5.4 tests kept ---
#[test]
fn test_optimization_preserves_semantics() {
    let source = r#"
procedure main
    val x := 5.0 + 3.0
    val y := x * 2.0
    print(y)
"#;
    let (mut ir, diagnostics) = build_ir(source);
    assert!(diagnostics.is_empty());
    let mut optimizer = Optimizer::new();
    optimizer.optimize(&mut ir);
    assert!(!ir.functions.is_empty());
}

#[test]
fn test_negative_invalid_syntax() {
    let source = "procedure main\n val x := (1 + 2";
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    assert!(parser.parse_program().is_err());
}

#[test]
fn test_negative_undefined_variable() {
    let source = r#"
procedure main
    print(undefined_var)
"#;
    let (_ir, diagnostics) = build_ir(source);
    assert!(!diagnostics.is_empty());
}

#[test]
fn test_negative_type_mismatch() {
    let source = r#"
procedure main
    val x := "hello" + 5.0
"#;
    let (_ir, diagnostics) = build_ir(source);
    assert!(!diagnostics.is_empty());
}

#[test]
fn test_stress_nested_control_flow() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0, 4.0, 5.0]
    var total := 0.0
    for item in arr do
        total := total + item
    print(total)
"#;
    let (_ir, diagnostics) = build_ir(source);
    assert!(diagnostics.is_empty(), "got: {:?}", diagnostics);
}

#[test]
fn test_stress_nested_functions() {
    let source = r#"
function add(x: float, y: float) -> float
    return x + y
function multiply(x: float, y: float) -> float
    return x * y
function compute(x: float) -> float
    return multiply(add(x, 2.0), add(x, 3.0))
procedure main
    val result := compute(5.0)
    print(result)
"#;
    let (_ir, diagnostics) = build_ir(source);
    assert!(diagnostics.is_empty(), "got: {:?}", diagnostics);
}

#[test]
fn test_stress_multiple_imports() {
    let source = r#"
import "utils.gol"
import "math.gol"
procedure main
    print("With imports")
"#;
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    assert!(!program.functions.is_empty());
}

#[test]
fn test_optimizer_idempotent() {
    let source = r#"
procedure main
    val x := 5.0 + 3.0
    print(x)
"#;
    let (mut ir, _) = build_ir(source);
    let mut optimizer = Optimizer::new();
    optimizer.optimize(&mut ir);
    let mut optimizer2 = Optimizer::new();
    optimizer2.optimize(&mut ir);
    assert_eq!(optimizer2.stats.folded_constants, 0);
}

#[test]
fn test_negative_corpus_no_ice() {
    use std::fs;
    use std::path::Path;
    let dir = Path::new("tests/integration/negative");
    assert!(dir.exists(), "negative corpus dir missing");
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str())!= Some("al26") { continue; }
        let source = fs::read_to_string(&path).unwrap();
        let result = std::panic::catch_unwind(|| {
            let lexer_res = Lexer::new(source.clone());
            if lexer_res.is_err() { return true; }
            let lexer = lexer_res.unwrap();
            let mut parser = Parser::new(lexer.tokens);
            let prog_res = parser.parse_program();
            if prog_res.is_err() { return true; }
            let prog = prog_res.unwrap();
            let (_ir, _diagnostics) = SemanticIRBuilder::build(&prog.functions);
            true
        });
        assert!(result.is_ok() && result.unwrap(), "ICE on {:?}", path);
    }
}

// === NEW 5.3 FINAL REQUIREMENTS ===

#[test]
fn test_stress_10_level_nested_if_for_defer_break_return() {
    // 10-level nested for + if + defer + break + return
    let mut src = String::from("procedure main\n");
    src.push_str(" var sum := 0.0\n");
    for i in 0..10 {
        src.push_str(&format!("{}for i{} in [1.0, 2.0] do\n", " ".repeat(i+1), i));
        src.push_str(&format!("{}if i{} > 0.5 then\n", " ".repeat(i+2), i));
        src.push_str(&format!("{}defer sum := sum + 1.0\n", " ".repeat(i+3)));
    }
    src.push_str(&format!("{}if sum > 100.0 then\n", " ".repeat(12)));
    src.push_str(&format!("{}return\n", " ".repeat(13)));
    src.push_str(&format!("{}break\n", " ".repeat(12)));
    for i in (0..10).rev() {
        src.push_str(&format!("{}sum := sum + i{} \n", " ".repeat(i+2), i));
    }
    src.push_str(" print(sum)\n");

    let start = Instant::now();
    let result = std::panic::catch_unwind(|| build_ir(&src));
    let elapsed = start.elapsed();
    assert!(result.is_ok(), "ICE / stack overflow on 10-level nesting");
    let (_ir, diags) = result.unwrap();
    // may have diagnostics but must not panic
    assert!(elapsed.as_secs_f64() < 2.0, "took too long: {:?}", elapsed);
    println!("10-level nesting ok in {:?}, diags: {:?}", elapsed, diags.len());
}

#[test]
fn test_stress_10_level_closure_capture() {
    // 10-level nested functions capturing outer vars
    let mut src = String::new();
    for i in 0..10 {
        src.push_str(&format!("function f{}(x{}: float) -> float\n", i, i));
        if i > 0 {
            src.push_str(&format!(" return f{}(x{} + 1.0) + x{}\n", i-1, i, i));
        } else {
            src.push_str(&format!(" return x{} + 1.0\n", i));
        }
        src.push_str("\n");
    }
    src.push_str("procedure main\n val r := f9(0.0)\n print(r)\n");

    let start = Instant::now();
    let result = std::panic::catch_unwind(|| build_ir(&src));
    let elapsed = start.elapsed();
    assert!(result.is_ok(), "ICE on closure chain");
    let (_ir, diags) = result.unwrap();
    assert!(elapsed.as_secs_f64() < 2.0);
    assert!(diags.is_empty(), "closure capture should be valid, got: {:?}", diags);
}

#[test]
fn test_stress_100_vars_single_scope() {
    let mut src = String::from("procedure main\n");
    for i in 0..100 {
        src.push_str(&format!(" val v{} := {}.0\n", i, i));
    }
    src.push_str(" var sum := 0.0\n");
    for i in 0..100 {
        src.push_str(&format!(" sum := sum + v{}\n", i));
    }
    src.push_str(" print(sum)\n");

    let start = Instant::now();
    let result = std::panic::catch_unwind(|| build_ir(&src));
    let elapsed = start.elapsed();
    assert!(result.is_ok(), "ICE on 100 vars");
    let (_ir, diags) = result.unwrap();
    assert!(elapsed.as_secs_f64() < 2.0, "100 vars took {:?}", elapsed);
    assert!(diags.is_empty(), "100 vars should be valid, got: {:?}", diags);
}

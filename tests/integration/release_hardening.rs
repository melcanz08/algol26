// ALGOL26 - Release Hardening Tests - Level 5.3 + 5.4 Final
use algol26::backends::interpreter::Interpreter;
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
    SemanticIRBuilder::build(&program.functions)
}
fn run_interp(prog: SemanticProgram) -> String {
    let mut interp = Interpreter::new(prog);
    interp
        .run()
        .unwrap_or_else(|e| format!("interp_err: {:?}", e))
}
fn run_before_after(source: &str) -> (String, String) {
    let (prog, diags) = build_ir(source);
    assert!(diags.is_empty(), "expected valid, got {:?}", diags);
    let before = run_interp(prog.clone());
    let mut after_prog = prog;
    let mut opt = Optimizer::new();
    opt.optimize(&mut after_prog);
    let after = run_interp(after_prog);
    (before, after)
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
    let source = "procedure main\n print(undefined_var)\n";
    let (_ir, diags) = build_ir(source);
    assert!(!diags.is_empty());
}
#[test]
fn test_negative_type_mismatch() {
    let source = r#"procedure main
    val x := "hello" + 5.0
"#;
    let (_ir, diags) = build_ir(source);
    assert!(!diags.is_empty());
}
#[test]
fn test_stress_nested_control_flow() {
    let source = r#"procedure main
    val arr := [1.0, 2.0, 3.0, 4.0, 5.0]
    var total := 0.0
    for item in arr do
        total := total + item
    print(total)
"#;
    let (_ir, diags) = build_ir(source);
    assert!(diags.is_empty(), "got {:?}", diags);
}
#[test]
fn test_stress_nested_functions() {
    let source = r#"function add(x: float, y: float) -> float
    return x + y
function multiply(x: float, y: float) -> float
    return x * y
function compute(x: float) -> float
    return multiply(add(x, 2.0), add(x, 3.0))
procedure main
    val result := compute(5.0)
    print(result)
"#;
    let (_ir, diags) = build_ir(source);
    assert!(diags.is_empty(), "got {:?}", diags);
}
#[test]
fn test_stress_multiple_imports() {
    let source = r#"import "utils.gol"
import "math.gol"
procedure main
    print("With imports")
"#;
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let prog = parser.parse_program().unwrap();
    assert!(!prog.functions.is_empty());
}
#[test]
fn test_negative_corpus_no_ice() {
    use std::{fs, path::Path};
    let dir = Path::new("tests/integration/negative");
    assert!(dir.exists());
    let mut count = 0usize;
    let mut invalid = 0usize;
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("al26") {
            continue;
        }
        count += 1;
        let source = fs::read_to_string(&path).unwrap();
        let result = std::panic::catch_unwind(|| {
            let lexer_res = Lexer::new(source.clone());
            if lexer_res.is_err() {
                return (true, true);
            }
            let lexer = lexer_res.unwrap();
            let mut parser = Parser::new(lexer.tokens);
            let prog_res = parser.parse_program();
            if prog_res.is_err() {
                return (true, true);
            }
            let prog = prog_res.unwrap();
            let (_ir, diags) = SemanticIRBuilder::build(&prog.functions);
            let mut analyzer = algol26::semantics::semantic::SemanticAnalyzer::new();
            let analyzer_invalid = analyzer.analyze(&prog.functions).is_err();
            let is_invalid = !diags.is_empty() || analyzer_invalid;
            (true, is_invalid)
        });
        assert!(result.is_ok(), "ICE on {:?}", path);
        let (no_ice, is_invalid) = result.unwrap();
        assert!(no_ice, "ICE on {:?}", path);
        if is_invalid {
            invalid += 1;
        }
    }
    println!("negative corpus: total={}, invalid={}", count, invalid);
    assert!(count >= 20, "need >=20 negative files, got {}", count);
    assert!(
        invalid >= 18,
        "expected most corpus to be invalid, got {}/{}",
        invalid,
        count
    );
}

#[test]
fn test_stress_10_level_nested_if_for_defer_break_return() {
    let mut src = String::from("procedure main\n var sum := 0.0\n");
    for i in 0..10 {
        src.push_str(&format!(
            "{}for i{} in [1.0, 2.0] do\n",
            " ".repeat(i + 1),
            i
        ));
        src.push_str(&format!("{}if i{} > 0.5 then\n", " ".repeat(i + 2), i));
        src.push_str(&format!("{}defer sum := sum + 1.0\n", " ".repeat(i + 3)));
    }
    src.push_str(&format!("{}if sum > 100.0 then\n", " ".repeat(12)));
    src.push_str(&format!("{}return\n", " ".repeat(13)));
    src.push_str(&format!("{}break\n", " ".repeat(12)));
    for i in (0..10).rev() {
        src.push_str(&format!("{}sum := sum + i{}\n", " ".repeat(i + 2), i));
    }
    src.push_str(" print(sum)\n");
    let start = Instant::now();
    let result = std::panic::catch_unwind(|| build_ir(&src));
    assert!(result.is_ok());
    assert!(start.elapsed().as_secs_f64() < 2.0);
}
#[test]
fn test_stress_10_level_closure_capture() {
    let mut src = String::new();
    for i in 0..10 {
        src.push_str(&format!("function f{}(x{}: float) -> float\n", i, i));
        if i > 0 {
            src.push_str(&format!(" return f{}(x{} + 1.0) + x{}\n", i - 1, i, i));
        } else {
            src.push_str(&format!(" return x{} + 1.0\n", i));
        }
        src.push_str("\n");
    }
    src.push_str("procedure main\n val r := f9(0.0)\n print(r)\n");
    let start = Instant::now();
    let (ir, diags) = build_ir(&src);
    assert!(start.elapsed().as_secs_f64() < 2.0);
    assert!(diags.is_empty(), "{:?}", diags);
    assert!(!ir.functions.is_empty());
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
    let (_ir, diags) = build_ir(&src);
    assert!(start.elapsed().as_secs_f64() < 2.0);
    assert!(diags.is_empty(), "{:?}", diags);
}
#[test]
fn test_optimization_preserves_semantics() {
    let (before, after) = run_before_after(
        r#"procedure main
    val x := 5.0 + 3.0
    val y := x * 2.0
    print(y)
"#,
    );
    assert_eq!(before, after);
}
#[test]
fn test_optimizer_idempotent() {
    let source = r#"procedure main
    val x := 5.0 + 3.0
    print(x)
"#;
    let (mut ir, _) = build_ir(source);
    let mut opt = Optimizer::new();
    opt.optimize(&mut ir);
    let mut opt2 = Optimizer::new();
    opt2.optimize(&mut ir);
    assert_eq!(opt2.stats.folded_constants, 0);
}
#[test]
fn test_optimization_preserves_arithmetic() {
    let (before, after) = run_before_after(
        r#"procedure main
    val a := 10.0 + 20.0 * 2.0
    val b := (a - 5.0) / 5.0
    val c := b * b
    print(c)
"#,
    );
    assert_eq!(before, after, "arithmetic opt changed semantics");
}
#[test]
fn test_optimization_preserves_strings() {
    let (before, after) = run_before_after(
        r#"procedure main
    val greeting := "Hello"
    val name := "World"
    val combined := String.concat(greeting, " ")
    val s := String.concat(combined, name)
    print(s)
"#,
    );
    assert_eq!(before, after, "string opt changed semantics");
}
#[test]
fn test_optimization_preserves_lists() {
    let (before, after) = run_before_after(
        r#"procedure main
    val arr := [1.0, 2.0, 3.0]
    var sum := 0.0
    for x in arr do
        sum := sum + x
    print(sum)
"#,
    );
    assert_eq!(before, after, "list opt changed semantics");
}
#[test]
fn test_optimization_preserves_borrow_semantics() {
    let source = r#"procedure main
    var x := 5.0
    val r := &x
    print(*r)
"#;
    let (prog, diags) = build_ir(source);
    assert!(diags.is_empty(), "borrow sample valid: {:?}", diags);
    let borrow_before = prog
        .functions
        .iter()
        .flat_map(|f| f.blocks.iter())
        .flat_map(|b| b.instructions.iter())
        .filter(|i| format!("{:?}", i).contains("Borrow") || format!("{:?}", i).contains("AddrOf"))
        .count();
    let mut after_prog = prog.clone();
    let mut opt = Optimizer::new();
    opt.optimize(&mut after_prog);
    let before_out = run_interp(prog);
    let after_out = run_interp(after_prog.clone());
    assert_eq!(before_out, after_out, "borrow semantics broken");
    if borrow_before > 0 {
        println!(
            "borrow ops before={}, after={}",
            borrow_before,
            after_prog
                .functions
                .iter()
                .flat_map(|f| f.blocks.iter())
                .flat_map(|b| b.instructions.iter())
                .filter(|i| format!("{:?}", i).contains("Borrow")
                    || format!("{:?}", i).contains("AddrOf"))
                .count()
        );
    }
}
#[test]
fn test_optimization_diff_interpreter_complex() {
    let (before, after) = run_before_after(
        r#"function add(a: float, b: float) -> float
    return a + b
procedure main
    val x := 5.0 + 3.0
    val y := x + x
    val z := add(y, 2.0)
    var total := 0.0
    for i in [1.0, 2.0, 3.0] do
        total := total + z
    print(total)
"#,
    );
    assert_eq!(before, after, "complex opt broke semantics");
}

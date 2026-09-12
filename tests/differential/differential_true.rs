// tests/differential/differential_true.rs
// ALGOL26 - True Differential Testing
// Runs the same program through LLVM and Interpreter, compares outputs

use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Strip the compiler's informational banner lines so tests can compare
/// only the program's actual stdout between the LLVM and interpreter paths.
fn strip_banners(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw)
        .lines()
        .filter(|l| {
            !l.starts_with("[Compiling ")
                && !l.starts_with("[Output: ")
                && !l.starts_with("[Interpreting ")
        })
        .map(|l| format!("{l}\n"))
        .collect()
}

pub fn run_llvm(source: &str) -> String {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("llvm_diff_{}.gol", id));
    let mut file = std::fs::File::create(&source_path).unwrap();
    file.write_all(source.as_bytes()).unwrap();

    let binary_path = temp_dir.join(format!("llvm_diff_bin_{}", id));

    let output = Command::new(find_compiler())
        .arg(source_path.to_str().unwrap())
        .arg("--output")
        .arg(binary_path.to_str().unwrap())
        .output()
        .expect("Failed to compile with LLVM");

    if !output.status.success() {
        panic!(
            "LLVM compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = Command::new(&binary_path)
        .output()
        .expect("Failed to run LLVM binary");

    let _ = std::fs::remove_file(&source_path);
    let _ = std::fs::remove_file(&binary_path);

    strip_banners(&output.stdout)
}

pub fn run_interpreter(source: &str) -> String {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("interp_diff_{}.gol", id));
    let mut file = std::fs::File::create(&source_path).unwrap();
    file.write_all(source.as_bytes()).unwrap();

    let output = Command::new(find_compiler())
        .arg("run")
        .arg("--interpreter")
        .arg(source_path.to_str().unwrap())
        .output()
        .expect("Failed to run interpreter");

    let _ = std::fs::remove_file(&source_path);

    if !output.status.success() {
        panic!(
            "Interpreter failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    strip_banners(&output.stdout)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_differential_basic_arithmetic() {
    let source = r#"
procedure main
    val x := 10.0
    val y := 20.0
    print(x + y)
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    assert_eq!(llvm_output.trim(), "30.0", "10.0 + 20.0 should be 30.0");
}

#[test]
fn test_differential_array_sum() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0, 4.0, 5.0]
    var total := 0.0

    for item in arr do
        total := total + item

    print(total)
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    assert_eq!(llvm_output.trim(), "15.0", "Array sum should be 15.0");
}

#[test]
fn test_differential_string_output() {
    let source = r#"
procedure main
    val greeting := "Hello"
    print(greeting)
    print("World")
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    let lines: Vec<&str> = llvm_output.lines().collect();
    assert_eq!(lines[0], "Hello");
    assert_eq!(lines[1], "World");
}

#[test]
fn test_differential_boolean_logic() {
    let source = r#"
procedure main
    val a := 10.0
    val b := 20.0

    if a > 5.0 and b > 15.0 then
        print("Both true")

    if a < 5.0 or b > 15.0 then
        print("One true")
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    let lines: Vec<&str> = llvm_output.lines().collect();
    assert_eq!(lines[0], "Both true");
    assert_eq!(lines[1], "One true");
}

#[test]
fn test_differential_functions_with_params() {
    let source = r#"
function add(x: float, y: float) -> float
    return x + y

procedure main
    val result := add(10.0, 32.0)
    print(result)
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    assert_eq!(llvm_output.trim(), "42.0", "10.0 + 32.0 should be 42.0");
}

#[test]
fn test_differential_mixed_types() {
    let source = r#"
procedure main
    val x := 5
    val y := 3.5
    val sum := x + y
    print(sum)
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    assert_eq!(llvm_output.trim(), "8.5", "5 + 3.5 should be 8.5");
}

#[test]
fn test_differential_while_loop() {
    let source = r#"
procedure main
    var counter := 0.0
    while counter < 3.0 do
        print(counter)
        counter := counter + 1.0
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    let lines: Vec<&str> = llvm_output.lines().collect();
    assert_eq!(lines.len(), 3, "Should print 3 lines");
    assert_eq!(lines[0], "0.0");
    assert_eq!(lines[1], "1.0");
    assert_eq!(lines[2], "2.0");
}

#[test]
fn test_differential_nested_calls() {
    let source = r#"
function double(x: float) -> float
    return x * 2.0

function add(x: float, y: float) -> float
    return x + y

procedure main
    val result := double(add(10.0, 11.0))
    print(result)
"#;

    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);

    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree"
    );
    assert_eq!(llvm_output.trim(), "42.0", "double(10.0 + 11.0) should be 42.0");
}

#[test]
fn test_differential_negate_float() {
    let source = r#"
procedure main
    val x := 5.0
    print(-x)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on float negation"
    );
    assert_eq!(llvm_output.trim(), "-5.0", "-5.0 expected");
}

#[test]
fn test_differential_mixed_subtract() {
    let source = r#"
procedure main
    val x := 5
    val y := 2.5
    print(x - y)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on Int - Float"
    );
    assert_eq!(llvm_output.trim(), "2.5", "5 - 2.5 should be 2.5");
}

#[test]
fn test_differential_mixed_multiply() {
    let source = r#"
procedure main
    val x := 4
    val y := 1.5
    print(x * y)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on Int * Float"
    );
    assert_eq!(llvm_output.trim(), "6.0", "4 * 1.5 should be 6.0");
}

#[test]
fn test_differential_mixed_divide() {
    let source = r#"
procedure main
    val x := 10
    val y := 2.0
    print(x / y)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on Int / Float"
    );
    assert_eq!(llvm_output.trim(), "5.0", "10 / 2.0 should be 5.0");
}

#[test]
fn test_differential_defer() {
    let source = r#"
procedure main
    defer print("cleanup")
    print("body")
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on defer"
    );
    assert_eq!(
        llvm_output.trim(),
        "body\ncleanup",
        "defer should print cleanup after body"
    );
}

#[test]
fn test_differential_defer_with_return() {
    let source = r#"
procedure main
    defer print("cleanup")
    print("body")
    return
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on defer with explicit return"
    );
    assert_eq!(
        llvm_output.trim(),
        "body\ncleanup",
        "defer should run before explicit return"
    );
}

#[test]
fn test_differential_int_print() {
    let source = r#"
procedure main
    print(42)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on Int print"
    );
    assert_eq!(llvm_output.trim(), "42", "Int 42 should print as \"42\"");
}

#[test]
fn test_differential_int_print_negative() {
    let source = r#"
procedure main
    print(0 - 7)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on negative Int print"
    );
    assert_eq!(llvm_output.trim(), "-7", "0 - 7 should print as \"-7\"");
}

#[test]
fn test_differential_int_division() {
    let source = r#"
procedure main
    val x := 10
    val y := 3
    print(x / y)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on Int division"
    );
    assert_eq!(
        llvm_output.trim(),
        "3",
        "10 / 3 with Int operands should be 3 (integer division)"
    );
}

#[test]
fn test_differential_int_vs_float_print() {
    let source = r#"
procedure main
    print(5)
    print(5.0)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on Int vs Float print"
    );
    let lines: Vec<&str> = llvm_output.lines().collect();
    assert_eq!(lines[0], "5", "Int 5 should print without a decimal");
    assert_eq!(lines[1], "5.0", "Float 5.0 should print with .0");
}

#[test]
fn test_differential_int_div_by_zero_literal() {
    let source = r#"
procedure main
    print(5 / 0)
"#;
    let (llvm_stdout, llvm_ok) = run_llvm_raw(source);
    let (interp_stdout, interp_ok) = run_interp_raw(source);

    assert!(
        !llvm_ok,
        "LLVM backend should exit non-zero on integer div by zero; stdout: {llvm_stdout:?}"
    );
    assert!(
        !interp_ok,
        "Interpreter should exit non-zero on integer div by zero; stdout: {interp_stdout:?}"
    );
    assert!(
        llvm_stdout.contains("integer division by zero"),
        "LLVM diagnostic missing; got: {llvm_stdout:?}"
    );
    assert!(
        interp_stdout.contains("integer division by zero"),
        "Interpreter diagnostic missing; got: {interp_stdout:?}"
    );
}

#[test]
fn test_differential_int_div_by_zero_runtime() {
    let source = r#"
procedure main
    val x := 10
    val y := 0
    print(x / y)
"#;
    let (llvm_stdout, llvm_ok) = run_llvm_raw(source);
    let (interp_stdout, interp_ok) = run_interp_raw(source);

    assert!(!llvm_ok, "LLVM should exit non-zero; got: {llvm_stdout:?}");
    assert!(!interp_ok, "Interpreter should exit non-zero; got: {interp_stdout:?}");
    assert!(llvm_stdout.contains("integer division by zero"));
    assert!(interp_stdout.contains("integer division by zero"));
}

#[test]
fn test_differential_float_div_by_zero() {
    // IEEE 754: 5.0 / 0.0 is +inf in both. No special handling needed.
    let source = r#"
procedure main
    print(5.0 / 0.0)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on float div by zero"
    );
}

#[test]
fn test_differential_string_length_builtin() {
    let source = r#"
procedure main
    val s := "hello"
    print(String.length(s))
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(interp_output.trim(), llvm_output.trim(), "..." );
    assert_eq!(llvm_output.trim(), "5");
}

#[test]
fn test_differential_list_length_builtin() {
    let source = r#"
procedure main
    val nums := [1.0, 2.0, 3.0]
    print(List.length(nums))
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on List.length"
    );
    assert_eq!(llvm_output.trim(), "3", "List.length of [1.0, 2.0, 3.0] should be 3");
}

#[test]
fn test_differential_method_syntax_no_parens() {
    let source = r#"
procedure main
    val s := "hello"
    print(s.length)
"#;
    let llvm_output = run_llvm(source);
    let interp_output = run_interpreter(source);
    assert_eq!(
        interp_output.trim(),
        llvm_output.trim(),
        "Interpreter and LLVM disagree on bare method syntax"
    );
    assert_eq!(llvm_output.trim(), "5", "\"hello\".length should be 5");
}

#[test]
fn test_differential_method_syntax_parens_matches_bare() {
    let bare = r#"
procedure main
    val s := "hello"
    print(s.length)
"#;
    let parens = r#"
procedure main
    val s := "hello"
    print(s.length())
"#;
    assert_eq!(
        run_llvm(bare).trim(),
        run_llvm(parens).trim(),
        "s.length and s.length() must agree on LLVM"
    );
    assert_eq!(
        run_interpreter(bare).trim(),
        run_interpreter(parens).trim(),
        "s.length and s.length() must agree on interpreter"
    );
}

#[test]
fn test_spawn_llvm_refused() {
    let source = r#"
procedure main
    print("before")
    spawn
        print("spawned")
    print("after")
"#;
    let (stdout, stderr, ok) = run_llvm_full(source);
    assert!(
        !ok,
        "LLVM should refuse spawn; stdout: {stdout:?}, stderr: {stderr:?}"
    );
    // The diagnostic might be on either stream depending on the
    // compiler's error printer. Check both.
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("does not support: spawn"),
        "LLVM refusal should name spawn; combined output: {combined:?}"
    );
}

#[test]
fn test_spawn_interpreter_sequential() {
    let source = r#"
procedure main
    print("before")
    spawn
        print("spawned")
    print("after")
"#;
    let interp = run_interpreter(source);
    assert_eq!(
        interp.trim(),
        "before\nspawned\nafter",
        "interpreter runs spawn sequentially in source order"
    );
}

#[test]
fn test_parallel_llvm_refused() {
    let source = r#"
procedure main
    print("start")
    parallel
        print("A")
        print("B")
    print("end")
"#;
    let (stdout, stderr, ok) = run_llvm_full(source);
    assert!(
        !ok,
        "LLVM should refuse parallel; stdout: {stdout:?}, stderr: {stderr:?}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("does not support: parallel"),
        "LLVM refusal should name parallel; combined output: {combined:?}"
    );
}

#[test]
fn test_parallel_interpreter_sequential() {
    let source = r#"
procedure main
    print("start")
    parallel
        print("A")
        print("B")
    print("end")
"#;
    let interp = run_interpreter(source);
    assert_eq!(
        interp.trim(),
        "start\nA\nB\nend",
        "interpreter runs parallel blocks sequentially in source order"
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn find_compiler() -> PathBuf {
    let candidates = ["target/release/algol26", "target/debug/algol26"];

    for candidate in candidates {
        if std::path::Path::new(candidate).exists() {
            return PathBuf::from(candidate);
        }
    }

    panic!("Compiler binary not found. Run cargo build --release first.");
}

/// Compile + run via LLVM, returning `(stdout, success)`.
/// Does not panic on non-zero exit — the caller decides.
fn run_llvm_raw(source: &str) -> (String, bool) {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("llvm_raw_{}.gol", id));
    std::fs::File::create(&source_path)
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let binary_path = temp_dir.join(format!("llvm_raw_bin_{}", id));

    let compile = Command::new(find_compiler())
        .arg(source_path.to_str().unwrap())
        .arg("--output")
        .arg(binary_path.to_str().unwrap())
        .output()
        .expect("compile invocation failed");

    assert!(
        compile.status.success(),
        "compilation failed unexpectedly: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&binary_path)
        .output()
        .expect("run invocation failed");

    let _ = std::fs::remove_file(&source_path);
    let _ = std::fs::remove_file(&binary_path);

    (
        String::from_utf8_lossy(&run.stdout).to_string(),
        run.status.success(),
    )
}

/// Compile + run via LLVM, capturing stdout, stderr, and success.
/// Unlike `run_llvm_raw`, this does not assert on compile success —
/// callers that expect refusal (capability-boundary tests) can check
/// `!ok` and inspect the streams for the diagnostic.
fn run_llvm_full(source: &str) -> (String, String, bool) {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("llvm_full_{}.gol", id));
    std::fs::File::create(&source_path)
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
    let binary_path = temp_dir.join(format!("llvm_full_bin_{}", id));

    let compile = Command::new(find_compiler())
        .arg(source_path.to_str().unwrap())
        .arg("--output")
        .arg(binary_path.to_str().unwrap())
        .output()
        .expect("compile invocation failed");

    let _ = std::fs::remove_file(&source_path);

    if !compile.status.success() {
        let _ = std::fs::remove_file(&binary_path);
        return (
            String::from_utf8_lossy(&compile.stdout).to_string(),
            String::from_utf8_lossy(&compile.stderr).to_string(),
            false,
        );
    }

    let run = Command::new(&binary_path)
        .output()
        .expect("run invocation failed");
    let _ = std::fs::remove_file(&binary_path);

    (
        String::from_utf8_lossy(&run.stdout).to_string(),
        String::from_utf8_lossy(&run.stderr).to_string(),
        run.status.success(),
    )
}

/// Run via `run --interpreter`, returning `(stdout, success)`.
fn run_interp_raw(source: &str) -> (String, bool) {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("interp_raw_{}.gol", id));
    std::fs::File::create(&source_path)
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();

    let run = Command::new(find_compiler())
        .arg("run")
        .arg("--interpreter")
        .arg(source_path.to_str().unwrap())
        .output()
        .expect("interpreter invocation failed");

    let _ = std::fs::remove_file(&source_path);

    (
        String::from_utf8_lossy(&run.stdout).to_string(),
        run.status.success(),
    )
}

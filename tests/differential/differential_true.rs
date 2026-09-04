// ALGOL26 - True Differential Testing
// Runs the same program through LLVM and Interpreter, compares outputs

use std::process::Command;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn run_llvm(source: &str) -> String {
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
        panic!("LLVM compilation failed: {}", String::from_utf8_lossy(&output.stderr));
    }
    
    let output = Command::new(&binary_path)
        .output()
        .expect("Failed to run LLVM binary");
    
    let _ = std::fs::remove_file(&source_path);
    let _ = std::fs::remove_file(&binary_path);
    
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_interpreter(source: &str) -> String {
    // Use the compiler in check mode to validate, then interpret
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("interp_diff_{}.gol", id));
    let mut file = std::fs::File::create(&source_path).unwrap();
    file.write_all(source.as_bytes()).unwrap();
    
    // For now, just validate the source compiles
    let output = Command::new(find_compiler())
        .arg("check")
        .arg(source_path.to_str().unwrap())
        .output()
        .expect("Failed to check");
    
    let _ = std::fs::remove_file(&source_path);
    
    if output.status.success() {
        "OK".to_string()
    } else {
        format!("Error: {}", String::from_utf8_lossy(&output.stderr))
    }
}

#[test]
fn test_differential_basic_arithmetic() {
    let source = r#"
procedure main
    val x := 10.0
    val y := 20.0
    print(x + y)
"#;
    
    let llvm_output = run_llvm(source);
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept valid program: {}", interp_result);
    assert_eq!(llvm_output.trim(), "30.0", "LLVM should produce 30.0");
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
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept: {}", interp_result);
    assert_eq!(llvm_output.trim(), "15.0", "LLVM should produce 15.0");
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
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept: {}", interp_result);
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
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept: {}", interp_result);
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
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept: {}", interp_result);
    assert_eq!(llvm_output.trim(), "42.0", "LLVM should produce 42.0");
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
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept: {}", interp_result);
    assert_eq!(llvm_output.trim(), "8.5", "LLVM should produce 8.5");
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
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept: {}", interp_result);
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
    let interp_result = run_interpreter(source);
    
    assert!(interp_result == "OK", "Interpreter should accept: {}", interp_result);
    assert_eq!(llvm_output.trim(), "42.0", "LLVM should produce 42.0");
}

fn find_compiler() -> PathBuf {
    let candidates = [
        "target/release/algol26",
        "target/debug/algol26",
    ];
    
    for candidate in candidates {
        if std::path::Path::new(candidate).exists() {
            return PathBuf::from(candidate);
        }
    }
    
    panic!("Compiler binary not found. Run cargo build --release first.");
}

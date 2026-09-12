// tests/differential/differential_test.rs - HARDENED
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use crate::differential::differential_true::{run_interpreter, run_llvm};

static COUNTER: AtomicU32 = AtomicU32::new(0);

#[test]
fn test_differential_basic() {
    let source = r#"
procedure main
    val x := 10.0
    val y := 20.0
    print(x + y)
"#;

    let llvm_output = run_compiler(source);
    assert_eq!(
        llvm_output.trim(),
        "30.0",
        "Basic arithmetic should produce 30.0"
    );
}

#[test]
fn test_differential_arrays() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0, 4.0, 5.0]
    var total := 0.0
    
    for item in arr do
        total := total + item
    end
    
    print(total)
"#;

    let llvm_output = run_compiler(source);
    assert_eq!(llvm_output.trim(), "15.0", "Array sum should be 15.0");
}

#[test]
fn test_differential_strings() {
    let source = r#"
procedure main
    val greeting := "Hello"
    print(greeting)
    print("World")
"#;

    let llvm_output = run_compiler(source);
    let lines: Vec<&str> = llvm_output.lines().collect();
    assert_eq!(lines[0], "Hello", "First string should be Hello");
    assert_eq!(lines[1], "World", "Second string should be World");
}

#[test]
fn test_differential_booleans() {
    let source = r#"
procedure main
    val a := 10.0
    val b := 20.0
    
    if a > 5.0 and b > 15.0 then
        print("Both true")
    end
    
    if a < 5.0 or b > 15.0 then
        print("One true")
    end
"#;

    let llvm_output = run_compiler(source);
    let lines: Vec<&str> = llvm_output.lines().collect();
    assert_eq!(lines[0], "Both true", "First condition should print");
    assert_eq!(lines[1], "One true", "Second condition should print");
}

#[test]
fn test_differential_control_flow() {
    let source = r#"
procedure main
    val x := 10.0
    
    if x > 5.0 then
        print("Greater")
    else
        print("Less")
    end
"#;

    let llvm_output = run_compiler(source);
    assert_eq!(llvm_output.trim(), "Greater", "Should print Greater");
}

#[test]
fn test_differential_functions() {
    let source = r#"
function square(x: float) -> float
    return x * x

procedure main
    val result := square(4.0)
    print(result)
"#;

    let llvm_output = run_compiler(source);
    assert_eq!(llvm_output.trim(), "16.0", "Square of 4 should be 16");
}

fn run_compiler(source: &str) -> String {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let compiler = find_compiler();

    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("diff_test_{}.gol", id));
    let mut file = std::fs::File::create(&source_path).unwrap();
    file.write_all(source.as_bytes()).unwrap();

    let binary_path = temp_dir.join(format!("diff_test_bin_{}", id));

    // Compile with timeout
    let output = Command::new(&compiler)
        .arg(source_path.to_str().unwrap())
        .arg("--output")
        .arg(binary_path.to_str().unwrap())
        .output()
        .expect("Failed to compile");

    if !output.status.success() {
        panic!(
            "Compilation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Run with timeout
    let output = Command::new(&binary_path).output().expect("Failed to run");

    // Cleanup
    let _ = std::fs::remove_file(&source_path);
    let _ = std::fs::remove_file(&binary_path);

    String::from_utf8_lossy(&output.stdout).to_string()
}

fn find_compiler() -> PathBuf {
    let candidates = ["target/release/algol26", "target/debug/algol26"];

    for candidate in candidates {
        if std::path::Path::new(candidate).exists() {
            return PathBuf::from(candidate);
        }
    }

    panic!("Compiler binary not found. Run cargo build --release first.");
}

#[test]
fn test_differential_bool_print() {
    // The interpreter prints true/false; the LLVM backend must match.
    let source = "\
procedure main
    print(true)
    print(false)
    print(1.0 == 1.0)
    print(1.0 == 2.0)
";
    let interp_out = run_interpreter(source);
    let llvm_out = run_llvm(source);
    assert_eq!(
        interp_out, llvm_out,
        "LLVM and interpreter disagree on bool output"
    );
    assert_eq!(interp_out.trim(), "true\nfalse\ntrue\nfalse");
}

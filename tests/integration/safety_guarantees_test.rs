// tests/integration/safety_guarantees_test.rs

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

#[test]
fn test_memory_safety_guarantees() {
    // Test use-after-free prevention
    let source = r#"
procedure main
    var x := 10.0
    free(&x)  // Should fail - can't free stack variable
    print(x)
"#;
    let result = compile_and_check(source);
    assert!(!result.success, "Use-after-free should be rejected");
}

#[test]
fn test_ownership_transfer() {
    // Test move semantics for non-Copy types
    let source = r#"
procedure main
    val s := "hello"
    val t := s    // Move
    print(t)      // Valid
"#;
    let result = compile_and_check(source);
    assert!(result.success, "Move should work correctly: {}", result.stderr);
}

#[test]
fn test_borrow_lifetime() {
    // Borrow should not outlive the borrowed value
    let source = r#"
function get_ref() -> &float
    val x := 10.0
    return &x  // ERROR: Returning reference to local variable
"#;
    let result = compile_and_check(source);
    assert!(!result.success, "Returning reference to local should be rejected");
}

#[test]
fn test_concurrent_safety() {
    // Race condition should be detected
    let source = r#"
procedure main
    var counter := 0.0
    
    parallel do
        counter := counter + 1.0
    end
    
    parallel do
        counter := counter + 1.0
    end
"#;
    let result = compile_and_check(source);
    assert!(!result.success, "Race condition in parallel should be detected");
}

fn compile_and_check(source: &str) -> CompileResult {
    use std::io::Write;
    
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("safety_test_{}.gol", id));
    let mut file = std::fs::File::create(&source_path).unwrap();
    file.write_all(source.as_bytes()).unwrap();
    
    let compiler = find_compiler();
    let output = Command::new(&compiler)
        .arg(source_path.to_str().unwrap())
        .output()
        .expect("Failed to run compiler");
    
    let _ = std::fs::remove_file(&source_path);
    
    CompileResult {
        success: output.status.success(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

fn find_compiler() -> std::path::PathBuf {
    for candidate in ["target/release/algol26", "target/debug/algol26"] {
        if std::path::Path::new(candidate).exists() {
            return std::path::PathBuf::from(candidate);
        }
    }
    panic!("Compiler not found. Run cargo build --release first.");
}

struct CompileResult {
    success: bool,
    stderr: String,
}
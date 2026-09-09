use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

// Semantic validation tests - prove safety guarantees
static COUNTER: AtomicU32 = AtomicU32::new(0);

#[test]
fn test_type_safety_guaranteed() {
    let source = r#"
procedure main
    var x := 10.0
    x := "hello"
"#;

    let result = compile_and_check(source);
    assert!(
        !result.success,
        "Type mismatch should be rejected: {}",
        result.stderr
    );
}

#[test]
fn test_immutability_guaranteed() {
    let source = r#"
procedure main
    val x := 10.0
    x := 20.0
"#;

    let result = compile_and_check(source);
    assert!(
        !result.success,
        "Mutation of val should be rejected: {}",
        result.stderr
    );
}

#[test]
fn test_bounds_guaranteed() {
    let source = r#"
procedure main
    val arr := [1.0, 2.0, 3.0]
    print(arr[10])
"#;

    let result = compile_and_check(source);
    assert!(
        !result.success,
        "Out-of-bounds should be rejected: {}",
        result.stderr
    );
}

#[test]
fn test_string_move_guaranteed() {
    // Strings are Move types, so this should fail
    let source = r#"
procedure main
    val s := "hello"
    val t := s    // Move
    print(s)      // ERROR: Use after move
"#;

    let result = compile_and_check(source);
    assert!(
        !result.success,
        "Use-after-move for String should be rejected: {}",
        result.stderr
    );
}

#[test]
fn test_float_copy_valid() {
    // Floats are Copy types, so this should succeed
    let source = r#"
procedure main
    var x := 10.0
    var y := x    // Copy
    print(x)      // Valid
    print(y)
"#;

    let result = compile_and_check(source);
    assert!(
        result.success,
        "Float copy should be valid: {}",
        result.stderr
    );
}

#[test]
fn test_valid_program_accepted() {
    let source = r#"
procedure main
    val x := 10.0
    val y := 20.0
    print(x + y)
"#;

    let result = compile_and_check(source);
    assert!(
        result.success,
        "Valid program should compile: {}",
        result.stderr
    );
}

#[test]
fn test_borrow_rules_enforced() {
    // Double mutable borrow should fail
    let source = r#"
procedure main
    var x := 10.0
    var y := &mut x
    var z := &mut x
"#;

    let result = compile_and_check(source);
    assert!(
        !result.success,
        "Double mutable borrow should be rejected: {}",
        result.stderr
    );
}

#[test]
fn test_borrow_scope_release() {
    // Borrow should be released at scope end
    let source = r#"
procedure main
    var x := 10.0
    
    if true then
        val y := &mut x
        print(y)
    end
    
    var z := &mut x  // Valid - y's borrow ended
    print(z)
"#;

    let result = compile_and_check(source);
    assert!(
        result.success,
        "Borrow should be released at scope end: {}",
        result.stderr
    );
}

#[test]
fn test_race_detection() {
    // Race condition should be detected
    let source = r#"
procedure main
    var shared := 0.0
    
    spawn
        print(shared)  // Read in spawn
    end
    
    shared := 20.0     // Write in main
"#;

    let result = compile_and_check(source);
    assert!(
        !result.success,
        "Race condition should be detected: {}",
        result.stderr
    );
}

fn compile_and_check(source: &str) -> CompileResult {
    use std::io::Write;

    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let temp_dir = std::env::temp_dir();
    let source_path = temp_dir.join(format!("semantic_test_{}.gol", id));
    let mut file = std::fs::File::create(&source_path).unwrap();
    file.write_all(source.as_bytes()).unwrap();

    let compiler = find_compiler();
    let output = Command::new(&compiler)
        .arg(source_path.to_str().unwrap())
        .output()
        .expect("Failed to run compiler");

    // Cleanup
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

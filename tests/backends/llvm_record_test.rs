// tests/backends/llvm_record_test.rs
//
// Verifies that record declarations lower to named LLVM struct
// types. ADR 0036 L1. The test compiles a source through the full
// pipeline with the LLVM backend and inspects the emitted `.ll`.

use algol26::backends::backend::Backend;
use algol26::backends::llvm_backend::LlvmBackend;
use std::io::Read;

/// Compile `source` through the LLVM backend and return the emitted
/// `.ll` text.
///
/// Uses the public `Compiler` API to run the full pipeline, then
/// invokes the backend directly and reads the `.ll` file it writes.
fn llvm_ir_for(source: &str) -> String {
    use algol26::compiler::Compiler;
    use std::path::PathBuf;

    let mut compiler = Compiler::new();
    let out_name = format!(
        "/tmp/algol26_llvm_record_test_{}",
        std::process::id()
    );

    // Full pipeline; the backend writes `<out_name>.ll`.
    compiler
        .compile(source, "test.gol", &out_name, false, false, false)
        .expect("LLVM compilation failed");

    let ll_path = PathBuf::from(format!("{}.ll", out_name));
    let mut text = String::new();
    std::fs::File::open(&ll_path)
        .expect("LLVM backend did not write a .ll file")
        .read_to_string(&mut text)
        .expect("failed to read .ll");
    let _ = std::fs::remove_file(&ll_path);
    text
}

#[test]
fn record_type_lowers_to_named_struct() {
    let source = "
rec Point
    x: Int
    y: Int

procedure main
    val p := Point { x: 1, y: 2 }
    print(p.x)
";
    let ir = llvm_ir_for(source);
    assert!(
        ir.contains("%Point = type { i64, i64 }"),
        "expected named struct type for Point, got:\n{}",
        ir
    );
}

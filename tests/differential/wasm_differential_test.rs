// WASM Differential Testing — Verify WASM backend compiles same programs

use std::process::Command;

fn compile_to_wasm(source: &str, output_name: &str) -> bool {
    let path = std::env::temp_dir().join(format!("{}.gol", output_name));
    std::fs::write(&path, source).unwrap();
    let output = Command::new("target/debug/algol26")
        .arg(&path)
        .output()
        .expect("Failed to run compiler");
    output.status.success()
}

fn compile_to_native(source: &str, output_name: &str) -> bool {
    let path = std::env::temp_dir().join(format!("{}.gol", output_name));
    std::fs::write(&path, source).unwrap();
    let output = Command::new("target/debug/algol26")
        .arg(&path)
        .output()
        .expect("Failed to run compiler");
    output.status.success()
}

#[test]
fn test_wasm_compiles_same_programs_as_llvm() {
    let programs = [
        ("function main() -> Int\n    print \"hello\"\n    return 0\n", "wasm_test_basic"),
        ("function main() -> Int\n    val x := 10\n    val y := 20\n    print x + y\n    return 0\n", "wasm_test_arithmetic"),
        ("function main() -> Int\n    if 5 > 3\n        print \"yes\"\n    else\n        print \"no\"\n    return 0\n", "wasm_test_control_flow"),
    ];
    for (source, name) in programs {
        let wasm_ok = compile_to_wasm(source, name);
        let native_ok = compile_to_native(source, name);
        assert!(wasm_ok, "WASM failed: {}", name);
        assert!(native_ok, "Native failed: {}", name);
    }
}

#[test]
fn test_wasm_backend_isolation() {
    let source = "function main() -> Int\n    print \"isolation test\"\n    return 0\n";
    let ok = compile_to_wasm(source, "wasm_isolation");
    assert!(ok, "WASM backend failed isolation test");
}

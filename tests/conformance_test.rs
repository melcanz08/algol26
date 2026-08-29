use std::process::Command;

fn compile_valid(program: &str) -> bool {
    let output = Command::new("cargo")
        .args(["run", "--release", "--", program, "--emit-llvm"])
        .output()
        .expect("Failed to run compiler");
    
    output.status.success()
}

fn compile_invalid(program: &str) -> bool {
    let output = Command::new("cargo")
        .args(["run", "--release", "--", program])
        .output()
        .expect("Failed to run compiler");
    
    !output.status.success()
}

#[test]
fn test_valid_programs() {
    let valid_dir = "tests/programs/valid";
    let entries = std::fs::read_dir(valid_dir).unwrap();
    
    let mut count = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().unwrap_or_default() == "gol" {
            let program = path.to_str().unwrap();
            assert!(
                compile_valid(program),
                "Valid program failed to compile: {}",
                program
            );
            count += 1;
        }
    }
    
    assert!(count > 0, "No valid test programs found");
    println!("✅ {} valid programs passed", count);
}

#[test]
fn test_invalid_programs() {
    let invalid_dir = "tests/programs/invalid";
    let entries = std::fs::read_dir(invalid_dir).unwrap();
    
    let mut count = 0;
    for entry in entries {
        let path = entry.unwrap().path();
        if path.extension().unwrap_or_default() == "gol" {
            let program = path.to_str().unwrap();
            assert!(
                compile_invalid(program),
                "Invalid program should have failed: {}",
                program
            );
            count += 1;
        }
    }
    
    assert!(count > 0, "No invalid test programs found");
    println!("✅ {} invalid programs correctly rejected", count);
}

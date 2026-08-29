// ALGOL26 - Binary entry point

use algol26::compiler::Compiler;
use algol26::diagnostics::{CompileError, ErrorCode};
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    let first_arg = &args[1];
    
    if first_arg == "--help" || first_arg == "-h" {
        print_usage();
        std::process::exit(0);
    }
    
    if first_arg == "--version" || first_arg == "-v" {
        println!("ALGOL26 Compiler v0.7.0");
        std::process::exit(0);
    }
    
    let (command, filename) = match first_arg.as_str() {
        "check" => {
            if args.len() < 3 {
                eprintln!("Error: 'check' requires a filename");
                std::process::exit(1);
            }
            ("check", args[2].clone())
        }
        "build" => {
            if args.len() < 3 {
                eprintln!("Error: 'build' requires a filename");
                std::process::exit(1);
            }
            ("build", args[2].clone())
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("Error: 'run' requires a filename");
                std::process::exit(1);
            }
            ("run", args[2].clone())
        }
        "wasm" => {
            if args.len() < 3 {
                eprintln!("Error: 'wasm' requires a filename");
                std::process::exit(1);
            }
            ("wasm", args[2].clone())
        }
        _ => {
            ("build", first_arg.clone())
        }
    };
    
    if !filename.ends_with(".gol") {
        eprintln!("Warning: Expected .gol file extension");
    }
    
    let source = match fs::read_to_string(&filename) {
        Ok(content) => content,
        Err(e) => {
            let err = CompileError::new(
                &format!("Failed to read file '{}': {}", filename, e),
                0, 0, "",
                ErrorCode::E0001,
            );
            err.display();
            std::process::exit(1);
        }
    };

    let remaining_args: Vec<&String> = args.iter().skip(if args.len() > 2 && (first_arg == "check" || first_arg == "build" || first_arg == "run") { 3 } else { 2 }).collect();
    
    let emit_llvm = remaining_args.iter().any(|a| a.as_str() == "--emit-llvm");
    let run = command == "run" || remaining_args.iter().any(|a| a.as_str() == "--run");
    
    let output_name = remaining_args.iter()
        .position(|a| a.as_str() == "--output" || a.as_str() == "-o")
        .and_then(|i| remaining_args.get(i + 1))
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let input_path = Path::new(&filename);
            let parent = input_path.parent().unwrap_or(Path::new("."));
            let stem = input_path.file_stem().unwrap_or_default();
            parent.join(stem).to_string_lossy().to_string()
        });

    match command {
        "check" => println!("[Checking {}]", filename),
        "build" => {
            println!("[Compiling {}]", filename);
            println!("[Output: {}]", output_name);
        }
        "run" => {
            println!("[Compiling and running {}]", filename);
            println!("[Output: {}]", output_name);
        }
        _ => {}
    }
    
    if command == "wasm" {
        // Use WASM backend
        println!("[Compiling to WASM: {}]", filename);
        
        // Parse the source and compile through WASM backend
        let mut compiler = Compiler::new();
        if let Err(e) = compiler.compile_to_wasm(&source, &filename, &output_name) {
            e.display();
            std::process::exit(1);
        }
    } else {
        // Use default LLVM backend
        let mut compiler = Compiler::new();
        if let Err(e) = compiler.compile(&source, &filename, &output_name, emit_llvm, run) {
            e.display();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("ALGOL26 Compiler v0.7.0");
    println!("=========================");
    println!();
    println!("Usage: algol26 <command> [options]");
    println!();
    println!("Commands:");
    println!("  check <file.gol>       Type-check only");
    println!("  build <file.gol>       Compile to executable (default)");
    println!("  wasm <file.gol>        Compile to WebAssembly");
    println!("  run <file.gol>         Compile and run immediately");
    println!("  <file.gol>             Same as 'build'");
    println!();
    println!("Options:");
    println!("  --emit-llvm            Only generate LLVM IR");
    println!("  --run                  Run after compilation");
    println!("  --output, -o NAME      Specify output name");
    println!("  --version, -v          Show version");
    println!("  --help, -h             Show this help");
    println!();
    println!("Examples:");
    println!("  algol26 check hello.gol");
    println!("  algol26 build hello.gol");
    println!("  algol26 run hello.gol");
    println!("  algol26 hello.gol");
}

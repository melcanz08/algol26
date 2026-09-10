// src/main.rs - ALGOL26 - Binary entry point

use algol26::common::diagnostics::{CompileError, ErrorCode};
use algol26::compiler::Compiler;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    // Extract known flags first, so they don't get mistaken for
    // positional arguments (filename, output, etc.).
    let use_interpreter = args.iter().any(|a| a == "--interpreter");
    let emit_llvm = args.iter().any(|a| a == "--emit-llvm");
    let run_flag = args.iter().any(|a| a == "--run");

    // Positional args: everything that isn't a recognized flag or
    // the value of a flag that takes one.
    let mut positional: Vec<String> = Vec::new();
    let mut i = 1;
    let mut output_from_cli: Option<String> = None;
    while i < args.len() {
        let a = &args[i];
        if a == "--interpreter"
            || a == "--emit-llvm"
            || a == "--run"
            || a == "--help"
            || a == "-h"
            || a == "--version"
            || a == "-v"
        {
            i += 1;
            continue;
        }
        if a == "--output" || a == "-o" {
            if i + 1 < args.len() {
                output_from_cli = Some(args[i + 1].clone());
            }
            i += 2;
            continue;
        }
        positional.push(a.clone());
        i += 1;
    }

    // Handle --help / --version now that we know they appeared.
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_usage();
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("ALGOL26 Compiler v{}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    // Determine command + filename from the positional args.
    let (command, filename) = match positional.first().map(|s| s.as_str()) {
        Some("check") => {
            let file = positional.get(1)
                .cloned()
                .unwrap_or_else(|| { eprintln!("Error: 'check' requires a filename"); std::process::exit(1); });
            ("check", file)
        }
        Some("build") => {
            let file = positional.get(1)
                .cloned()
                .unwrap_or_else(|| { eprintln!("Error: 'build' requires a filename"); std::process::exit(1); });
            ("build", file)
        }
        Some("run") => {
            let file = positional.get(1)
                .cloned()
                .unwrap_or_else(|| { eprintln!("Error: 'run' requires a filename"); std::process::exit(1); });
            ("run", file)
        }
        Some("wasm") => {
            let file = positional.get(1)
                .cloned()
                .unwrap_or_else(|| { eprintln!("Error: 'wasm' requires a filename"); std::process::exit(1); });
            ("wasm", file)
        }
        Some(other) => ("build", other.to_string()),
        None => {
            print_usage();
            std::process::exit(1);
        }
    };

    let run = command == "run" || run_flag;

    let output_name = output_from_cli.unwrap_or_else(|| {
        let input_path = Path::new(&filename);
        let parent = input_path.parent().unwrap_or(Path::new("."));
        let stem = input_path.file_stem().unwrap_or_default();
        parent.join(stem).to_string_lossy().to_string()
    });

    if !filename.ends_with(".gol") {
        eprintln!("Warning: Expected .gol file extension");
    }

    // Load the source file. All command variants — check, build, run,
    // wasm, interpreter — need it.
    let source = match fs::read_to_string(&filename) {
        Ok(content) => content,
        Err(e) => {
            let err = CompileError::simple(
                &format!("Failed to read file '{}': {}", filename, e),
                0, 0, "", ErrorCode::E0001,
            );
            err.display();
            std::process::exit(1);
        }
    };


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
        println!("[Compiling to WASM: {}]", filename);
        let mut compiler = Compiler::new();
        if let Err(e) = compiler.compile_to_wasm(&source, &filename, &output_name) {
            e.display();
            std::process::exit(1);
        }
    } else if use_interpreter {
        println!("[Interpreting {}]", filename);
        let mut compiler = Compiler::new();
        if let Err(e) = compiler.run_interpreter(&source, &filename) {
            e.display();
            std::process::exit(1);
        }
    } else {
        let mut compiler = Compiler::new();
        if let Err(e) = compiler.compile(&source, &filename, &output_name, emit_llvm, run) {
            e.display();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("ALGOL26 Compiler v{}", env!("CARGO_PKG_VERSION"));
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
    println!("  --interpreter          Run through the interpreter (skips LLVM)");
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

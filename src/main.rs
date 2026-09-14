// src/main.rs - ALGOL26 - Binary entry point

use algol26::common::diagnostics::{CompileError, ErrorCode};
use algol26::compiler::Compiler;
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    // `inspect` has its own sub-flags and bypasses the compile path
    // entirely. Intercept before the generic flag parser runs so
    // `--passes`, `--tokens`, etc. don't get pushed into positional.
    if args.get(1).map(|s| s.as_str()) == Some("inspect") {
        let rest: Vec<&str> = args[2..].iter().map(|s| s.as_str()).collect();
        run_inspect(&rest);
        std::process::exit(0);
    }
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
    println!("  inspect [sub] [file]   Inspect compiler internals");
    println!("  <file.gol>             Same as 'build'");
    println!();
    println!("Inspect subcommands:");
    println!("  --passes               List registered passes and their contracts");
    println!("  --capabilities         Print the feature × backend capability matrix");
    println!("  --type-table <file.gol> Check the analyzer's type table for completeness");
    println!("  --tokens <file.gol>    Dump lexer tokens");
    println!("  --ast    <file.gol>    Dump parsed AST (before type checking)");
    println!("  --ir     <file.gol>    Dump semantic IR (after type checking)");
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
    println!("  algol26 inspect --passes");
    println!("  algol26 inspect --ir hello.gol");
}

// ─── inspect ─────────────────────────────────────────────────────────

fn run_inspect(args: &[&str]) {
    let mut passes = false;
    let mut tokens = false;
    let mut ast = false;
    let mut ir = false;
    let mut file: Option<String> = None;
    let mut capabilities = false;
    let mut type_table = false;

    for a in args {
        match *a {
            "--passes" => passes = true,
            "--tokens" => tokens = true,
            "--ast" => ast = true,
            "--ir" => ir = true,
            "--capabilities" => capabilities = true,
            "--type-table" => type_table = true,
            "--help" | "-h" => {
                print_inspect_usage();
                return;
            }
            other if other.starts_with("--") => {
                eprintln!("Error: unknown inspect flag '{}'", other);
                print_inspect_usage();
                std::process::exit(1);
            }
            other => {
                if file.is_some() {
                    eprintln!("Error: inspect takes at most one file");
                    std::process::exit(1);
                }
                file = Some(other.to_string());
            }
        }
    }

    if passes {
        inspect_passes();
        return;
    }

    if capabilities {
        inspect_capabilities();
        return;
    }

    let Some(path) = file else {
        eprintln!("Error: inspect needs a file, or --passes / --capabilities");
        print_inspect_usage();
        std::process::exit(1);
    };

    let source = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("Error: cannot read '{}': {}", path, e);
        std::process::exit(1);
    });
    let filename = Path::new(&path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.clone());

    let mut compiler = Compiler;

    if tokens {
        inspect_tokens(&compiler, &source);
    } else if ast {
        inspect_ast(&mut compiler, &source, &filename);
    } else if ir {
        inspect_ir(&mut compiler, &source, &filename);
    } else if type_table {
        inspect_type_table(&mut compiler, &source, &filename);
    } else {
        eprintln!("Error: inspect needs one of --passes, --capabilities, --type-table, --tokens, --ast, --ir");
        print_inspect_usage();
        std::process::exit(1);
    }
}

fn inspect_passes() {
    use algol26::compiler::passes::build_ir::BuildSemanticIRPass;
    use algol26::compiler::passes::optimize::OptimizePass;
    use algol26::compiler::passes::type_check::TypeCheckPass;
    use algol26::compiler::passes::verify_ir::VerifyIrPass;
    use algol26::compiler::program::Program;
    use algol26::compiler::registry::PassRegistry;

    let mut reg: PassRegistry<Program> = PassRegistry::new();
    reg.register(TypeCheckPass);
    reg.register(BuildSemanticIRPass);
    reg.register(OptimizePass);
    reg.register(VerifyIrPass);

    println!("Registered passes:");
    for c in reg.contracts() {
        println!();
        println!("  {} [{}]", c.id, match c.kind {
            algol26::compiler::pass::PassKind::Analysis    => "analysis",
            algol26::compiler::pass::PassKind::Transform   => "transform",
            algol26::compiler::pass::PassKind::Verification => "verify",
            algol26::compiler::pass::PassKind::Lowering    => "lowering",
            algol26::compiler::pass::PassKind::Annotation  => "annotation",
        });
        println!("    level:         {} -> {}", c.input, c.output);
        println!("    may_fail:      {}", c.may_fail);
        if !c.requires.is_empty() {
            println!("    requires:");
            for r in c.requires { println!("      - {}", r); }
        }
        if !c.guarantees.is_empty() {
            println!("    guarantees:");
            for g in c.guarantees { println!("      - {}", g); }
        }
        if !c.must_preserve.is_empty() {
            println!("    must_preserve:");
            for m in c.must_preserve { println!("      - {}", m); }
        }
    }
}

fn inspect_tokens(compiler: &Compiler, source: &str) {
    match compiler.lex_source_for(source) {
        Ok(lexed) => {
            println!("{} token(s):", lexed.tokens.len());
            for (i, st) in lexed.tokens.iter().enumerate() {
                println!(
                    "  {:>4}  {}:{}  {:?}",
                    i,
                    st.span.start_line,
                    st.span.start_column,
                    st.token
                );
            }
        }
        Err(e) => {
            e.display();
            std::process::exit(1);
        }
    }
}

fn inspect_ast(compiler: &mut Compiler, source: &str, filename: &str) {
    match compiler.parse_source_for(source, filename) {
        Ok(parsed) => {
            println!("{} function(s):", parsed.functions.len());
            for f in parsed.functions.iter() {
                println!();
                println!("{:#?}", f);
            }
            if !parsed.traits.is_empty() {
                println!("\n{} trait(s):", parsed.traits.len());
                for t in &parsed.traits { println!("  {:?}", t); }
            }
            if !parsed.impls.is_empty() {
                println!("\n{} impl block(s):", parsed.impls.len());
                for i in &parsed.impls { println!("  {:?}", i); }
            }
        }
        Err(e) => {
            e.display();
            std::process::exit(1);
        }
    }
}

fn inspect_ir(compiler: &mut Compiler, source: &str, filename: &str) {
    match compiler.build_semantic_ir_for(source, filename) {
        Ok(ir) => {
            println!("{:#?}", ir);
        }
        Err(e) => {
            e.display();
            std::process::exit(1);
        }
    }
}

fn inspect_capabilities() {
    use algol26::compiler::capabilities::CapabilityMatrix;
    let m = CapabilityMatrix::standard();
    print!("{}", m.render_table());
}

fn inspect_type_table(compiler: &mut Compiler, source: &str, filename: &str) {
    match compiler.type_check_source_for(source, filename) {
        Ok(typed) => {
            println!("{} function(s)", typed.functions.len());
            println!("{} type_table entries", typed.type_table.len());
            let warnings = match compiler.run_type_table_complete_pass_public(typed) {
                Ok(w) => w,
                Err(e) => {
                    e.display();
                    std::process::exit(1);
                }
            };
            if warnings == 0 {
                println!("✓ type table complete");
            } else {
                println!("⚠ {} warning(s) above", warnings);
            }
        }
        Err(e) => {
            e.display();
            std::process::exit(1);
        }
    }
}

fn print_inspect_usage() {
    eprintln!("Usage: algol26 inspect [--passes|--capabilities] [--tokens|--ast|--ir] [file.gol]");
    eprintln!();
    eprintln!("  --passes         list registered compiler passes and their contracts");
    eprintln!("  --capabilities   print the feature × backend capability matrix");
    eprintln!("  --type-table <file>  check the analyzer's type table for completeness");
    eprintln!("  --tokens <file>  dump lexer tokens");
    eprintln!("  --ast    <file>  dump the parsed AST (before type checking)");
    eprintln!("  --ir     <file>  dump the semantic IR (after type checking)");
}
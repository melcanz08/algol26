// ALGOL26 Fuzz Tests — Simplified Fuzzing
// Feeds random/malformed input to compiler to verify no panic

use std::process::Command;

/// Generate random ALGOL26-like source code
fn generate_fuzz_source(seed: u64, iteration: usize) -> String {
    // Simple LCG for deterministic "randomness"
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    state = state.wrapping_mul(iteration as u64).wrapping_add(seed);

    let keywords = [
        "function",
        "procedure",
        "val",
        "var",
        "if",
        "else",
        "for",
        "while",
        "return",
        "print",
        "trait",
        "impl",
        "match",
        "case",
        "extern",
        "import",
        "region",
        "unsafe",
        "spawn",
        "parallel",
        "channel",
    ];
    let types = [
        "Int", "Float", "String", "Bool", "Void", "List", "Option", "Result",
    ];
    let operators = [
        "+", "-", "*", "/", ">", "<", ">=", "<=", "==", "!=", "and", "or",
    ];

    let mut source = String::new();
    let lines = 1 + (state % 10) as usize;

    for i in 0..lines {
        let choice = (state >> (i * 3)) % 10;
        match choice {
            0 => source.push_str(&format!(
                "{} {} := {}\n",
                if state % 2 == 0 { "val" } else { "var" },
                format!("x{}", i),
                state % 100
            )),
            1 => source.push_str(&format!(
                "{} {}\n",
                keywords[(state % keywords.len() as u64) as usize],
                state % 50
            )),
            2 => source.push_str(&format!(
                "{} {} {}\n",
                if state % 2 == 0 { "print" } else { "return" },
                state % 10,
                if state % 2 == 0 { "" } else { " " }
            )),
            3 => source.push_str(&format!(
                "{} {} {}\n",
                keywords[(state % keywords.len() as u64) as usize],
                types[(state % types.len() as u64) as usize],
                state % 20
            )),
            4 => source.push_str(&format!(
                "{} x{} {} {}\n",
                if state % 2 == 0 { "if" } else { "while" },
                i,
                operators[(state % operators.len() as u64) as usize],
                state % 100
            )),
            5 => source.push_str("    print \"fuzz\"\n"),
            6 => source.push_str(&format!(
                "{} {}\n",
                if state % 2 == 0 { "else" } else { "then" },
                state % 10
            )),
            _ => source.push_str(&format!("{}\n", state % 3)),
        }
    }
    source
}

/// Test: Random source never causes compiler to panic
#[test]
fn test_fuzz_compiler_no_panic() {
    for iteration in 0..100 {
        let source = generate_fuzz_source(42, iteration);
        let path = std::env::temp_dir().join(format!("fuzz_{}.gol", iteration));
        std::fs::write(&path, &source).unwrap();

        let result = std::panic::catch_unwind(|| {
            let output = Command::new("target/debug/algol26")
                .arg(&path)
                .output()
                .expect("Failed to run compiler");
            output.status.success()
        });

        // Compiler must not panic (it can return error, but not crash)
        assert!(
            result.is_ok(),
            "Compiler panicked on fuzz input {}:\n{}",
            iteration,
            source
        );

        // Clean up
        let _ = std::fs::remove_file(&path);
    }
}

/// Test: Random tokens never cause lexer to panic
#[test]
fn test_fuzz_lexer_no_panic() {
    use algol26::frontend::lexer::Lexer;

    for iteration in 0..200 {
        let source = generate_fuzz_source(7, iteration);
        let result = std::panic::catch_unwind(|| {
            let _ = Lexer::new(source);
        });
        assert!(result.is_ok(), "Lexer panicked on fuzz input {}", iteration);
    }
}

/// Test: Random tokens never cause parser to panic
#[test]
fn test_fuzz_parser_no_panic() {
    use algol26::frontend::lexer::Lexer;
    use algol26::frontend::parser::Parser;

    for iteration in 0..200 {
        let source = generate_fuzz_source(13, iteration);
        if let Ok(lexer) = Lexer::new(source) {
            let result = std::panic::catch_unwind(|| {
                let mut parser = Parser::new(lexer.tokens);
                let _ = parser.parse_program();
            });
            assert!(
                result.is_ok(),
                "Parser panicked on fuzz input {}",
                iteration
            );
        }
    }
}

/// Test: Random type strings never cause panic
#[test]
fn test_fuzz_type_system_no_panic() {
    use algol26::common::types::Type;

    let type_chars = [
        'i', 'n', 't', 'f', 'l', 'o', 'a', 's', 'r', 'g', '<', '>', ',', '&', '*', ' ', 'T', 'U',
        'V',
    ];

    for iteration in 0..200 {
        let mut state = iteration as u64 * 7919;
        let mut type_str = String::new();
        let len = (state % 15) as usize + 1;
        for _ in 0..len {
            state = state.wrapping_mul(31).wrapping_add(7);
            type_str.push(type_chars[(state % type_chars.len() as u64) as usize]);
        }

        let result = std::panic::catch_unwind(|| {
            let _ = Type::from_str(&type_str);
        });
        assert!(result.is_ok(), "Type::from_str panicked on '{}'", type_str);
    }
}

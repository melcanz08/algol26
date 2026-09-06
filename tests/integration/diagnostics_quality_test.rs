use algol26::common::diagnostics::ErrorCode;
use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::semantic::SemanticAnalyzer;
use std::fs;

fn analyze_source(src: &str) -> Result<(), algol26::common::diagnostics::CompileError> {
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze(&program.functions)
}

#[test]
fn test_diag_undefined_var_has_code_and_name() {
    let src = r#"
function main()
    print unknownVar
"#;
    let err = analyze_source(src).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::E0003);
    assert!(err.message.contains("unknownVar") || err.to_string().contains("unknownVar"));
    assert!(err.suggestion.is_some(), "should have help text");
}

#[test]
fn test_diag_type_mismatch_has_expected() {
    let src = r#"
function main()
    var x: Int := 3.14
"#;
    let err = analyze_source(src).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::E0002);
    assert!(err.message.to_lowercase().contains("type") || err.message.contains("mismatch"));
}

#[test]
fn test_diag_double_borrow_e0007_with_help() {
    let src = fs::read_to_string("tests/integration/negative/double_borrow.al26").unwrap();
    let err = analyze_source(&src).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::E0007);
    assert!(err.message.contains("arr"), "should mention variable name");
    assert!(err.suggestion.is_some());
}

#[test]
fn test_diag_use_after_move_has_line() {
    let src = fs::read_to_string("tests/integration/negative/use_after_move.al26").unwrap();
    let err = analyze_source(&src).unwrap_err();
    assert_eq!(err.error_code, ErrorCode::E0007);
    assert!(err.message.contains("moved") || err.message.contains("Use of moved"));
    assert!(err.suggestion.is_some());
}

#[test]
fn test_all_negative_corpus_produce_structured_errors() {
    for path in fs::read_dir("tests/integration/negative").unwrap() {
        let path = path.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("al26") {
            continue;
        }
        let src = fs::read_to_string(&path).unwrap();
        let lexer = match Lexer::new(src.clone()) {
            Ok(l) => l,
            Err(e) => {
                assert_eq!(e.error_code, ErrorCode::E0001);
                continue;
            }
        };
        let mut parser = Parser::new(lexer.tokens);
        let prog = match parser.parse_program() {
            Ok(p) => p,
            Err(e) => {
                assert!(matches!(e.error_code, ErrorCode::E0001 | ErrorCode::E0002));
                continue;
            }
        };
        let mut analyzer = SemanticAnalyzer::new();
        match analyzer.analyze(&prog.functions) {
            Ok(_) => {
                if path.to_str().unwrap().contains("double_borrow")
                    || path.to_str().unwrap().contains("use_after_move")
                {
                    panic!("Expected error for {:?}, got Ok", path);
                }
            }
            Err(e) => {
                assert!(!e.message.is_empty(), "empty message for {:?}", path);
                assert!(
                    e.error_code.as_str().starts_with('E'),
                    "no error code for {:?}",
                    path
                );
                println!(
                    "{:?} => {} [{}] help: {:?}",
                    path,
                    e.message,
                    e.error_code.as_str(),
                    e.suggestion
                );
            }
        }
    }
}

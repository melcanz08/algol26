use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;

#[test]
fn test_parse_simple_ffi() {
    let source = r#"
extern "C" function sqrt(x: Float) -> Float from "m"
"#;
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let (functions, _, _) = parser.parse_program().unwrap();
    
    assert_eq!(functions.len(), 1);
    let func = &functions[0];
    assert!(func.is_extern);
    
    let ffi = func.ffi_info.as_ref().unwrap();
    assert_eq!(ffi.abi, "C");
    assert_eq!(ffi.library, "m");
}

#[test]
fn test_parse_symbol_renaming() {
    let source = r#"
extern "C" function my_func(x: Int) -> Int from "libcustom" as "real_c_name"
"#;
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let (functions, _, _) = parser.parse_program().unwrap();
    
    let ffi = functions[0].ffi_info.as_ref().unwrap();
    assert_eq!(ffi.symbol_name, Some("real_c_name".to_string()));
    assert_eq!(ffi.library, "libcustom");
}
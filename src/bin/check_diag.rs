use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;
use algol26::semantics::semantic_builder::SemanticIRBuilder;
use std::fs;

fn main() {
    for path in std::fs::read_dir("tests/integration/negative").unwrap() {
        let path = path.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("al26") { continue; }
        let src = fs::read_to_string(&path).unwrap();
        println!("\n=== {:?} ===", path);
        let lexer = match Lexer::new(src.clone()) {
            Ok(l) => l,
            Err(e) => { println!("LEX ERR: {:?}", e); continue; }
        };
        let mut parser = Parser::new(lexer.tokens);
        match parser.parse_program() {
            Ok(p) => {
                let (_ir, diags) = SemanticIRBuilder::build(&p.functions);
                if diags.is_empty() { println!("NO DIAGS (would be ICE risk)"); }
                for d in &diags { println!("  DIAG: {}", d); }
            },
            Err(e) => println!("PARSE ERR: {:?}", e),
        }
    }
}

use algol26::lexer::Lexer;

fn main() {
    let source = "function identity<T>(x: T) -> T\n    return x\n";
    let lexer = Lexer::new(source.to_string()).unwrap();
    for (i, token) in lexer.tokens.iter().enumerate() {
        println!("{:3}: {:?}", i, token);
    }
}

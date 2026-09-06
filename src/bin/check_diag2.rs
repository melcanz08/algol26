use algol26::compiler::Compiler;
use std::fs;
fn main() {
    for path in ["tests/integration/negative/double_borrow.al26","tests/integration/negative/use_after_move.al26"] {
        let src = fs::read_to_string(path).unwrap();
        let mut c = Compiler::new();
        println!("\n=== {} ===", path);
        match c.compile(&src, path, "/tmp/out", false, false) {
            Ok(_) => println!("COMPILE OK (BUG - should error)"),
            Err(e) => { println!("COMPILE ERR (expected):"); e.display(); }
        }
    }
}

// ADR 0054: indentation form and brace form produce the same AST.
// Each test parses two versions of the same program and asserts
// the ASTs are identical.

use algol26::frontend::lexer::Lexer;
use algol26::frontend::parser::Parser;

/// Parse `src` and return a normalized debug form of the AST with
/// every `Span { ... }` replaced by `<span>`. The two notations
/// necessarily produce different spans at block boundaries — an
/// indented body begins at its first statement's line, a brace
/// body begins at its `{` — so comparing spans directly is
/// comparing positions, not structure. We want to assert that the
/// AST *shape* is identical.
fn parse(src: &str) -> String {
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let raw = format!("{:?}", program);
    strip_spans(&raw)
}

fn strip_spans(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if raw[i..].starts_with("Span {") {
            // Skip `Span {` and consume balanced braces.
            i += "Span {".len();
            let mut depth = 1i32;
            while i < bytes.len() && depth > 0 {
                match bytes[i] as char {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                i += 1;
            }
            out.push_str("<span>");
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[test]
fn proc_body_brace_matches_indent() {
    let indented = "
proc main
    val x := 1
    print(x)
";
    let braced = "
proc main {
    val x := 1
    print(x)
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn if_brace_matches_indent() {
    let indented = "
proc main
    if x > 0 then
        print(1)
    else
        print(2)
";
    let braced = "
proc main {
    if x > 0 {
        print(1)
    } else {
        print(2)
    }
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn for_brace_matches_indent() {
    let indented = "
proc main
    for x in [1, 2, 3]
        print(x)
";
    let braced = "
proc main {
    for x in [1, 2, 3] {
        print(x)
    }
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn rec_brace_matches_indent() {
    let indented = "
rec Point
    x: Int
    y: Int
";
    let braced = "
rec Point {
    x: Int
    y: Int
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn enum_brace_matches_indent() {
    let indented = "
enum Tier
    Gold
    Silver
";
    let braced = "
enum Tier {
    Gold
    Silver
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn impl_brace_matches_indent() {
    let indented = "
impl Point
    fn x(self: &Point) -> Int
        self.x
";
    let braced = "
impl Point {
    fn x(self: &Point) -> Int {
        self.x
    }
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn match_brace_matches_indent() {
    let indented = "
proc main
    match d
        case A
            1
        case B
            2
";
    let braced = "
proc main {
    match d {
        case A then {
            1
        }
        case B then {
            2
        }
    }
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn proc_trailing_expr_brace_matches_indent() {
    let indented = "
proc filter(students: List<Int>) -> List<Int>
    val out: List<Int> := []
    out
";
    let braced = "
proc filter(students: List<Int>) -> List<Int> {
    val out: List<Int> := []
    out
}
";
    assert_eq!(parse(indented), parse(braced));
}

#[test]
fn val_append_in_loop_brace_matches_indent() {
    let indented = "
proc honors_only(students: List<Int>) -> List<Int>
    val honors: List<Int> := []
    for s in students
        if s >= 90
            honors.append(s)
    honors
";
    let braced = "
proc honors_only(students: List<Int>) -> List<Int> {
    val honors: List<Int> := []
    for s in students {
        if s >= 90 {
            honors.append(s)
        }
    }
    honors
}
";
    assert_eq!(parse(indented), parse(braced));
}

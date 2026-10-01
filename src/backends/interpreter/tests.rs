// src/backends/interpreter/tests.rs
//
// End-to-end interpreter tests for `rec` records (ADR 0024),
// `Map<K, V>` (ADR 0027), `List.append` (ADR 0028), and traits.

use super::Interpreter;
use crate::compiler::Compiler;

/// Run `source` through the full production pipeline — lex, parse,
/// process imports, desugar, expand impl methods, assign ExprIds,
/// analyze, build IR, verify — then interpret. This exercises the
/// same frontend the CLI does, including the impl-method expansion
/// that renames `impl Show for Point`'s `show` to `Point_show`
/// before the builder looks it up.
fn run_source(source: &str) -> String {
    let mut compiler = Compiler::new();
    let verified = compiler
        .run_pipeline_for(source, "test.gol")
        .expect("pipeline should reach verified IR");
    let semantic_program = verified.program().clone();
    let mut interp = Interpreter::new(semantic_program);
    interp.run().expect("interpret")
}

#[test]
fn field_access_reads_value() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    val p := Point { x: 1, y: 2 }
    print(p.x)
    print(p.y)
"#,
    );
    assert_eq!(output, "1\n2");
}

#[test]
fn field_access_assigns_when_var() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    var p := Point { x: 1, y: 2 }
    p.x := 99
    print(p.x)
    print(p.y)
"#,
    );
    assert_eq!(output, "99\n2");
}

#[test]
fn record_pattern_match_destructures() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    val p := Point { x: 1, y: 2 }
    match p
        case Point { x, y }
            print(x + y)
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn record_in_list() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    val pts := [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]
    print(pts[1].x)
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn map_insert_and_get_roundtrip() {
    let output = run_source(
        r#"
procedure main
    var m := Map<String, Int> {}
    m.insert("a", 1)
    m.insert("b", 2)
    val x := m.get("a")
    match x
        case Some(v)
            print(v)
        case None
            print(0)
"#,
    );
    assert_eq!(output, "1");
}

#[test]
fn map_get_missing_key_returns_none() {
    let output = run_source(
        r#"
procedure main
    val m := Map { "a": 1 }
    val x := m.get("z")
    match x
        case Some(v)
            print(v)
        case None
            print(0)
"#,
    );
    assert_eq!(output, "0");
}

#[test]
fn map_insert_overwrites_existing_key() {
    let output = run_source(
        r#"
procedure main
    var m := Map<String, Int> {}
    m.insert("a", 1)
    m.insert("a", 99)
    val x := m.get("a")
    match x
        case Some(v)
            print(v)
        case None
            print(0)
"#,
    );
    assert_eq!(output, "99");
}

#[test]
fn map_contains_returns_bool() {
    let output = run_source(
        r#"
procedure main
    val m := Map { "a": 1, "b": 2 }
    print(m.contains("a"))
    print(m.contains("z"))
"#,
    );
    assert_eq!(output, "true\nfalse");
}

#[test]
fn map_keys_returns_all_keys() {
    let output = run_source(
        r#"
procedure main
    val m := Map { "b": 1, "a": 2, "c": 3 }
    val ks := m.keys()
    print(ks.length())
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn map_values_returns_all_values() {
    let output = run_source(
        r#"
procedure main
    val m := Map { "a": 1, "b": 2, "c": 3 }
    val vs := m.values()
    print(vs.length())
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn map_length_counts_entries() {
    let output = run_source(
        r#"
procedure main
    val m := Map { "a": 1, "b": 2 }
    print(m.length())
"#,
    );
    assert_eq!(output, "2");
}

#[test]
fn map_with_int_keys() {
    let output = run_source(
        r#"
procedure main
    var m := Map<Int, String> {}
    m.insert(1, "one")
    m.insert(2, "two")
    val x := m.get(2)
    match x
        case Some(v)
            print(v)
        case None
            print("missing")
"#,
    );
    assert_eq!(output, "two");
}

#[test]
fn map_with_bool_keys() {
    let output = run_source(
        r#"
procedure main
    var m := Map<Bool, Int> {}
    m.insert(true, 1)
    m.insert(false, 0)
    val x := m.get(true)
    match x
        case Some(v)
            print(v)
        case None
            print(99)
"#,
    );
    assert_eq!(output, "1");
}

#[test]
fn map_of_maps() {
    let output = run_source(
        r#"
procedure main
    var inner := Map<String, Int> {}
    inner.insert("x", 100)
    inner.insert("y", 200)

    var outer := Map<String, Map<String, Int>> {}
    outer.insert("nested", inner)

    val retrieved := outer.get("nested")
    match retrieved
        case Some(inner_map)
            val x := inner_map.get("x")
            match x
                case Some(v)
                    print(v)
                case None
                    print(0)
        case None
            print(-1)
"#,
    );
    assert_eq!(output, "100");
}

#[test]
fn map_iteration_via_keys() {
    let output = run_source(
        r#"
procedure main
    var m := Map<String, Int> {}
    m.insert("a", 1)
    m.insert("b", 2)
    m.insert("c", 3)
    var total := 0
    for k in m.keys()
        val v := m.get(k)
        match v
            case Some(n)
                total := total + n
            case None
                total := total
    print(total)
"#,
    );
    assert_eq!(output, "6");
}

#[test]
fn map_values_can_be_lists() {
    let output = run_source(
        r#"
procedure main
    var m := Map<String, List<Int>> {}
    m.insert("a", [1, 2, 3])
    m.insert("b", [4, 5])

    val x := m.get("a")
    match x
        case Some(list)
            print(list.length())
        case None
            print(0)
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn append_grows_list_by_one() {
    let output = run_source(
        r#"
procedure main
    var xs := [1, 2]
    xs.append(3)
    print(xs.length())
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn append_to_empty_annotated_list() {
    let output = run_source(
        r#"
procedure main
    var xs: List<Int> := []
    xs.append(1)
    xs.append(2)
    print(xs.length())
"#,
    );
    assert_eq!(output, "2");
}

#[test]
fn append_moves_non_copy_value() {
    // Appending a String moves it into the list. This test just
    // verifies the runtime grows; the move semantics are
    // enforced by the analyzer.
    let output = run_source(
        r#"
procedure main
    var names := ["a", "b"]
    names.append("c")
    print(names.length())
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn append_copies_copy_value() {
    let output = run_source(
        r#"
procedure main
    var xs := [1, 2]
    val x := 3
    xs.append(x)
    print(xs.length())
    print(x)
"#,
    );
    assert_eq!(output, "3\n3");
}

#[test]
fn append_to_nested_list() {
    let output = run_source(
        r#"
procedure main
    var outer := [[1, 2]]
    outer.append([3, 4])
    print(outer.length())
"#,
    );
    assert_eq!(output, "2");
}

#[test]
fn filter_via_append() {
    let output = run_source(
        r#"
procedure main
    val source := [1, 2, 3, 4, 5, 6]
    var small: List<Int> := []
    for x in source
        if x < 3
            small.append(x)
    print(small.length())
"#,
    );
    assert_eq!(output, "2");
}

#[test]
fn append_invalidates_static_length() {
    // Before append, the analyzer knew xs had 3 elements. After
    // append, the check is dropped, so xs[3] is accepted at
    // compile time and succeeds at runtime.
    let output = run_source(
        r#"
procedure main
    var xs := [10, 20, 30]
    xs.append(40)
    print(xs[3])
"#,
    );
    assert_eq!(output, "40");
}

#[test]
fn int_division_then_compare_diagnostic() {
    let output = run_source(
        r#"
procedure main
    val x := 5
    print(x / 2)
    print(x - (x / 2) * 2)
"#,
    );
    assert_eq!(output, "2\n1");
}

#[test]
fn diag_if_append_no_loop() {
    let output = run_source(
        r#"
procedure main
    var small: List<Int> := []
    val x := 2
    if x < 3
        small.append(x)
    print(small.length())
"#,
    );
    assert_eq!(output, "1");
}

#[test]
fn diag_for_append_no_if() {
    let output = run_source(
        r#"
procedure main
    val source := [1, 2, 3]
    var small: List<Int> := []
    for x in source
        small.append(x)
    print(small.length())
"#,
    );
    assert_eq!(output, "3");
}

#[test]
fn append_inside_match_case() {
    let output = run_source(
        r#"
procedure main
    var small: List<Int> := []
    val x := 2
    match x
        case 1
            small.append(100)
        case 2
            small.append(200)
        case _
            small.append(0)
    print(small.length())
    print(small[0])
"#,
    );
    assert_eq!(output, "1\n200");
}

#[test]
fn var_decl_call_evaluates_once() {
    let output = run_source(
        r#"
function make() -> Int
    print("making")
    return 42

procedure main
    val x := make()
    print(x)
"#,
    );
    assert_eq!(output, "making\n42");
}

#[test]
fn unused_var_decl_call_side_effect_preserved() {
    let output = run_source(
        r#"
function make() -> Int
    print("side effect")
    return 42

procedure main
    val unused := make()
    print("done")
"#,
    );
    assert_eq!(output, "side effect\ndone");
}

#[test]
fn field_access_binds_to_var() {
    // `val q := p.x` must preserve the field's type through
    // TypedIRValue::FieldAccess::type_of(). Without the FieldAccess
    // arm in type_of(), the declared type of `q` is Unknown.
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

procedure main
    val p := Point { x: 1, y: 2 }
    val q := p.x
    print(q + 10)
"#,
    );
    assert_eq!(output, "11");
}

#[test]
fn record_method_call_dispatches_to_impl() {
    let output = run_source(
        r#"
rec Point
    x: Int
    y: Int

trait Show
    function show() -> String

impl Show for Point
    function show() -> String
        return "(" + Int.to_string(self.x) + "," + Int.to_string(self.y) + ")"

procedure main
    val p := Point { x: 1, y: 2 }
    print(p.show())
"#,
    );
    assert_eq!(output, "(1,2)");
}

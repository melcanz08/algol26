// src/semantics/analyzer/tests.rs

use super::*;
use crate::frontend::lexer::Lexer;
use crate::frontend::parser::Parser;

#[cfg(test)]
fn analyze(source: &str) -> Result<()> {
    let lexer = Lexer::new(source.to_string())?;
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program()?;

    // Number the AST before semantic analysis — this helper bypasses
    // `prepare_frontend`, so `assign_expr_ids` doesn't run automatically.
    let mut functions = program.functions;
    crate::compiler::assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_spans(
        &functions,
        &program.traits,
        &program.impls,
        &program.records,
        &program.distinct_decls,
        &program.enum_decls,
        &program.subrange_decls,
    )
}

#[test]
fn test_mut_borrow_released_at_scope_exit() {
    // Borrow in the inner block should be released before the second
    // borrow at the outer scope.
    let source = "\
proc main
    var x := 5.0
    region r
        var y := &mut x
    var z := &mut x
";
    analyze(source).expect("scope-exit release should allow re-borrow");
}

#[test]
fn test_double_mut_borrow_same_scope_fails() {
    let source = "\
proc main
    var x := 5.0
    var y := &mut x
    var z := &mut x
";
    let result = analyze(source);
    assert!(result.is_err(), "double mut-borrow should fail");
}

#[test]
fn test_literal_null_deref_rejected() {
    let source = "\
proc main
    val x := *null
";
    assert!(analyze(source).is_err());
}

#[test]
fn test_known_null_binding_deref_rejected() {
    let source = "\
proc main
    val p := null
    val x := *p
";
    assert!(analyze(source).is_err());
}

#[test]
fn test_null_as_value_accepted() {
    let source = "\
proc main
    val p := null
    if p = null then
        print(\"ok\")
";
    assert!(analyze(source).is_ok());
}

#[test]
fn test_if_branch_void_mismatch_rejected() {
    let source = "\
proc main
    val x := if true
        print(\"done\")
    else
        42.0
";
    assert!(
        analyze(source).is_err(),
        "if with one Void and one value branch should be rejected"
    );
}

#[test]
fn test_if_both_branches_void_accepted() {
    let source = "\
proc main
    if true
        print(\"a\")
    else
        print(\"b\")
";
    assert!(
        analyze(source).is_ok(),
        "if with both branches Void should be accepted"
    );
}

#[test]
fn test_if_both_branches_produce_value_accepted() {
    let source = "\
proc main
    val x := if true
        1.0
    else
        2.0
    print(x)
";
    assert!(
        analyze(source).is_ok(),
        "if with both branches producing values should be accepted"
    );
}

#[test]
fn test_match_arm_void_mismatch_rejected() {
    let source = "\
proc main
    val x := match 1
        case 1
            42.0
        case 2
            print(\"done\")
";
    assert!(
        analyze(source).is_err(),
        "match with mixed Void and value arms should be rejected"
    );
}

#[test]
fn test_assign_to_val_rejected() {
    let source = "\
proc main
    val x := 5.0
    x := 10.0
";
    let result = analyze(source);
    assert!(
        result.is_err(),
        "assigning to a `val` must be rejected by the analyzer"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("immutable"),
        "expected immutability message, got: {}",
        msg
    );
}

#[test]
fn test_assign_to_var_accepted() {
    let source = "\
proc main
    var x := 5.0
    x := 10.0
";
    assert!(
        analyze(source).is_ok(),
        "assigning to a `var` must be accepted"
    );
}

#[test]
fn test_for_loop_local_move_accepted() {
    // A variable declared inside a loop body is recreated each
    // iteration. Moving it must not be flagged as a loop-body move.
    let source = "\
proc main
    for i in [1.0, 2.0]
        val local := \"hello\"
        val other := local
";
    assert!(
        analyze(source).is_ok(),
        "moving a locally-declared var inside a loop must be accepted"
    );
}

#[test]
fn test_for_loop_outer_move_rejected() {
    // Moving an outer variable inside a loop body is a genuine problem:
    // iteration 2 would re-move an already-moved value.
    let source = "\
proc main
    val x := \"hello\"
    for i in [1.0, 2.0]
        val y := x
";
    let result = analyze(source);
    assert!(
        result.is_err(),
        "moving an outer var in a loop body must be rejected"
    );
}

#[test]
fn test_param_is_assignable() {
    // Parameters are local bindings; assigning to them must be allowed.
    // This mirrors the verifier's `mutability: true` for params, and
    // is required for functions like `increment(x: &mut float)` that
    // write through their parameter.
    let source = "\
proc bump(x: &mut float)
    x := x + 1.0
";
    assert!(
        analyze(source).is_ok(),
        "assignment to a function parameter must be accepted"
    );
}

#[test]
fn test_uncaptured_move_in_nested_scope_accepted() {
    // Without a defer, moving a variable into a nested scope is fine.
    // `y` is used inside the region, where it's in scope.
    let source = "\
proc main
    val x := \"hello\"
    region r
        val y := x
        print(y)
";
    assert!(
        analyze(source).is_ok(),
        "moving an uncaptured variable in a nested scope must be accepted"
    );
}

#[test]
fn test_defer_capture_blocks_move_in_nested_scope() {
    // `defer print(x)` captures `x`. Moving `x` inside a nested
    // scope must be rejected — the defer runs at the *outer* scope's
    // exit and would observe a moved-from value.
    let source = "\
proc main
    val x := \"hello\"
    defer print(x)
    region r
        val y := x
        print(y)
";
    assert!(
        analyze(source).is_err(),
        "moving a defer-captured variable in a nested scope must be rejected"
    );
}

// ─── PR-9d: match exhaustiveness ────────────────────────────────────────

#[test]
fn test_match_option_with_both_arms_accepted() {
    let source = "\
proc unwrap_or(m: Option<Float>, d: Float) -> Float
    return match m
        case Some(v)
            v
        case None
            d
";
    assert!(
        analyze(source).is_ok(),
        "exhaustive Option match must be accepted"
    );
}

#[test]
fn test_match_option_missing_none_rejected() {
    let source = "\
proc unwrap(m: Option<Float>) -> Float
    return match m
        case Some(v)
            v
";
    let result = analyze(source);
    assert!(
        result.is_err(),
        "match on Option without None or _ must be rejected"
    );
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("Option"),
        "expected Option exhaustiveness error, got: {}",
        msg
    );
}

#[test]
fn test_match_option_with_wildcard_accepted() {
    let source = "\
proc unwrap(m: Option<Float>) -> Float
    return match m
        case Some(v)
            v
        case _
            0.0
";
    assert!(
        analyze(source).is_ok(),
        "Option match with `_` fallback must be accepted"
    );
}

#[test]
fn test_match_result_missing_error_rejected() {
    let source = "\
proc unwrap(r: Result<Float, String>) -> Float
    return match r
        case Ok(v)
            v
";
    let result = analyze(source);
    assert!(
        result.is_err(),
        "match on Result without Error or _ must be rejected"
    );
}

#[test]
fn test_match_bool_missing_false_rejected() {
    let source = "\
proc f(b: Bool) -> Float
    return match b
        case true
            1.0
";
    let result = analyze(source);
    assert!(
        result.is_err(),
        "match on Bool without false or _ must be rejected"
    );
}

// ─── PR-9d: path-return analysis ────────────────────────────────────────

#[test]
fn test_path_return_through_match_accepted() {
    let source = "\
proc f(m: Option<Float>) -> Float
    match m
        case Some(v)
            return v
        case None
            return 0.0
";
    assert!(
        analyze(source).is_ok(),
        "function whose match arms all return must be accepted"
    );
}

#[test]
fn test_variadic_extern_accepts_extra_args() {
    let source = "\
extern \"C\" fn printf(fmt: String, ...) -> Int

proc main
    printf(\"hello\\n\")
    printf(\"value: %lld\\n\", 42)
";
    assert!(
        analyze(source).is_ok(),
        "variadic extern must accept extra args"
    );
}

#[test]
fn test_variadic_extern_rejects_too_few_args() {
    let source = "\
extern \"C\" fn printf(fmt: String, ...) -> Int

proc main
    printf()
";
    let result = analyze(source);
    assert!(result.is_err(), "variadic extern must reject too few args");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("at least"),
        "expected 'at least' in message, got: {}",
        msg
    );
}

#[test]
fn break_out_of_region_is_rejected() {
    let source = "\
proc main
    var x := 10.0
    while x > 0
        region r
            break
";
    assert!(analyze(source).is_err());
}

#[test]
fn continue_out_of_region_is_rejected() {
    let source = "\
proc main
    var x := 10.0
    while x > 0
        region r
            continue
";
    assert!(analyze(source).is_err());
}

#[test]
fn loop_inside_region_break_accepted() {
    let source = "\
proc main
    var x := 10.0
    region r
        while x > 0
            break
";
    assert!(analyze(source).is_ok());
}

#[test]
fn nested_loop_inside_region_break_accepted() {
    let source = "\
proc main
    var x := 10.0
    region r
        while x > 0
            var y := 5.0
            while y > 0
                break
";
    assert!(analyze(source).is_ok());
}

// ─── Stage 3.1: instantiation recording ────────────────────────────────

#[test]
fn records_instantiation_for_generic_call_with_int_argument() {
    use crate::compiler::assign_expr_ids;

    let source = "\
fn identity<T>(x: T) -> T
    x
proc main
    val x := identity(42)
    print(x)
";
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &functions,
            &program.traits,
            &program.impls,
            &program.records,
            &[],
            &[],
            &[],
        )
        .expect("analysis should succeed");

    let instantiations = analyzer.take_instantiations();
    assert_eq!(
        instantiations.len(),
        1,
        "expected exactly one instantiation record, got: {:?}",
        instantiations
    );
    let inst = &instantiations[0];
    assert_eq!(inst.function, "identity");
    assert_eq!(inst.type_params, vec!["T".to_string()]);
    assert_eq!(inst.type_args, vec![Type::Int]);
}

#[test]
fn records_instantiation_for_generic_call_with_reference_argument() {
    // The adversarial case from the ADR: `identity(p)` where
    // `p := &v`. The pre-typecheck monomorphizer that once ran
    // before analysis could not infer this argument's type; the
    // analyzer can. The recorded instantiation feeds
    // `InstantiationPlan`, which specializes `identity` at
    // `Borrow<Float>`.
    use crate::compiler::assign_expr_ids;

    let source = "\
fn identity<T>(x: T) -> T
    x
proc main
    val v := 1.0
    val p := &v
    val q := identity(p)
    print(q)
";
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &functions,
            &program.traits,
            &program.impls,
            &program.records,
            &[],
            &[],
            &[],
        )
        .expect("analysis should succeed");

    let instantiations = analyzer.take_instantiations();
    assert_eq!(
        instantiations.len(),
        1,
        "expected exactly one instantiation record, got: {:?}",
        instantiations
    );
    let inst = &instantiations[0];
    assert_eq!(inst.function, "identity");
    assert_eq!(inst.type_params, vec!["T".to_string()]);
    assert_eq!(inst.type_args, vec![Type::borrow(Type::Float)]);
}

#[test]
fn non_generic_calls_record_no_instantiation() {
    use crate::compiler::assign_expr_ids;

    let source = "\
fn add(x: Int, y: Int) -> Int
    x + y
proc main
    val z := add(1, 2)
    print(z)
";
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze_with_spans(
            &functions,
            &program.traits,
            &program.impls,
            &program.records,
            &[],
            &[],
            &[],
        )
        .expect("analysis should succeed");

    assert!(
        analyzer.take_instantiations().is_empty(),
        "non-generic calls should not produce instantiation records"
    );
}

// ─── ADR 0015: unsafe enforcement ──────────────────────────────
//
// Two operations are gated by `unsafe` blocks: raw pointer
// dereference and the `alloc`/`free` builtins. `AddrOf` on a
// non-place expression is already rejected unconditionally and
// is not affected by these tests.

/// Parse `source`, assign ExprIds, run the analyzer. Local to
/// this test group so it does not collide with the module's
/// other helpers.
fn analyze_unsafe(source: &str) -> Result<()> {
    use crate::compiler::assign_expr_ids;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;
    let lexer = Lexer::new(source.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let mut functions = program.functions;
    assign_expr_ids(&mut functions);
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_traits(
        &functions,
        &program.traits,
        &program.impls,
        &program.records,
    )
}

#[test]
fn test_alloc_outside_unsafe_rejected() {
    let source = r#"
proc main
    val p := alloc(4)
"#;
    let err = analyze_unsafe(source).unwrap_err();
    assert!(
        err.message.contains("unsafe"),
        "expected `unsafe` diagnostic, got: {}",
        err.message,
    );
}

#[test]
fn test_alloc_inside_unsafe_accepted() {
    let source = r#"
proc main
    unsafe
        val p := alloc(4)
"#;
    analyze_unsafe(source).expect("alloc inside unsafe should be accepted");
}

#[test]
fn test_free_inside_unsafe_accepted() {
    let source = r#"
proc main
    unsafe
        val p := alloc(4)
        free(p)
"#;
    analyze_unsafe(source).expect("free inside unsafe should be accepted");
}

#[test]
fn test_nested_unsafe_depth() {
    // A free nested inside two unsafe blocks sees unsafe_depth == 2
    // and is accepted. Exercises the increment/decrement pairing.
    let source = r#"
proc main
    unsafe
        unsafe
            val p := alloc(4)
            free(p)
"#;
    analyze_unsafe(source).expect("nested unsafe blocks should be accepted");
}

#[test]
fn test_unsafe_does_not_leak_across_block_boundary() {
    // After an unsafe block ends, unsafe_depth returns to zero,
    // so an operation in a subsequent statement is rejected.
    let source = r#"
proc main
    unsafe
        val q := alloc(4)
        free(q)
    val p := alloc(8)
"#;
    let err = analyze_unsafe(source).unwrap_err();
    assert!(
        err.message.contains("unsafe"),
        "expected `unsafe` diagnostic after block close, got: {}",
        err.message,
    );
}

#[test]
fn test_deref_pointer_parameter_outside_unsafe_rejected() {
    // Dereferencing a parameter of raw-pointer type is gated the
    // same way `alloc` is. If the type-annotation parser does
    // not accept `Pointer<Int>` as a parameter type, replace
    // with the syntax the language actually uses.
    let source = r#"
proc use_pointer(p: Pointer<Int>)
    val x := *p
"#;
    let err = analyze_unsafe(source).unwrap_err();
    assert!(
        err.message.contains("unsafe"),
        "expected `unsafe` diagnostic for pointer deref, got: {}",
        err.message,
    );
}

#[test]
fn test_deref_pointer_parameter_inside_unsafe_accepted() {
    let source = r#"
proc use_pointer(p: Pointer<Int>)
    unsafe
        val x := *p
"#;
    analyze_unsafe(source).expect("pointer deref inside unsafe should be accepted");
}

#[test]
fn field_access_rejects_immutable() {
    let source = r#"
rec Point
    x: Int
    y: Int

proc main
    val p := Point { x: 1, y: 2 }
    p.x := 99
"#;
    let lexer = crate::frontend::lexer::Lexer::new(source.to_string()).unwrap();
    let mut parser = crate::frontend::parser::Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();
    let mut functions = program.functions;
    crate::compiler::assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    let result = analyzer.analyze_with_spans(
        &functions,
        &program.traits,
        &program.impls,
        &program.records,
        &[],
        &[],
        &[],
    );

    let err = result.expect_err("expected immutability error");
    let msg = format!("{}", err);
    assert!(
        msg.contains("immutable"),
        "expected immutability diagnostic, got: {}",
        msg
    );
}

#[test]
fn unknown_record_name_in_signature_rejected() {
    let source = "\
fn f() -> MissingRecord
    0
";
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("MissingRecord"),
        "expected the message to name the type, got: {}",
        err.message,
    );
}

#[test]
fn declared_record_in_signature_accepted() {
    let source = "\
rec Point
    x: Int
    y: Int

fn origin() -> Point
    Point { x: 0, y: 0 }
";
    analyze(source).expect("record declared before use should resolve");
}

#[test]
fn single_letter_type_param_still_resolves() {
    let source = "\
fn identity<T>(x: T) -> T
    x
proc main
    print(identity(42))
";
    analyze(source).expect("type parameter should not be treated as a user type");
}

#[test]
fn unknown_record_as_parameter_type_rejected() {
    let source = "\
fn f(p: MissingRecord) -> Int
    0
";
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("MissingRecord"),
        "got: {}",
        err.message,
    );
}

fn analyze_source(source: &str) -> Result<()> {
    use crate::compiler::assign_expr_ids;
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    let lexer = Lexer::new(source.to_string()).expect("lex");
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().expect("parse");
    let mut functions = program.functions;
    assign_expr_ids(&mut functions);

    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_spans(
        &functions,
        &program.traits,
        &program.impls,
        &program.records,
        &program.distinct_decls,
        &program.enum_decls,
        &program.subrange_decls,
    )
}

#[test]
fn copy_record_survives_assignment() {
    let source = r#"
rec Point
    x: Int
    y: Int

proc main
    val p := Point { x: 1, y: 2 }
    val q := p
    print(p.x)
"#;
    let result = analyze_source(source);
    assert!(
        result.is_ok(),
        "Point should be Copy; got: {:?}",
        result.err()
    );
}

#[test]
fn non_copy_record_moves_on_assignment() {
    let source = r#"
rec Person
    name: String
    age: Int

proc main
    val p := Person { name: "Alice", age: 30 }
    val q := p
    print(p.name)
"#;
    let result = analyze_source(source);
    let err = result.expect_err("Person should not be Copy");
    let msg = format!("{}", err);
    assert!(
        msg.contains("moved"),
        "expected move diagnostic, got: {}",
        msg
    );
}

#[test]
fn copy_record_survives_argument_pass() {
    let source = r#"
rec Point
    x: Int
    y: Int

fn manhattan(p: Point) -> Int
    p.x + p.y
proc main
    val p := Point { x: 3, y: 4 }
    val d := manhattan(p)
    print(p.x)
    print(d)
"#;
    let result = analyze_source(source);
    assert!(result.is_ok(), "got: {:?}", result.err());
}

#[test]
fn nested_copy_record_is_copy() {
    let source = r#"
rec Point
    x: Int
    y: Int

rec Line
    left: Point
    right: Point

proc main
    val a := Point { x: 0, y: 0 }
    val b := Point { x: 1, y: 1 }
    val l := Line { left: a, right: b }
    val m := l
    print(l.left.x)
"#;
    let result = analyze_source(source);
    assert!(
        result.is_ok(),
        "Line should be Copy; got: {:?}",
        result.err()
    );
}

#[test]
fn mixed_record_is_not_copy() {
    let source = r#"
rec Point
    x: Int
    y: Int

rec NamedPoint
    name: String
    pt: Point

proc main
    val p := NamedPoint { name: "origin", pt: Point { x: 0, y: 0 } }
    val q := p
    print(p.name)
"#;
    let result = analyze_source(source);
    let err = result.expect_err("NamedPoint should not be Copy");
    let msg = format!("{}", err);
    assert!(
        msg.contains("moved"),
        "expected move diagnostic, got: {}",
        msg
    );
}

#[test]
fn copy_does_not_affect_mutability() {
    // `Point` is Copy, but a `val` binding is still immutable.
    let source = r#"
rec Point
    x: Int
    y: Int

proc main
    val p := Point { x: 1, y: 2 }
    p.x := 99
"#;
    let result = analyze_source(source);
    let err = result.expect_err("val binding should be immutable");
    let msg = format!("{}", err);
    assert!(
        msg.contains("immutable"),
        "expected immutability diagnostic, got: {}",
        msg
    );
}

#[test]
fn copy_record_can_still_be_borrowed() {
    let source = r#"
rec Point
    x: Int
    y: Int

proc main
    var p := Point { x: 1, y: 2 }
    val r := &p
    val v := *r
    print(v.x)
"#;
    let result = analyze_source(source);
    assert!(
        result.is_ok(),
        "borrow of Copy record failed: {:?}",
        result.err()
    );
}

#[test]
fn option_of_copy_record_is_copy() {
    let source = r#"
rec Point
    x: Int
    y: Int

proc main
    val maybe := Some(Point { x: 1, y: 2 })
    val other := maybe
    print(maybe)
"#;
    let result = analyze_source(source);
    assert!(
        result.is_ok(),
        "Option<Point> should be Copy; got: {:?}",
        result.err()
    );
}

#[test]
fn map_literal_infers_key_and_value_types() {
    let source = r#"
proc main
    val m := Map { "a": 1, "b": 2 }
"#;
    let result = analyze_source(source);
    assert!(result.is_ok(), "got: {:?}", result.err());
}

#[test]
fn map_with_explicit_type_args_accepts_matching_entries() {
    let source = r#"
proc main
    val m := Map<String, Int> { "a": 1 }
"#;
    assert!(analyze_source(source).is_ok());
}

#[test]
fn empty_map_uses_expected_type_from_annotation() {
    let source = r#"
proc main
    var m: Map<String, Int> := Map {}
"#;
    assert!(analyze_source(source).is_ok());
}

#[test]
fn empty_map_without_context_is_rejected() {
    let source = r#"
proc main
    val m := Map {}
"#;
    let err = analyze_source(source).expect_err("should need a type annotation");
    let msg = format!("{}", err);
    assert!(
        msg.contains("Empty map literal"),
        "expected empty-literal diagnostic, got: {}",
        msg
    );
}

#[test]
fn map_key_type_must_be_hashable() {
    let source = r#"
proc main
    val m := Map<Float, Int> { 1.5: 1 }
"#;
    let err = analyze_source(source).expect_err("Float key should be rejected");
    let msg = format!("{}", err);
    assert!(
        msg.contains("Map keys must be Int, String, or Bool"),
        "expected key-type diagnostic, got: {}",
        msg
    );
}

#[test]
fn map_insert_requires_mutable_binding() {
    let source = r#"
proc main
    val m := Map<String, Int> {}
    m.insert("a", 1)
"#;
    let err = analyze_source(source).expect_err("insert on val should be rejected");
    let msg = format!("{}", err);
    assert!(
        msg.contains("immutable"),
        "expected immutability diagnostic, got: {}",
        msg
    );
}

#[test]
fn map_insert_accepts_matching_types() {
    let source = r#"
proc main
    var m := Map<String, Int> {}
    m.insert("a", 1)
"#;
    assert!(analyze_source(source).is_ok());
}

#[test]
fn map_insert_rejects_wrong_value_type() {
    let source = r#"
proc main
    var m := Map<String, Int> {}
    m.insert("a", "not an int")
"#;
    let err = analyze_source(source).expect_err("String value into Map<String, Int> should fail");
    let msg = format!("{}", err);
    assert!(
        msg.contains("value type mismatch"),
        "expected value-type diagnostic, got: {}",
        msg
    );
}

#[test]
fn map_get_returns_option_of_value_type() {
    // Type-level assertion: the call succeeds; the precise return
    // type is exercised indirectly through the type table.
    let source = r#"
proc main
    val m := Map { "a": 1 }
    val x := m.get("a")
    match x
        case Some(v)
            print(v)
        case None
            print(0)
"#;
    assert!(analyze_source(source).is_ok());
}

#[test]
fn map_length_accepts_zero_args() {
    let source = r#"
proc main
    val m := Map { "a": 1 }
    val n := m.length()
    print(n)
"#;
    assert!(analyze_source(source).is_ok());
}

#[test]
fn append_requires_var() {
    let source = r#"
proc main
    val xs: List<Int> := []
    xs.append(1)
"#;
    let err = analyze_source(source).expect_err("append on val should fail");
    let msg = format!("{}", err);
    assert!(
        msg.contains("immutable"),
        "expected immutability diagnostic, got: {}",
        msg
    );
}

#[test]
fn append_rejects_wrong_element_type() {
    let source = r#"
proc main
    var xs: List<Int> := []
    xs.append("nope")
"#;
    let err = analyze_source(source).expect_err("String into List<Int> should fail");
    let msg = format!("{}", err);
    assert!(
        msg.contains("element type mismatch"),
        "expected element-type diagnostic, got: {}",
        msg
    );
}

#[test]
fn option_of_record_in_signature_resolves() {
    let source = r#"
rec Sale
    amount: Int

fn maybe_sale() -> Option<Sale>
    None
proc main
    match maybe_sale()
        case Some(s)
            print(s.amount)
        case None
            print(0)
"#;
    analyze(source).expect("Option<Sale> should resolve the record inside the generic");
}

#[test]
fn enum_variant_value_has_enum_type() {
    let source = r#"
enum Day
    Monday
    Tuesday
    Saturday

proc main
    val d := Day.Saturday
end
"#;
    analyze(source).expect("Day.Saturday should type-check");
}

#[test]
fn enum_variant_unknown_name_rejected() {
    let source = r#"
enum Day
    Monday
    Tuesday

proc main
    val d := Day.NotAVariant
end
"#;
    let result = analyze(source);
    assert!(result.is_err(), "Day.NotAVariant should be rejected");
}

#[test]
fn set_of_enum_in_signature_resolves() {
    let source = r#"
enum Day
    Monday
    Tuesday
    Wednesday

fn first(s: Set<Day>) -> Day
    Day.Monday
proc main
end
"#;
    analyze(source).expect("Set<Day> should resolve in a signature");
}

#[test]
fn set_of_bool_in_signature_resolves() {
    let source = r#"
fn is_empty(s: Set<Bool>) -> Bool
    true
proc main
end
"#;
    analyze(source).expect("Set<Bool> should resolve");
}

#[test]
fn set_of_int_in_signature_rejected() {
    let source = r#"
fn f(s: Set<Int>) -> Int
    0
proc main
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("set element type"),
        "expected domain-size diagnostic, got: {}",
        err.message
    );
}

#[test]
fn set_of_float_in_signature_rejected() {
    let source = r#"
fn f(s: Set<Float>) -> Float
    0.0
proc main
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("set element type"),
        "expected domain-size diagnostic, got: {}",
        err.message
    );
}

#[test]
fn set_of_small_subrange_resolves() {
    // Int in 0..63 has exactly 64 values — fits.
    let source = r#"
type Byte Int in 0..63

fn f(s: Set<Byte>) -> Int
    0
proc main
end
"#;
    analyze(source).expect("Set<Byte> should resolve");
}

#[test]
fn set_of_large_subrange_rejected() {
    // Int in 0..100 has 101 values — one over the ceiling.
    let source = r#"
type Percentage Int in 0..100

fn f(s: Set<Percentage>) -> Int
    0
proc main
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("set element type"),
        "expected domain-size diagnostic, got: {}",
        err.message
    );
}

#[test]
fn empty_set_literal_resolves() {
    let source = r#"
enum Day
    Monday
    Tuesday
    Wednesday

proc main
    val s: Set<Day> := Set<Day> {}
end
"#;
    analyze(source).expect("Set<Day> {} should typecheck");
}

#[test]
fn set_literal_with_variant_elements() {
    let source = r#"
enum Day
    Monday
    Tuesday
    Wednesday
    Thursday
    Friday
    Saturday
    Sunday

proc main
    val weekend: Set<Day> := Set<Day> { Day.Saturday, Day.Sunday }
end
"#;
    analyze(source).expect("Set<Day> { Day.Saturday, Day.Sunday } should typecheck");
}

#[test]
fn set_literal_element_type_mismatch_rejected() {
    let source = r#"
enum Day
    Monday
    Tuesday

proc main
    val s: Set<Day> := Set<Day> { 1, 2 }
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("set element"),
        "expected element-mismatch diagnostic, got: {}",
        err.message
    );
}

#[test]
fn set_literal_with_bad_element_type_rejected() {
    // Set<Int> in the literal itself should be rejected by
    // resolve_type_syntax, same as in a signature.
    let source = r#"
proc main
    val s := Set<Int> {}
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("set element type"),
        "expected domain-size diagnostic, got: {}",
        err.message
    );
}

#[test]
fn set_literal_of_bool() {
    let source = r#"
proc main
    val s: Set<Bool> := Set<Bool> {}
end
"#;
    analyze(source).expect("Set<Bool> {} should typecheck");
}

#[test]
fn set_literal_of_small_subrange() {
    let source = r#"
type Byte Int in 0..63

proc main
    val s: Set<Byte> := Set<Byte> {}
end
"#;
    analyze(source).expect("Set<Byte> {} should typecheck");
}

#[test]
fn membership_in_set_literal() {
    let source = r#"
enum Day
    Monday
    Tuesday
    Saturday
    Sunday

proc main
    val weekend: Set<Day> := Set<Day> { Day.Saturday, Day.Sunday }
    val b: Bool := Day.Saturday in weekend
end
"#;
    analyze(source).expect("Day.Saturday in Set<Day> should typecheck");
}

#[test]
fn membership_type_mismatch_rejected() {
    let source = r#"
enum Day
    Monday
    Tuesday

enum Color
    Red
    Blue

proc main
    val s: Set<Day> := Set<Day> {}
    val b: Bool := Color.Red in s
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("`in` type mismatch"),
        "expected `in` mismatch, got: {}",
        err.message
    );
}

#[test]
fn membership_with_non_set_rhs_rejected() {
    let source = r#"
proc main
    val b: Bool := 5 in 10
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("`in` requires a set"),
        "expected `in` set-required diagnostic, got: {}",
        err.message
    );
}

#[test]
fn set_union_same_element_type() {
    let source = r#"
enum Day
    Monday
    Tuesday

proc main
    val a: Set<Day> := Set<Day> { Day.Monday }
    val b: Set<Day> := Set<Day> { Day.Tuesday }
    val u: Set<Day> := a + b
end
"#;
    analyze(source).expect("Set<Day> + Set<Day> should typecheck");
}

#[test]
fn set_union_mismatched_element_types_rejected() {
    let source = r#"
enum Day
    Monday

enum Color
    Red

proc main
    val a: Set<Day> := Set<Day> { Day.Monday }
    val b: Set<Color> := Set<Color> { Color.Red }
    val u := a + b
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message.contains("set element types must match"),
        "expected element-type mismatch, got: {}",
        err.message
    );
}

#[test]
fn set_subset_operator_returns_bool() {
    let source = r#"
enum Day
    Monday
    Tuesday

proc main
    val a: Set<Day> := Set<Day> {}
    val b: Set<Day> := Set<Day> {}
    val s: Bool := a <= b
end
"#;
    analyze(source).expect("Set <= Set should typecheck to Bool");
}

#[test]
fn set_equality_returns_bool() {
    let source = r#"
enum Day
    Monday
    Tuesday

proc main
    val a: Set<Day> := Set<Day> {}
    val b: Set<Day> := Set<Day> {}
    val s: Bool := a = b
end
"#;
    analyze(source).expect("Set == Set should typecheck to Bool");
}

#[test]
fn set_plus_int_rejected() {
    let source = r#"
enum Day
    Monday

proc main
    val a: Set<Day> := Set<Day> {}
    val n := a + 5
end
"#;
    let err = analyze(source).unwrap_err();
    assert!(
        err.message
            .contains("set operator requires both operands to be sets"),
        "expected mixed-operand diagnostic, got: {}",
        err.message
    );
}

// src/semantics/analyzer/tests.rs — append

// ─── ADR 0038: dynamic dispatch coercion ────────────────────────────

#[test]
fn dyn_trait_coercion_accepts_implementing_type() {
    let source = "\
trait Shape
    fn area(self: &Self) -> Float

rec Circle
    r: Float

impl Shape for Circle
    fn area(self: &Circle) -> Float
        self.r
proc main
    val c := Circle { r: 2.0 }
    val s: &dyn Shape := &c
";
    analyze(source).expect("`&Circle` should coerce to `&dyn Shape` when Circle: Shape");
}

#[test]
fn dyn_trait_coercion_rejects_missing_impl() {
    let source = "\
trait Shape
    fn area(self: &Self) -> Float

rec Square
    side: Float

proc main
    val sq := Square { side: 2.0 }
    val s: &dyn Shape := &sq
";
    let err =
        analyze(source).expect_err("`&Square` should not coerce without `impl Shape for Square`");
    assert!(
        err.message.contains("Square") && err.message.contains("Shape"),
        "diagnostic should name both the concrete type and the trait: {}",
        err.message
    );
}

#[test]
fn dyn_trait_requires_object_safe_trait() {
    // A trait method whose receiver is by value is not dispatchable
    // through a fat pointer. The object-safety check fires when the
    // `&dyn Consuming` annotation is resolved — before any coercion
    // needs a concrete value. No `impl` is present; object safety is
    // a property of the trait declaration alone.
    let source = "\
trait Consuming
    fn take(self: Self) -> Int

proc main
    var x: &dyn Consuming := 0
";
    let err = analyze(source).expect_err("by-value receiver should disqualify a trait from `dyn`");
    assert!(
        err.message.contains("object-safe"),
        "diagnostic should explain the object-safety violation: {}",
        err.message
    );
}

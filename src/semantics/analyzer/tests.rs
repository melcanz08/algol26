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
    )
}

#[test]
fn test_mut_borrow_released_at_scope_exit() {
    // Borrow in the inner block should be released before the second
    // borrow at the outer scope.
    let source = "\
procedure main
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
procedure main
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
procedure main
    val x := *null
";
    assert!(analyze(source).is_err());
}

#[test]
fn test_known_null_binding_deref_rejected() {
    let source = "\
procedure main
    val p := null
    val x := *p
";
    assert!(analyze(source).is_err());
}

#[test]
fn test_null_as_value_accepted() {
    let source = "\
procedure main
    val p := null
    if p == null then
        print(\"ok\")
";
    assert!(analyze(source).is_ok());
}

#[test]
fn test_if_branch_void_mismatch_rejected() {
    let source = "\
procedure main
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
procedure main
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
procedure main
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
procedure main
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
procedure main
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
procedure main
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
procedure main
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
procedure main
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
procedure bump(x: &mut float)
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
procedure main
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
procedure main
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
function unwrap_or(m: Option<Float>, d: Float) -> Float
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
function unwrap(m: Option<Float>) -> Float
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
function unwrap(m: Option<Float>) -> Float
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
function unwrap(r: Result<Float, String>) -> Float
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
function f(b: Bool) -> Float
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
function f(m: Option<Float>) -> Float
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
extern \"C\" function printf(fmt: String, ...) -> Int

procedure main
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
extern \"C\" function printf(fmt: String, ...) -> Int

procedure main
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
procedure main
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
procedure main
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
procedure main
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
procedure main
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
function identity<T>(x: T) -> T
    return x

procedure main
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
    // `p := &v`. The pre-typecheck monomorphizer could not infer
    // this argument's type; the analyzer can. Stage 3.1 records the
    // fact so Stage 3.2's monomorphizer no longer needs to guess.
    use crate::compiler::assign_expr_ids;

    let source = "\
function identity<T>(x: T) -> T
    return x

procedure main
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
function add(x: Int, y: Int) -> Int
    return x + y

procedure main
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
procedure main
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
procedure main
    unsafe
        val p := alloc(4)
"#;
    analyze_unsafe(source).expect("alloc inside unsafe should be accepted");
}

#[test]
fn test_free_inside_unsafe_accepted() {
    let source = r#"
procedure main
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
procedure main
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
procedure main
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
procedure use_pointer(p: Pointer<Int>)
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
procedure use_pointer(p: Pointer<Int>)
    unsafe
        val x := *p
"#;
    analyze_unsafe(source).expect("pointer deref inside unsafe should be accepted");
}

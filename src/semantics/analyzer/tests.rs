// src/semantics/analyzer/tests.rs

use super::*;
use crate::frontend::lexer::Lexer;
use crate::frontend::parser::Parser;

#[cfg(test)]
fn analyze(source: &str) -> Result<()> {
    let lexer = Lexer::new(source.to_string())?;
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program()?;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.analyze_with_spans(
        &program.functions,
        &program.traits,
        &program.impls,
        &std::collections::HashMap::new(),
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

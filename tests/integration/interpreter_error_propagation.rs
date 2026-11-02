// tests/integration/interpreter_error_propagation.rs
//
// G27B invariant: errors raised inside a called function must
// propagate with their original cause, not be replaced by a
// generic "function X returned no value" message.
//
// Before the G27B fix (ad08a53), `eval_call` and
// `call_function_with_values` checked `self.return_value.take()`
// BEFORE checking `result.is_err()`. When a callee errored before
// reaching a return instruction, `return_value` was `None`, and
// the "returned no value" message was emitted instead of the
// actual error. That mask hid G27A for multiple debugging rounds.
//
// These tests assert:
//   1. The innermost cause appears verbatim in the top-level error.
//   2. The chain names every enclosing frame.
//   3. The "returned no value" mask does NOT appear.
//
// Test 3 goes three frames deep to ensure the fix isn't
// coincidentally working at depth 1 only.

use algol26::compiler::Compiler;

fn interpret_expect_err(source: &str) -> String {
    let mut compiler = Compiler::new();
    let result = compiler.run_interpreter(source, "<g27-test>");
    let err = result.expect_err("program should raise a runtime error");
    format!("{}", err)
}

#[test]
fn g27_division_by_zero_cause_survives_two_frames() {
    // main -> outer -> inner_errors -> division by zero.
    // The 'division by zero' leaf must reach the top-level message,
    // and both enclosing frame names must appear in the chain.
    let source = r#"
proc inner_errors() -> String
    val x := 1
    val y := 0
    return Int.to_string(x / y)

proc outer() -> String
    return inner_errors()

proc main
    print(outer())
"#;
    let msg = interpret_expect_err(source);

    assert!(
        msg.contains("division by zero"),
        "G27B: innermost cause must survive; got: {}",
        msg
    );
    assert!(
        msg.contains("inner_errors"),
        "G27B: chain must name 'inner_errors'; got: {}",
        msg
    );
    assert!(
        msg.contains("outer"),
        "G27B: chain must name 'outer'; got: {}",
        msg
    );
    assert!(
        !msg.contains("returned no value"),
        "G27B regression: the 'returned no value' mask leaked; got: {}",
        msg
    );
}

#[test]
fn g27_builtin_leaf_error_survives_one_frame() {
    // main -> load -> File.read("/nonexistent...").
    // File.read on a missing file is a leaf builtin error. It must
    // reach the top level with its message and path intact.
    let source = r#"
proc load() -> String
    return File.read("/tmp/g27-this-path-does-not-exist-abcdef")

proc main
    print(load())
"#;
    let msg = interpret_expect_err(source);

    assert!(
        msg.contains("cannot read"),
        "G27B: File.read error message lost; got: {}",
        msg
    );
    assert!(
        msg.contains("/tmp/g27-this-path-does-not-exist-abcdef"),
        "G27B: File.read path lost; got: {}",
        msg
    );
    assert!(
        !msg.contains("returned no value"),
        "G27B regression: mask leaked; got: {}",
        msg
    );
}

#[test]
fn g27_error_cause_survives_three_frames() {
    // main -> a -> b -> c -> division by zero.
    // Three-deep chain. Every frame name must appear, and the leaf
    // cause must survive. This is the depth at which the old mask
    // would have replaced the leaf with the OUTERMOST frame's
    // 'returned no value' message, losing every frame in between.
    let source = r#"
proc c() -> Int
    val zero := 0
    return 1 / zero

proc b() -> Int
    return c()

proc a() -> Int
    return b()

proc main
    print(a())
"#;
    let msg = interpret_expect_err(source);

    assert!(
        msg.contains("division by zero"),
        "G27B: leaf cause lost at depth 3; got: {}",
        msg
    );
    for frame in ["a", "b", "c"] {
        assert!(
            msg.contains(frame),
            "G27B: chain missing frame '{}' at depth 3; got: {}",
            frame,
            msg
        );
    }
    assert!(
        !msg.contains("returned no value"),
        "G27B regression: mask leaked at depth 3; got: {}",
        msg
    );
}

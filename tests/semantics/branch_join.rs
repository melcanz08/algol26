use algol26::semantics::analyzer::SemanticAnalyzer;
use algol26::semantics::state::{OwnershipState, SemanticState};

fn analyze(src: &str) -> SemanticAnalyzer {
    // Whatever helper the existing integration tests use to parse
    // and run the analyzer on a source string. Substitute the real
    // entry point here.
    unimplemented!("wire to your existing test harness")
}

#[test]
fn move_in_one_branch_is_maybe_moved_after_if() {
    let src = r#"
        fn f(cond: Bool) -> Void {
            val x := some_non_copy();
            if cond then { val y := x } else { }
        }
    "#;
    let a = analyze(src);
    let st = a.state().vars.get("x").expect("x should be tracked");
    assert_eq!(st.ownership, OwnershipState::MaybeMoved);
}

#[test]
fn move_in_both_branches_is_moved_after_if() {
    let src = r#"
        fn f(cond: Bool) -> Void {
            val x := some_non_copy();
            if cond then { val y := x } else { val z := x }
        }
    "#;
    let a = analyze(src);
    let st = a.state().vars.get("x").unwrap();
    assert_eq!(st.ownership, OwnershipState::Moved);
}

#[test]
fn no_move_in_either_branch_is_owned_after_if() {
    let src = r#"
        fn f(cond: Bool) -> Void {
            val x := some_non_copy();
            if cond then { val y := 0 } else { val z := 1 }
        }
    "#;
    let a = analyze(src);
    let st = a.state().vars.get("x").unwrap();
    assert_eq!(st.ownership, OwnershipState::Owned);
}

#[test]
fn move_in_expression_if_is_tracked() {
    let src = r#"
        fn f(cond: Bool) -> Int {
            val x := some_non_copy();
            val y := if cond then consume(x) else 0;
            return y;
        }
    "#;
    let a = analyze(src);
    let st = a.state().vars.get("x").unwrap();
    assert_eq!(st.ownership, OwnershipState::MaybeMoved);
}
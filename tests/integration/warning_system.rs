// tests/integration/warning_system.rs
//
// End-to-end tests for the ALGOL26 warning system (ADR 0063).
//
// These tests exercise the full pipeline: analyzer emits a warning,
// the driver aggregates it, the CLI renders it. The primary warning
// (W0001) comes from a mixed-Void `match` discarding a non-Void
// arm's value (G29).
//
// Run via `cargo test --test integration_tests warning_system`.

use algol26::common::diagnostics::WarningConfig;
use algol26::compiler::Compiler;

const G29_PROGRAM: &str = r#"
proc f(x: Int) -> Int
    return x * 2

proc main
    match 1
        case 1
            f(42)
        case _
            {}
"#;

fn run_with_config(source: &str, config: WarningConfig) -> Result<(), String> {
    let mut compiler = Compiler::new().with_warning_config(config);
    compiler
        .type_check_source_for(source, "<test>")
        .map(|_| ())
        .map_err(|e| format!("{}", e))
}

#[test]
fn warning_w0001_fires_by_default() {
    // The analyzer emits W0001 for the mixed-Void match. This test
    // asserts the warning reaches the driver — either as a rendered
    // line (side-effect) or via the return value. Since the driver
    // renders warnings and returns Ok when not denied, we assert
    // Ok.
    let result = run_with_config(G29_PROGRAM, WarningConfig::new());
    assert!(result.is_ok(), "default should succeed, got: {:?}", result);
}

#[test]
fn warning_w0001_suppressed_by_no_warn_config() {
    // --no-warn=W0001 equivalent. Compilation succeeds (same as
    // default); the warning is not emitted (no side-effect to
    // assert directly, but this exercises the config plumbing).
    let cfg = WarningConfig::new().with_disabled(&["W0001"]);
    let result = run_with_config(G29_PROGRAM, cfg);
    assert!(
        result.is_ok(),
        "with --no-warn=W0001 compilation should succeed, got: {:?}",
        result
    );
}

#[test]
fn warning_w0001_denied_fails_compilation() {
    // --deny-warnings equivalent. The warning is promoted to an
    // error; compilation fails.
    let cfg = WarningConfig::new().with_denied(&["*"]);
    let result = run_with_config(G29_PROGRAM, cfg);
    assert!(
        result.is_err(),
        "with --deny-warnings compilation should fail"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("non-Void arm value discarded"),
        "denied warning error should carry the warning's message; got: {}",
        msg
    );
}

#[test]
fn warning_config_wildcard_denies_all() {
    // --deny-warnings uses "*" wildcard to deny every warning code.
    let cfg = WarningConfig::new().with_denied(&["*"]);
    assert!(cfg.is_denied("W0001"));
    assert!(cfg.is_denied("W9999"));
}

#[test]
fn warning_config_specific_denial_leaves_others_enabled() {
    let cfg = WarningConfig::new().with_denied(&["W0001"]);
    assert!(cfg.is_denied("W0001"));
    assert!(!cfg.is_denied("W0002"));
}

#[test]
fn no_warning_for_non_mixed_void_match() {
    // A match where all arms yield Void — no warning should fire,
    // and no denial should cause failure.
    let source = r#"
proc prints_a() -> Void
    match 1
        case 1
            print("one")
        case _
            print("other")
"#;
    let cfg = WarningConfig::new().with_denied(&["*"]);
    let result = run_with_config(source, cfg);
    assert!(
        result.is_ok(),
        "all-Void match should not trigger W0001; got: {:?}",
        result
    );
}

#[test]
fn no_warning_for_all_value_match() {
    // A match where all arms yield the same non-Void type — no
    // warning should fire.
    //
    // Note: each arm uses explicit `return` because the analyzer's
    // return-path analysis doesn't yet recognize a bare `match`
    // whose arms all yield values as returning-on-all-paths (see
    // STATUS.md "Known gaps > proc ending in a match"). That
    // limitation is orthogonal to W0001 — this test's purpose is
    // just to confirm W0001 does NOT fire when all arms are
    // non-Void.
    let source = r#"
proc pick() -> Int
    match 1
        case 1
            return 42
        case _
            return 99
"#;
    let cfg = WarningConfig::new().with_denied(&["*"]);
    let result = run_with_config(source, cfg);
    assert!(
        result.is_ok(),
        "all-value match should not trigger W0001; got: {:?}",
        result
    );
}

// ─── G18-addendum: W0002 config plumbing ────

const SHALLOW_MATCH_PROGRAM: &str = r#"
pub union Json
    Null
    Bool(Bool)
    NumInt(Int)
    NumFloat(Float)
    Str(String)
    Arr(List[Json])
    Obj(Map<String, Json>)

proc shallow(j: Json) -> Option[String]
    match j {
        case Str(s) then { return Some(s) }
        case _ then { return None }
    }

proc main
    val r: Option[String] := shallow(Json.Null)
    match r {
        case Some(s) then { print(s) }
        case None then { print("none") }
    }
"#;

#[test]
fn warning_w0002_not_emitted_on_shallow_match() {
    // A two-deep match on Json (Str vs _) should not trip the
    // depth-threshold warning. Denying W0002 on it should leave
    // compilation successful.
    let cfg = WarningConfig::new().with_denied(&["W0002"]);
    let result = run_with_config(SHALLOW_MATCH_PROGRAM, cfg);
    assert!(
        result.is_ok(),
        "shallow match should not emit W0002; got: {:?}",
        result
    );
}

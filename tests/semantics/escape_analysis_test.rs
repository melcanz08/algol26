// tests/semantics/escape_analysis_test.rs
use algol26::semantics::escape::EscapeAnalyzer;

#[test]
fn test_escape_in_closure() {
    let mut analyzer = EscapeAnalyzer::new();
    
    analyzer.declare("captured", false);
    analyzer.enter_scope();
    
    // Simulate closure capture
    analyzer.take_reference("captured");
    analyzer.reference("captured", true); // Outlives scope
    
    analyzer.exit_scope();
    
    let escapes = analyzer.get_escapes();
    assert!(!escapes.is_empty(), "Closure capture should cause escape");
    assert_eq!(escapes[0].variable, "captured");
}

#[test]
fn test_escape_in_spawn() {
    let mut analyzer = EscapeAnalyzer::new();
    
    analyzer.declare("shared_data", false);
    analyzer.enter_scope();
    
    // Spawned thread captures variable
    analyzer.reference("shared_data", true);
    
    analyzer.exit_scope();
    
    assert!(analyzer.has_escapes(), "Spawn capture should cause escape");
}

#[test]
fn test_no_escape_for_local_vars() {
    let mut analyzer = EscapeAnalyzer::new();
    
    analyzer.enter_scope();
    analyzer.declare("local_var", false);
    analyzer.reference("local_var", false);
    analyzer.exit_scope();
    
    assert!(!analyzer.has_escapes(), "Local variable should not escape");
}
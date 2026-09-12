use super::*;

#[test]
fn test_no_race_with_empty_functions() {
    let mut detector = RaceDetector::new();
    let races = detector.analyze(&[]);
    assert!(races.is_empty());
}

#[test]
fn test_read_only_access_no_race() {
    let mut detector = RaceDetector::new();
    detector
        .main_accesses
        .insert("x".to_string(), AccessType::Read);
    detector.spawned_accesses.push({
        let mut map = HashMap::new();
        map.insert("x".to_string(), AccessType::Read);
        map
    });

    // Simulate the check
    let mut races = Vec::new();
    for spawned in &detector.spawned_accesses {
        for (var, spawn_access) in spawned {
            if let Some(main_access) = detector.main_accesses.get(var) {
                if detector.is_race(spawn_access, main_access) {
                    races.push(format!("Race: {}", var));
                }
            }
        }
    }

    assert!(races.is_empty(), "Read-only access should not be a race");
}

#[test]
fn test_write_write_race_detected() {
    let mut detector = RaceDetector::new();
    detector
        .main_accesses
        .insert("x".to_string(), AccessType::Write);
    detector.spawned_accesses.push({
        let mut map = HashMap::new();
        map.insert("x".to_string(), AccessType::Write);
        map
    });

    let mut races = Vec::new();
    for spawned in &detector.spawned_accesses {
        for (var, spawn_access) in spawned {
            if let Some(main_access) = detector.main_accesses.get(var) {
                if detector.is_race(spawn_access, main_access) {
                    races.push(format!("Race: {}", var));
                }
            }
        }
    }

    assert!(!races.is_empty(), "Write-write should be detected as race");
}

#[test]
fn test_read_write_race_detected() {
    let mut detector = RaceDetector::new();
    detector
        .main_accesses
        .insert("x".to_string(), AccessType::Write);
    detector.spawned_accesses.push({
        let mut map = HashMap::new();
        map.insert("x".to_string(), AccessType::Read);
        map
    });

    let mut races = Vec::new();
    for spawned in &detector.spawned_accesses {
        for (var, spawn_access) in spawned {
            if let Some(main_access) = detector.main_accesses.get(var) {
                if detector.is_race(spawn_access, main_access) {
                    races.push(format!("Race: {}", var));
                }
            }
        }
    }

    assert!(!races.is_empty(), "Read-write should be detected as race");
}

#[test]
fn test_merge_access_function() {
    let mut access = AccessType::Read;
    RaceDetector::merge_access(&mut access, AccessType::Write);
    assert_eq!(access, AccessType::ReadWrite);

    let mut access = AccessType::Write;
    RaceDetector::merge_access(&mut access, AccessType::Write);
    assert_eq!(access, AccessType::Write);

    let mut access = AccessType::ReadWrite;
    RaceDetector::merge_access(&mut access, AccessType::Read);
    assert_eq!(access, AccessType::ReadWrite);
}
#[test]
fn test_val_sharing_is_not_a_race() {
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    let src = "\
procedure main
    val x := 42
    spawn
        print(x)
    print(x)
";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let mut detector = RaceDetector::new();
    let races = detector.analyze(&program.functions);
    assert!(
        races.is_empty(),
        "read-only sharing of a val must not be a race, got: {:?}",
        races
    );
}

#[test]
fn test_var_read_during_spawn_is_conservatively_flagged() {
    use crate::frontend::lexer::Lexer;
    use crate::frontend::parser::Parser;

    let src = "\
procedure main
    var x := 42
    spawn
        print(x)
    print(x)
";
    let lexer = Lexer::new(src.to_string()).unwrap();
    let mut parser = Parser::new(lexer.tokens);
    let program = parser.parse_program().unwrap();

    let mut detector = RaceDetector::new();
    let races = detector.analyze(&program.functions);
    assert!(
        !races.is_empty(),
        "mutable variable shared across spawn must be flagged"
    );
}

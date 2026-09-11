// algol26/src/race.rs

use crate::frontend::ast::{Expr, FunctionDecl, Stmt};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct RaceDetector {
    // Track variables accessed in spawned blocks
    spawned_accesses: Vec<HashMap<String, AccessType>>,
    // Track variables accessed in main thread
    main_accesses: HashMap<String, AccessType>,
    // Track variable declarations and their mutability
    variable_mutability: HashMap<String, bool>, // true = mutable, false = immutable
    // Track scope depth for proper analysis
    scope_depth: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccessType {
    Read,
    Write,
    ReadWrite,
}

impl Default for RaceDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl RaceDetector {
    pub fn new() -> Self {
        RaceDetector {
            spawned_accesses: Vec::new(),
            main_accesses: HashMap::new(),
            variable_mutability: HashMap::new(),
            scope_depth: 0,
        }
    }

    pub fn analyze(&mut self, functions: &[FunctionDecl]) -> Vec<String> {
        let mut races = Vec::new();

        // First pass: collect variable declarations
        for func in functions {
            self.collect_declarations(func);
        }

        // Second pass: analyze function bodies
        for func in functions {
            self.analyze_function(func);
        }

        // Check for races - properly detect ALL conflict patterns
        for spawned in &self.spawned_accesses {
            for (var, spawn_access) in spawned {
                if let Some(main_access) = self.main_accesses.get(var) {
                    if self.is_race(spawn_access, main_access) {
                        races.push(format!(
                            "Data race detected: variable '{}' accessed concurrently (spawn: {:?}, main: {:?})",
                            var, spawn_access, main_access
                        ));
                    }
                }
            }
        }

        // Check races between multiple spawned blocks
        for (i, spawn1) in self.spawned_accesses.iter().enumerate() {
            for (j, spawn2) in self.spawned_accesses.iter().enumerate() {
                if i < j {
                    for (var, access1) in spawn1 {
                        if let Some(access2) = spawn2.get(var) {
                            if self.is_race(access1, access2) {
                                races.push(format!(
                                    "Data race detected: variable '{}' accessed concurrently between spawned blocks (spawn1: {:?}, spawn2: {:?})",
                                    var, access1, access2
                                ));
                            }
                        }
                    }
                }
            }
        }

        races
    }

    fn is_race(&self, access1: &AccessType, access2: &AccessType) -> bool {
        // A race occurs when:
        // - Both are writing (write-write conflict)
        // - One is writing and other is reading (read-write conflict)
        // - Both are read-write
        let writes1 = matches!(access1, AccessType::Write | AccessType::ReadWrite);
        let writes2 = matches!(access2, AccessType::Write | AccessType::ReadWrite);
        let reads1 = matches!(access1, AccessType::Read | AccessType::ReadWrite);
        let reads2 = matches!(access2, AccessType::Read | AccessType::ReadWrite);

        // Write-Write race
        if writes1 && writes2 {
            return true;
        }

        // Read-Write race (only if variable is mutable)
        if (writes1 && reads2) || (reads1 && writes2) {
            return true;
        }

        false
    }

    fn collect_declarations(&mut self, func: &FunctionDecl) {
        for stmt in &func.body {
            self.collect_declarations_from_stmt(stmt);
        }
    }

    fn collect_declarations_from_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl { name, mutable, .. } => {
                self.variable_mutability.insert(name.clone(), *mutable);
            }
            Stmt::Expression(Expr::If {
                then_branch,
                else_branch,
                ..
            }) => {
                if let Expr::Block { statements, .. } = then_branch.as_ref() {
                    for s in statements {
                        self.collect_declarations_from_stmt(s);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let Expr::Block { statements, .. } = else_stmts.as_ref() {
                        for s in statements {
                            self.collect_declarations_from_stmt(s);
                        }
                    }
                }
            }
            Stmt::Expression(Expr::While { body, .. }) => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Expression(Expr::For { body, .. }) => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Spawn { body } => {
                for s in body {
                    self.collect_declarations_from_stmt(s);
                }
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    for s in block {
                        self.collect_declarations_from_stmt(s);
                    }
                }
            }
            _ => {}
        }
    }

    fn analyze_function(&mut self, func: &FunctionDecl) {
        for stmt in &func.body {
            self.analyze_stmt(stmt, false);
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt, in_spawn: bool) {
        self.scope_depth += 1;

        match stmt {
            Stmt::Spawn { body } => {
                let mut spawn_accesses = HashMap::new();
                for s in body {
                    self.analyze_stmt_in_collection(s, &mut spawn_accesses);
                }
                self.spawned_accesses.push(spawn_accesses);
            }
            Stmt::Assign { name, value } => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        Self::merge_access_map(accesses, name, AccessType::Write);
                    }
                } else {
                    Self::merge_access_map(&mut self.main_accesses, name, AccessType::Write);
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::VarDecl { name, value, mutable, .. } => {
                // Only `var` bindings participate in race analysis. A
                // `val` is written exactly once, before any concurrent
                // observer could exist, and never reassigned — so it
                // cannot race with anything. Recording it as a write
                // produces false positives on read-only sharing (e.g.
                // `val x := 42; spawn { print(x) }`).
                if *mutable {
                    if in_spawn {
                        if let Some(accesses) = self.spawned_accesses.last_mut() {
                            Self::merge_access_map(accesses, name, AccessType::Write);
                        }
                    } else {
                        Self::merge_access_map(&mut self.main_accesses, name, AccessType::Write);
                    }
                }
                self.analyze_expr(value, in_spawn);
            }
            Stmt::Expression(Expr::If {
                condition,
                then_branch,
                else_branch,
            }) => {
                self.analyze_expr(condition, in_spawn);
                if let Expr::Block { statements, .. } = then_branch.as_ref() {
                    for s in statements {
                        self.analyze_stmt(s, in_spawn);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let Expr::Block { statements, .. } = else_stmts.as_ref() {
                        for s in statements {
                            self.analyze_stmt(s, in_spawn);
                        }
                    }
                }
            }
            Stmt::Expression(Expr::For {
                var,
                iterable,
                body,
                ..
            }) => {
                if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        Self::merge_access_map(accesses, var, AccessType::ReadWrite);
                    }
                } else {
                    Self::merge_access_map(&mut self.main_accesses, var, AccessType::ReadWrite);
                }
                self.analyze_expr(iterable, in_spawn);
                for s in body {
                    self.analyze_stmt(s, in_spawn);
                }
            }
            Stmt::Expression(Expr::While {
                condition, body, ..
            }) => {
                self.analyze_expr(condition, in_spawn);
                for s in body {
                    self.analyze_stmt(s, in_spawn);
                }
            }
            Stmt::Print { expr } => {
                self.analyze_expr(expr, in_spawn);
            }
            Stmt::Parallel { blocks } => {
                for block in blocks {
                    let mut block_accesses = HashMap::new();
                    for s in block {
                        self.analyze_stmt_in_collection(s, &mut block_accesses);
                    }
                    self.spawned_accesses.push(block_accesses);
                }
            }
            Stmt::Expression(expr) => {
                self.analyze_expr(expr, in_spawn);
            }
            _ => {}
        }

        self.scope_depth -= 1;
    }

    fn analyze_stmt_in_collection(
        &mut self,
        stmt: &Stmt,
        accesses: &mut HashMap<String, AccessType>,
    ) {
        match stmt {
            Stmt::Assign { name, value } => {
                Self::merge_access_map(accesses, name, AccessType::Write);
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::VarDecl { name, value, mutable, .. } => {
                if *mutable {
                    Self::merge_access_map(accesses, name, AccessType::Write);
                }
                self.collect_expr_accesses(value, accesses);
            }
            Stmt::Print { expr } => {
                self.collect_expr_accesses(expr, accesses);
            }
            Stmt::Expression(Expr::If {
                condition,
                then_branch,
                else_branch,
            }) => {
                self.collect_expr_accesses(condition, accesses);
                if let Expr::Block { statements, .. } = then_branch.as_ref() {
                    for s in statements {
                        self.analyze_stmt_in_collection(s, accesses);
                    }
                }
                if let Some(else_stmts) = else_branch {
                    if let Expr::Block { statements, .. } = else_stmts.as_ref() {
                        for s in statements {
                            self.analyze_stmt_in_collection(s, accesses);
                        }
                    }
                }
            }
            Stmt::Expression(Expr::While {
                condition, body, ..
            }) => {
                self.collect_expr_accesses(condition, accesses);
                for s in body {
                    self.analyze_stmt_in_collection(s, accesses);
                }
            }
            Stmt::Expression(Expr::For {
                var,
                iterable,
                body,
                ..
            }) => {
                Self::merge_access_map(accesses, var, AccessType::ReadWrite);
                self.collect_expr_accesses(iterable, accesses);
                for s in body {
                    self.analyze_stmt_in_collection(s, accesses);
                }
            }
            _ => {}
        }
    }

    fn collect_expr_accesses(&mut self, expr: &Expr, accesses: &mut HashMap<String, AccessType>) {
        match expr {
            Expr::Var(name, _) => {
                Self::merge_access_map(accesses, name, AccessType::Read);
            }
            Expr::Binary { left, right, .. } => {
                self.collect_expr_accesses(left, accesses);
                self.collect_expr_accesses(right, accesses);
            }
            Expr::Unary { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.collect_expr_accesses(arg, accesses);
                }
            }
            Expr::ArrayAccess {
                array: collection,
                index,
                ..
            } => {
                self.collect_expr_accesses(collection, accesses);
                self.collect_expr_accesses(index, accesses);
            }
            Expr::List(elements) => {
                for elem in elements {
                    self.collect_expr_accesses(elem, accesses);
                }
            }
            Expr::Deref { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            Expr::AddrOf { expr, .. } => {
                self.collect_expr_accesses(expr, accesses);
            }
            _ => {}
        }
    }

    fn merge_access_map(map: &mut HashMap<String, AccessType>, key: &str, new_access: AccessType) {
        map.entry(key.to_string())
            .and_modify(|existing| Self::merge_access(existing, new_access.clone()))
            .or_insert(new_access);
    }

    fn merge_access(existing: &mut AccessType, new: AccessType) {
        *existing = match (&*existing, &new) {
            (AccessType::Read, AccessType::Read) => AccessType::Read,
            (AccessType::Read, AccessType::Write) => AccessType::ReadWrite,
            (AccessType::Write, AccessType::Read) => AccessType::ReadWrite,
            (AccessType::ReadWrite, _) => AccessType::ReadWrite,
            (_, AccessType::ReadWrite) => AccessType::ReadWrite,
            (AccessType::Write, AccessType::Write) => AccessType::Write,
        };
    }

    fn analyze_expr(&mut self, expr: &Expr, in_spawn: bool) {
        match expr {
            Expr::Var(name, _) => {
                let target = if in_spawn {
                    if let Some(accesses) = self.spawned_accesses.last_mut() {
                        accesses
                    } else {
                        return;
                    }
                } else {
                    &mut self.main_accesses
                };
                Self::merge_access_map(target, name, AccessType::Read);
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left, in_spawn);
                self.analyze_expr(right, in_spawn);
            }
            Expr::Unary { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            Expr::FunctionCall { args, .. } => {
                for arg in args {
                    self.analyze_expr(arg, in_spawn);
                }
            }
            Expr::ArrayAccess {
                array: collection,
                index,
                ..
            } => {
                self.analyze_expr(collection, in_spawn);
                self.analyze_expr(index, in_spawn);
            }
            Expr::List(elements) => {
                for elem in elements {
                    self.analyze_expr(elem, in_spawn);
                }
            }
            Expr::Deref { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            Expr::AddrOf { expr, .. } => {
                self.analyze_expr(expr, in_spawn);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
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
}

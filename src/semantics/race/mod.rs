// algol26/src/race/mod.rs

use crate::frontend::ast::{Expr, FunctionDecl, Stmt};
use std::collections::HashMap;

mod collect; 
mod analyze; 
#[cfg(test)] mod tests;


#[derive(Debug, Clone)]
pub struct RaceDetector {
    // Track variables accessed in spawned blocks
    pub(super) spawned_accesses: Vec<HashMap<String, AccessType>>,
    // Track variables accessed in main thread
    pub(super) main_accesses: HashMap<String, AccessType>,
    // Track variable declarations and their mutability
    pub(super) variable_mutability: HashMap<String, bool>, // true = mutable, false = immutable
    // Track scope depth for proper analysis
    pub(super) scope_depth: usize,
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
}


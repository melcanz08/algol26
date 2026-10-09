// algol26/src/race/mod.rs

use crate::frontend::ast::{Expr, ExprKind, FunctionDecl, Stmt};
use std::collections::HashMap;

mod analyze;
mod collect;
#[cfg(test)]
mod tests;

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
        // Names already flagged for a concrete race, so the
        // conservative pass below can skip them and avoid duplicate
        // messages.
        let mut flagged_vars: std::collections::HashSet<String> = std::collections::HashSet::new();

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
                        flagged_vars.insert(var.clone());
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
                                flagged_vars.insert(var.clone());
                            }
                        }
                    }
                }
            }
        }

        // Conservative pass: a `var` binding visible to both main
        // and a spawn is treated as a potential race even when the
        // only recorded accesses are reads. Rationale: the detector
        // is per-function and does not follow writes through
        // function calls, so it cannot prove that no spawn-visible
        // mutation exists in code it did not analyze. `val` bindings
        // are exempt — a `val` is written exactly once at
        // declaration, before any spawn can observe it, and never
        // reassigned. See `test_val_sharing_is_not_a_race` and
        // `test_var_sharing_with_spawn_is_conservatively_flagged`.
        //
        // This replaces the previous mechanism (recording the
        // declaration itself as a Write), which produced the same
        // rejection but with a misleading message that named a
        // Write that does not exist in the source. The policy is
        // unchanged; the diagnostic now describes the situation
        // that actually triggered it.
        //
        // The conservative flag is removed when ADR 0044 lands:
        // place-based tracking can distinguish a read of `x` from
        // an indirect write through `f(&mut x)`, which the current
        // name-based model cannot.
        for spawned in &self.spawned_accesses {
            for var in spawned.keys() {
                if self.main_accesses.contains_key(var)
                    && self.variable_mutability.get(var).copied().unwrap_or(false)
                    && !flagged_vars.contains(var)
                {
                    races.push(format!(
                        "variable '{}' is a `var` binding shared between main                          and a spawn. The race detector cannot prove that no                          concurrent mutation reaches it — it does not follow                          writes through function calls. Change the binding to                          `val` if it is not reassigned, or restructure to                          avoid sharing a mutable binding across a spawn.",
                        var
                    ));
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

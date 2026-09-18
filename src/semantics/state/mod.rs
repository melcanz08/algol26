// src/semantics/state/mod.rs
// v0.9-B — Semantic state with region hierarchy + escape tracking
// Extends v0.9-A with proper outlives logic

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarState {
    Uninitialized,
    MaybeUninitialized,
    Available,
    Moved,
    MaybeMoved,
}

impl VarState {
    pub fn join(self, other: Self) -> Self {
        use VarState::*;
        match (self, other) {
            (a, b) if a == b => a,
            (Uninitialized, Available) | (Available, Uninitialized) => MaybeUninitialized,
            (Available, Moved) | (Moved, Available) => MaybeMoved,
            (Uninitialized, Moved) | (Moved, Uninitialized) => MaybeMoved,
            (MaybeMoved, _) | (_, MaybeMoved) => MaybeMoved,
            (MaybeUninitialized, _) | (_, MaybeUninitialized) => MaybeUninitialized,
            _ => MaybeMoved,
        }
    }
    pub fn is_available(&self) -> bool {
        matches!(self, VarState::Available)
    }
    pub fn is_moved(&self) -> bool {
        matches!(self, VarState::Moved | VarState::MaybeMoved)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    Shared,
    Mutable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BorrowLifetime {
    Temporary(CallId),
    Local(String),
    Region(String),
    Static,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageLifetime {
    Local(String),
    Region(String),
    Static,
}

#[derive(Debug, Clone)]
pub struct BorrowState {
    pub place: String,
    pub kind: BorrowKind,
    pub lifetime: BorrowLifetime,
    pub created_at: NodeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionState {
    Active,
    Freed,
    MaybeFreed,
}

#[derive(Debug, Clone, Default)]
pub struct EscapeSet {
    pub escapes: HashMap<String, HashSet<String>>,
}

impl EscapeSet {
    pub fn mark_escape(&mut self, from: String, to: String) {
        self.escapes.entry(from).or_default().insert(to);
    }
    pub fn escapes(&self, var: &str) -> bool {
        self.escapes.contains_key(var)
    }
    pub fn get_escapes(&self, var: &str) -> Option<&HashSet<String>> {
        self.escapes.get(var)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SemanticState {
    pub vars: HashMap<String, VarState>,
    pub borrows: HashMap<String, BorrowState>,
    pub regions: HashMap<String, RegionState>,
    pub escapes: EscapeSet,
    // v0.9-B: region hierarchy
    pub region_stack: Vec<String>,
    pub region_parent: HashMap<String, Option<String>>,
    pub var_region: HashMap<String, String>, // var -> innermost region at declaration
    pub next_call_id: u64,
    pub next_node_id: u64,
}

impl SemanticState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fresh_call_id(&mut self) -> CallId {
        let id = CallId(self.next_call_id);
        self.next_call_id += 1;
        id
    }

    pub fn fresh_node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn current_region(&self) -> Option<&String> {
        self.region_stack.last()
    }

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        if ancestor == descendant {
            return true;
        }
        let mut current = self.region_parent.get(descendant);
        while let Some(Some(parent)) = current {
            if parent == ancestor {
                return true;
            }
            current = self.region_parent.get(parent);
        }
        false
    }

    pub fn join(a: &Self, b: &Self) -> Self {
        let mut vars = HashMap::new();
        let all_keys: HashSet<String> = a.vars.keys().chain(b.vars.keys()).cloned().collect();
        for k in all_keys {
            let va = a.vars.get(&k).copied().unwrap_or(VarState::Uninitialized);
            let vb = b.vars.get(&k).copied().unwrap_or(VarState::Uninitialized);
            vars.insert(k, va.join(vb));
        }

        let mut borrows = HashMap::new();
        for k in a.vars.keys() {
            if let (Some(ba), Some(bb)) = (a.borrows.get(k), b.borrows.get(k)) {
                if ba.place == bb.place && ba.kind == bb.kind {
                    borrows.insert(k.clone(), ba.clone());
                }
            }
        }

        let mut regions = HashMap::new();
        let all_regions: HashSet<String> = a.regions.keys().chain(b.regions.keys()).cloned().collect();
        for r in all_regions {
            let ra = a.regions.get(&r);
            let rb = b.regions.get(&r);
            let joined = match (ra, rb) {
                (Some(RegionState::Active), Some(RegionState::Active)) => RegionState::Active,
                (Some(RegionState::Freed), Some(RegionState::Freed)) => RegionState::Freed,
                (Some(_), Some(_)) => RegionState::MaybeFreed,
                (Some(s), None) | (None, Some(s)) => s.clone(),
                (None, None) => continue,
            };
            regions.insert(r, joined);
        }

        let mut escapes = EscapeSet::default();
        for (k, v) in &a.escapes.escapes {
            for dest in v {
                escapes.mark_escape(k.clone(), dest.clone());
            }
        }
        for (k, v) in &b.escapes.escapes {
            for dest in v {
                escapes.mark_escape(k.clone(), dest.clone());
            }
        }

        // v0.9-B: join region hierarchy - keep common parents, merge stacks as longest common prefix
        let mut region_parent = a.region_parent.clone();
        for (k, v) in &b.region_parent {
            region_parent.entry(k.clone()).or_insert(v.clone());
        }

        let mut var_region = HashMap::new();
        for (k, ra) in &a.var_region {
            if let Some(rb) = b.var_region.get(k) {
                if ra == rb {
                    var_region.insert(k.clone(), ra.clone());
                }
            }
        }

        let region_stack = {
            let mut common = Vec::new();
            for (aa, bb) in a.region_stack.iter().zip(b.region_stack.iter()) {
                if aa == bb {
                    common.push(aa.clone());
                } else {
                    break;
                }
            }
            common
        };

        SemanticState {
            vars,
            borrows,
            regions,
            escapes,
            region_stack,
            region_parent,
            var_region,
            next_call_id: a.next_call_id.max(b.next_call_id),
            next_node_id: a.next_node_id.max(b.next_node_id),
        }
    }

    // --- Transfer helpers ---

    pub fn declare(&mut self, name: String, state: VarState) {
        if let Some(cur) = self.current_region().cloned() {
            self.var_region.insert(name.clone(), cur);
        }
        self.vars.insert(name, state);
    }

    pub fn move_out(&mut self, name: &str) {
        self.vars.insert(name.to_string(), VarState::Moved);
        self.borrows.retain(|_, b| b.place != name);
    }

    /// Record that `borrower` holds a borrow of `place`.
    ///
    /// The borrower's region is *not* set here — a borrow expression
    /// does not relocate the borrower into the current region. The
    /// borrower's region is fixed by `declare` when the variable is
    /// first introduced. Setting it here would incorrectly attribute
    /// a local declared outside a region to that region if the local
    /// happened to be borrowed inside it.
    pub fn borrow(&mut self, borrower: String, place: String, kind: BorrowKind, lifetime: BorrowLifetime) {
        let node = self.fresh_node_id();
        self.borrows.insert(borrower, BorrowState { place, kind, lifetime, created_at: node });
    }

    pub fn borrow_temporary(&mut self, _place: String, _kind: BorrowKind) -> BorrowLifetime {
        let call_id = self.fresh_call_id();
        BorrowLifetime::Temporary(call_id)
    }

    pub fn end_temporary_borrows(&mut self, call_id: CallId) {
        self.borrows.retain(|_, b| b.lifetime != BorrowLifetime::Temporary(call_id));
    }

    pub fn enter_region(&mut self, name: String) {
        let parent = self.current_region().cloned();
        self.region_parent.insert(name.clone(), parent);
        self.region_stack.push(name.clone());
        self.regions.insert(name, RegionState::Active);
    }

    pub fn exit_region(&mut self, name: &str) -> Vec<String> {
        // Returns list of places whose borrows outlive this region (for diagnostics)
        let mut outliving = Vec::new();
        self.regions.insert(name.to_string(), RegionState::Freed);
        
        // Pop from stack if top matches
        if self.region_stack.last().map(|s| s.as_str()) == Some(name) {
            self.region_stack.pop();
        } else {
            self.region_stack.retain(|r| r != name);
        }

        // Check borrows that outlive the freed region
        for (borrower, bstate) in &self.borrows {
            let storage_lifetime = self.storage_lifetime_of(&bstate.place);
            let borrow_lifetime = &bstate.lifetime;
            
            // If storage is in the region being freed and borrow lives longer -> error
            if let StorageLifetime::Region(storage_reg) = &storage_lifetime {
                if (storage_reg == name || self.is_ancestor(name, storage_reg))
                    && borrow_lifetime.outlives_region(&storage_lifetime, self)
                {
                    outliving.push(format!("{} (borrowed by {} lives in {:?} but storage in {} freed)", bstate.place, borrower, borrow_lifetime, name));
                }
            }
        }

        outliving
    }

    pub fn storage_lifetime_of(&self, place: &str) -> StorageLifetime {
        if let Some(reg) = self.var_region.get(place) {
            StorageLifetime::Region(reg.clone())
        } else {
            StorageLifetime::Local(place.to_string())
        }
    }

    pub fn is_borrowed(&self, place: &str) -> bool {
        self.borrows.values().any(|b| b.place == place)
    }

    pub fn is_mutably_borrowed(&self, place: &str) -> bool {
        self.borrows.values().any(|b| b.place == place && b.kind == BorrowKind::Mutable)
    }

    pub fn mark_escape(&mut self, from: String, to: String) {
        self.escapes.mark_escape(from, to);
    }
}

impl BorrowLifetime {
    pub fn outlives(&self, _storage: &StorageLifetime) -> bool {
        // Legacy simple version - kept for backward compat tests
        match self {
            BorrowLifetime::Temporary(_) => false,
            BorrowLifetime::Local(_) => false,
            BorrowLifetime::Region(_) => false,
            BorrowLifetime::Static => true,
        }
    }

    pub fn outlives_region(&self, storage: &StorageLifetime, state: &SemanticState) -> bool {
        match (self, storage) {
            (BorrowLifetime::Static, _) => true,
            (BorrowLifetime::Temporary(_), _) => false,
            // Borrow in outer region, storage in inner region that is being freed
            (BorrowLifetime::Region(borrow_reg), StorageLifetime::Region(storage_reg)) => {
                if borrow_reg == storage_reg {
                    return false; // same region, freed together
                }
                // If borrow region is ancestor of storage region, it outlives storage
                state.is_ancestor(borrow_reg, storage_reg)
            }
            (BorrowLifetime::Region(borrow_reg), StorageLifetime::Local(storage_var)) => {
                if let Some(storage_reg) = state.var_region.get(storage_var) {
                    if borrow_reg == storage_reg {
                        return false;
                    }
                    return state.is_ancestor(borrow_reg, storage_reg);
                }
                // Local storage with no region - if borrow is region, it might outlive function?
                // For simplicity, region borrows outlive locals declared outside region? No.
                // Actually local declared outside region should be ok.
                false
            }
            (BorrowLifetime::Local(borrower_var), StorageLifetime::Region(storage_reg)) => {
                if let Some(borrow_reg) = state.var_region.get(borrower_var) {
                    if borrow_reg == storage_reg {
                        return false;
                    }
                    return state.is_ancestor(borrow_reg, storage_reg);
                }
                // Borrower is local with no region (outside), storage is inside region being freed
                // If borrower is outside the freed region, it outlives storage
                true
            }
            (BorrowLifetime::Local(borrower_var), StorageLifetime::Local(storage_var)) => {
                // Both locals - check regions
                let borrower_reg = state.var_region.get(borrower_var);
                let storage_reg = state.var_region.get(storage_var);
                match (borrower_reg, storage_reg) {
                    (Some(br), Some(sr)) => {
                        if br == sr {
                            return false;
                        }
                        state.is_ancestor(br, sr)
                    }
                    (None, Some(_)) => {
                        // borrower outside any region, storage inside -> outlives
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_available_moved_is_maybe_moved() {
        let mut a = SemanticState::new();
        a.declare("x".into(), VarState::Available);
        let mut b = SemanticState::new();
        b.declare("x".into(), VarState::Moved);
        let j = SemanticState::join(&a, &b);
        assert_eq!(j.vars["x"], VarState::MaybeMoved);
    }

    #[test]
    fn temporary_borrow_does_not_escape() {
        let mut s = SemanticState::new();
        s.declare("x".into(), VarState::Available);
        let lt = s.borrow_temporary("x".into(), BorrowKind::Mutable);
        assert!(!lt.outlives(&StorageLifetime::Local("x".into())));
    }

    #[test]
    fn move_ends_borrow() {
        let mut s = SemanticState::new();
        s.declare("x".into(), VarState::Available);
        s.borrow("r".into(), "x".into(), BorrowKind::Mutable, BorrowLifetime::Local("r".into()));
        assert!(s.is_borrowed("x"));
        s.move_out("x");
        assert!(!s.is_borrowed("x"));
    }

    // v0.9-B tests
    #[test]
    fn region_enter_exit() {
        let mut s = SemanticState::new();
        s.enter_region("r1".into());
        assert_eq!(s.current_region(), Some(&"r1".to_string()));
        s.declare("x".into(), VarState::Available);
        assert_eq!(s.var_region.get("x"), Some(&"r1".to_string()));
        let outliving = s.exit_region("r1");
        assert!(outliving.is_empty());
        assert_eq!(s.regions.get("r1"), Some(&RegionState::Freed));
    }

    #[test]
    fn region_borrow_outlives_inner_storage() {
        let mut s = SemanticState::new();
        s.enter_region("outer".into());
        s.declare("r".into(), VarState::Available); // r in outer
        s.enter_region("inner".into());
        s.declare("x".into(), VarState::Available); // x in inner
        // r borrows x, but r lives in outer, x in inner
        s.borrow("r".into(), "x".into(), BorrowKind::Shared, BorrowLifetime::Region("outer".into()));
        
        let outliving = s.exit_region("inner");
        assert!(!outliving.is_empty(), "borrow in outer should outlive inner storage: {:?}", outliving);
    }

    #[test]
    fn same_region_borrow_ok() {
        let mut s = SemanticState::new();
        s.enter_region("r1".into());
        s.declare("x".into(), VarState::Available);
        s.declare("r".into(), VarState::Available);
        s.borrow("r".into(), "x".into(), BorrowKind::Shared, BorrowLifetime::Region("r1".into()));
        
        let outliving = s.exit_region("r1");
        assert!(outliving.is_empty(), "same region borrow should not outlive");
    }

    #[test]
    fn escape_tracking() {
        let mut s = SemanticState::new();
        s.declare("x".into(), VarState::Available);
        s.mark_escape("x".into(), "return".into());
        assert!(s.escapes.escapes("x"));
        assert!(s.escapes.get_escapes("x").unwrap().contains("return"));
    }

    #[test]
    fn local_outside_outlives_inner() {
        let mut s = SemanticState::new();
        s.declare("r".into(), VarState::Available); // r outside
        s.enter_region("inner".into());
        s.declare("x".into(), VarState::Available); // x inside
        s.borrow("r".into(), "x".into(), BorrowKind::Shared, BorrowLifetime::Local("r".into()));
        
        let outliving = s.exit_region("inner");
        assert!(!outliving.is_empty(), "local outside borrowing inner should outlive");
    }
}
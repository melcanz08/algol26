// src/ir/cfg/dataflow.rs

use crate::semantics::state::{SemanticState, VarState};
use std::collections::{HashMap, HashSet, VecDeque};

pub type BlockId = usize;
#[derive(Debug, Clone, Default)]
pub struct Cfg {
    pub entry: BlockId,
    pub blocks: HashMap<BlockId, CfgBlock>,
    pub successors: HashMap<BlockId, Vec<BlockId>>,
    pub predecessors: HashMap<BlockId, Vec<BlockId>>,
}
#[derive(Debug, Clone)]
pub struct CfgBlock {
    pub id: BlockId,
    pub instructions: Vec<CfgInstruction>,
}
/// ADR 0016: one function's control-flow graph plus the metadata
/// the dataflow engine needs to seed its entry state. Block IDs are
/// the function's `SemanticProgram`-local IDs; `cfg.entry` is the
/// function's declared entry block.
#[derive(Debug, Clone)]
pub struct FunctionCfg {
    pub name: String,
    pub cfg: Cfg,
    /// Parameter names in declaration order. Seeded into the entry
    /// state as `VarState::available()` so the first `Use` of a
    /// parameter is not a spurious `E-INIT-001`.
    pub params: Vec<String>,
}
#[derive(Debug, Clone)]
pub enum CfgInstruction {
    Declare {
        name: String,
    },
    Assign {
        name: String,
    },
    Move {
        name: String,
    },
    Borrow {
        borrower: String,
        place: String,
        mutable: bool,
    },
    Call {
        name: String,
        args: Vec<String>,
    },
    Use {
        name: String,
    },
    RegionEnter {
        name: String,
    },
    RegionExit {
        name: String,
    },
    ReturnRef {
        place: String,
    },
    Escape {
        from: String,
        to: String,
    },
    Unsupported {
        op: String,
    },
    Return,
    Branch {
        condition: String,
    },
    Nop,
}
impl Cfg {
    pub fn new(entry: BlockId) -> Self {
        Self {
            entry,
            ..Default::default()
        }
    }
    pub fn add_block(&mut self, block: CfgBlock) {
        let id = block.id;
        self.blocks.insert(id, block);
        self.successors.entry(id).or_default();
        self.predecessors.entry(id).or_default();
    }
    pub fn add_edge(&mut self, from: BlockId, to: BlockId) {
        self.successors.entry(from).or_default().push(to);
        self.predecessors.entry(to).or_default().push(from);
    }
    pub fn successors(&self, id: BlockId) -> &[BlockId] {
        self.successors
            .get(&id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
    pub fn predecessors(&self, id: BlockId) -> &[BlockId] {
        self.predecessors
            .get(&id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}
pub trait Transfer {
    fn transfer(&self, block: &CfgBlock, incoming: SemanticState) -> TransferResult;
}
#[derive(Debug, Clone)]
pub struct TransferResult {
    pub outgoing: SemanticState,
    pub diagnostics: Vec<DataflowDiagnostic>,
}
#[derive(Debug, Clone)]
pub struct DataflowDiagnostic {
    pub message: String,
    pub block: BlockId,
    pub is_error: bool,
}
pub struct OwnershipTransfer;
impl Transfer for OwnershipTransfer {
    fn transfer(&self, block: &CfgBlock, mut incoming: SemanticState) -> TransferResult {
        let mut diags = Vec::new();
        for instr in &block.instructions {
            match instr {
                CfgInstruction::Declare { name } => {
                    incoming.declare(name.clone(), crate::semantics::state::VarState::available());
                }
                CfgInstruction::Assign { name } => {
                    if incoming.is_mutably_borrowed(name) {
                        diags.push(DataflowDiagnostic {
                            message: format!(
                                "E-BORROW-004: Cannot assign to '{}' while mutably borrowed",
                                name
                            ),
                            block: block.id,
                            is_error: true,
                        });
                    }
                    // Match the analyzer's rule: assignment to a
                    // moved or uninitialized target is rejected.
                    // The analyzer catches this at the AST level;
                    // this makes the dataflow layer agree so that
                    // any future producer bypassing the analyzer
                    // still gets the rejection.
                    if let Some(st) = incoming.vars.get(name) {
                        if st.ownership == crate::semantics::state::OwnershipState::Moved {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-002: Cannot assign to moved variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        } else if st.ownership
                            == crate::semantics::state::OwnershipState::MaybeMoved
                        {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-002: Cannot assign to maybe-moved variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        if st.init == crate::semantics::state::InitState::Uninitialized {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-INIT-001: Cannot assign to uninitialized variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                    }
                    // A successful assignment re-establishes the
                    // target as Available. Done unconditionally so
                    // the transfer function is total: whether or
                    // not an error was emitted, downstream blocks
                    // see a defined state.
                    incoming
                        .vars
                        .insert(name.clone(), crate::semantics::state::VarState::available());
                }
                CfgInstruction::Move { name } => {
                    if incoming.is_borrowed(name) {
                        diags.push(DataflowDiagnostic {
                            message: format!("E-MOVE-002: Cannot move '{}' while borrowed", name),
                            block: block.id,
                            is_error: true,
                        });
                    }
                    // Reject move of a source that is already moved
                    // or never initialized. Every current producer
                    // of `Move` (only `Free`) emits a preceding
                    // `Use`, which catches these — but the check
                    // belongs on `Move` itself so the transfer is
                    // complete and independent of producer shape.
                    if let Some(st) = incoming.vars.get(name) {
                        if st.ownership == crate::semantics::state::OwnershipState::Moved {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-002: Cannot assign to moved variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        } else if st.ownership
                            == crate::semantics::state::OwnershipState::MaybeMoved
                        {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-002: Cannot assign to maybe-moved variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        if st.init == crate::semantics::state::InitState::Uninitialized {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-INIT-001: Cannot assign to uninitialized variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                    }
                    incoming.move_out(name);
                }
                CfgInstruction::Borrow {
                    borrower,
                    place,
                    mutable,
                } => {
                    if let Some(st) = incoming.vars.get(place) {
                        if st.is_moved() {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-001: Cannot borrow moved value '{}'",
                                    place
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        if st.is_uninitialized() {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-INIT-001: Cannot borrow uninitialized '{}'",
                                    place
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                    }
                    let kind = if *mutable {
                        crate::semantics::state::BorrowKind::Mutable
                    } else {
                        crate::semantics::state::BorrowKind::Shared
                    };
                    let lt = if borrower.starts_with("__tmp_call_") {
                        incoming.borrow_temporary(place.clone(), kind)
                    } else if let Some(borrower_reg) = incoming.var_region.get(borrower).cloned() {
                        // FIX: borrow lifetime = borrower's declaration region (outer), not current (inner)
                        // This makes r in outer borrowing x in inner correctly outlive
                        crate::semantics::state::BorrowLifetime::Region(borrower_reg)
                    } else if let Some(cur) = incoming.current_region().cloned() {
                        crate::semantics::state::BorrowLifetime::Region(cur)
                    } else {
                        crate::semantics::state::BorrowLifetime::Local(borrower.clone())
                    };
                    incoming.borrow(borrower.clone(), place.clone(), kind, lt);
                }
                CfgInstruction::Call { name: _, args } => {
                    let mut to_end = Vec::new();
                    for b_state in incoming.borrows.values() {
                        if let crate::semantics::state::BorrowLifetime::Temporary(id) =
                            b_state.lifetime
                        {
                            if args.contains(&b_state.place) {
                                to_end.push(id);
                            }
                        }
                    }
                    for id in to_end {
                        incoming.end_temporary_borrows(id);
                    }
                }
                CfgInstruction::RegionEnter { name } => {
                    incoming.enter_region(name.clone());
                }
                CfgInstruction::RegionExit { name } => {
                    let outliving = incoming.exit_region(name);
                    for msg in outliving {
                        diags.push(DataflowDiagnostic {
                            message: format!(
                                "E-REGION-001: Reference outlives region '{}': {}",
                                name, msg
                            ),
                            block: block.id,
                            is_error: true,
                        });
                    }
                }
                CfgInstruction::ReturnRef { place } => {
                    incoming.mark_escape(place.clone(), "return".into());
                    diags.push(DataflowDiagnostic {
                        message: format!("E-ESCAPE-001: Reference '{}' escapes via return", place),
                        block: block.id,
                        is_error: true,
                    });
                }
                CfgInstruction::Escape { from, to } => {
                    incoming.mark_escape(from.clone(), to.clone());
                    diags.push(DataflowDiagnostic {
                        message: format!("E-ESCAPE-002: Reference '{}' escapes to {}", from, to),
                        block: block.id,
                        is_error: true,
                    });
                }
                CfgInstruction::Use { name } => {
                    if let Some(state) = incoming.vars.get(name) {
                        // Two independent checks. A variable that is
                        // both maybe-uninitialized and maybe-moved
                        // now produces both diagnostics, which the
                        // compressed single-enum state could not
                        // express.
                        match state.init {
                            crate::semantics::state::InitState::Uninitialized => {
                                diags.push(DataflowDiagnostic {
                                    message: format!("E-INIT-001: Use of uninitialized '{}'", name),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            crate::semantics::state::InitState::MaybeUninitialized => {
                                diags.push(DataflowDiagnostic {
                                    message: format!(
                                        "E-INIT-002: Use of maybe-uninitialized '{}'",
                                        name
                                    ),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            crate::semantics::state::InitState::Initialized => {}
                        }
                        match state.ownership {
                            crate::semantics::state::OwnershipState::Moved => {
                                diags.push(DataflowDiagnostic {
                                    message: format!("E-MOVE-001: Use of moved '{}'", name),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            crate::semantics::state::OwnershipState::MaybeMoved => {
                                diags.push(DataflowDiagnostic {
                                    message: format!("E-MOVE-002: Use of maybe-moved '{}'", name),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            crate::semantics::state::OwnershipState::Owned => {}
                        }
                    }
                    if incoming.is_mutably_borrowed(name) {
                        diags.push(DataflowDiagnostic {
                            message: format!(
                                "E-BORROW-004: Cannot use '{}' while mutably borrowed",
                                name
                            ),
                            block: block.id,
                            is_error: true,
                        });
                    }
                }
                CfgInstruction::Unsupported { op } => {
                    diags.push(DataflowDiagnostic { message: format!("E-UNSUPPORTED-001: Unsupported IR operation in dataflow: {} (H fail-closed)", op), block: block.id, is_error: true, });
                }
                CfgInstruction::Nop | CfgInstruction::Return | CfgInstruction::Branch { .. } => {}
            }
        }
        TransferResult {
            outgoing: incoming,
            diagnostics: diags,
        }
    }
}
pub struct DataflowEngine<T: Transfer> {
    pub transfer: T,
}
impl<T: Transfer> DataflowEngine<T> {
    pub fn new(transfer: T) -> Self {
        Self { transfer }
    }
    /// ADR 0016: single-CFG entry point. Uses an empty entry state.
    /// Retained for tests that construct a `Cfg` directly; production
    /// callers use `run_all`.
    pub fn run(&self, cfg: &Cfg) -> DataflowResult {
        self.run_with_entry(cfg, SemanticState::new())
    }

    /// ADR 0016: run the dataflow analysis for every function
    /// independently, seeding each function's entry state from its
    /// declared parameters. Diagnostics are tagged with the
    /// function name and deduplicated, because the worklist may
    /// re-process a block whenever its incoming state changes.
    pub fn run_all(&self, functions: &[FunctionCfg]) -> DataflowResult {
        let mut all_in: HashMap<BlockId, SemanticState> = HashMap::new();
        let mut all_out: HashMap<BlockId, SemanticState> = HashMap::new();
        let mut visited: HashSet<BlockId> = HashSet::new();
        let mut seen: HashSet<(String, BlockId, String)> = HashSet::new();
        let mut diagnostics: Vec<DataflowDiagnostic> = Vec::new();

        for func in functions {
            let entry_state = entry_state_for(&func.params);
            let result = self.run_with_entry(&func.cfg, entry_state);
            for (id, st) in result.in_states {
                all_in.insert(id, st);
            }
            for (id, st) in result.out_states {
                all_out.insert(id, st);
            }
            for id in result.visited {
                visited.insert(id);
            }
            for diag in result.diagnostics {
                let key = (func.name.clone(), diag.block, diag.message.clone());
                if seen.insert(key) {
                    diagnostics.push(DataflowDiagnostic {
                        message: format!("[{}] {}", func.name, diag.message),
                        block: diag.block,
                        is_error: diag.is_error,
                    });
                }
            }
        }

        DataflowResult {
            in_states: all_in,
            out_states: all_out,
            diagnostics,
            visited,
        }
    }

    /// The actual worklist loop, parameterized by an explicit entry
    /// state.
    fn run_with_entry(&self, cfg: &Cfg, entry_state: SemanticState) -> DataflowResult {
        let mut in_states: HashMap<BlockId, SemanticState> = HashMap::new();
        let mut out_states: HashMap<BlockId, SemanticState> = HashMap::new();
        let mut all_diagnostics = Vec::new();
        let mut worklist: VecDeque<BlockId> = VecDeque::from([cfg.entry]);
        let mut visited: HashSet<BlockId> = HashSet::new();

        while let Some(block_id) = worklist.pop_front() {
            let block = match cfg.blocks.get(&block_id) {
                Some(b) => b,
                None => continue,
            };
            let incoming = if block_id == cfg.entry {
                in_states
                    .get(&block_id)
                    .cloned()
                    .unwrap_or_else(|| entry_state.clone())
            } else {
                let preds = cfg.predecessors(block_id);
                if preds.is_empty() {
                    SemanticState::new()
                } else {
                    let mut joined: Option<SemanticState> = None;
                    for pred in preds {
                        if let Some(out) = out_states.get(pred) {
                            joined = Some(match joined {
                                None => out.clone(),
                                Some(j) => SemanticState::join(&j, out),
                            });
                        }
                    }
                    joined.unwrap_or_default()
                }
            };
            in_states.insert(block_id, incoming.clone());
            let result = self.transfer.transfer(block, incoming);
            all_diagnostics.extend(result.diagnostics);
            let changed = match out_states.get(&block_id) {
                None => true,
                Some(prev) => !Self::states_equal(prev, &result.outgoing),
            };
            if changed {
                out_states.insert(block_id, result.outgoing.clone());
                for succ in cfg.successors(block_id) {
                    if !worklist.contains(succ) {
                        worklist.push_back(*succ);
                    }
                }
            }
            visited.insert(block_id);
        }

        DataflowResult {
            in_states,
            out_states,
            diagnostics: all_diagnostics,
            visited,
        }
    }
    fn states_equal(a: &SemanticState, b: &SemanticState) -> bool {
        a.vars == b.vars
            && a.borrows.len() == b.borrows.len()
            && a.borrows.iter().all(|(k, v)| {
                if let Some(bv) = b.borrows.get(k) {
                    bv.place == v.place && bv.kind == v.kind && bv.lifetime == v.lifetime
                } else {
                    false
                }
            })
            && a.regions == b.regions
            && a.region_stack == b.region_stack
            && a.var_region == b.var_region
            && a.escapes.escapes == b.escapes.escapes
    }
}
pub struct DataflowResult {
    pub in_states: HashMap<BlockId, SemanticState>,
    pub out_states: HashMap<BlockId, SemanticState>,
    pub diagnostics: Vec<DataflowDiagnostic>,
    pub visited: HashSet<BlockId>,
}
/// ADR 0016: seed a function's entry state from its parameter list.
/// Each parameter starts initialized and owned; mutability is an
/// analyzer-level concern and is not part of `VarState`.
fn entry_state_for(params: &[String]) -> SemanticState {
    let mut s = SemanticState::new();
    for name in params {
        s.declare(name.clone(), VarState::available());
    }
    s
}
impl DataflowResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.is_error)
    }
}
pub fn build_cfg_from_blocks(block_count: usize, edges: &[(BlockId, BlockId)]) -> Cfg {
    let mut cfg = Cfg::new(0);
    for i in 0..block_count {
        cfg.add_block(CfgBlock {
            id: i,
            instructions: vec![],
        });
    }
    for (from, to) in edges {
        cfg.add_edge(*from, *to);
    }
    cfg
}

// src/ir/cfg/dataflow.rs

use crate::semantics::state::SemanticState;
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
                    incoming.declare(name.clone(), crate::semantics::state::VarState::Available);
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
                    match incoming.vars.get(name) {
                        Some(crate::semantics::state::VarState::Moved) => {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-002: Cannot assign to moved variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        Some(crate::semantics::state::VarState::MaybeMoved) => {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-002: Cannot assign to maybe-moved variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        Some(crate::semantics::state::VarState::Uninitialized) => {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-INIT-001: Cannot assign to uninitialized variable '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        _ => {}
                    }
                    // A successful assignment re-establishes the
                    // target as Available. Done unconditionally so
                    // the transfer function is total: whether or
                    // not an error was emitted, downstream blocks
                    // see a defined state.
                    incoming
                        .vars
                        .insert(name.clone(), crate::semantics::state::VarState::Available);
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
                    match incoming.vars.get(name) {
                        Some(crate::semantics::state::VarState::Moved) => {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-001: Cannot move already-moved '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        Some(crate::semantics::state::VarState::Uninitialized) => {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-INIT-001: Cannot move uninitialized '{}'",
                                    name
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        _ => {}
                    }
                    incoming.move_out(name);
                }
                CfgInstruction::Borrow {
                    borrower,
                    place,
                    mutable,
                } => {
                    if let Some(st) = incoming.vars.get(place) {
                        if matches!(
                            st,
                            crate::semantics::state::VarState::Moved
                                | crate::semantics::state::VarState::MaybeMoved
                        ) {
                            diags.push(DataflowDiagnostic {
                                message: format!(
                                    "E-MOVE-001: Cannot borrow moved value '{}'",
                                    place
                                ),
                                block: block.id,
                                is_error: true,
                            });
                        }
                        if matches!(
                            st,
                            crate::semantics::state::VarState::Uninitialized
                                | crate::semantics::state::VarState::MaybeUninitialized
                        ) {
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
                        match state {
                            crate::semantics::state::VarState::Uninitialized => {
                                diags.push(DataflowDiagnostic {
                                    message: format!("E-INIT-001: Use of uninitialized '{}'", name),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            crate::semantics::state::VarState::MaybeUninitialized => {
                                diags.push(DataflowDiagnostic {
                                    message: format!(
                                        "E-INIT-002: Use of maybe-uninitialized '{}'",
                                        name
                                    ),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            crate::semantics::state::VarState::Moved => {
                                diags.push(DataflowDiagnostic {
                                    message: format!("E-MOVE-001: Use of moved '{}'", name),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            crate::semantics::state::VarState::MaybeMoved => {
                                diags.push(DataflowDiagnostic {
                                    message: format!("E-MOVE-002: Use of maybe-moved '{}'", name),
                                    block: block.id,
                                    is_error: true,
                                });
                            }
                            _ => {}
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
    pub fn run(&self, cfg: &Cfg, _initial: SemanticState) -> DataflowResult {
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
                in_states.get(&block_id).cloned().unwrap_or_default()
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

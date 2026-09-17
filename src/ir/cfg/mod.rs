// src/ir/cfg/mod.rs
//
// CFG module — re-exports dataflow engine + builder

pub mod dataflow;
pub mod builder;

pub use dataflow::{Cfg, CfgBlock, CfgInstruction, BlockId, DataflowEngine, OwnershipTransfer};
pub use builder::build_cfg_from_semantic_program;

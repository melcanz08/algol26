// src/ir/cfg/mod.rs
//
// CFG module — re-exports dataflow engine + builder

pub mod builder;
pub mod dataflow;

pub use builder::build_cfgs_from_semantic_program;
pub use dataflow::{
    BlockId, Cfg, CfgBlock, CfgInstruction, DataflowEngine, FunctionCfg, OwnershipTransfer,
};

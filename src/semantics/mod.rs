pub mod control_flow;
pub mod escape;
pub mod expr_translator;
pub mod flow_analyzer;
pub mod flow_result;
pub mod race;
pub mod semantic;
pub mod semantic_builder;
pub mod trait_registry;
pub mod type_checker;

#[cfg(test)] mod borrow_checker_extra_test;

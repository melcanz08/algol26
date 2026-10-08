// src/ir/vtable_gather.rs
//
// ADR 0038 D3b. Walk the IR and collect every `(trait, concrete)`
// pair reachable through a `TypedIRValue::DynTrait` value.
//
// The gather runs once, after `SemanticIRBuilder::build` returns
// and before any backend consumes the program. Its output is
// `SemanticProgram::vtables`, a map keyed by the `vtable_id` string
// the builder assigned at each coercion site. Both backends look up
// `(trait_name, concrete_type, method_names)` from that map when
// emitting dispatch.

use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticProgram, VtableEntry};
use std::collections::HashMap;

/// Populate `program.vtables`. Idempotent: calling twice produces
/// the same map. Safe to call on a program with no `dyn Trait`.
pub fn gather_vtables(program: &mut SemanticProgram) {
    // Method names in declaration order, keyed by trait name. Absent
    // traits (a `dyn Trait` referring to an unregistered trait) yield
    // an empty method list; the diagnostic that matters already fired
    // in the analyzer.
    let trait_methods: HashMap<String, Vec<String>> = program
        .trait_decls
        .iter()
        .map(|t| {
            (
                t.name.clone(),
                t.methods.iter().map(|m| m.name.clone()).collect(),
            )
        })
        .collect();

    let mut entries: HashMap<String, VtableEntry> = HashMap::new();
    for func in &program.functions {
        for block in &func.blocks {
            for instr in &block.instructions {
                walk_instruction(instr, &mut entries, &trait_methods);
            }
            if let Some(term) = &block.terminator {
                walk_terminator(term, &mut entries, &trait_methods);
            }
        }
    }
    program.vtables = entries;
}

fn record(
    vtable_id: &str,
    target_type: &Type,
    concrete_type: Type,
    entries: &mut HashMap<String, VtableEntry>,
    trait_methods: &HashMap<String, Vec<String>>,
) {
    if entries.contains_key(vtable_id) {
        return;
    }
    let (trait_id, trait_name) = match target_type {
        Type::Borrow(inner) | Type::MutBorrow(inner) => match inner.as_ref() {
            Type::DynTrait {
                trait_id,
                trait_name,
            } => (*trait_id, trait_name.clone()),
            _ => return,
        },
        _ => return,
    };
    let method_names = trait_methods.get(&trait_name).cloned().unwrap_or_default();
    entries.insert(
        vtable_id.to_string(),
        VtableEntry {
            trait_id,
            trait_name,
            concrete_type,
            method_names,
        },
    );
}

fn walk_value(
    v: &crate::ir::semantic_ir::TypedIRValue,
    entries: &mut HashMap<String, VtableEntry>,
    trait_methods: &HashMap<String, Vec<String>>,
) {
    use crate::ir::semantic_ir::TypedIRValue;
    match v {
        TypedIRValue::DynTrait {
            data,
            vtable_id,
            target_type,
        } => {
            // Recurse into `data` first so a nested `dyn Trait` inside
            // an inner expression is recorded too.
            walk_value(data, entries, trait_methods);
            let concrete = data.type_of();
            record(vtable_id, target_type, concrete, entries, trait_methods);
        }
        TypedIRValue::List(items, _) | TypedIRValue::Array(items, _, _) => {
            for item in items {
                walk_value(item, entries, trait_methods);
            }
        }
        TypedIRValue::Record { fields, .. } => {
            for (_, field) in fields {
                walk_value(field, entries, trait_methods);
            }
        }
        TypedIRValue::Map { entries: es, .. } => {
            for (k, val) in es {
                walk_value(k, entries, trait_methods);
                walk_value(val, entries, trait_methods);
            }
        }
        TypedIRValue::Some(inner) => walk_value(inner, entries, trait_methods),
        TypedIRValue::Ok { value, .. } | TypedIRValue::Error { value, .. } => {
            walk_value(value, entries, trait_methods);
        }
        TypedIRValue::Cast { value, .. } => walk_value(value, entries, trait_methods),
        TypedIRValue::BinaryOp { left, right, .. } => {
            walk_value(left, entries, trait_methods);
            walk_value(right, entries, trait_methods);
        }
        TypedIRValue::Call { args, .. } => {
            for a in args {
                walk_value(a, entries, trait_methods);
            }
        }
        TypedIRValue::ArrayAccess { array, index, .. } => {
            walk_value(array, entries, trait_methods);
            walk_value(index, entries, trait_methods);
        }
        TypedIRValue::BorrowShared { expr, .. }
        | TypedIRValue::BorrowMutable { expr, .. }
        | TypedIRValue::ReadReference { expr, .. }
        | TypedIRValue::AddrOf { expr, .. } => walk_value(expr, entries, trait_methods),
        TypedIRValue::Range(a, b) => {
            walk_value(a, entries, trait_methods);
            walk_value(b, entries, trait_methods);
        }
        TypedIRValue::FieldAccess { object, .. } => walk_value(object, entries, trait_methods),
        TypedIRValue::SetSingleton { element, .. } => walk_value(element, entries, trait_methods),
        // Leaves: Int, Float, Bool, String, Void, NullPtr, PtrLiteral,
        // Variable, None, Set.
        _ => {}
    }
}

fn walk_instruction(
    instr: &crate::ir::semantic_ir::Instruction,
    entries: &mut HashMap<String, VtableEntry>,
    trait_methods: &HashMap<String, Vec<String>>,
) {
    use crate::ir::semantic_ir::Instruction;
    match instr {
        Instruction::Declare { value, .. } | Instruction::Assign { value, .. } => {
            walk_value(value, entries, trait_methods);
        }
        Instruction::WriteReference { reference, value } => {
            walk_value(reference, entries, trait_methods);
            walk_value(value, entries, trait_methods);
        }
        Instruction::ArrayAssign {
            array,
            index,
            value,
        } => {
            walk_value(array, entries, trait_methods);
            walk_value(index, entries, trait_methods);
            walk_value(value, entries, trait_methods);
        }
        Instruction::FieldAssign { value, .. } => walk_value(value, entries, trait_methods),
        Instruction::Print { value } => walk_value(value, entries, trait_methods),
        Instruction::Call { args, .. } => {
            for a in args {
                walk_value(a, entries, trait_methods);
            }
        }
        Instruction::IteratorInit { iterable, .. } => walk_value(iterable, entries, trait_methods),
        Instruction::SendChannel { value, .. } => walk_value(value, entries, trait_methods),
        Instruction::Allocate { size, .. } => walk_value(size, entries, trait_methods),
        Instruction::Free { ptr } => walk_value(ptr, entries, trait_methods),
        Instruction::BoundsCheck { value, .. } => walk_value(value, entries, trait_methods),
        _ => {}
    }
}

fn walk_terminator(
    term: &crate::ir::semantic_ir::Terminator,
    entries: &mut HashMap<String, VtableEntry>,
    trait_methods: &HashMap<String, Vec<String>>,
) {
    use crate::ir::semantic_ir::{SemanticPattern, Terminator};
    match term {
        Terminator::Return { value: Some(v), .. } => walk_value(v, entries, trait_methods),
        Terminator::Branch { condition, .. } => walk_value(condition, entries, trait_methods),
        Terminator::Switch { value, cases, .. } => {
            walk_value(value, entries, trait_methods);
            for (pat, _) in cases {
                if let SemanticPattern::Literal(lit) = pat {
                    walk_value(lit, entries, trait_methods);
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::types::TraitId;
    use crate::frontend::ast::Visibility;
    use crate::ir::semantic_ir::{Instruction, SemanticBlock, SemanticFunction, TypedIRValue};

    fn program_with_dyn_trait(vtable_id: &str, trait_id: u32, trait_name: &str) -> SemanticProgram {
        let mut p = SemanticProgram::new();
        p.trait_decls.push(crate::frontend::ast::TraitDecl {
            name: trait_name.to_string(),
            methods: vec![crate::frontend::ast::TraitMethod {
                name: "area".to_string(),
                params: vec![],
                return_type: None,
            }],
            constants: vec![],
            visibility: Visibility::Private,
            module: None,
        });
        let target_type = Type::borrow(Type::dyn_trait(TraitId(trait_id), trait_name));
        let value = TypedIRValue::DynTrait {
            data: Box::new(TypedIRValue::Variable(
                "c".to_string(),
                Type::record("Circle", vec![]),
            )),
            vtable_id: vtable_id.to_string(),
            target_type,
        };
        let mut block = SemanticBlock::new(0);
        block.instructions.push(Instruction::Declare {
            name: "s".to_string(),
            mutable: false,
            type_: Type::borrow(Type::dyn_trait(TraitId(trait_id), trait_name)),
            value,
        });
        p.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![block],
            entry_block: 0,
            is_extern: false,
        });
        p
    }

    #[test]
    fn gather_records_pair() {
        let mut p = program_with_dyn_trait("0_Circle", 0, "Shape");
        gather_vtables(&mut p);
        assert_eq!(p.vtables.len(), 1);
        let entry = p.vtables.get("0_Circle").expect("entry");
        assert_eq!(entry.trait_id, TraitId(0));
        assert_eq!(entry.trait_name, "Shape");
        assert_eq!(entry.method_names, vec!["area".to_string()]);
    }

    #[test]
    fn gather_idempotent() {
        let mut p = program_with_dyn_trait("0_Circle", 0, "Shape");
        gather_vtables(&mut p);
        let first = p.vtables.clone();
        gather_vtables(&mut p);
        let second = p.vtables.clone();
        assert_eq!(first.len(), second.len());
        assert!(second.contains_key("0_Circle"));
    }

    #[test]
    fn gather_empty_on_plain_program() {
        let mut p = SemanticProgram::new();
        gather_vtables(&mut p);
        assert!(p.vtables.is_empty());
    }
}

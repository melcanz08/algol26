// src/backends/capabilities/scan.rs

use super::Feature;
use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::HashSet;
/// Classify a function name into a `Feature`, if it maps to one.
///
/// This function is the **dispatch half** of the capability contract.
/// The `Feature` enum documents *what each feature is*; this function
/// documents *which built-in names trigger which feature*. Keeping
/// both in sync is what makes the capability matrix trustworthy: the
/// matrix refuses a program iff the set of features it uses intersects
/// the LLVM backend's "cannot lower" set, and that set is exactly the
/// variants classified here.
///
/// # Classification rules
///
/// | Name pattern                | Feature             |
/// |-----------------------------|---------------------|
/// | `String.*` (see exclusions) | `StringFunctions`   |
/// | `File.*`                    | `FileFunctions`     |
/// | `List.sum` / `.max` / `.min`| `ListAggregates`    |
/// | anything else               | (none — permitted)  |
///
/// Other `Feature` variants — `Result`, `Spawn`, `Fork`, `Channels`,
/// `Ffi`, `ListPrint` — are classified by `scan_value`,
/// `scan_instruction`, and `scan_terminator`, not by name here.
///
/// # Exclusions
///
/// `String.length` / `String.len` are deliberately **not** classified
/// under `StringFunctions`. They have a real LLVM lowering
/// (`IRCodeGen::compile_builtin_value` emits a `strlen` call), so a
/// program whose only string operation is `.length` should compile
/// through LLVM rather than being sent to the interpreter. Classifying
/// them here would regress that case.
///
/// `List.length` is likewise not classified under `ListAggregates`.
/// The LLVM backend lowers it from static list lengths tracked in
/// `IRCodeGen::list_lengths`.
///
/// # Maintenance
///
/// Two invariants keep this function honest against the LLVM backend:
///
/// 1. **Every name classified here must lack a `compile_builtin_value`
///    arm.** If you add an LLVM lowering for a name, remove it from
///    the corresponding classification rule (or add it to the
///    exclusions at the top of this function).
///
/// 2. **Every name *not* classified here must have a
///    `compile_builtin_value` arm**, unless it's a user function
///    (dispatched by `module.get_function`) or one of the explicit
///    exclusions above. Otherwise the program passes the matrix and
///    then fails at codegen with "unhandled builtin."
///
/// When adding a new `Feature` variant, add its display string in
/// `Feature::description` and its name pattern here in the same commit.
pub(super) fn scan_call_name(name: &str, used: &mut HashSet<Feature>) {
    // `String.length` / `String.len` have an LLVM lowering via strlen;
    // skip them so programs that only need string length still compile
    // through LLVM. See "Exclusions" in the doc-comment above.
    if name == "String.length" || name == "String.len" {
        return;
    }

    if name.starts_with("String.") {
        used.insert(Feature::StringFunctions);
    } else if name.starts_with("File.") {
        used.insert(Feature::FileFunctions);
    } else if name == "List.sum" || name == "List.max" || name == "List.min" {
        used.insert(Feature::ListAggregates);
    }
}

pub(super) fn scan_instruction(
    instr: &Instruction,
    extern_fns: &HashSet<&str>,
    used: &mut HashSet<Feature>,
) {
    match instr {
        Instruction::Declare { value, .. } => scan_value(value, extern_fns, used),
        Instruction::Assign { value, .. } => scan_value(value, extern_fns, used),
        Instruction::ArrayAssign { array, index, value } => {
            scan_value(array, extern_fns, used);
            scan_value(index, extern_fns, used);
            scan_value(value, extern_fns, used);
        }
        Instruction::Print { value } => {
            // A print of a list-typed value needs a special lowering.
            // `type_of()` returns the claimed static type, which for a
            // list literal is `List<T>` and for a list variable is the
            // declared `List<T>`. For `arr[i]` it returns `T`, which is
            // not a list, so indexing stays on the normal path.
            if matches!(value.type_of(), Type::List(_)) {
                used.insert(Feature::ListPrint);
            }
            scan_value(value, extern_fns, used);
        }
        Instruction::Call { func, args, .. } => {
            if extern_fns.contains(func.as_str()) {
                used.insert(Feature::Ffi);
            }
            scan_call_name(func, used);
            for a in args {
                scan_value(a, extern_fns, used);
            }
        }
        Instruction::IteratorInit { iterable, .. } => scan_value(iterable, extern_fns, used),
        Instruction::ChannelDecl { .. } => {
            used.insert(Feature::Channels);
        }
        Instruction::Send { value, .. } | Instruction::ChannelSend { value, .. } => {
            used.insert(Feature::Channels);
            scan_value(value, extern_fns, used);
        }
        Instruction::Receive { .. } | Instruction::ChannelReceive { .. } => {
            used.insert(Feature::Channels);
        }
        Instruction::Allocate { size, .. } => scan_value(size, extern_fns, used),
        Instruction::Free { ptr } => scan_value(ptr, extern_fns, used),
        Instruction::Nop => {}
    }
}

pub(super) fn scan_value(
    value: &TypedIRValue,
    extern_fns: &HashSet<&str>,
    used: &mut HashSet<Feature>,
) {
    match value {
        TypedIRValue::Ok { value, .. } | TypedIRValue::Error { value, .. } => {
            used.insert(Feature::Result);
            scan_value(value, extern_fns, used);
        }
        TypedIRValue::List(elements, _) | TypedIRValue::Array(elements, _, _) => {
            for e in elements {
                scan_value(e, extern_fns, used);
            }
        }
        TypedIRValue::Some(inner) => scan_value(inner, extern_fns, used),
        TypedIRValue::Cast { value, .. } => scan_value(value, extern_fns, used),
        TypedIRValue::BinaryOp { left, right, .. } => {
            scan_value(left, extern_fns, used);
            scan_value(right, extern_fns, used);
        }
        TypedIRValue::Call { function, args, .. } => {
            if extern_fns.contains(function.as_str()) {
                used.insert(Feature::Ffi);
            }
            scan_call_name(function, used);
            for a in args {
                scan_value(a, extern_fns, used);
            }
        }
        TypedIRValue::ArrayAccess { array, index, .. } => {
            scan_value(array, extern_fns, used);
            scan_value(index, extern_fns, used);
        }
        TypedIRValue::Borrow { expr, .. }
        | TypedIRValue::MutBorrow { expr, .. }
        | TypedIRValue::Deref { expr, .. }
        | TypedIRValue::AddrOf { expr, .. } => scan_value(expr, extern_fns, used),
        TypedIRValue::Range(start, end) => {
            scan_value(start, extern_fns, used);
            scan_value(end, extern_fns, used);
        }
        TypedIRValue::FieldAccess { object, .. } => scan_value(object, extern_fns, used),
        _ => {}
    }
}

pub(super) fn scan_terminator(
    term: &Terminator,
    extern_fns: &HashSet<&str>,
    used: &mut HashSet<Feature>,
) {
    match term {
        Terminator::Return { value: Some(v), .. } => scan_value(v, extern_fns, used),
        Terminator::Branch { condition, .. } => scan_value(condition, extern_fns, used),
        Terminator::Switch { value, cases, .. } => {
            scan_value(value, extern_fns, used);
            for (pat, _) in cases {
                match pat {
                    SemanticPattern::Ok { .. } | SemanticPattern::Error { .. } => {
                        used.insert(Feature::Result);
                    }
                    SemanticPattern::Literal(lit) => scan_value(lit, extern_fns, used),
                    _ => {}
                }
            }
        }
        Terminator::Spawn { .. } => {
            used.insert(Feature::Spawn);
        }
        Terminator::Fork { .. } => {
            used.insert(Feature::Fork);
        }
        Terminator::IteratorNext { .. }
        | Terminator::Jump { .. }
        | Terminator::Return { value: None, .. } => {}
    }
}

/// Return the set of features that appear anywhere in `program`.
pub(super) fn scan_features(program: &SemanticProgram) -> HashSet<Feature> {
    let extern_fns: HashSet<&str> = program
        .functions
        .iter()
        .filter(|f| f.is_extern)
        .map(|f| f.name.as_str())
        .collect();

    let mut used = HashSet::new();
    for func in &program.functions {
        for block in &func.blocks {
            for instr in &block.instructions {
                scan_instruction(instr, &extern_fns, &mut used);
            }
            if let Some(term) = &block.terminator {
                scan_terminator(term, &extern_fns, &mut used);
            }
        }
    }
    used
}
// src/backends/capabilities/scan.rs

use super::Feature;
use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::HashSet;

/// True if `ty` mentions `Borrow<_>` or `MutBorrow<_>` anywhere in
/// its structure. Used by the ADR 0019 reference-capability check.
///
/// `Pointer<_>` alone does not count — raw pointers from `alloc`
/// are gated by `RawMemory`. A `Pointer<Borrow<Int>>` does count,
/// because the element is a reference.
fn type_mentions_reference(ty: &Type) -> bool {
    match ty {
        Type::Borrow(_) | Type::MutBorrow(_) => true,
        Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Array(inner, _)
        | Type::Channel(inner) => type_mentions_reference(inner),
        Type::Result { ok, error } => type_mentions_reference(ok) || type_mentions_reference(error),
        Type::Tuple(elems) => elems.iter().any(type_mentions_reference),
        Type::Function {
            params,
            return_type,
        } => params.iter().any(type_mentions_reference) || type_mentions_reference(return_type),
        Type::Generic { args, .. } => args.iter().any(type_mentions_reference),
        Type::Map(k, v) => type_mentions_reference(k) || type_mentions_reference(v),
        Type::Record(_, args) => args.iter().any(type_mentions_reference),
        _ => false,
    }
}

/// True if `ty` mentions `DynTrait` anywhere in its structure —
/// including as the inner type of a `Borrow` / `MutBorrow`, and
/// inside any composite. See ADR 0038.
fn type_mentions_dyn_trait(ty: &Type) -> bool {
    match ty {
        Type::DynTrait { .. } => true,
        Type::Borrow(inner)
        | Type::MutBorrow(inner)
        | Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Array(inner, _)
        | Type::Channel(inner)
        | Type::Set(inner) => type_mentions_dyn_trait(inner),
        Type::Result { ok, error } => type_mentions_dyn_trait(ok) || type_mentions_dyn_trait(error),
        Type::Tuple(elems) => elems.iter().any(type_mentions_dyn_trait),
        Type::Function {
            params,
            return_type,
        } => params.iter().any(type_mentions_dyn_trait) || type_mentions_dyn_trait(return_type),
        Type::Generic { args, .. } | Type::Record(_, args) => {
            args.iter().any(type_mentions_dyn_trait)
        }
        Type::Map(k, v) => type_mentions_dyn_trait(k) || type_mentions_dyn_trait(v),
        Type::Distinct { base, .. } | Type::Subrange { base, .. } => type_mentions_dyn_trait(base),
        _ => false,
    }
}

/// True if `ty` is a record type used *by value* — i.e. a top-level
/// `Type::Record`. This is the signature-position check: a record
/// parameter or return value requires the backend to agree on an
/// ABI for record-sized values.
///
/// Deliberately does **not** recurse into `Borrow` / `MutBorrow`:
/// `&Book` is a reference and is gated by `Feature::References`.
/// It also does not recurse into `List<Book>` / `Option<Book>` —
/// those are gated by their own container features (`ListAppend`,
/// `Option`), and if a backend supports those containers it must
/// already handle records inside them.
fn is_record_by_value(ty: &Type) -> bool {
    matches!(ty, Type::Record(_, _))
}

/// True if `ty` contains a generic record usage — `Type::Record`
/// with a non-empty type-argument list. Recurses into composite
/// types so `List<Box<Int>>` and `Option<Box<T>>` are caught.
fn mentions_generic_record(ty: &Type) -> bool {
    match ty {
        Type::Record(_, args) if !args.is_empty() => true,
        Type::Record(_, _) => false,
        Type::List(inner)
        | Type::Option(inner)
        | Type::Pointer(inner)
        | Type::Array(inner, _)
        | Type::Channel(inner)
        | Type::Borrow(inner)
        | Type::MutBorrow(inner)
        | Type::Set(inner) => mentions_generic_record(inner),
        Type::Result { ok, error } => mentions_generic_record(ok) || mentions_generic_record(error),
        Type::Tuple(elems) => elems.iter().any(mentions_generic_record),
        Type::Function {
            params,
            return_type,
        } => params.iter().any(mentions_generic_record) || mentions_generic_record(return_type),
        Type::Generic { args, .. } => args.iter().any(mentions_generic_record),
        Type::Map(k, v) => mentions_generic_record(k) || mentions_generic_record(v),
        Type::Distinct { base, .. } => mentions_generic_record(base),
        Type::Subrange { base, .. } => mentions_generic_record(base),
        _ => false,
    }
}

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
/// | Name pattern                              | Feature          |
/// |-------------------------------------------|------------------|
/// | `Int.to_string`                            | `IntToString`    |
/// | `String.to_int`                            | `StringToInt`    |
/// | `String.concat` / `.substring` / `.trim`   | `StringOps`      |
/// | `String.to_upper` / `.to_lower`            | `StringOps`      |
/// | `String.split`                             | `StringSplit`    |
/// | `File.*`                                   | `FileFunctions`  |
/// | `List.sum` / `.max` / `.min`               | `ListAggregates` |
/// | anything else                              | (none — permitted) |
///
/// Other `Feature` variants — `Result`, `Spawn`, `Fork`, `Channels`,
/// `Ffi`, `ListPrint` — are classified by `scan_value`,
/// `scan_instruction`, and `scan_terminator`, not by name here.
///
/// # Exclusions
///
/// `String.length` / `String.len` are deliberately **not** classified
/// under `StringOps` / `StringSplit`. They have a real LLVM lowering
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
    // ─── Map method names ───
    // Checked before the `is_builtin_name` guard because Map
    // methods are dispatched by the analyzer's custom path and
    // are not registered in the builtin signature table.
    if name.starts_with("Map.") {
        used.insert(Feature::Map);
        return;
    }

    // ─── List.append ───
    // Like Map methods, `List.append` is dispatched by the
    // analyzer's custom path and is not registered in the
    // builtin signature table.
    if name == "List.append" {
        used.insert(Feature::ListAppend);
        return;
    }
    // Only names that appear in the analyzer/verifier builtin table
    // are candidates. A user-defined function named `String.helper`
    // does not need LLVM's String lowering (it has its own body)
    // and must not be classified as `StringOps` / `StringSplit`.
    if !crate::ir::verifier::builtins::is_builtin_name(name) {
        return;
    }

    // `String.length` / `String.len` have an LLVM lowering via strlen;
    // skip them so programs that only need string length still compile
    // through LLVM. See "Exclusions" in the doc-comment above.
    if name == "String.length" || name == "String.len" {
        return;
    }

    // The int/string conversions take priority over the `String.`
    // prefix below.
    // The classification reflects backend capability, not namespace.
    if name == "Int.to_string" {
        used.insert(Feature::IntToString);
        return;
    }
    if name == "String.to_int" {
        used.insert(Feature::StringToInt);
        return;
    }

    // String.split is a separate feature: it returns a dynamic list,
    // which the LLVM backend does not yet model. The other String.*
    // operations produce a String from a String and are lowerable
    // anywhere malloc exists.
    if name == "String.split" {
        used.insert(Feature::StringSplit);
        return;
    }

    if name.starts_with("String.") {
        used.insert(Feature::StringOps);
    } else if name.starts_with("File.") {
        used.insert(Feature::FileFunctions);
    } else if name == "List.sum" || name == "List.max" || name == "List.min" {
        used.insert(Feature::ListAggregates);
    } else if name == "args" {
        // ADR 0023. LLVM and WASM have no lowering for command-
        // line arguments; the interpreter reads the process's
        // arguments directly.
        used.insert(Feature::CommandLineArgs);
    }
}

pub(super) fn scan_instruction(
    instr: &Instruction,
    extern_fns: &HashSet<&str>,
    used: &mut HashSet<Feature>,
) {
    match instr {
        Instruction::Declare { type_, value, .. } => {
            if type_mentions_reference(type_) {
                used.insert(Feature::References);
            }
            if type_mentions_dyn_trait(type_) {
                used.insert(Feature::DynamicDispatch);
            }
            scan_value(value, extern_fns, used);
        }
        Instruction::Assign { value, .. } => scan_value(value, extern_fns, used),
        Instruction::WriteReference { reference, value } => {
            scan_value(reference, extern_fns, used);
            scan_value(value, extern_fns, used);
        }
        Instruction::ArrayAssign {
            array,
            index,
            value,
        } => {
            scan_value(array, extern_fns, used);
            scan_value(index, extern_fns, used);
            scan_value(value, extern_fns, used);
        }
        Instruction::FieldAssign { value, .. } => {
            used.insert(Feature::Records); // ← must be present
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
        Instruction::SendChannel { value, .. } => {
            used.insert(Feature::Channels);
            scan_value(value, extern_fns, used);
        }
        Instruction::ReceiveChannel { .. } => {
            used.insert(Feature::Channels);
        }
        Instruction::Allocate { size, .. } => {
            used.insert(Feature::RawMemory);
            scan_value(size, extern_fns, used);
        }
        Instruction::Free { ptr } => {
            used.insert(Feature::RawMemory);
            scan_value(ptr, extern_fns, used);
        }
        // Region enter/exit are scoping hints. If the body
        // contains alloc/free, those instructions insert
        // `Feature::RawMemory` on their own — regions themselves
        // do not need a feature gate.
        Instruction::RegionEnter { .. } | Instruction::RegionExit { .. } => {}
        Instruction::VirtualCall { receiver, args, .. } => {
            // ADR 0038. The receiver is by construction a
            // `DynTrait` value, so `scan_value` will set
            // `Feature::DynamicDispatch` when it inspects it.
            // Explicit arm so a future change to `scan_value`
            // cannot silently drop the gate.
            used.insert(Feature::DynamicDispatch);
            scan_value(receiver, extern_fns, used);
            for a in args {
                scan_value(a, extern_fns, used);
            }
        }
        // ADR 0031: BoundsCheck is transparent to the capability
        // matrix. The value's own features (if any — subrange
        // construction only accepts Int or enum arguments) are
        // scanned; the check itself introduces no feature.
        Instruction::BoundsCheck { value, .. } => scan_value(value, extern_fns, used),
        Instruction::Nop => {}
    }
}

pub(super) fn scan_value(
    value: &TypedIRValue,
    extern_fns: &HashSet<&str>,
    used: &mut HashSet<Feature>,
) {
    // Any value whose claimed type contains a generic record usage
    // is gated. The verifier should reject these before codegen, but
    // it doesn't yet; the capability check is the backstop.
    if mentions_generic_record(&value.type_of()) {
        used.insert(Feature::GenericRecords);
    }
    if type_mentions_dyn_trait(&value.type_of()) {
        used.insert(Feature::DynamicDispatch);
    }
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
        TypedIRValue::Some(inner) => {
            used.insert(Feature::Option);
            scan_value(inner, extern_fns, used);
        }
        TypedIRValue::None { .. } => {
            used.insert(Feature::Option);
        }
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
        TypedIRValue::VirtualCall { receiver, args, .. } => {
            // ADR 0038. The receiver is a DynTrait value, so
            // scan_value sets Feature::DynamicDispatch when it
            // inspects it; insert explicitly too so a future change
            // to the DynTrait arm cannot silently drop the gate.
            used.insert(Feature::DynamicDispatch);
            scan_value(receiver, extern_fns, used);
            for a in args {
                scan_value(a, extern_fns, used);
            }
        }
        TypedIRValue::ArrayAccess { array, index, .. } => {
            scan_value(array, extern_fns, used);
            scan_value(index, extern_fns, used);
        }

        TypedIRValue::BorrowShared { expr, .. }
        | TypedIRValue::BorrowMutable { expr, .. }
        | TypedIRValue::ReadReference { expr, .. }
        | TypedIRValue::AddrOf { expr, .. } => {
            // ADR 0019. Any reference operation requires the
            // capability, regardless of the operand's type. The
            // type-driven check below covers the complementary case
            // — a reference value that reached the IR without an
            // explicit operation (a function parameter of reference
            // type, a Declare whose type annotation mentions `&T`).
            used.insert(Feature::References);
            scan_value(expr, extern_fns, used);
        }
        TypedIRValue::Range(start, end) => {
            scan_value(start, extern_fns, used);
            scan_value(end, extern_fns, used);
        }
        TypedIRValue::FieldAccess { object, .. } => {
            used.insert(Feature::Records);
            scan_value(object, extern_fns, used);
        }
        TypedIRValue::Record { fields, .. } => {
            used.insert(Feature::Records);
            for (_, v) in fields {
                scan_value(v, extern_fns, used);
            }
        }
        TypedIRValue::Map { entries, .. } => {
            used.insert(Feature::Map);
            for (k, v) in entries {
                scan_value(k, extern_fns, used);
                scan_value(v, extern_fns, used);
            }
        }
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
                    SemanticPattern::Record { .. } => {
                        used.insert(Feature::Records);
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

    // Generic records: any RecordDecl with non-empty type_params.
    // This is the earliest reliable signal — it fires before the
    // IR builder assigns a Type::Record with non-empty args, and
    // it catches the case where the record is only declared (not
    // used) but its field types still contain TypeVars.
    for rec in &program.records {
        if !rec.type_params.is_empty() {
            used.insert(Feature::GenericRecords);
        }
    }
    for func in &program.functions {
        // ADR 0019. Parameters and return type are on the
        // SemanticFunction, not on any instruction, so the
        // instruction walk below misses them. A parameter of type
        // `&T` or `&mut T` makes the function require the
        // capability even when its body never uses a reference
        // operation explicitly (the caller's argument is the
        // reference operation).
        for (_, ty) in &func.params {
            if type_mentions_reference(ty) {
                used.insert(Feature::References);
            }
            if type_mentions_dyn_trait(ty) {
                used.insert(Feature::DynamicDispatch);
            }
            if is_record_by_value(ty) {
                used.insert(Feature::RecordByValue);
            }
            if mentions_generic_record(ty) {
                used.insert(Feature::GenericRecords);
            }
        }
        if type_mentions_reference(&func.return_type) {
            used.insert(Feature::References);
        }
        if type_mentions_dyn_trait(&func.return_type) {
            used.insert(Feature::DynamicDispatch);
        }
        if is_record_by_value(&func.return_type) {
            used.insert(Feature::RecordByValue);
        }
        if mentions_generic_record(&func.return_type) {
            used.insert(Feature::GenericRecords);
        }

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

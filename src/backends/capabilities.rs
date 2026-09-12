// src/backends/capabilities.rs
//
// Feature capability matrix for backends. Each backend declares which
// language features it can lower. A backend that is asked to lower a
// program using an unsupported feature returns a compile error naming
// the missing feature and pointing to the interpreter as a fallback.
//
// This module is the single source of truth for "does backend X
// support feature Y". The compiler invokes check_backend before
// lowering; it replaces the older ad-hoc checks
// (program_uses_result_features in compiler.rs,
// validate_wasm_compatibility in wasm_backend.rs).

use crate::common::diagnostics::{CompileError, ErrorCode, Result};
use crate::common::types::Type;
use crate::ir::semantic_ir::{
    Instruction, SemanticPattern, SemanticProgram, Terminator, TypedIRValue,
};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// `Result<T, E>` values and any use of `try/catch` (which lowers
    /// to a switch over a Result value).
    Result,
    /// `spawn` blocks.
    Spawn,
    /// `parallel` blocks (Fork terminator).
    Fork,
    /// Channel declarations, sends, and receives.
    Channels,
    /// Calls to `extern` functions.
    Ffi,
    /// `String.*` builtins: concat, substring, to_upper, to_lower.
    /// `String.length` is a special case handled separately in the
    /// LLVM codegen via strlen.
    StringFunctions,
    /// `File.*` builtins: read, write, append.
    FileFunctions,
        /// `List.*` aggregate builtins: sum, max, min.
    ListAggregates,
    /// `print(x)` where `x` has a list type. The interpreter formats
    /// lists as `[a, b, c]`; the LLVM backend has no lowering for it
    /// (it would need a per-element printf loop).
    ListPrint,
}

impl Feature {
    pub fn description(&self) -> &'static str {
        match self {
            Feature::Result => "Result<T, E> and try/catch",
            Feature::Spawn => "spawn",
            Feature::Fork => "parallel",
            Feature::Channels => "channels",
            Feature::Ffi => "foreign function calls (extern)",
            Feature::StringFunctions => "String.* operations (concat, substring, to_upper, to_lower)",
            Feature::FileFunctions => "File.* operations (read, write, append)",
            Feature::ListAggregates => "List.* aggregates (sum, max, min)",
            Feature::ListPrint => "printing a list value",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    pub name: &'static str,
    pub supported: HashSet<Feature>,
    /// Whether the "use --interpreter instead" hint is meaningful for
    /// this backend. True for LLVM and WASM; false for the interpreter
    /// itself.
    pub has_interpreter_fallback: bool,
}

impl BackendCapabilities {
    pub fn llvm() -> Self {
        let mut supported = HashSet::new();
        supported.insert(Feature::Ffi);
        BackendCapabilities {
            name: "LLVM",
            supported,
            has_interpreter_fallback: true,
        }
    }

    pub fn wasm() -> Self {
        BackendCapabilities {
            name: "WASM",
            supported: HashSet::new(),
            has_interpreter_fallback: true,
        }
    }

    pub fn interpreter() -> Self {
        let mut supported = HashSet::new();
        supported.insert(Feature::Result);
        supported.insert(Feature::Spawn);
        supported.insert(Feature::Fork);
        supported.insert(Feature::Channels);
        supported.insert(Feature::StringFunctions);
        supported.insert(Feature::FileFunctions);
        supported.insert(Feature::ListAggregates);
        supported.insert(Feature::ListPrint);
        // FFI is not supported by the interpreter.
        BackendCapabilities {
            name: "interpreter",
            supported,
            has_interpreter_fallback: false,
        }
    }
}

/// Return the set of features that appear anywhere in `program`.
pub fn scan_features(program: &SemanticProgram) -> HashSet<Feature> {
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
fn scan_call_name(name: &str, used: &mut HashSet<Feature>) {
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

fn scan_instruction(
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

fn scan_value(
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

fn scan_terminator(
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

/// Check whether `program` uses only features the backend supports.
/// Returns an error naming the unsupported features otherwise.
pub fn check_backend(program: &SemanticProgram, caps: &BackendCapabilities) -> Result<()> {
    let used = scan_features(program);
    let mut missing: Vec<Feature> = used.difference(&caps.supported).copied().collect();

    if missing.is_empty() {
        return Ok(());
    }

    missing.sort_by_key(|f| format!("{:?}", f));

    let names: Vec<&str> = missing.iter().map(|f| f.description()).collect();
    let mut msg = format!(
        "The {} backend does not support: {}.",
        caps.name,
        names.join(", ")
    );

    if caps.has_interpreter_fallback {
        msg.push_str(
            "\n\nRun through the interpreter instead:\n\n    \
             algol26 run --interpreter <file.gol>\n\n\
             The interpreter exercises the same semantic layer and \
             supports these features.",
        );
    }

    Err(CompileError::simple(&msg, 0, 0, "", ErrorCode::E0002))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::semantic_ir::{
        SemanticBlock, SemanticFunction, Terminator, TypedIRValue,
    };

    fn program_with(instr: Instruction, term: Terminator) -> SemanticProgram {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![instr],
                terminator: Some(term),
            }],
            entry_block: entry,
            is_extern: false,
        });
        program
    }

    fn simple_return() -> Terminator {
        Terminator::Return { value: None, type_: Type::Void }
    }

    #[test]
    fn llvm_accepts_plain_program() {
        let program = program_with(
            Instruction::Print { value: TypedIRValue::Int(1) },
            simple_return(),
        );
        assert!(check_backend(&program, &BackendCapabilities::llvm()).is_ok());
    }

    #[test]
    fn llvm_rejects_result_values() {
        let program = program_with(
            Instruction::Declare {
                name: "r".to_string(),
                mutable: false,
                type_: Type::result(Type::Int, Type::String),
                value: TypedIRValue::Ok {
                    value: Box::new(TypedIRValue::Int(42)),
                    result_type: Type::result(Type::Int, Type::String),
                },
            },
            simple_return(),
        );
        let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
        assert!(err.message.contains("Result"));
        assert!(err.message.contains("interpreter"));
    }

    #[test]
    fn interpreter_accepts_result_values() {
        let program = program_with(
            Instruction::Declare {
                name: "r".to_string(),
                mutable: false,
                type_: Type::result(Type::Int, Type::String),
                value: TypedIRValue::Ok {
                    value: Box::new(TypedIRValue::Int(42)),
                    result_type: Type::result(Type::Int, Type::String),
                },
            },
            simple_return(),
        );
        assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());
    }

    #[test]
    fn llvm_rejects_spawn() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        let spawned = program.new_block_id();
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![
                SemanticBlock {
                    id: entry,
                    instructions: vec![],
                    terminator: Some(Terminator::Spawn { entry_block: spawned }),
                },
                SemanticBlock {
                    id: spawned,
                    instructions: vec![],
                    terminator: Some(simple_return()),
                },
            ],
            entry_block: entry,
            is_extern: false,
        });
        let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
        assert!(err.message.contains("spawn"));
    }

    #[test]
    fn wasm_rejects_channels() {
        let program = program_with(
            Instruction::ChannelDecl {
                name: "ch".to_string(),
                type_: Type::channel(Type::Int),
            },
            simple_return(),
        );
        let err = check_backend(&program, &BackendCapabilities::wasm()).unwrap_err();
        assert!(err.message.contains("channels"));
    }

    #[test]
    fn interpreter_rejects_ffi() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        // Simulate an extern function and a call to it.
        program.functions.push(SemanticFunction {
            name: "puts".to_string(),
            params: vec![("s".to_string(), Type::String)],
            return_type: Type::Int,
            blocks: vec![],
            entry_block: 0,
            is_extern: true,
        });
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![Instruction::Print {
                    value: TypedIRValue::Call {
                        function: "puts".to_string(),
                        args: vec![TypedIRValue::String("hi".to_string())],
                        return_type: Type::Int,
                    },
                }],
                terminator: Some(simple_return()),
            }],
            entry_block: entry,
            is_extern: false,
        });
        let err = check_backend(&program, &BackendCapabilities::interpreter()).unwrap_err();
        assert!(err.message.contains("foreign"));
    }
        #[test]
    fn llvm_rejects_list_print() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![
                    Instruction::Declare {
                        name: "xs".to_string(),
                        mutable: false,
                        type_: Type::list(Type::Float),
                        value: TypedIRValue::List(
                            vec![TypedIRValue::Float(1.0)],
                            Type::Float,
                        ),
                    },
                    Instruction::Print {
                        value: TypedIRValue::Variable(
                            "xs".to_string(),
                            Type::list(Type::Float),
                        ),
                    },
                ],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        });
        let err = check_backend(&program, &BackendCapabilities::llvm()).unwrap_err();
        assert!(
            err.message.contains("printing a list"),
            "expected list-print diagnostic, got: {}",
            err.message
        );
    }

    #[test]
    fn interpreter_accepts_list_print() {
        let mut program = SemanticProgram::new();
        let entry = program.new_block_id();
        program.functions.push(SemanticFunction {
            name: "main".to_string(),
            params: vec![],
            return_type: Type::Void,
            blocks: vec![SemanticBlock {
                id: entry,
                instructions: vec![Instruction::Print {
                    value: TypedIRValue::List(
                        vec![TypedIRValue::Float(1.0)],
                        Type::Float,
                    ),
                }],
                terminator: Some(Terminator::Return {
                    value: None,
                    type_: Type::Void,
                }),
            }],
            entry_block: entry,
            is_extern: false,
        });
        assert!(check_backend(&program, &BackendCapabilities::interpreter()).is_ok());
    }
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn classified_builtins_are_exactly_the_unlowered_set() {
        // Every name classified here must be absent from the LLVM
        // backend's compile_builtin_value. That check lives in a
        // separate crate, so this test just pins the classification
        // itself — a change to scan_call_name has to notice this test.
        let cases: &[(&str, Option<Feature>)] = &[
            ("String.concat", Some(Feature::StringFunctions)),
            ("String.substring", Some(Feature::StringFunctions)),
            ("String.to_upper", Some(Feature::StringFunctions)),
            ("String.to_lower", Some(Feature::StringFunctions)),
            ("String.length", None),
            ("String.len", None),
            ("File.read", Some(Feature::FileFunctions)),
            ("File.write", Some(Feature::FileFunctions)),
            ("File.append", Some(Feature::FileFunctions)),
            ("List.sum", Some(Feature::ListAggregates)),
            ("List.max", Some(Feature::ListAggregates)),
            ("List.min", Some(Feature::ListAggregates)),
            ("List.length", None),
            ("Math.sqrt", None),
            ("print", None),
        ];

        for (name, expected) in cases {
            let mut used = HashSet::new();
            scan_call_name(name, &mut used);
            match expected {
                Some(f) => assert!(
                    used.contains(f),
                    "'{name}' should be classified as {f:?}, got {used:?}"
                ),
                None => assert!(
                    used.is_empty(),
                    "'{name}' should not be classified, got {used:?}"
                ),
            }
        }
    }
}
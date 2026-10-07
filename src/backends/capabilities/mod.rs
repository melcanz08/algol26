// src/backends/capabilities/mod.rs
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
use crate::ir::semantic_ir::SemanticProgram;
use std::collections::HashSet;

mod scan;
use scan::scan_features;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;

// When adding a variant, three things must change together:
//   1. This enum
//   2. src/backends/capabilities/tests.rs (accept/reject per backend)
//   3. tests/coverage_matrix.rs (a FeatureRow claiming the tests)
//   4. tests/coverage_maturity.rs (EXPECTED_MATURITY entry)
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
    /// `String.*` operations that produce a String from a String:
    /// concat, substring, to_upper, to_lower, trim.
    /// `String.length` is a special case handled separately in the
    /// LLVM codegen via strlen.
    StringOps,
    /// `String.split` — returns `List<String>`, blocked on the
    /// dynamic-list model that LLVM does not yet have.
    StringSplit,
    /// `File.*` builtins: read, write, append.
    FileFunctions,
    /// `List.*` aggregate builtins: sum, max, min.
    ListAggregates,
    /// `print(x)` where `x` has a list type. The interpreter formats
    /// lists as `[a, b, c]`; the LLVM backend has no lowering for it
    /// (it would need a per-element printf loop).
    ListPrint,
    /// `Option<T>` values: `Some(x)` and `None`. LLVM has no
    /// tag+payload representation, so it silently unwrapped
    /// `Some(x)` to `x` and `None` to null — wrong code for any
    /// program that stored or tested an Option.
    Option,
    /// `alloc` and `free`.
    RawMemory,
    /// Reference operations (`&`, `&mut`, `*`).
    References,
    /// `args()` — command-line arguments as `List<String>`.
    CommandLineArgs,
    /// `rec` declarations, literals, field access and assignment.
    Records,
    /// A record type appearing directly as a function parameter or
    /// return type — i.e. passed or returned *by value* across a
    /// function boundary. LLVM and WASM lower record literals and
    /// field access (`Feature::Records`) but do not yet agree on an
    /// ABI for record-typed signatures: the function declaration
    /// uses pointers, the body emits a by-value return, and LLVM
    /// rejects the mismatch. See ADR 0036.
    RecordByValue,
    /// Generic records (`rec Box<T>`). LLVM and WASM codegen for
    /// records with type parameters is a follow-up (ADR 0036 covers
    /// non-generic records). Without this gate, codegen panics with
    /// `map_type called on unresolved type TypeVar("T")`.
    GenericRecords,
    /// `Int.to_string` — formats an i64 into a heap string.
    IntToString,
    /// `String.to_int` — returns `Option<Int>`, blocked on Option.
    StringToInt,
    /// `Map<K, V>` literals and methods.
    Map,
    /// `List.append`.
    ListAppend,
}

impl Feature {
    /// Every variant, in display order. Used by the capability matrix
    /// renderer and the sync test.
    pub fn all() -> &'static [Feature] {
        &[
            Feature::Result,
            Feature::Spawn,
            Feature::Fork,
            Feature::Channels,
            Feature::Ffi,
            Feature::StringOps,
            Feature::StringSplit,
            Feature::FileFunctions,
            Feature::ListAggregates,
            Feature::ListPrint,
            Feature::Option,
            Feature::RawMemory,
            Feature::References,
            Feature::CommandLineArgs,
            Feature::Records,
            Feature::RecordByValue,
            Feature::GenericRecords,
            Feature::IntToString,
            Feature::StringToInt,
            Feature::Map,
            Feature::ListAppend,
        ]
    }

    /// Short identifier for tables and CLI output.
    pub fn name(&self) -> &'static str {
        match self {
            Feature::Result => "result",
            Feature::Spawn => "spawn",
            Feature::Fork => "fork",
            Feature::Channels => "channels",
            Feature::Ffi => "ffi",
            Feature::StringOps => "string.ops",
            Feature::StringSplit => "string.split",
            Feature::FileFunctions => "file.*",
            Feature::ListAggregates => "list.agg",
            Feature::ListPrint => "print(list)",
            Feature::Option => "option",
            Feature::RawMemory => "raw-memory",
            Feature::References => "references",
            Feature::CommandLineArgs => "args",
            Feature::Records => "records",
            Feature::RecordByValue => "records.by-value",
            Feature::GenericRecords => "records.generic",
            Feature::IntToString => "int.to_string",
            Feature::StringToInt => "string.to_int",
            Feature::Map => "map",
            Feature::ListAppend => "list.append",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Feature::Result => "Result / try-catch",
            Feature::Spawn => "spawn",
            Feature::Fork => "parallel",
            Feature::Channels => "channels",
            Feature::Ffi => "foreign function interface (FFI / extern)",
            Feature::StringOps => "String operations (concat, substring, to_upper, to_lower, trim)",
            Feature::StringSplit => "String.split (returns List<String>)",
            Feature::FileFunctions => "File.* operations",
            Feature::ListAggregates => "List.sum / max / min",
            Feature::ListPrint => "printing a list (print(list))",
            Feature::Option => "Option values (Some / None)",
            Feature::RawMemory => "alloc / free",
            Feature::References => "reference operations (&x, &mut x, *r)",
            Feature::CommandLineArgs => "command-line arguments (args())",
            Feature::Records => "records (rec declarations, literals, field access)",
            Feature::RecordByValue => "records passed or returned by value in function signatures",
            Feature::GenericRecords => "generic records (rec Box<T>)",
            Feature::IntToString => "Int.to_string",
            Feature::StringToInt => "String.to_int (returns Option)",
            Feature::Map => "Map<K, V>",
            Feature::ListAppend => "List.append",
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
        supported.insert(Feature::RawMemory);
        supported.insert(Feature::References);
        supported.insert(Feature::Records);
        supported.insert(Feature::IntToString);
        supported.insert(Feature::StringOps);
        supported.insert(Feature::Option);
        // By-value record parameters and returns use the LLVM
        // struct ABI: parameters arrive as struct values, returns
        // are struct values stored into the caller's alloca. See
        // ADR 0036 follow-up.
        supported.insert(Feature::RecordByValue);
        BackendCapabilities {
            name: "LLVM",
            supported,
            has_interpreter_fallback: true,
        }
    }

    pub fn wasm() -> Self {
        let mut supported = HashSet::new();
        supported.insert(Feature::Records);
        supported.insert(Feature::References);
        supported.insert(Feature::IntToString);
        supported.insert(Feature::StringOps);
        supported.insert(Feature::Option);
        // WASM shares IRCodeGen with LLVM; the by-value record ABI
        // applies to both.
        supported.insert(Feature::RecordByValue);
        BackendCapabilities {
            name: "WASM",
            supported,
            has_interpreter_fallback: true,
        }
    }

    pub fn interpreter() -> Self {
        let mut supported = HashSet::new();
        supported.insert(Feature::Result);
        supported.insert(Feature::Spawn);
        supported.insert(Feature::Fork);
        supported.insert(Feature::StringOps);
        supported.insert(Feature::StringSplit);
        supported.insert(Feature::FileFunctions);
        supported.insert(Feature::ListAggregates);
        supported.insert(Feature::ListPrint);
        supported.insert(Feature::Option);
        supported.insert(Feature::RawMemory);
        supported.insert(Feature::CommandLineArgs);
        supported.insert(Feature::Records);
        supported.insert(Feature::IntToString);
        supported.insert(Feature::StringToInt);
        supported.insert(Feature::Map);
        supported.insert(Feature::ListAppend);
        supported.insert(Feature::RecordByValue);
        supported.insert(Feature::GenericRecords);
        supported.insert(Feature::References);
        BackendCapabilities {
            name: "interpreter",
            supported,
            has_interpreter_fallback: false,
        }
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

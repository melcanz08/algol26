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
    /// `Option<T>` values: `Some(x)` and `None`. The LLVM backend
    /// has no tag+payload representation, so it silently unwrapped
    /// `Some(x)` to `x` and `None` to null — producing wrong code
    /// for any program that stored or tested an Option value. The
    /// interpreter handles both correctly.
    Option,
    /// `alloc(n)` / `free(p)`. Neither backend has a heap model;
    /// both were silently no-op'ing these instructions. Refuse
    /// rather than pretend.
    RawMemory,
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
            Feature::StringFunctions,
            Feature::FileFunctions,
            Feature::ListAggregates,
            Feature::ListPrint,
            Feature::Option,
            Feature::RawMemory,
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
            Feature::StringFunctions => "string.*",
            Feature::FileFunctions => "file.*",
            Feature::ListAggregates => "list.agg",
            Feature::ListPrint => "print(list)",
            Feature::Option => "option",
            Feature::RawMemory => "raw-memory",
        }
    }
    pub fn description(&self) -> &'static str {
        match self {
            Feature::Result => "Result<T, E> and try/catch",
            Feature::Spawn => "spawn",
            Feature::Fork => "parallel",
            Feature::Channels => "channels",
            Feature::Ffi => "foreign function calls (extern)",
            Feature::StringFunctions => {
                "String.* operations (concat, substring, to_upper, to_lower)"
            }
            Feature::FileFunctions => "File.* operations (read, write, append)",
            Feature::ListAggregates => "List.* aggregates (sum, max, min)",
            Feature::ListPrint => "printing a list value",
            Feature::Option => "Option<T>: Some(x) and None",
            Feature::RawMemory => "alloc / free (raw memory)",
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
        // Step 5: alloc/free lower to malloc/free; LLVM now
        // accepts programs that use them.
        supported.insert(Feature::RawMemory);
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
        supported.insert(Feature::Option);
        supported.insert(Feature::RawMemory);
        // FFI is not supported by the interpreter (a tree-walker
        // cannot call into C).
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

// src/backends/interpreter/runtime.rs


use std::fmt;

/// Errors produced by the interpreter while evaluating IR.
///
/// Three categories:
///   - `TypeMismatch` — the IR was malformed for the interpreter.
///     This is a compiler bug, not a user error: `TypeCheckPass`
///     should have prevented it.
///   - `Runtime` — the user's program did something illegal at
///     runtime (division by zero, out-of-bounds index, etc.).
///   - `Unsupported` — the IR uses a construct the interpreter
///     does not model. The capability matrix should have refused
///     this program before it reached the interpreter; this is a
///     defense-in-depth error.
#[derive(Debug, Clone)]
pub enum EvalError {
    TypeMismatch {
        op: &'static str,
        left: &'static str,
        right: &'static str,
    },
    Runtime(String),
    Unsupported {
        construct: &'static str,
        hint: &'static str,
    },
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeMismatch { op, left, right } => write!(
                f,
                "internal: `{}` received {} and {} — IR builder should have coerced",
                op, left, right
            ),
            Self::Runtime(msg) => write!(f, "runtime error: {}", msg),
            Self::Unsupported { construct, hint } => write!(
                f,
                "interpreter does not support `{}`: {}",
                construct, hint
            ),
        }
    }
}

impl std::error::Error for EvalError {}

#[derive(Debug, Clone)]
pub enum RuntimeValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<RuntimeValue>),
    Option(Option<Box<RuntimeValue>>),
    Result { is_ok: bool, value: Box<RuntimeValue> },
    Void,
}

impl RuntimeValue {
    pub fn as_bool(&self) -> bool {
        match self {
            RuntimeValue::Bool(b) => *b,
            RuntimeValue::Int(i) => *i != 0,
            RuntimeValue::Float(f) => *f != 0.0,
            RuntimeValue::String(s) => !s.is_empty(),
            RuntimeValue::List(l) => !l.is_empty(),
            RuntimeValue::Option(Some(_)) => true,
            RuntimeValue::Option(None) => false,
            RuntimeValue::Result { is_ok, .. } => *is_ok,
            RuntimeValue::Void => false,
        }
    }

    pub fn display(&self) -> String {
        match self {
            RuntimeValue::Int(i) => crate::common::types::print::format_int(*i),
            RuntimeValue::Float(f) => crate::common::types::print::format_float(*f),
            RuntimeValue::String(s) => s.clone(),
            RuntimeValue::Bool(b) => crate::common::types::print::format_bool(*b),
            RuntimeValue::List(l) => {
                let items: Vec<String> = l.iter().map(|v| v.display()).collect();
                format!("[{}]", items.join(", "))
            }
            RuntimeValue::Option(Some(v)) => format!("Some({})", v.display()),
            RuntimeValue::Option(None) => "None".to_string(),
            RuntimeValue::Result { is_ok: true, value } => format!("Ok({})", value.display()),
            RuntimeValue::Result { is_ok: false, value } => format!("Error({})", value.display()),
            RuntimeValue::Void => String::new(),
        }
    }
}

/// Human-readable name for a `RuntimeValue` variant, used only for
/// diagnostics when an internal invariant is broken.
pub(super) fn runtime_kind(v: &RuntimeValue) -> &'static str {
    match v {
        RuntimeValue::Int(_) => "Int",
        RuntimeValue::Float(_) => "Float",
        RuntimeValue::String(_) => "String",
        RuntimeValue::Bool(_) => "Bool",
        RuntimeValue::List(_) => "List",
        RuntimeValue::Option(_) => "Option",
        RuntimeValue::Result { .. } => "Result",
        RuntimeValue::Void => "Void",
    }
}
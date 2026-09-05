// src/common/types.rs - Unified Type System for ALGOL26

#![allow(dead_code)]

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Void,

    /// Opaque/raw pointer — no type information (e.g., FFI void*)
    /// Use Pointer(T) for typed pointers like *Int
    Ptr,

    // Special types
    Unknown,
    Never, // Bottom type for diverging expressions

    // Composite types
    List(Box<Type>),
    Option(Box<Type>),
    Result {
        ok: Box<Type>,
        error: Box<Type>,
    },

    // Memory management types
    Pointer(Box<Type>),
    Borrow(Box<Type>),
    MutBorrow(Box<Type>),

    // Concurrency types
    Channel(Box<Type>),

    // Function types (for future use)
    Function {
        params: Vec<Type>,
        return_type: Box<Type>,
    },

    // Generic type parameter
    TypeVar(String), // T, U, V

    // Instantiated generic type
    Generic {
        name: String,
        args: Vec<Type>,
    },
}

impl Type {
    // Type constructors
    pub fn int() -> Self {
        Type::Int
    }
    pub fn float() -> Self {
        Type::Float
    }
    pub fn string() -> Self {
        Type::String
    }
    pub fn bool() -> Self {
        Type::Bool
    }
    pub fn void() -> Self {
        Type::Void
    }
    pub fn ptr() -> Self {
        Type::Ptr
    }
    pub fn unknown() -> Self {
        Type::Unknown
    }
    pub fn never() -> Self {
        Type::Never
    }
    pub fn type_var(name: &str) -> Self {
        Type::TypeVar(name.to_string())
    }
    pub fn generic(name: &str, args: Vec<Type>) -> Self {
        Type::Generic {
            name: name.to_string(),
            args,
        }
    }

    pub fn list(element_type: Type) -> Self {
        Type::List(Box::new(element_type))
    }

    pub fn option(inner_type: Type) -> Self {
        Type::Option(Box::new(inner_type))
    }

    pub fn result(ok_type: Type, error_type: Type) -> Self {
        Type::Result {
            ok: Box::new(ok_type),
            error: Box::new(error_type),
        }
    }

    pub fn pointer(inner_type: Type) -> Self {
        Type::Pointer(Box::new(inner_type))
    }

    pub fn borrow(inner_type: Type) -> Self {
        Type::Borrow(Box::new(inner_type))
    }

    pub fn mut_borrow(inner_type: Type) -> Self {
        Type::MutBorrow(Box::new(inner_type))
    }

    pub fn channel(inner_type: Type) -> Self {
        Type::Channel(Box::new(inner_type))
    }

    // Parsing from string
    pub fn from_str(s: &str) -> Self {
        let s_trimmed = s.trim();
        let s_lower = s_trimmed.to_lowercase();

        // Check if it's a type variable (single uppercase letter)
        if s_trimmed.len() == 1 {
            let c = s_trimmed.chars().next().unwrap();
            if c.is_uppercase() {
                return Type::TypeVar(s_trimmed.to_string());
            }
        }

        match s_lower.as_str() {
            "int" | "integer" | "i64" => Type::Int,
            "float" | "double" | "f64" => Type::Float,
            "string" | "str" => Type::String,
            "bool" | "boolean" => Type::Bool,
            "void" | "unit" | "()" => Type::Void,
            "ptr" | "pointer" | "*" => Type::Ptr,
            "unknown" | "_" => Type::Unknown,
            "never" | "!" => Type::Never,

            // Simple generic types (unparameterized)
            "list" => Type::list(Type::Unknown),
            "option" => Type::option(Type::Unknown),
            "result" => Type::result(Type::Unknown, Type::Unknown),
            "channel" => Type::channel(Type::Unknown),

            // Pointer types
            "*int" | "*i64" => Type::pointer(Type::Int),
            "*float" | "*f64" => Type::pointer(Type::Float),
            "*string" | "*str" => Type::pointer(Type::String),
            "*bool" => Type::pointer(Type::Bool),
            "*void" => Type::pointer(Type::Void),
            "*unknown" => Type::pointer(Type::Unknown),

            // Borrow types
            "&int" => Type::borrow(Type::Int),
            "&float" => Type::borrow(Type::Float),
            "&string" => Type::borrow(Type::String),
            "&bool" => Type::borrow(Type::Bool),

            // Mutable borrow types
            "&mut int" => Type::mut_borrow(Type::Int),
            "&mut float" => Type::mut_borrow(Type::Float),
            "&mut string" => Type::mut_borrow(Type::String),
            "&mut bool" => Type::mut_borrow(Type::Bool),

            _ => {
                // Try to parse generic types like List<int>, Option<float>, etc.
                if let Some(inner) = s_lower
                    .strip_prefix("list<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::list(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("option<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::option(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("channel<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::channel(Type::from_str(inner))
                } else if s_lower.starts_with("result<") && s_lower.ends_with('>') {
                    // Parse Result<OkType, ErrorType>
                    let inner = &s_trimmed[7..s_trimmed.len() - 1];
                    let parts: Vec<&str> = inner.splitn(2, ',').collect();
                    if parts.len() == 2 {
                        Type::result(
                            Type::from_str(parts[0].trim()),
                            Type::from_str(parts[1].trim()),
                        )
                    } else {
                        Type::Unknown
                    }
                } else if let Some(inner) = s_lower.strip_prefix("&mut ").map(|s| s.trim()) {
                    Type::mut_borrow(Type::from_str(inner))
                } else if let Some(inner) = s_lower.strip_prefix('&') {
                    Type::borrow(Type::from_str(inner))
                } else if let Some(inner) = s_lower.strip_prefix('*') {
                    Type::pointer(Type::from_str(inner))
                } else {
                    Type::Unknown
                }
            }
        }
    }

    // Type checking helpers
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Type::Int | Type::Float | Type::String | Type::Bool | Type::Void
        )
    }

    pub fn is_composite(&self) -> bool {
        matches!(self, Type::List(_) | Type::Option(_) | Type::Result { .. })
    }

    pub fn is_pointer_like(&self) -> bool {
        matches!(
            self,
            Type::Ptr | Type::Pointer(_) | Type::Borrow(_) | Type::MutBorrow(_)
        )
    }

    pub fn is_unknown(&self) -> bool {
        matches!(self, Type::Unknown)
    }

    pub fn is_copy(&self) -> bool {
        matches!(self, Type::Int | Type::Float | Type::Bool)
    }

    pub fn is_type_var(&self) -> bool {
        matches!(self, Type::TypeVar(_))
    }

    // Type compatibility and coercion
    pub fn can_coerce_to(&self, target: &Type) -> bool {
        if self == target {
            return true;
        }

        // TypeVar can coerce to/from anything (it's generic)
        if matches!(self, Type::TypeVar(_)) || matches!(target, Type::TypeVar(_)) {
            return true;
        }

        match (self, target) {
            // Unknown must be RESOLVED before coercion can be checked.
            // It does NOT mean 'any type' — it means 'not yet inferred'.
            // If Unknown appears here, the program hasn't been fully type-checked.

            // Numeric coercion
            (Type::Int, Type::Float) => true,
            (Type::Float, Type::Int) => false, // Lossy, require explicit cast

            // Ptr coercion
            (Type::Ptr, Type::Ptr) => true,
            (Type::Ptr, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::Ptr) => true,

            // List covariance
            (Type::List(a), Type::List(b)) => a.can_coerce_to(b),

            // Option covariance
            (Type::Option(a), Type::Option(b)) => a.can_coerce_to(b),

            // Result covariance
            (Type::Result { ok: ok1, error: e1 }, Type::Result { ok: ok2, error: e2 }) => {
                ok1.can_coerce_to(ok2) && e1.can_coerce_to(e2)
            }

            // Generic covariance
            (Type::Generic { name: n1, args: a1 }, Type::Generic { name: n2, args: a2 }) => {
                n1 == n2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2.iter()).all(|(x, y)| x.can_coerce_to(y))
            }

            _ => false,
        }
    }

    pub fn common_supertype(&self, other: &Type) -> Type {
        if self == other {
            return self.clone();
        }

        match (self, other) {
            // TypeVar unification - concrete type wins
            (Type::TypeVar(_), t) => t.clone(),
            (t, Type::TypeVar(_)) => t.clone(),

            // Numeric promotion
            (Type::Int, Type::Float) | (Type::Float, Type::Int) => Type::Float,

            // Ptr supertype
            (Type::Ptr, Type::Ptr) => Type::Ptr,
            (Type::Ptr, Type::Pointer(t)) | (Type::Pointer(t), Type::Ptr) => {
                Type::Pointer(t.clone())
            }

            // List common element type
            (Type::List(a), Type::List(b)) => Type::list(a.common_supertype(b)),

            // Option common inner type
            (Type::Option(a), Type::Option(b)) => Type::option(a.common_supertype(b)),

            // Result common types
            (Type::Result { ok: ok1, error: e1 }, Type::Result { ok: ok2, error: e2 }) => {
                Type::result(ok1.common_supertype(ok2), e1.common_supertype(e2))
            }

            // Unknown handling
            (Type::Unknown, t) | (t, Type::Unknown) => t.clone(),

            // Default to Unknown
            _ => Type::Unknown,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Type::Int => "Int".to_string(),
            Type::Float => "Float".to_string(),
            Type::String => "String".to_string(),
            Type::Bool => "Bool".to_string(),
            Type::Void => "Void".to_string(),
            Type::Ptr => "Ptr".to_string(),
            Type::Unknown => "Unknown".to_string(),
            Type::Never => "Never".to_string(),
            Type::TypeVar(v) => v.clone(),
            Type::Generic { name, args } => {
                let args_str: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
            Type::List(t) => format!("List<{}>", t),
            Type::Option(t) => format!("Option<{}>", t),
            Type::Result { ok, error } => format!("Result<{}, {}>", ok, error),
            Type::Pointer(t) => format!("*{}", t),
            Type::Borrow(t) => format!("&{}", t),
            Type::MutBorrow(t) => format!("&mut {}", t),
            Type::Channel(t) => format!("Channel<{}>", t),
            Type::Function {
                params,
                return_type,
            } => {
                let param_str: Vec<String> = params.iter().map(|p| p.to_string()).collect();
                format!("fn({}) -> {}", param_str.join(", "), return_type)
            }
        };
        write!(f, "{}", name)
    }
}

// Convenience type aliases
pub type TypeResult = Result<Type, String>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_parsing() {
        assert_eq!(Type::from_str("int"), Type::Int);
        assert_eq!(Type::from_str("Float"), Type::Float);
        assert_eq!(Type::from_str("ptr"), Type::Ptr);
        assert_eq!(Type::from_str("list"), Type::list(Type::Unknown));
        assert_eq!(Type::from_str("list<int>"), Type::list(Type::Int));
        assert_eq!(Type::from_str("option<float>"), Type::option(Type::Float));
        assert_eq!(Type::from_str("*int"), Type::pointer(Type::Int));
        assert_eq!(Type::from_str("&int"), Type::borrow(Type::Int));
        assert_eq!(Type::from_str("&mut int"), Type::mut_borrow(Type::Int));
        assert_eq!(Type::from_str("T"), Type::TypeVar("T".to_string()));
    }

    #[test]
    fn test_type_coercion() {
        assert!(Type::Int.can_coerce_to(&Type::Float));
        assert!(!Type::Float.can_coerce_to(&Type::Int));
        // REMOVED: Unknown can no longer coerce (must be resolved first)
        assert!(Type::list(Type::Int).can_coerce_to(&Type::list(Type::Float)));
        assert!(Type::Ptr.can_coerce_to(&Type::Ptr));
        assert!(Type::Ptr.can_coerce_to(&Type::Pointer(Box::new(Type::Int))));
        assert!(Type::TypeVar("T".to_string()).can_coerce_to(&Type::Int));
        assert!(Type::Int.can_coerce_to(&Type::TypeVar("T".to_string())));
    }

    #[test]
    fn test_common_supertype() {
        assert_eq!(Type::Int.common_supertype(&Type::Float), Type::Float);
        assert_eq!(
            Type::list(Type::Int).common_supertype(&Type::list(Type::Float)),
            Type::list(Type::Float)
        );
        assert_eq!(Type::Ptr.common_supertype(&Type::Ptr), Type::Ptr);
        assert_eq!(
            Type::TypeVar("T".to_string()).common_supertype(&Type::Int),
            Type::Int
        );
        assert_eq!(
            Type::Int.common_supertype(&Type::TypeVar("T".to_string())),
            Type::Int
        );
    }

    #[test]
    fn test_display() {
        assert_eq!(Type::Int.to_string(), "Int");
        assert_eq!(Type::Ptr.to_string(), "Ptr");
        assert_eq!(Type::TypeVar("T".to_string()).to_string(), "T");
        assert_eq!(Type::list(Type::Float).to_string(), "List<Float>");
        assert_eq!(
            Type::Result {
                ok: Box::new(Type::Int),
                error: Box::new(Type::String)
            }
            .to_string(),
            "Result<Int, String>"
        );
    }
}

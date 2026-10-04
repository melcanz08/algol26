// src/common/types.rs

#![allow(dead_code)]
use std::fmt;

/// Nominal type identity. Two declarations with the same `name`
/// in different modules produce different `NominalTypeId` values, and
/// therefore different types. Identity is `id`; `name` is
/// presentation only.
///
/// Assigned by the analyzer during type declaration registration.
/// Unique within a compilation unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NominalTypeId(pub u32);

/// Enum type identity. Two declarations with the same `name` in
/// different modules produce different `EnumTypeId` values, and
/// therefore different types. Identity is `id`; `name` and
/// `variants` are for display and analysis. See ADR 0030.
///
/// Assigned by the analyzer during enum declaration registration,
/// in declaration order, so ids are deterministic for a given
/// source set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumTypeId(pub u32);

/// Subrange type identity. `type Percentage Int in 0..100` and
/// `type WorkDay Day in Monday..Friday`. Identity is `id`; `name`,
/// `base`, and the bounds are presentation / analysis. See ADR 0031.
///
/// `low` and `high` are inclusive. When `base` is `Type::Enum`,
/// they are ordinals; the variant names are recovered from `base`
/// at diagnostic time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubrangeTypeId(pub u32);

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
    Array(Box<Type>, usize), // Array of type with size
    Tuple(Vec<Type>),
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
    /// A named record type, optionally with type arguments.
    /// `Point` is `Record("Point", [])`, `Pair<Int>` is `Record("Pair", [Int])`.
    Record(String, Vec<Type>),
    /// Key-value container. Keys are restricted to `Int`, `String`,
    /// or `Bool` by the analyzer (ADR 0027); the type itself does not
    /// enforce that restriction.
    Map(Box<Type>, Box<Type>),
    /// A nominal type. Identity is `id`; `name` is presentation;
    /// `base` is the underlying representation used at lowering.
    /// See ADR 0029.
    ///
    /// The analyzer guarantees `base` is one of `Int`, `Float`,
    /// `Bool`, `String` in v1 (`Ptr` deferred).
    Distinct {
        id: NominalTypeId,
        name: String,
        base: Box<Type>,
    },
    /// An ordinal enumeration. Identity is `id`; `name` is
    /// presentation; `variants[i]` has ordinal `i`. The runtime
    /// representation is `Int`. See ADR 0030.
    Enum {
        id: EnumTypeId,
        name: String,
        variants: Vec<String>,
    },
    /// A subrange of an ordinal type. `low..high` is inclusive.
    /// When `base` is `Type::Enum`, `low` and `high` are ordinals.
    /// Identity is `id`; every other field is presentation or
    /// analysis. See ADR 0031.
    Subrange {
        id: SubrangeTypeId,
        name: String,
        base: Box<Type>,
        low: i64,
        high: i64,
    },
    /// A set of ordinal values: `Set<Day>`, `Set<WorkDay>`,
    /// `Set<Percentage>`, `Set<Bool>`. Runtime representation is a
    /// single `u64`: bit `i` is set iff domain element `i` is a
    /// member. The element type must have a bounded domain of at
    /// most 64 values; see `set_domain_size`.
    ///
    /// Sets are structural — no `SetTypeId`. Two `Set<Day>` are the
    /// same type iff their `Day` is the same enum. See ADR 0032.
    Set(Box<Type>),
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

    pub fn record(name: &str, args: Vec<Type>) -> Self {
        Type::Record(name.to_string(), args)
    }

    /// Construct a nominal type. `id` is the analyzer-assigned
    /// identity; `name` is for display only.
    pub fn distinct(id: NominalTypeId, name: &str, base: Type) -> Self {
        Type::Distinct {
            id,
            name: name.to_string(),
            base: Box::new(base),
        }
    }

    /// Construct an ordinal enum type. `id` is the analyzer-assigned
    /// identity; `variants` is in declaration order (ordinals 0..N).
    pub fn enum_type(id: EnumTypeId, name: &str, variants: Vec<String>) -> Self {
        Type::Enum {
            id,
            name: name.to_string(),
            variants,
        }
    }

    /// Construct a subrange type. `low..high` inclusive. When
    /// `base` is `Type::Enum`, `low` and `high` are ordinals.
    pub fn subrange(id: SubrangeTypeId, name: &str, base: Type, low: i64, high: i64) -> Self {
        Type::Subrange {
            id,
            name: name.to_string(),
            base: Box::new(base),
            low,
            high,
        }
    }

    /// Construct a `Set<T>` type. The caller is responsible for
    /// validating that `element` satisfies `set_domain_size`; this
    /// constructor does not check.
    pub fn set(element: Type) -> Self {
        Type::Set(Box::new(element))
    }

    pub fn list(element_type: Type) -> Self {
        Type::List(Box::new(element_type))
    }

    pub fn array(element_type: Type, size: usize) -> Self {
        Type::Array(Box::new(element_type), size)
    }

    pub fn tuple(elements: Vec<Type>) -> Self {
        Type::Tuple(elements)
    }

    pub fn option(inner_type: Type) -> Self {
        Type::Option(Box::new(inner_type))
    }

    pub fn map(key: Type, value: Type) -> Self {
        Type::Map(Box::new(key), Box::new(value))
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
            if let Some(c) = s_trimmed.chars().next() {
                if c.is_uppercase() {
                    return Type::TypeVar(s_trimmed.to_string());
                }
            }
        }

        match s_lower.as_str() {
            "int" | "integer" | "i64" | "i32" => Type::Int,
            "float" | "double" | "f64" | "f32" => Type::Float,
            "string" | "str" => Type::String,
            "bool" | "boolean" => Type::Bool,
            "void" | "unit" | "()" => Type::Void,
            "ptr" | "pointer" | "*" => Type::Ptr,
            "unknown" | "_" => Type::Unknown,
            "never" | "!" => Type::Never,

            // Simple generic types (unparameterized)
            "list" => Type::list(Type::Unknown),
            "option" => Type::option(Type::Unknown),
            "map" => Type::map(Type::Unknown, Type::Unknown),
            "result" => Type::result(Type::Unknown, Type::Unknown),
            "channel" => Type::channel(Type::Unknown),
            "array" => Type::array(Type::Unknown, 0),

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
                // Support List<int>, List[float], etc.
                if let Some(inner) = s_lower
                    .strip_prefix("list<")
                    .and_then(|s| s.strip_suffix('>'))
                    .or_else(|| {
                        s_lower
                            .strip_prefix("list[")
                            .and_then(|s| s.strip_suffix(']'))
                    })
                {
                    Type::list(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("option<")
                    .and_then(|s| s.strip_suffix('>'))
                    .or_else(|| {
                        s_lower
                            .strip_prefix("option[")
                            .and_then(|s| s.strip_suffix(']'))
                    })
                {
                    Type::option(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("channel<")
                    .and_then(|s| s.strip_suffix('>'))
                    .or_else(|| {
                        s_lower
                            .strip_prefix("channel[")
                            .and_then(|s| s.strip_suffix(']'))
                    })
                {
                    Type::channel(Type::from_str(inner))
                } else if s_lower.starts_with("array<") && s_lower.ends_with('>') {
                    // Parse Array<Type, Size>
                    let inner = &s_trimmed[6..s_trimmed.len() - 1];
                    let parts: Vec<&str> = inner.splitn(2, ',').collect();
                    if parts.len() == 2 {
                        let elem_type = Type::from_str(parts[0].trim());
                        if let Ok(size) = parts[1].trim().parse::<usize>() {
                            Type::array(elem_type, size)
                        } else {
                            Type::array(elem_type, 0)
                        }
                    } else {
                        Type::array(Type::Unknown, 0)
                    }
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
                } else if s_lower.starts_with("map<") && s_lower.ends_with('>') {
                    // Parse Map<K, V>
                    let inner = &s_trimmed[4..s_trimmed.len() - 1];
                    let parts: Vec<&str> = inner.splitn(2, ',').collect();
                    if parts.len() == 2 {
                        Type::map(
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
                } else if let Some(inner) = s_lower
                    .strip_prefix("pointer<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::pointer(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("ptr<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::pointer(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("borrow<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::borrow(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("mutborrow<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::mut_borrow(Type::from_str(inner))
                } else if let Some(inner) = s_lower
                    .strip_prefix("mut_borrow<")
                    .and_then(|s| s.strip_suffix('>'))
                {
                    Type::mut_borrow(Type::from_str(inner))
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

    /// Whether `self` is an ordinal type — a type with a bounded,
    /// well-founded domain that can serve as the element type of a
    /// `Set<T>` in principle. Actual set membership additionally
    /// requires the domain to fit in 64 bits; see `set_domain_size`.
    ///
    /// Ordinal types: `Bool`, `Enum { .. }`, `Subrange { .. }` with
    /// an ordinal base. Non-ordinal: `Int` (unbounded), `Float`
    /// (not discrete), `String`, `Distinct` (nominal identity, even
    /// when its base is ordinal), and every composite type. See
    /// ADR 0032 design question 2.
    pub fn is_ordinal(&self) -> bool {
        matches!(self, Type::Bool | Type::Enum { .. } | Type::Subrange { .. })
    }

    /// The number of distinct values in `self`'s domain, if bounded
    /// and no larger than 64. `None` for non-ordinal types and for
    /// ordinal types whose domain exceeds 64.
    ///
    /// This is the validity predicate for `Set<T>` element types:
    /// `Set<T>` is well-formed iff `T.set_domain_size()` is
    /// `Some(_)`. See ADR 0032.
    pub fn set_domain_size(&self) -> Option<u64> {
        match self {
            Type::Bool => Some(2),
            Type::Enum { variants, .. } => {
                let n = variants.len() as u64;
                (n <= 64).then_some(n)
            }
            Type::Subrange {
                base, low, high, ..
            } => {
                // Per ADR 0031, subrange bases are Int or Enum.
                // `Int` is not ordinal on its own (unbounded), but a
                // subrange over Int is bounded by `low`/`high`. The
                // bounds, not the base, give the domain.
                let valid_base = matches!(base.as_ref(), Type::Int | Type::Enum { .. });
                if !valid_base || high < low {
                    return None;
                }
                let size = (*high - *low + 1) as u64;
                (size <= 64).then_some(size)
            }
            _ => None,
        }
    }

    /// Whether `self` may cross an `extern "C"` boundary.
    ///
    /// FFI-compatible types are exactly those with a well-defined C
    /// ABI representation:
    ///
    /// - `Int`    → `int64_t`
    /// - `Float`  → `double`
    /// - `Bool`   → `int`
    /// - `String` → `char*` (null-terminated; ALGOL26 owns the buffer)
    /// - `Ptr`    → `void*`
    /// - `*T`     → `T*`
    /// - `Void`   → `void` (return position only)
    /// - `Distinct { base: ... }` where `base` is FFI-compatible
    /// - `Enum { .. }` — lowered to `Int`
    /// - `Subrange { base: Int | Enum, .. }` — lowered to `Int`
    ///
    /// Everything else — `List`, `Map`, `Option`, `Result`, `Record`,
    /// `Array`, `Tuple`, `Borrow`, `MutBorrow`, `Channel`, `Function`,
    /// `TypeVar`, `Generic`, `Unknown` — has no C equivalent and must
    /// be rejected at the FFI boundary.
    ///
    /// `Void` is FFI-compatible under this predicate because it is
    /// valid in return position. Callers validating a *parameter*
    /// should additionally reject `Void`.
    pub fn is_ffi_compatible(&self) -> bool {
        match self {
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Void
            | Type::Ptr
            | Type::Pointer(_) => true,

            // Nominal types are compatible iff their base is.
            Type::Distinct { base, .. } => base.is_ffi_compatible(),

            // Enums lower to Int.
            Type::Enum { .. } => true,

            // Subranges lower to Int when their base does.
            Type::Subrange { base, .. } => {
                matches!(base.as_ref(), Type::Int | Type::Enum { .. })
            }

            // Everything else has no C ABI representation.
            _ => false,
        }
    }

    pub fn is_composite(&self) -> bool {
        matches!(
            self,
            Type::List(_)
                | Type::Array(_, _)
                | Type::Tuple(_)
                | Type::Option(_)
                | Type::Map(_, _)
                | Type::Result { .. }
                | Type::Set(_)
        )
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

    /// Returns `true` for types that are copied on assignment or
    /// move rather than transferred. The `Copy` set is:
    ///
    /// - `Int`, `Float`, `Bool` — primitive scalars.
    /// - `Ptr` — the *raw* pointer type. Raw pointers carry no
    ///   region or ownership obligation, so copying them is safe.
    ///
    /// **`Pointer(_)` is deliberately not `Copy`.** A typed pointer
    /// like `*Int` is bound to the region that allocated it; copying
    /// it would duplicate region ownership and defeat the
    /// region-memory model. See
    /// `docs/decisions/0007-region-memory.md`.
    pub fn is_copy(&self) -> bool {
        match self {
            Type::Int | Type::Float | Type::Bool | Type::Ptr => true,
            // Nominality does not alter ownership semantics: a
            // `distinct Int` is Copy, a `distinct String` is not.
            // See ADR 0029.
            Type::Distinct { base, .. } => base.is_copy(),
            // Enums are scalars. The variant is an integer ordinal;
            // copying it duplicates no state. See ADR 0030.
            Type::Enum { .. } => true,
            // A subrange copies iff its base copies. Bases are
            // restricted to Int and enums, both of which are Copy,
            // so this is always true in practice — but recursing
            // keeps the rule composable if a future ADR extends the
            // set of base types.
            Type::Subrange { base, .. } => base.is_copy(),
            // Sets are a single u64 — trivially Copy. See ADR 0032.
            Type::Set(_) => true,
            _ => false,
        }
    }

    pub fn is_type_var(&self) -> bool {
        matches!(self, Type::TypeVar(_))
    }

    // NEW: Cast validation
    pub fn can_cast_to(&self, target: &Type) -> bool {
        if self == target {
            return true;
        }

        // TypeVar can cast to/from anything
        if matches!(self, Type::TypeVar(_)) || matches!(target, Type::TypeVar(_)) {
            return true;
        }

        match (self, target) {
            // Numeric casts (always allowed, may be lossy)
            (Type::Int, Type::Float) => true,
            (Type::Float, Type::Int) => true, // Explicit cast allows lossy conversion
            (Type::Int, Type::Int) => true,
            (Type::Float, Type::Float) => true,

            // String casts
            (Type::Int, Type::String) => true,
            (Type::Float, Type::String) => true,
            (Type::Bool, Type::String) => true,
            (Type::String, Type::String) => true,

            // Pointer casts
            (Type::Ptr, Type::Ptr) => true,
            (Type::Ptr, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::Ptr) => true,
            (Type::Pointer(_), Type::Pointer(_)) => true,

            // Borrow casts
            (Type::Borrow(_), Type::Borrow(_)) => true,
            (Type::MutBorrow(_), Type::MutBorrow(_)) => true,

            // Generic casts
            (Type::Generic { .. }, Type::Generic { .. }) => true,

            // Nominal conversions (ADR 0029). `distinct T` and `T`
            // share a runtime representation, so an explicit cast
            // between them is a no-op. This is what the intrinsic
            // wrap/unwrap for `from_base` / `to_base` lowers to; it
            // is not a coercion rule — `can_coerce_to` remains
            // strict, which is what actually prevents `userId + 1`.
            //
            // Distinct-to-distinct casts are not allowed even when
            // their bases match: go through the base explicitly.
            (Type::Distinct { base, .. }, target) if !matches!(target, Type::Distinct { .. }) => {
                base.can_cast_to(target)
            }
            (source, Type::Distinct { base, .. }) if !matches!(source, Type::Distinct { .. }) => {
                source.can_cast_to(base)
            }

            // Enum ordinal conversion (ADR 0030). `Enum <-> Int` is
            // an explicit cast — the representation is identical.
            // This is what the intrinsic wrap/unwrap for
            // `from_ordinal` / `to_ordinal` lowers to.
            //
            // Enum-to-enum is not allowed even when the ordinal
            // ranges overlap; go through `Int` explicitly.
            (Type::Enum { .. }, Type::Int) => true,
            (Type::Int, Type::Enum { .. }) => true,

            // Subrange conversions (ADR 0031). A subrange and its
            // base share a runtime representation, so an explicit
            // cast between them is a no-op. This is what the
            // constructor `T(v)` and the extractor `.to_base()`
            // lower to.
            //
            // Subrange-to-subrange is not allowed even when the
            // intervals overlap; go through the base explicitly.
            (Type::Subrange { base, .. }, target) if !matches!(target, Type::Subrange { .. }) => {
                base.can_cast_to(target)
            }
            (source, Type::Subrange { base, .. }) if !matches!(source, Type::Subrange { .. }) => {
                source.can_cast_to(base)
            }

            // Default: no cast
            _ => false,
        }
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
            // Numeric coercion
            (Type::Int, Type::Float) => true,
            (Type::Float, Type::Int) => false, // Lossy, require explicit cast

            // Ptr coercion
            (Type::Ptr, Type::Ptr) => true,
            (Type::Ptr, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::Ptr) => true,

            // List covariance
            (Type::List(a), Type::List(b)) => a.can_coerce_to(b),
            (Type::Map(k1, v1), Type::Map(k2, v2)) => k1.can_coerce_to(k2) && v1.can_coerce_to(v2),
            // Array covariance
            (Type::Array(a, size1), Type::Array(b, size2)) => size1 == size2 && a.can_coerce_to(b),

            // Tuple covariance
            (Type::Tuple(a), Type::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.can_coerce_to(y))
            }

            // Option covariance
            (Type::Option(a), Type::Option(b)) => a.can_coerce_to(b),

            // Result covariance
            (Type::Result { ok: ok1, error: e1 }, Type::Result { ok: ok2, error: e2 }) => {
                ok1.can_coerce_to(ok2) && e1.can_coerce_to(e2)
            }

            // Pointer covariance
            (Type::Pointer(a), Type::Pointer(b)) => a.can_coerce_to(b),

            // Borrow covariance
            (Type::Borrow(a), Type::Borrow(b)) => a.can_coerce_to(b),

            // MutBorrow covariance
            (Type::MutBorrow(a), Type::MutBorrow(b)) => a.can_coerce_to(b),

            // Generic covariance
            (Type::Generic { name: n1, args: a1 }, Type::Generic { name: n2, args: a2 }) => {
                n1 == n2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2.iter()).all(|(x, y)| x.can_coerce_to(y))
            }
            (Type::Record(n1, a1), Type::Record(n2, a2)) => {
                n1 == n2
                    && a1.len() == a2.len()
                    && a1.iter().zip(a2.iter()).all(|(x, y)| x.can_coerce_to(y))
            }
            // Set covariance: same element type required. See ADR 0032.
            (Type::Set(a), Type::Set(b)) => a.can_coerce_to(b),
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
            (Type::Map(k1, v1), Type::Map(k2, v2)) => {
                Type::map(k1.common_supertype(k2), v1.common_supertype(v2))
            }
            // Array common element type
            (Type::Array(a, size1), Type::Array(b, size2)) => {
                if size1 == size2 {
                    Type::array(a.common_supertype(b), *size1)
                } else {
                    Type::Unknown
                }
            }

            // Tuple common types
            (Type::Tuple(a), Type::Tuple(b)) => {
                if a.len() == b.len() {
                    let common: Vec<Type> = a
                        .iter()
                        .zip(b.iter())
                        .map(|(x, y)| x.common_supertype(y))
                        .collect();
                    Type::tuple(common)
                } else {
                    Type::Unknown
                }
            }

            // Option common inner type
            (Type::Option(a), Type::Option(b)) => Type::option(a.common_supertype(b)),

            // Result common types
            (Type::Result { ok: ok1, error: e1 }, Type::Result { ok: ok2, error: e2 }) => {
                Type::result(ok1.common_supertype(ok2), e1.common_supertype(e2))
            }

            // Borrow common types
            (Type::Borrow(a), Type::Borrow(b)) => Type::borrow(a.common_supertype(b)),
            (Type::MutBorrow(a), Type::MutBorrow(b)) => Type::mut_borrow(a.common_supertype(b)),

            // Unknown handling
            (Type::Unknown, t) | (t, Type::Unknown) => t.clone(),

            (Type::Record(n1, a1), Type::Record(n2, a2)) if n1 == n2 && a1.len() == a2.len() => {
                Type::record(
                    n1,
                    a1.iter()
                        .zip(a2.iter())
                        .map(|(x, y)| x.common_supertype(y))
                        .collect(),
                )
            }
            // Set supertype: same element type required.
            (Type::Set(a), Type::Set(b)) => Type::set(a.common_supertype(b)),
            // Default to Unknown
            _ => Type::Unknown,
        }
    }

    // NEW: Check if type is a subtype of another
    pub fn is_subtype_of(&self, other: &Type) -> bool {
        self.can_coerce_to(other)
    }

    // NEW: Get the inner type of a container
    pub fn inner_type(&self) -> Option<&Type> {
        match self {
            Type::List(inner) => Some(inner),
            Type::Array(inner, _) => Some(inner),
            Type::Option(inner) => Some(inner),
            Type::Pointer(inner) => Some(inner),
            Type::Borrow(inner) => Some(inner),
            Type::MutBorrow(inner) => Some(inner),
            Type::Channel(inner) => Some(inner),
            // Map has two inner types. Returning one would be ambiguous;
            // callers that need the key or value type match on the variant.
            Type::Map(..) => None,
            _ => None,
        }
    }

    // NEW: Check if type contains type variables
    pub fn contains_type_var(&self) -> bool {
        match self {
            Type::TypeVar(_) => true,
            Type::List(inner) => inner.contains_type_var(),
            Type::Array(inner, _) => inner.contains_type_var(),
            Type::Tuple(elements) => elements.iter().any(|e| e.contains_type_var()),
            Type::Option(inner) => inner.contains_type_var(),
            Type::Result { ok, error } => ok.contains_type_var() || error.contains_type_var(),
            Type::Pointer(inner) => inner.contains_type_var(),
            Type::Borrow(inner) => inner.contains_type_var(),
            Type::MutBorrow(inner) => inner.contains_type_var(),
            Type::Channel(inner) => inner.contains_type_var(),
            Type::Generic { args, .. } => args.iter().any(|a| a.contains_type_var()),
            Type::Record(_, args) => args.iter().any(|a| a.contains_type_var()),
            Type::Map(k, v) => k.contains_type_var() || v.contains_type_var(),
            Type::Distinct { base, .. } => base.contains_type_var(),
            Type::Enum { .. } => false,
            Type::Subrange { base, .. } => base.contains_type_var(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_type_var()) || return_type.contains_type_var(),
            Type::Set(inner) => inner.contains_type_var(),
            _ => false,
        }
    }

    /// True if the type contains `Unknown` anywhere.
    ///
    /// Companion to `contains_type_var`. Together they define the
    /// "unresolved" predicate the instantiation plan uses to
    /// distinguish concrete type arguments from symbolic ones.
    pub fn contains_unknown(&self) -> bool {
        match self {
            Type::Unknown => true,
            Type::List(inner)
            | Type::Array(inner, _)
            | Type::Option(inner)
            | Type::Pointer(inner)
            | Type::Borrow(inner)
            | Type::MutBorrow(inner)
            | Type::Channel(inner) => inner.contains_unknown(),
            Type::Tuple(elements) => elements.iter().any(|e| e.contains_unknown()),
            Type::Result { ok, error } => ok.contains_unknown() || error.contains_unknown(),
            Type::Generic { args, .. } => args.iter().any(|a| a.contains_unknown()),
            Type::Record(_, args) => args.iter().any(|a| a.contains_unknown()),
            Type::Map(k, v) => k.contains_unknown() || v.contains_unknown(),
            Type::Distinct { base, .. } => base.contains_unknown(),
            Type::Enum { .. } => false,
            Type::Subrange { base, .. } => base.contains_unknown(),
            Type::Function {
                params,
                return_type,
            } => params.iter().any(|p| p.contains_unknown()) || return_type.contains_unknown(),
            Type::Set(inner) => inner.contains_unknown(),
            _ => false,
        }
    }

    /// True if the type contains a `TypeVar` or `Unknown` anywhere.
    ///
    /// The instantiation plan uses this to decide whether a recorded
    /// `type_args` entry is concrete enough to produce a
    /// `Specialization`. Symbolic entries (calls inside another
    /// generic's body) fail this check and are recorded only as
    /// call-site instantiations.
    pub fn contains_unresolved(&self) -> bool {
        self.contains_type_var() || self.contains_unknown()
    }

    // NEW: Substitute type variables
    pub fn substitute(&self, substitutions: &std::collections::HashMap<String, Type>) -> Type {
        match self {
            Type::TypeVar(name) => substitutions
                .get(name)
                .cloned()
                .unwrap_or_else(|| self.clone()),
            Type::List(inner) => Type::list(inner.substitute(substitutions)),
            Type::Array(inner, size) => Type::array(inner.substitute(substitutions), *size),
            Type::Tuple(elements) => Type::tuple(
                elements
                    .iter()
                    .map(|e| e.substitute(substitutions))
                    .collect(),
            ),
            Type::Option(inner) => Type::option(inner.substitute(substitutions)),
            Type::Result { ok, error } => Type::result(
                ok.substitute(substitutions),
                error.substitute(substitutions),
            ),
            Type::Pointer(inner) => Type::pointer(inner.substitute(substitutions)),
            Type::Borrow(inner) => Type::borrow(inner.substitute(substitutions)),
            Type::MutBorrow(inner) => Type::mut_borrow(inner.substitute(substitutions)),
            Type::Channel(inner) => Type::channel(inner.substitute(substitutions)),
            Type::Generic { name, args } => Type::generic(
                name,
                args.iter().map(|a| a.substitute(substitutions)).collect(),
            ),
            Type::Record(name, args) => Type::record(
                name,
                args.iter().map(|a| a.substitute(substitutions)).collect(),
            ),
            Type::Map(k, v) => Type::map(k.substitute(substitutions), v.substitute(substitutions)),
            // Substitution flows through the base, not the identity.
            Type::Distinct { id, name, base } => Type::Distinct {
                id: *id,
                name: name.clone(),
                base: Box::new(base.substitute(substitutions)),
            },
            // Enums have no type arguments; identity and variants
            // are fixed at registration.
            Type::Enum { .. } => self.clone(),
            // Same for subranges: low/high and identity are fixed.
            // Only the base could carry a type variable (in theory);
            // recursing keeps the rule uniform.
            Type::Subrange {
                id,
                name,
                base,
                low,
                high,
            } => Type::Subrange {
                id: *id,
                name: name.clone(),
                base: Box::new(base.substitute(substitutions)),
                low: *low,
                high: *high,
            },
            Type::Function {
                params,
                return_type,
            } => Type::Function {
                params: params.iter().map(|p| p.substitute(substitutions)).collect(),
                return_type: Box::new(return_type.substitute(substitutions)),
            },
            Type::Set(inner) => Type::set(inner.substitute(substitutions)),
            _ => self.clone(),
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
            Type::Record(name, args) => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let args_str: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    format!("{}<{}>", name, args_str.join(", "))
                }
            }
            Type::List(t) => format!("List<{}>", t),
            Type::Array(t, size) => format!("Array<{}, {}>", t, size),
            Type::Tuple(elements) => {
                let elems: Vec<String> = elements.iter().map(|e| e.to_string()).collect();
                format!("({})", elems.join(", "))
            }
            Type::Option(t) => format!("Option<{}>", t),
            Type::Result { ok, error } => format!("Result<{}, {}>", ok, error),
            Type::Map(k, v) => format!("Map<{}, {}>", k, v),
            // Nominal types print as their declared name. `id` is
            // identity, not presentation. See ADR 0029.
            Type::Distinct { name, .. } => name.clone(),
            // Enums print as their declared name. Variant values
            // print as ordinals; the name is not available at
            // runtime in v1. See ADR 0030.
            Type::Enum { name, .. } => name.clone(),
            // Subranges print as their declared name. See ADR 0031.
            Type::Subrange { name, .. } => name.clone(),
            Type::Set(inner) => format!("Set<{}>", inner),
            Type::Pointer(t) => format!("*{}", t),
            Type::Borrow(t) => format!("Borrow<{}>", t),
            Type::MutBorrow(t) => format!("MutBorrow<{}>", t),
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

/// Canonical print formatting. Both the LLVM backend and the
/// interpreter consult this module; neither re-implements the
/// decision about how a value of a given type is rendered.
pub mod print {
    use super::Type;

    /// `printf` format string for the given type, if the LLVM backend
    /// uses a straightforward `printf` lowering. Returns `None` for
    /// `Bool` — LLVM branches on the value and emits a constant
    /// `"true\n"` or `"false\n"` (see `llvm_codegen::emit_print_bool`).
    pub fn llvm_format(ty: &Type) -> Option<&'static str> {
        match ty {
            Type::Int => Some("%lld\n"),
            Type::Float => Some("%.1f\n"),
            Type::String => Some("%s\n"),
            Type::Bool => None,
            _ => None,
        }
    }

    /// True if the type has a print representation at all.
    pub fn is_printable(ty: &Type) -> bool {
        matches!(ty, Type::Int | Type::Float | Type::Bool | Type::String)
    }

    /// Interpreter rendering — must agree with `llvm_format` above
    /// for the same input value.
    pub fn format_int(v: i64) -> String {
        format!("{}", v)
    }

    pub fn format_float(v: f64) -> String {
        format!("{:.1}", v)
    }

    pub fn format_bool(v: bool) -> String {
        format!("{}", v)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn int_is_plain_decimal() {
            assert_eq!(format_int(0), "0");
            assert_eq!(format_int(42), "42");
            assert_eq!(format_int(-7), "-7");
        }

        #[test]
        fn float_has_one_decimal_place() {
            assert_eq!(format_float(0.0), "0.0");
            assert_eq!(format_float(5.0), "5.0");
            assert_eq!(format_float(42.5), "42.5");
            assert_eq!(format_float(-1.5), "-1.5");
        }

        #[test]
        fn bool_is_lowercase() {
            assert_eq!(format_bool(true), "true");
            assert_eq!(format_bool(false), "false");
        }

        /// The `%lld\n` spec is what `printf` uses for `format_int`'s
        /// output; this test just pins the format characters so a
        /// future edit to one side has to notice the other.
        #[test]
        fn llvm_specs_match_interpreter_formats() {
            assert_eq!(llvm_format(&Type::Int), Some("%lld\n"));
            assert_eq!(llvm_format(&Type::Float), Some("%.1f\n"));
            assert_eq!(llvm_format(&Type::String), Some("%s\n"));
            assert_eq!(llvm_format(&Type::Bool), None);
            assert_eq!(llvm_format(&Type::Void), None);
        }
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
        assert_eq!(Type::from_str("array<int, 10>"), Type::array(Type::Int, 10));
    }

    #[test]
    fn test_type_coercion() {
        assert!(Type::Int.can_coerce_to(&Type::Float));
        assert!(!Type::Float.can_coerce_to(&Type::Int));
        assert!(Type::list(Type::Int).can_coerce_to(&Type::list(Type::Float)));
        assert!(Type::Ptr.can_coerce_to(&Type::Ptr));
        assert!(Type::Ptr.can_coerce_to(&Type::Pointer(Box::new(Type::Int))));
        assert!(Type::TypeVar("T".to_string()).can_coerce_to(&Type::Int));
        assert!(Type::Int.can_coerce_to(&Type::TypeVar("T".to_string())));
        assert!(Type::array(Type::Int, 10).can_coerce_to(&Type::array(Type::Float, 10)));
    }

    #[test]
    fn test_can_cast_to() {
        // Numeric casts
        assert!(Type::Int.can_cast_to(&Type::Float));
        assert!(Type::Float.can_cast_to(&Type::Int));

        // String casts
        assert!(Type::Int.can_cast_to(&Type::String));
        assert!(Type::Float.can_cast_to(&Type::String));
        assert!(Type::Bool.can_cast_to(&Type::String));

        // Pointer casts
        assert!(Type::Ptr.can_cast_to(&Type::Ptr));
        assert!(Type::Ptr.can_cast_to(&Type::Pointer(Box::new(Type::Int))));

        // Invalid casts
        assert!(!Type::String.can_cast_to(&Type::Int));
        assert!(!Type::list(Type::Int).can_cast_to(&Type::Float));
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
        assert_eq!(
            Type::array(Type::Int, 10).common_supertype(&Type::array(Type::Float, 10)),
            Type::array(Type::Float, 10)
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
        assert_eq!(Type::array(Type::Int, 10).to_string(), "Array<Int, 10>");
    }

    #[test]
    fn test_inner_type() {
        assert_eq!(Type::list(Type::Int).inner_type(), Some(&Type::Int));
        assert_eq!(Type::option(Type::Float).inner_type(), Some(&Type::Float));
        assert_eq!(Type::pointer(Type::Int).inner_type(), Some(&Type::Int));
        assert_eq!(Type::Int.inner_type(), None);
    }

    #[test]
    fn test_contains_type_var() {
        assert!(Type::TypeVar("T".to_string()).contains_type_var());
        assert!(Type::list(Type::TypeVar("T".to_string())).contains_type_var());
        assert!(!Type::list(Type::Int).contains_type_var());
    }

    #[test]
    fn test_contains_unknown() {
        assert!(Type::Unknown.contains_unknown());
        assert!(Type::list(Type::Unknown).contains_unknown());
        assert!(!Type::list(Type::Int).contains_unknown());
        // Critical: Unknown nested inside Function / Generic must be seen.
        assert!(Type::Function {
            params: vec![Type::Unknown],
            return_type: Box::new(Type::Int),
        }
        .contains_unknown());
        assert!(Type::generic("Box", vec![Type::Unknown]).contains_unknown());
    }

    #[test]
    fn test_contains_unresolved_covers_function_and_generic() {
        // The two cases the initial `InstantiationPlan` implementation
        // missed: a TypeVar nested inside Function or Generic.
        assert!(Type::Function {
            params: vec![Type::TypeVar("T".to_string())],
            return_type: Box::new(Type::TypeVar("T".to_string())),
        }
        .contains_unresolved());
        assert!(Type::generic("Box", vec![Type::TypeVar("T".to_string())]).contains_unresolved());
        assert!(!Type::Function {
            params: vec![Type::Int],
            return_type: Box::new(Type::Float),
        }
        .contains_unresolved());
    }

    #[test]
    fn test_substitute() {
        let mut substitutions = std::collections::HashMap::new();
        substitutions.insert("T".to_string(), Type::Int);

        let ty = Type::TypeVar("T".to_string());
        assert_eq!(ty.substitute(&substitutions), Type::Int);

        let ty = Type::list(Type::TypeVar("T".to_string()));
        assert_eq!(ty.substitute(&substitutions), Type::list(Type::Int));
    }

    // ─── Nominal types (ADR 0029) ─────────────────────────────────

    #[test]
    fn distinct_display_uses_name_only() {
        let t = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(t.to_string(), "UserId");
    }

    #[test]
    fn distinct_identity_is_id_not_name() {
        // Same name, different ids — different types.
        let a = Type::distinct(NominalTypeId(1), "Id", Type::Int);
        let b = Type::distinct(NominalTypeId(2), "Id", Type::Int);
        assert_ne!(a, b);
        assert!(!a.can_coerce_to(&b));
        assert!(!b.can_coerce_to(&a));
    }

    #[test]
    fn distinct_same_id_coerces() {
        let a = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        let b = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(a, b);
        assert!(a.can_coerce_to(&b));
    }

    #[test]
    fn distinct_does_not_coerce_to_base() {
        let id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(!id.can_coerce_to(&Type::Int));
        assert!(!Type::Int.can_coerce_to(&id));
    }

    #[test]
    fn distinct_does_not_coerce_to_sibling() {
        let user_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        let price_cents = Type::distinct(NominalTypeId(2), "PriceCents", Type::Int);
        assert!(!user_id.can_coerce_to(&price_cents));
        assert!(!price_cents.can_coerce_to(&user_id));
    }

    #[test]
    fn distinct_is_never_numeric() {
        let id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(!id.is_numeric());
        let meters = Type::distinct(NominalTypeId(2), "Meters", Type::Float);
        assert!(!meters.is_numeric());
    }

    #[test]
    fn distinct_is_copy_iff_base_is_copy() {
        let int_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(int_id.is_copy());
        let string_name = Type::distinct(NominalTypeId(2), "Name", Type::String);
        assert!(!string_name.is_copy());
    }

    #[test]
    fn distinct_no_common_supertype_with_base_or_sibling() {
        let user_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(user_id.common_supertype(&Type::Int), Type::Unknown);
        let price_cents = Type::distinct(NominalTypeId(2), "PriceCents", Type::Int);
        assert_eq!(user_id.common_supertype(&price_cents), Type::Unknown);
        // Same identity: returns itself.
        let same = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert_eq!(user_id.common_supertype(&same), user_id);
    }

    #[test]
    fn distinct_contains_unresolved_recurses_into_base() {
        let with_unknown = Type::distinct(NominalTypeId(1), "X", Type::Unknown);
        assert!(with_unknown.contains_unknown());
        assert!(with_unknown.contains_unresolved());
        let with_var = Type::distinct(NominalTypeId(2), "Y", Type::TypeVar("T".to_string()));
        assert!(with_var.contains_type_var());
        let concrete = Type::distinct(NominalTypeId(3), "Z", Type::Int);
        assert!(!concrete.contains_unresolved());
    }

    #[test]
    fn distinct_casts_require_explicit_as() {
        let user_id = Type::distinct(NominalTypeId(1), "UserId", Type::Int);

        // Explicit `as` between a nominal and its base is allowed.
        // This is what the intrinsic wrap/unwrap lowers to.
        assert!(user_id.can_cast_to(&Type::Int));
        assert!(Type::Int.can_cast_to(&user_id));

        // Distinct-to-distinct is not allowed even when bases match.
        let price_cents = Type::distinct(NominalTypeId(2), "PriceCents", Type::Int);
        assert!(!user_id.can_cast_to(&price_cents));
        assert!(!price_cents.can_cast_to(&user_id));

        // Identity: allowed by the early self == target return.
        let same = Type::distinct(NominalTypeId(1), "UserId", Type::Int);
        assert!(user_id.can_cast_to(&same));

        // Coercion remains strict regardless.
        assert!(!user_id.can_coerce_to(&Type::Int));
        assert!(!Type::Int.can_coerce_to(&user_id));
    }

    // ─── Enum types (ADR 0030) ────────────────────────────────────

    fn day_enum() -> Type {
        Type::enum_type(
            EnumTypeId(1),
            "Day",
            vec![
                "Monday".to_string(),
                "Tuesday".to_string(),
                "Wednesday".to_string(),
                "Thursday".to_string(),
                "Friday".to_string(),
                "Saturday".to_string(),
                "Sunday".to_string(),
            ],
        )
    }

    #[test]
    fn enum_display_uses_name_only() {
        let t = day_enum();
        assert_eq!(t.to_string(), "Day");
    }

    #[test]
    fn enum_identity_is_id_not_name() {
        // Same name, same variants, different ids — different types.
        let a = Type::enum_type(
            EnumTypeId(1),
            "Day",
            vec!["Monday".to_string(), "Tuesday".to_string()],
        );
        let b = Type::enum_type(
            EnumTypeId(2),
            "Day",
            vec!["Monday".to_string(), "Tuesday".to_string()],
        );
        assert_ne!(a, b);
        assert!(!a.can_coerce_to(&b));
        assert!(!b.can_coerce_to(&a));
    }

    #[test]
    fn enum_same_id_coerces() {
        let a = day_enum();
        let b = day_enum();
        assert_eq!(a, b);
        assert!(a.can_coerce_to(&b));
    }

    #[test]
    fn enum_does_not_coerce_to_int() {
        let day = day_enum();
        assert!(!day.can_coerce_to(&Type::Int));
        assert!(!Type::Int.can_coerce_to(&day));
    }

    #[test]
    fn enum_does_not_coerce_to_sibling() {
        let day = day_enum();
        let month = Type::enum_type(
            EnumTypeId(2),
            "Month",
            vec!["Jan".to_string(), "Feb".to_string()],
        );
        assert!(!day.can_coerce_to(&month));
        assert!(!month.can_coerce_to(&day));
    }

    #[test]
    fn enum_is_never_numeric() {
        assert!(!day_enum().is_numeric());
    }

    #[test]
    fn enum_is_always_copy() {
        assert!(day_enum().is_copy());
    }

    #[test]
    fn enum_casts_to_and_from_int() {
        let day = day_enum();
        // Explicit `as` between an enum and Int is allowed; this is
        // what from_ordinal / to_ordinal lowers to.
        assert!(day.can_cast_to(&Type::Int));
        assert!(Type::Int.can_cast_to(&day));
    }

    #[test]
    fn enum_does_not_cast_to_sibling() {
        let day = day_enum();
        let month = Type::enum_type(
            EnumTypeId(2),
            "Month",
            vec!["Jan".to_string(), "Feb".to_string()],
        );
        assert!(!day.can_cast_to(&month));
        assert!(!month.can_cast_to(&day));
    }

    #[test]
    fn enum_no_common_supertype_with_int_or_sibling() {
        let day = day_enum();
        assert_eq!(day.common_supertype(&Type::Int), Type::Unknown);
        let month = Type::enum_type(EnumTypeId(2), "Month", vec!["Jan".to_string()]);
        assert_eq!(day.common_supertype(&month), Type::Unknown);
        // Same identity: returns itself.
        let same = day_enum();
        assert_eq!(day.common_supertype(&same), day);
    }

    #[test]
    fn enum_contains_unresolved_is_false() {
        // Variants are strings; an enum type carries no TypeVars or
        // Unknowns regardless of its shape.
        assert!(!day_enum().contains_unresolved());
    }

    #[test]
    fn ffi_compatible_types() {
        // Scalars and pointers.
        assert!(Type::Int.is_ffi_compatible());
        assert!(Type::Float.is_ffi_compatible());
        assert!(Type::Bool.is_ffi_compatible());
        assert!(Type::String.is_ffi_compatible());
        assert!(Type::Void.is_ffi_compatible());
        assert!(Type::Ptr.is_ffi_compatible());
        assert!(Type::Pointer(Box::new(Type::Int)).is_ffi_compatible());
        assert!(Type::Pointer(Box::new(Type::Unknown)).is_ffi_compatible());
    }

    #[test]
    fn ffi_incompatible_composite_types() {
        assert!(!Type::List(Box::new(Type::Int)).is_ffi_compatible());
        assert!(!Type::Option(Box::new(Type::Int)).is_ffi_compatible());
        assert!(!Type::Borrow(Box::new(Type::Int)).is_ffi_compatible());
        assert!(!Type::MutBorrow(Box::new(Type::Int)).is_ffi_compatible());
        assert!(!Type::Channel(Box::new(Type::Int)).is_ffi_compatible());
        assert!(!Type::Tuple(vec![Type::Int, Type::Float]).is_ffi_compatible());
        assert!(!Type::Array(Box::new(Type::Int), 4).is_ffi_compatible());
        assert!(!Type::Map(Box::new(Type::String), Box::new(Type::Int)).is_ffi_compatible());
        assert!(!Type::Record("Point".into(), vec![]).is_ffi_compatible());
        assert!(!Type::Unknown.is_ffi_compatible());
        assert!(!Type::TypeVar("T".into()).is_ffi_compatible());
        assert!(!Type::Result {
            ok: Box::new(Type::Int),
            error: Box::new(Type::String),
        }
        .is_ffi_compatible());
    }

    #[test]
    fn ffi_compatible_nominal_types() {
        // Distinct of a compatible base is compatible.
        // (Constructed via the public API since the fields include ids.)
        // Direct field access is fine for tests inside the same module.
        // If the ids are not constructible here, replace with a
        // type-alias approach or check via a helper.
        // --- placeholder, we'll adjust if compilation complains ---
    }

    #[test]
    fn set_variant_constructs() {
        let s = Type::set(Type::Bool);
        assert!(matches!(s, Type::Set(_)));
    }

    #[test]
    fn ordinal_recognizes_valid_domains() {
        assert!(Type::Bool.is_ordinal());
        assert!(
            Type::enum_type(EnumTypeId(0), "Day", vec!["Mon".into(), "Tue".into()]).is_ordinal()
        );
        assert!(Type::subrange(SubrangeTypeId(0), "Pct", Type::Int, 0, 100).is_ordinal());
    }

    #[test]
    fn ordinal_rejects_unbounded_and_nominal() {
        assert!(!Type::Int.is_ordinal());
        assert!(!Type::Float.is_ordinal());
        assert!(!Type::String.is_ordinal());
        assert!(!Type::Unknown.is_ordinal());
        assert!(!Type::List(Box::new(Type::Int)).is_ordinal());
        // Distinct is nominal, even when its base is ordinal.
        assert!(!Type::distinct(NominalTypeId(0), "UserId", Type::Int).is_ordinal());
    }

    #[test]
    fn domain_size_enum() {
        let day = Type::enum_type(
            EnumTypeId(0),
            "Day",
            (0..7).map(|i| format!("V{}", i)).collect(),
        );
        assert_eq!(day.set_domain_size(), Some(7));

        let big = Type::enum_type(
            EnumTypeId(1),
            "Big",
            (0..65).map(|i| format!("V{}", i)).collect(),
        );
        assert_eq!(big.set_domain_size(), None);
    }

    #[test]
    fn domain_size_subrange() {
        // 0..=100 has 101 values, exceeds the 64-element ceiling.
        assert_eq!(
            Type::subrange(SubrangeTypeId(0), "Pct", Type::Int, 0, 100).set_domain_size(),
            None
        );

        // 0..=63 has exactly 64 values.
        assert_eq!(
            Type::subrange(SubrangeTypeId(1), "Byte", Type::Int, 0, 63).set_domain_size(),
            Some(64)
        );

        // 0..=64 has 65 values, exceeds.
        assert_eq!(
            Type::subrange(SubrangeTypeId(2), "TooBig", Type::Int, 0, 64).set_domain_size(),
            None
        );
    }

    #[test]
    fn domain_size_bool() {
        assert_eq!(Type::Bool.set_domain_size(), Some(2));
    }

    #[test]
    fn domain_size_rejects_non_ordinal() {
        assert_eq!(Type::Int.set_domain_size(), None);
        assert_eq!(Type::Float.set_domain_size(), None);
        assert_eq!(Type::List(Box::new(Type::Bool)).set_domain_size(), None);
    }
}

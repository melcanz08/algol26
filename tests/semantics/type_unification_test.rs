// tests/semantics/type_unification_test.rs - HARDENED

use algol26::common::types::Type;

#[test]
fn test_unified_type_system() {
    // Test basic types
    assert_eq!(Type::from_str("int"), Type::Int);
    assert_eq!(Type::from_str("float"), Type::Float);
    assert_eq!(Type::from_str("string"), Type::String);
    assert_eq!(Type::from_str("bool"), Type::Bool);
    assert_eq!(Type::from_str("ptr"), Type::Ptr);
    assert_eq!(Type::from_str("unknown"), Type::Unknown);

    // Test generic types
    assert_eq!(Type::from_str("list<int>"), Type::list(Type::Int));
    assert_eq!(Type::from_str("option<float>"), Type::option(Type::Float));
    assert_eq!(
        Type::from_str("result<int, string>"),
        Type::result(Type::Int, Type::String)
    );
    assert_eq!(Type::from_str("borrow<float>"), Type::borrow(Type::Float));
    assert_eq!(
        Type::from_str("mut_borrow<float>"),
        Type::mut_borrow(Type::Float)
    );
    assert_eq!(Type::from_str("pointer<int>"), Type::pointer(Type::Int));

    // Test type coercion
    assert!(Type::Int.can_coerce_to(&Type::Float));
    assert!(!Type::Float.can_coerce_to(&Type::Int)); // Lossy conversion
    assert!(Type::list(Type::Int).can_coerce_to(&Type::list(Type::Float)));
    assert!(Type::option(Type::Int).can_coerce_to(&Type::option(Type::Float)));

    // Test common supertype
    assert_eq!(Type::Int.common_supertype(&Type::Float), Type::Float);
    assert_eq!(Type::Float.common_supertype(&Type::Int), Type::Float);
    assert_eq!(
        Type::list(Type::Int).common_supertype(&Type::list(Type::Float)),
        Type::list(Type::Float)
    );
    assert_eq!(Type::Int.common_supertype(&Type::Int), Type::Int);
}

#[test]
fn test_type_display() {
    assert_eq!(Type::Int.to_string(), "Int");
    assert_eq!(Type::Float.to_string(), "Float");
    assert_eq!(Type::String.to_string(), "String");
    assert_eq!(Type::Bool.to_string(), "Bool");
    assert_eq!(Type::Ptr.to_string(), "Ptr");
    assert_eq!(Type::list(Type::Float).to_string(), "List<Float>");
    assert_eq!(Type::option(Type::Int).to_string(), "Option<Int>");
    assert_eq!(
        Type::result(Type::Int, Type::String).to_string(),
        "Result<Int, String>"
    );
    assert_eq!(Type::borrow(Type::Int).to_string(), "Borrow<Int>");
    assert_eq!(Type::mut_borrow(Type::Int).to_string(), "MutBorrow<Int>");
}

#[test]
fn test_type_helpers() {
    // Numeric types
    assert!(Type::Int.is_numeric());
    assert!(Type::Float.is_numeric());
    assert!(!Type::String.is_numeric());
    assert!(!Type::Bool.is_numeric());

    // Composite types
    assert!(Type::list(Type::Int).is_composite());
    assert!(Type::option(Type::Int).is_composite());
    assert!(Type::result(Type::Int, Type::String).is_composite());
    assert!(!Type::Int.is_composite());

    // Pointer-like types
    assert!(Type::pointer(Type::Int).is_pointer_like());
    assert!(Type::borrow(Type::Int).is_pointer_like());
    assert!(Type::mut_borrow(Type::Int).is_pointer_like());
    assert!(Type::Ptr.is_pointer_like());
    assert!(!Type::Int.is_pointer_like());
}

#[test]
fn test_type_equality() {
    assert_eq!(Type::Int, Type::Int);
    assert_eq!(Type::list(Type::Int), Type::list(Type::Int));
    assert_ne!(Type::Int, Type::Float);
    assert_ne!(Type::list(Type::Int), Type::list(Type::Float));
}

#[test]
fn test_cast_rules() {
    // Numeric casts
    assert!(Type::Int.can_cast_to(&Type::Float));
    assert!(Type::Float.can_cast_to(&Type::Int)); // With potential loss

    // String casts
    assert!(Type::Int.can_cast_to(&Type::String));
    assert!(Type::Float.can_cast_to(&Type::String));
    assert!(Type::Bool.can_cast_to(&Type::String));

    // Invalid casts
    assert!(!Type::String.can_cast_to(&Type::Int)); // Ambiguous
    assert!(!Type::list(Type::Int).can_cast_to(&Type::Float));
}

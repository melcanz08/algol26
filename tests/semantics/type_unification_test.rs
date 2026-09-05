// tests/type_unification_test.rs

use algol26::common::types::Type;

#[test]
fn test_unified_type_system() {
    // Test basic types
    assert_eq!(Type::from_str("int"), Type::Int);
    assert_eq!(Type::from_str("float"), Type::Float);
    assert_eq!(Type::from_str("string"), Type::String);
    assert_eq!(Type::from_str("bool"), Type::Bool);

    // Test generic types
    assert_eq!(Type::from_str("list<int>"), Type::list(Type::Int));
    assert_eq!(Type::from_str("option<float>"), Type::option(Type::Float));
    assert_eq!(
        Type::from_str("result<int, string>"),
        Type::result(Type::Int, Type::String)
    );

    // Test type coercion
    assert!(Type::Int.can_coerce_to(&Type::Float));
    assert!(Type::list(Type::Int).can_coerce_to(&Type::list(Type::Float)));

    // Test common supertype
    assert_eq!(Type::Int.common_supertype(&Type::Float), Type::Float);
    assert_eq!(
        Type::list(Type::Int).common_supertype(&Type::list(Type::Float)),
        Type::list(Type::Float)
    );
}

#[test]
fn test_type_display() {
    assert_eq!(Type::Int.to_string(), "Int");
    assert_eq!(Type::list(Type::Float).to_string(), "List<Float>");
    assert_eq!(
        Type::result(Type::Int, Type::String).to_string(),
        "Result<Int, String>"
    );
}

#[test]
fn test_type_helpers() {
    assert!(Type::Int.is_numeric());
    assert!(Type::Float.is_numeric());
    assert!(!Type::String.is_numeric());

    assert!(Type::list(Type::Int).is_composite());
    assert!(Type::pointer(Type::Int).is_pointer_like());
    assert!(Type::borrow(Type::Int).is_pointer_like());
    assert!(Type::mut_borrow(Type::Int).is_pointer_like());
}

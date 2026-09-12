// src/ir/verifier/value.rs

use super::*;

pub(super) fn verify_value(value: &TypedIRValue, env: &VerifyEnv) -> Result<Type, String> {
    Ok(match value {
        TypedIRValue::Int(_) => Type::Int,
        TypedIRValue::Float(_) => Type::Float,
        TypedIRValue::Bool(_) => Type::Bool,
        TypedIRValue::String(_) => Type::String,
        TypedIRValue::Void => Type::Void,
        TypedIRValue::NullPtr => Type::Ptr,
        TypedIRValue::PtrLiteral(_) => Type::Ptr,
        TypedIRValue::Variable(name, claimed) => {
            let actual = env
                .variables
                .get(name)
                .ok_or_else(|| format!("Use of undefined variable '{}' in verified IR", name))?
                .clone();

            if !claimed.is_unknown() && !actual.is_unknown() && claimed != &actual {
                return Err(format!(
                    "Variable '{}' claimed type {:?} but declared as {:?}",
                    name, claimed, actual
                ));
            }
            actual
        }
        TypedIRValue::List(elements, claimed_elem) => {
            let mut common: Option<Type> = None;
            for elem in elements {
                let t = verify_value(elem, env)?;
                common = Some(match common {
                    None => t,
                    Some(prev) => prev.common_supertype(&t),
                });
            }
            let elem_ty = common.unwrap_or(Type::Unknown);
            if !claimed_elem.is_unknown()
                && !elem_ty.is_unknown()
                && claimed_elem != &elem_ty
            {
                return Err(format!(
                    "List claims element type {:?} but elements imply {:?}",
                    claimed_elem, elem_ty
                ));
            }
            Type::list(elem_ty)
        }
        TypedIRValue::BinaryOp {
            op,
            left,
            right,
            result_type,
        } => {
            let lt = verify_value(left, env)?;
            let rt = verify_value(right, env)?;
            let expected = compute_binop_type(op, &lt, &rt)?;

            if !result_type.is_unknown()
                && !expected.is_unknown()
                && result_type != &expected
            {
                return Err(format!(
                    "BinaryOp claims result {:?} but {:?} {:?} {:?} implies {:?}",
                    result_type, lt, op, rt, expected
                ));
            }
            expected
        }
        TypedIRValue::Cast { value, target_type } => {
            let source_ty = verify_value(value, env)?;

            // Reject casts the language does not consider legal, but
            // only when both types are known. `Unknown` on either side
            // is permissive: the analyzer has already made the decision,
            // and the verifier is a second check that should not invent
            // errors the analyzer accepted.
            if !source_ty.is_unknown()
                && !target_type.is_unknown()
                && source_ty != *target_type
                && !source_ty.can_cast_to(target_type)
            {
                return Err(format!(
                    "Cast from {:?} to {:?} is not permitted",
                    source_ty, target_type
                ));
            }
            target_type.clone()
        }
        TypedIRValue::ArrayAccess {
            array,
            index,
            element_type,
        } => {
            let arr_ty = verify_value(array, env)?;
            let idx_ty = verify_value(index, env)?;

            if !idx_ty.is_unknown() && idx_ty != Type::Int {
                return Err(format!("Array index must be Int, found {:?}", idx_ty));
            }

            let elem = match arr_ty {
                Type::List(t) => *t,
                Type::Array(t, _) => *t,
                Type::Unknown => Type::Unknown,
                other => {
                    return Err(format!("Array access on non-list type {:?}", other))
                }
            };

            if !element_type.is_unknown() && !elem.is_unknown() && element_type != &elem {
                return Err(format!(
                    "ArrayAccess claims element {:?} but array yields {:?}",
                    element_type, elem
                ));
            }
            elem
        }
        TypedIRValue::Borrow { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = Type::borrow(inner);
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "Borrow claims {:?} but operand is {:?}",
                    target_type, expected
                ));
            }
            expected
        }
        TypedIRValue::MutBorrow { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = Type::mut_borrow(inner);
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "MutBorrow claims {:?} but operand is {:?}",
                    target_type, expected
                ));
            }
            expected
        }
        TypedIRValue::Deref { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = match inner {
                Type::Pointer(t) | Type::Borrow(t) | Type::MutBorrow(t) => *t,
                Type::Unknown => Type::Unknown,
                other => {
                    return Err(format!("Deref on non-pointer type {:?}", other))
                }
            };
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "Deref claims {:?} but pointer points to {:?}",
                    target_type, expected
                ));
            }
            expected
        }
        TypedIRValue::AddrOf { expr, target_type } => {
            let inner = verify_value(expr, env)?;
            let expected = Type::pointer(inner);
            if !target_type.is_unknown() && !expected.is_unknown() && target_type != &expected {
                return Err(format!(
                    "AddrOf claims {:?} but expected {:?}",
                    target_type, expected
                ));
            }
            expected
        }
        TypedIRValue::Call {
            function,
            args,
            return_type,
        } => {
            let arg_types: Result<Vec<_>, _> =
                args.iter().map(|a| verify_value(a, env)).collect();
            let arg_types = arg_types?;

            let sig = env.function_sigs.get(function).ok_or_else(|| {
                format!("Call to undefined function '{}' in verified IR", function)
            })?;

            if arg_types.len() != sig.params.len() {
                return Err(format!(
                    "Call to '{}' expects {} args, found {}",
                    function,
                    sig.params.len(),
                    arg_types.len()
                ));
            }

            for (i, ((_, param_ty), arg_ty)) in
                sig.params.iter().zip(&arg_types).enumerate()
            {
                if arg_ty.is_unknown() || param_ty.is_unknown() {
                    continue;
                }
                // A parameter whose type mentions a type variable is
                // generic; the analyzer has already bound the variable
                // against the actual argument. Skip the check.
                if contains_type_var(param_ty) {
                    continue;
                }
                // `List<Unknown>` (and similar) act as wildcards for built-ins
                // such as List.length. Treat a parameter type that is a
                // composite containing Unknown as matching any instantiation.
                if types_compatible_for_call(arg_ty, param_ty) {
                    continue;
                }
                return Err(format!(
                    "Call to '{}' arg {} type mismatch: expected {:?}, found {:?}",
                    function, i, param_ty, arg_ty
                ));
            }

                        // A generic function's signature return type is a type
            // variable (possibly nested). The analyzer has already
            // bound it against the actual argument types and rewritten
            // the call site; the verifier has no scope to re-derive
            // that binding. Skip the comparison and trust the claimed
            // type.
            let sig_is_generic = contains_type_var(&sig.return_type);

            if !sig_is_generic
                && !return_type.is_unknown()
                && !sig.return_type.is_unknown()
                && return_type != &sig.return_type
            {
                return Err(format!(
                    "Call to '{}' claims return {:?} but signature says {:?}",
                    function, return_type, sig.return_type
                ));
            }

            // If the signature is generic, return the claimed type so
            // downstream verification uses the concrete form the
            // analyzer produced. Otherwise return the signature type.
            if sig_is_generic && !return_type.is_unknown() {
                return_type.clone()
            } else {
                sig.return_type.clone()
            }
        }
        TypedIRValue::Array(elements, elem_type, len) => {
            if elements.len() != *len {
                return Err(format!(
                    "Array literal claims length {} but contains {} elements",
                    len,
                    elements.len()
                ));
            }

            let mut common: Option<Type> = None;
            for e in elements {
                let t = verify_value(e, env)?;
                common = Some(match common {
                    None => t,
                    Some(prev) => prev.common_supertype(&t),
                });
            }
            let actual = common.unwrap_or(Type::Unknown);

            if !elem_type.is_unknown()
                && !actual.is_unknown()
                && elem_type != &actual
            {
                return Err(format!(
                    "Array literal claims element type {:?} but elements imply {:?}",
                    elem_type, actual
                ));
            }
            Type::array(actual, *len)
        }
        TypedIRValue::Range(start, end) => {
            let st = verify_value(start, env)?;
            let et = verify_value(end, env)?;

            if !st.is_numeric() && !st.is_unknown() {
                return Err(format!(
                    "Range start must be numeric, found {:?}",
                    st
                ));
            }
            if !et.is_numeric() && !et.is_unknown() {
                return Err(format!(
                    "Range end must be numeric, found {:?}",
                    et
                ));
            }
            Type::list(st.common_supertype(&et))
        }
        TypedIRValue::FieldAccess {
            object,
            field: _,
            field_type,
        } => {
            // The analyzer has already validated that the field exists
            // on the object's type. Verify the object is sound and
            // return the claimed field type.
            let _obj = verify_value(object, env)?;
            field_type.clone()
        }
        // Any remaining variant falls through with its self-described
        // type. This is a safety valve; every variant should eventually
        // be handled explicitly.
        _ => value.type_of(),
    })
}

pub(super) fn compute_binop_type(op: &SemanticBinOp, lt: &Type, rt: &Type) -> Result<Type, String> {
    match op {
        SemanticBinOp::Add => {
            if *lt == Type::String && *rt == Type::String {
                Ok(Type::String)
            } else if lt.is_numeric() && rt.is_numeric() {
                Ok(lt.common_supertype(rt))
            } else if lt.is_unknown() || rt.is_unknown() {
                Ok(Type::Unknown)
            } else {
                Err(format!(
                    "Add: incompatible operand types {:?} and {:?}",
                    lt, rt
                ))
            }
        }
        SemanticBinOp::Subtract
        | SemanticBinOp::Multiply
        | SemanticBinOp::Divide => {
            if lt.is_numeric() && rt.is_numeric() {
                Ok(lt.common_supertype(rt))
            } else if lt.is_unknown() || rt.is_unknown() {
                Ok(Type::Unknown)
            } else {
                Err(format!(
                    "Arithmetic: incompatible operand types {:?} and {:?}",
                    lt, rt
                ))
            }
        }
        SemanticBinOp::Greater
        | SemanticBinOp::Less
        | SemanticBinOp::GreaterEqual
        | SemanticBinOp::LessEqual => {
            if (lt.is_numeric() && rt.is_numeric()) || lt.is_unknown() || rt.is_unknown() {
                Ok(Type::Bool)
            } else {
                Err(format!(
                    "Comparison: incompatible operand types {:?} and {:?}",
                    lt, rt
                ))
            }
        }
        SemanticBinOp::Equal | SemanticBinOp::NotEqual => Ok(Type::Bool),
    }
}

/// True if `arg_ty` is acceptable for a parameter of type `param_ty`.
///
/// Differs from `can_coerce_to` in two ways:
/// 1. `Unknown` inside a composite parameter (e.g. `List<Unknown>` from
///    a built-in signature) matches any concrete instantiation.
/// 2. Numeric coercions (Int→Float) are permitted as before.
pub(super) fn types_compatible_for_call(arg_ty: &Type, param_ty: &Type) -> bool {
    // Numeric coercion.
    if arg_ty.is_numeric() && param_ty.is_numeric() {
        return true;
    }

    // Exact match.
    if arg_ty == param_ty {
        return true;
    }

    // Structural match with Unknown-as-wildcard.
    match (arg_ty, param_ty) {
        (_, Type::Unknown) | (Type::Unknown, _) => true,
        (Type::List(a), Type::List(p)) => types_compatible_for_call(a, p),
        (Type::Array(a, _), Type::Array(p, _)) => types_compatible_for_call(a, p),
        (Type::Option(a), Type::Option(p)) => types_compatible_for_call(a, p),
        (Type::Result { ok: ao, error: ae }, Type::Result { ok: po, error: pe }) => {
            types_compatible_for_call(ao, po) && types_compatible_for_call(ae, pe)
        }
        (Type::Pointer(a), Type::Pointer(p)) => types_compatible_for_call(a, p),
        (Type::Borrow(a), Type::Borrow(p)) => types_compatible_for_call(a, p),
        (Type::MutBorrow(a), Type::MutBorrow(p)) => types_compatible_for_call(a, p),
        (Type::Channel(a), Type::Channel(p)) => types_compatible_for_call(a, p),
        _ => arg_ty.can_coerce_to(param_ty),
    }
}
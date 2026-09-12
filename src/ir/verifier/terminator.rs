// src/ir/verifier/terminator.rs

use super::*;

pub(super) fn verify_terminator(
    func: &SemanticFunction,
    terminator: &Terminator,
    env: &mut VerifyEnv,
) -> Result<(), String> {
    match terminator {
        Terminator::Return { value, type_ } => {
            if let Some(val) = value {
                let actual_type = verify_value(val, env)?;
                if !actual_type.is_unknown()
                    && !type_.is_unknown()
                    && actual_type != *type_
                    && !actual_type.can_coerce_to(type_)
                {
                    return Err(format!(
                        "Function '{}' return type mismatch: expected {:?}, found {:?}",
                        func.name, type_, actual_type
                    ));
                }
            }

            if !type_.is_unknown()
                && !func.return_type.is_unknown()
                && *type_ != func.return_type
            {
                return Err(format!(
                    "Function '{}' declared return type {:?} but returns {:?}",
                    func.name, func.return_type, type_
                ));
            }
            Ok(())
        }
        Terminator::Branch { condition, .. } => {
            let cond_ty = verify_value(condition, env)?;
            if !cond_ty.is_unknown() && cond_ty != Type::Bool {
                return Err(format!(
                    "Function '{}' branch condition must be Bool, found {:?}",
                    func.name, cond_ty
                ));
            }
            Ok(())
        }
        Terminator::Switch { value, .. } => {
            let value_type = verify_value(value, env)?;
            if value_type == Type::Void {
                return Err(format!(
                    "Function '{}' switch value cannot be Void",
                    func.name
                ));
            }
            Ok(())
        }
        Terminator::IteratorNext {
            iterator, target, ..
        } => {
            let elem_ty = env
                .iterator_elem_types
                .get(iterator)
                .cloned()
                .unwrap_or(Type::Unknown);
            env.variables.insert(target.clone(), elem_ty);
            env.mutability.insert(target.clone(), false);
            Ok(())
        }
        _ => Ok(()),
    }
}
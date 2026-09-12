// src/ir/verifier/instruction.rs

use super::*;

pub(super) fn verify_instruction(
    func: &SemanticFunction,
    instr: &Instruction,
    env: &mut VerifyEnv,
) -> Result<(), String> {
    match instr {
        Instruction::Nop => Ok(()),
        Instruction::Declare {
            name,
            type_,
            value,
            mutable,
        } => {
            let value_ty = verify_value(value, env)?;

            // `Void` is a legitimate placeholder for variables that are
            // declared in one block and assigned in another.
            if !matches!(value, TypedIRValue::Void)
                && *type_ != Type::Unknown
                && value_ty != Type::Unknown
                && !value_ty.can_coerce_to(type_)
                && value_ty != *type_
            {
                return Err(format!(
                    "Function '{}': Declare '{}' as {:?} but value has type {:?}",
                    func.name, name, type_, value_ty
                ));
            }

            env.variables.insert(name.clone(), type_.clone());
            env.mutability.insert(name.clone(), *mutable);
            Ok(())
        }
        Instruction::Assign { target, value } => {
            let target_ty = env
                .variables
                .get(target)
                .ok_or_else(|| {
                    format!(
                        "Function '{}': Assign to undefined variable '{}'",
                        func.name, target
                    )
                })?
                .clone();

            if !env.mutability.get(target).copied().unwrap_or(false) {
                return Err(format!(
                    "Function '{}': Assign to immutable variable '{}'",
                    func.name, target
                ));
            }

            let value_ty = verify_value(value, env)?;

            let expected = match &target_ty {
                Type::MutBorrow(inner) => (**inner).clone(),
                _ => target_ty.clone(),
            };

            if !types_compatible_for_call(&value_ty, &expected) {
                return Err(format!(
                    "Function '{}': Assign to '{}' expected {:?}, found {:?}",
                    func.name, target, expected, value_ty
                ));
            }

            Ok(())
        }
        Instruction::Print { value } => {
            verify_value(value, env)?;
            Ok(())
        }
        // ─── Stage 2 additions ───
        Instruction::ArrayAssign {
            array,
            index,
            value,
        } => {
            let arr_ty = verify_value(array, env)?;
            let idx_ty = verify_value(index, env)?;

            if !idx_ty.is_unknown() && idx_ty != Type::Int {
                return Err(format!(
                    "Function '{}': ArrayAssign index must be Int, found {:?}",
                    func.name, idx_ty
                ));
            }

            let elem = match arr_ty {
                Type::List(t) => *t,
                Type::Array(t, _) => *t,
                Type::Unknown => Type::Unknown,
                other => {
                    return Err(format!(
                        "Function '{}': ArrayAssign on non-list type {:?}",
                        func.name, other
                    ));
                }
            };

            let val_ty = verify_value(value, env)?;
            if !elem.is_unknown()
                && !val_ty.is_unknown()
                && !val_ty.can_coerce_to(&elem)
                && val_ty != elem
            {
                return Err(format!(
                    "Function '{}': ArrayAssign value type {:?} does not coerce to element type {:?}",
                    func.name, val_ty, elem
                ));
            }
            Ok(())
        }
        Instruction::Call {
            func: callee,
            args,
            result,
        } => {
            let arg_types: Result<Vec<_>, _> =
                args.iter().map(|a| verify_value(a, env)).collect();
            let arg_types = arg_types?;

            let sig = env.function_sigs.get(callee).ok_or_else(|| {
                format!(
                    "Function '{}': Call to undefined function '{}'",
                    func.name, callee
                )
            })?;

            if arg_types.len() != sig.params.len() {
                return Err(format!(
                    "Function '{}': Call to '{}' expects {} args, found {}",
                    func.name, callee, sig.params.len(), arg_types.len()
                ));
            }

            for (i, ((_, param_ty), arg_ty)) in
                sig.params.iter().zip(&arg_types).enumerate()
            {
                if arg_ty.is_unknown() || param_ty.is_unknown() {
                    continue;
                }
                if types_compatible_for_call(arg_ty, param_ty) {
                    continue;
                }
                return Err(format!(
                    "Function '{}': Call to '{}' arg {} type mismatch: expected {:?}, found {:?}",
                    func.name, callee, i, param_ty, arg_ty
                ));
            }

            if let Some(name) = result {
                let result_ty = if contains_type_var(&sig.return_type) {
                    Type::Unknown
                } else {
                    sig.return_type.clone()
                };
                env.variables.insert(name.clone(), result_ty);
                env.mutability.insert(name.clone(), false);
            }
            Ok(())
        }
        Instruction::IteratorInit { iterator, iterable } => {
            let iter_ty = verify_value(iterable, env)?;

            let elem_ty = match &iter_ty {
                Type::List(t) => (**t).clone(),
                Type::Array(t, _) => (**t).clone(),
                Type::Unknown => Type::Unknown,
                other => {
                    return Err(format!(
                        "Function '{}': IteratorInit on non-list type {:?}",
                        func.name, other
                    ));
                }
            };

            // Record the element type. `IteratorNext` reads it back to
            // bind the loop variable with the actual element type.
            env.iterator_elem_types.insert(iterator.clone(), elem_ty);
            Ok(())
        }
        Instruction::ChannelDecl { name, type_ } => {
            match type_ {
                Type::Channel(_) | Type::Unknown => {}
                other => {
                    return Err(format!(
                        "Function '{}': ChannelDecl '{}' declared with non-channel type {:?}",
                        func.name, name, other
                    ));
                }
            }
            env.variables.insert(name.clone(), type_.clone());
            env.mutability.insert(name.clone(), false);
            Ok(())
        }
        Instruction::Send { channel, value }
        | Instruction::ChannelSend { channel, value } => {
            let chan_ty = env.variables.get(channel).ok_or_else(|| {
                format!(
                    "Function '{}': Send on undeclared channel '{}'",
                    func.name, channel
                )
            })?;
            if !matches!(chan_ty, Type::Channel(_) | Type::Unknown) {
                return Err(format!(
                    "Function '{}': Send on non-channel variable '{}' of type {:?}",
                    func.name, channel, chan_ty
                ));
            }
            verify_value(value, env)?;
            Ok(())
        }
        Instruction::Receive { channel, target }
        | Instruction::ChannelReceive { channel, target } => {
            let chan_ty = env.variables.get(channel).ok_or_else(|| {
                format!(
                    "Function '{}': Receive on undeclared channel '{}'",
                    func.name, channel
                )
            })?;
            let elem_ty = match chan_ty {
                Type::Channel(t) => (**t).clone(),
                Type::Unknown => Type::Unknown,
                other => {
                    return Err(format!(
                        "Function '{}': Receive on non-channel variable '{}' of type {:?}",
                        func.name, channel, other
                    ));
                }
            };
            env.variables.insert(target.clone(), elem_ty);
            env.mutability.insert(target.clone(), true);
            Ok(())
        }
        Instruction::Allocate {
            target,
            size,
            type_,
        } => {
            let size_ty = verify_value(size, env)?;
            if !size_ty.is_unknown() && size_ty != Type::Int {
                return Err(format!(
                    "Function '{}': Allocate size must be Int, found {:?}",
                    func.name, size_ty
                ));
            }
            env.variables.insert(target.clone(), type_.clone());
            env.mutability.insert(target.clone(), true);
            Ok(())
        }
        Instruction::Free { ptr } => {
            let ptr_ty = verify_value(ptr, env)?;
            match ptr_ty {
                Type::Pointer(_) | Type::Ptr | Type::Unknown => Ok(()),
                other => Err(format!(
                    "Function '{}': Free on non-pointer type {:?}",
                    func.name, other
                )),
            }
        }
    }
}
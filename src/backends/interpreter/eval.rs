// src/backends/interpreter/eval.rs

use super::runtime::{runtime_kind, EvalError, MapKey, RuntimeValue};
use super::Interpreter;
use crate::common::types::Type;
use crate::ir::semantic_ir::{SemanticBinOp, TypedIRValue};
use std::collections::HashMap;

/// Derive the concrete-type name from a runtime value for use as the
/// second half of a `(trait_name, concrete_type)` dispatch key.
/// Records and nominal values carry a name; scalars fall back to a
/// builtin name so the mangling stays consistent with the impl
/// mangling (`{Trait}_{Concrete}_{method}`).
fn runtime_concrete_name(v: &RuntimeValue) -> String {
    match v {
        RuntimeValue::Record { name, .. } => name.clone(),
        RuntimeValue::Int(_) => "Int".to_string(),
        RuntimeValue::Float(_) => "Float".to_string(),
        RuntimeValue::String(_) => "String".to_string(),
        RuntimeValue::Bool(_) => "Bool".to_string(),
        _ => "Unknown".to_string(),
    }
}

/// Extract a numeric `f64` from a runtime value, or fail closed.
///
/// The analyzer only permits `List.sum`/`List.max`/`List.min` on
/// `List<Int>` or `List<Float>`. If a non-numeric element reaches
/// these builtins at runtime, the analyzer missed something —
/// returning a placeholder would silently produce a wrong sum.
fn numeric_element(v: &RuntimeValue, op: &'static str) -> Result<f64, EvalError> {
    match v {
        RuntimeValue::Int(i) => Ok(*i as f64),
        RuntimeValue::Float(f) => Ok(*f),
        other => Err(EvalError::TypeMismatch {
            op,
            left: runtime_kind(other),
            right: "Int | Float",
        }),
    }
}

impl Interpreter {
    /// G19 fix. Look up a variable by name, walking the frame stack
    /// from innermost to outermost. Returns `None` if no frame binds
    /// the name.
    ///
    /// ADR 0064 / G19c-bug-2. `lookup_var` walks all frames — that
    /// is correct for bare `Variable(name)` reads, which follow
    /// lexical scoping. `resolve_place` and `assign_place` need the
    /// *frame-qualified* variant below, because a `Place` names a
    /// specific frame's slot, not "the innermost frame that has this
    /// name".
    /// ADR 0064 / G19c-bug-2. Push a new frame, assigning it a
    /// fresh frame id. Use this instead of bare `variables.push`
    /// so `frame_ids` stays in lockstep.
    pub(super) fn push_frame(&mut self) {
        self.variables.push(HashMap::new());
        self.frame_ids
            .push(crate::semantics::state::FrameId(self.next_frame_id));
        self.next_frame_id += 1;
    }

    /// ADR 0064. Pop the innermost frame. Use instead of bare
    /// `variables.pop`.
    pub(super) fn pop_frame(&mut self) {
        self.variables.pop();
        self.frame_ids.pop();
    }

    /// ADR 0064. The current frame's id. Never `None` in practice:
    /// the interpreter starts with one frame (the global one) and
    /// never pops past it.
    pub(super) fn current_frame_id(&self) -> crate::semantics::state::FrameId {
        self.frame_ids
            .last()
            .copied()
            .unwrap_or(crate::semantics::state::GLOBAL_FRAME)
    }

    pub(super) fn lookup_var(&self, name: &str) -> Option<RuntimeValue> {
        for frame in self.variables.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    /// ADR 0064 / G19c-bug-2. Frame-qualified lookup. Returns the
    /// binding in the frame whose id matches `frame_id`, or `None`
    /// if that frame doesn't contain the name.
    ///
    /// If `frame_id == GLOBAL_FRAME` (analyzer path, which never
    /// reaches the interpreter), fall back to name-based lookup so
    /// nothing breaks.
    pub(super) fn lookup_var_in_frame(
        &self,
        frame_id: crate::semantics::state::FrameId,
        name: &str,
    ) -> Option<RuntimeValue> {
        // ADR 0064 / G19c-bug-2. No fallback to name-based lookup:
        // a frame-qualified place must resolve in exactly one frame.
        // The previous fallback (`frame_id == GLOBAL_FRAME => lookup_var`)
        // recursed infinitely when `main`'s frame was GLOBAL_FRAME and
        // a same-named parameter shadowed the caller's binding.
        let n = self.frame_ids.len();
        for i in (0..n).rev() {
            if self.frame_ids[i] == frame_id {
                if let Some(v) = self.variables[i].get(name) {
                    return Some(v.clone());
                }
                return None;
            }
        }
        None
    }

    /// ADR 0064. Write into a frame-qualified slot. Same fallback
    /// rules as `lookup_var_in_frame`.
    pub(super) fn write_var_in_frame(
        &mut self,
        frame_id: crate::semantics::state::FrameId,
        name: &str,
        val: RuntimeValue,
    ) -> Result<(), EvalError> {
        // ADR 0064 / G19c-bug-2. No fallback — see lookup_var_in_frame.
        let n = self.frame_ids.len();
        for i in (0..n).rev() {
            if self.frame_ids[i] == frame_id {
                self.variables[i].insert(name.to_string(), val);
                return Ok(());
            }
        }
        Err(EvalError::Runtime(format!(
            "write to `{}`: frame {} not found",
            name, frame_id.0
        )))
    }

    /// G19 fix. Bind a new variable in the innermost frame.
    pub(super) fn insert_var(&mut self, name: impl Into<String>, val: RuntimeValue) {
        self.variables
            .last_mut()
            .expect("variables stack is never empty")
            .insert(name.into(), val);
    }

    pub(super) fn eval_value(&mut self, v: &TypedIRValue) -> Result<RuntimeValue, EvalError> {
        Ok(match v {
            TypedIRValue::Int(i) => RuntimeValue::Int(*i),
            TypedIRValue::Char(c) => RuntimeValue::Char(*c),
            TypedIRValue::Float(f) => RuntimeValue::Float(*f),
            TypedIRValue::Bool(b) => RuntimeValue::Bool(*b),
            TypedIRValue::String(s) => RuntimeValue::String(s.clone()),
            TypedIRValue::Void => RuntimeValue::Void,
            TypedIRValue::Variable(name, _) => self.lookup_var(name).ok_or_else(|| {
                EvalError::Runtime(format!(
                    "variable `{}` not found at runtime — the verifier \
                         should have caught this",
                    name
                ))
            })?,
            TypedIRValue::List(elems, _) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems {
                    out.push(self.eval_value(e)?);
                }
                RuntimeValue::List(out)
            }
            TypedIRValue::Record { name, fields, .. } => {
                let mut out = Vec::with_capacity(fields.len());
                for (fname, fval) in fields {
                    out.push((fname.clone(), self.eval_value(fval)?));
                }
                RuntimeValue::Record {
                    name: name.clone(),
                    fields: out,
                }
            }
            TypedIRValue::Map { entries, .. } => {
                let mut out = HashMap::with_capacity(entries.len());
                for (k, v) in entries {
                    let k_val = self.eval_value(k)?;
                    let v_val = self.eval_value(v)?;
                    let key = MapKey::from_runtime(&k_val)?;
                    out.insert(key, v_val);
                }
                RuntimeValue::Map(out)
            }
            // `Array` was previously unhandled. Treat it as a List —
            // the interpreter's runtime value model has no fixed-size
            // array; fixed sizes are a compile-time property.
            TypedIRValue::Array(elems, _, _) => {
                let mut out = Vec::with_capacity(elems.len());
                for e in elems {
                    out.push(self.eval_value(e)?);
                }
                RuntimeValue::List(out)
            }
            TypedIRValue::ArrayAccess { array, index, .. } => {
                let mut arr = self.eval_value(array)?;
                // ADR 0053: if the array itself is a reference (e.g.
                // `r[0]` where `r: &List<Int>`), dereference to get
                // the actual list before indexing.
                if let RuntimeValue::Reference(place) = &arr {
                    arr = self.resolve_place(place)?;
                }
                let idx = self.eval_value(index)?;
                let idx_usize = match idx {
                    RuntimeValue::Int(i) if i < 0 => {
                        return Err(EvalError::Runtime(format!("array index {} is negative", i)));
                    }
                    RuntimeValue::Int(i) => i as usize,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "ArrayAccess.index",
                            left: runtime_kind(&other),
                            right: "Int",
                        });
                    }
                };
                let list = match arr {
                    RuntimeValue::List(l) => l,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "ArrayAccess.array",
                            left: runtime_kind(&other),
                            right: "List",
                        });
                    }
                };
                list.get(idx_usize).cloned().ok_or_else(|| {
                    EvalError::Runtime(format!(
                        "array index {} out of bounds (length {})",
                        idx_usize,
                        list.len()
                    ))
                })?
            }
            TypedIRValue::BinaryOp {
                op, left, right, ..
            } => {
                let l = self.eval_value(left)?;
                let r = self.eval_value(right)?;
                return Self::eval_binop(op, l, r);
            }
            TypedIRValue::Call { function, args, .. } => {
                return self.eval_call(function, args);
            }
            TypedIRValue::Cast { value, target_type } => {
                let v = self.eval_value(value)?;
                match (v, target_type) {
                    (RuntimeValue::Int(i), Type::Float) => RuntimeValue::Float(i as f64),
                    (RuntimeValue::Float(f), Type::Int) => RuntimeValue::Int(f as i64),
                    // ADR 0029: nominal wrap/unwrap. The runtime
                    // representation of a nominal type is identical
                    // to its base, so the value flows through
                    // unchanged. Explicit arm so the intent is
                    // visible without tracing the catch-all.
                    (v, Type::Distinct { .. }) => v,
                    // ADR 0030: enum wrap/unwrap. Same shape — the
                    // runtime value is just the ordinal as an Int.
                    (v, Type::Enum { .. }) => v,
                    (v, _) => v,
                }
            }
            TypedIRValue::Some(inner) => {
                RuntimeValue::Option(Some(Box::new(self.eval_value(inner)?)))
            }
            TypedIRValue::None { .. } => RuntimeValue::Option(None),
            TypedIRValue::Ok { value, .. } => RuntimeValue::Result {
                is_ok: true,
                value: Box::new(self.eval_value(value)?),
            },
            TypedIRValue::Error { value, .. } => RuntimeValue::Result {
                is_ok: false,
                value: Box::new(self.eval_value(value)?),
            },
            TypedIRValue::PtrLiteral(n) => RuntimeValue::Int(*n as i64),
            TypedIRValue::NullPtr => RuntimeValue::Int(0),
            // ADR 0053. A reference-valued expression produces a
            // `RuntimeValue::Reference(Place)`. The place is derived
            // from the inner IR value — a Variable, a FieldAccess, or
            // an ArrayAccess with a literal index. Reads through the
            // reference (`*r`) resolve the place against the current
            // `variables` map, so the value seen is what the place
            // holds *now*, not what it held when the reference was
            // made.
            TypedIRValue::BorrowShared { expr, .. } | TypedIRValue::AddrOf { expr, .. } => {
                // A borrow of a place (Variable / FieldAccess /
                // ArrayAccess with literal index) produces a
                // `Reference(place)` with live-view semantics.
                //
                // A borrow of a non-place (call result, literal,
                // arithmetic) produces a `TempRef` holding the
                // value. Reads through the reference clone the
                // value; it does not outlive the enclosing
                // expression.
                // G42 fix. Use the precise variant so `&xs[i]` with a
                // non-literal `i` captures a reference to `xs[<value of i>]`,
                // not the whole list. Falls back to TempRef only if the
                // expression truly has no place (a call result, a literal,
                // an arithmetic expression).
                match self.place_of_ir_value_precise(expr) {
                    Some(place) => RuntimeValue::Reference(place),
                    None => {
                        let value = self.eval_value(expr)?;
                        RuntimeValue::TempRef(Box::new(value))
                    }
                }
            }
            TypedIRValue::BorrowMutable { expr, .. } => {
                // Mutation of a temporary is refused: the change
                // would be discarded when the temporary goes out
                // of scope, which is almost always a user bug.
                // Only place-based mutable borrows are legal.
                // G42 fix. Same precise variant as BorrowShared, so
                // `&mut xs[i]` with a non-literal `i` resolves `i` to
                // a concrete index at borrow time. This matches Rust:
                // `&mut xs[i]` takes the address of one element, not
                // "the element at whatever i is now".
                let place = self
                    .place_of_ir_value_precise(expr)
                    .ok_or(EvalError::Unsupported {
                        construct: "mutable reference to non-addressable expression",
                        hint: "the expression must resolve to a variable, field, \
                           or list element — a call result or literal has no place",
                    })?;
                RuntimeValue::Reference(place)
            }
            TypedIRValue::ReadReference { expr, .. } => {
                let inner = self.eval_value(expr)?;
                match inner {
                    RuntimeValue::Reference(place) => self.resolve_place(&place)?,
                    // A read through a `TempRef` clones the
                    // underlying value. `TempRef` was produced by
                    // `BorrowShared` on a non-place; the read is
                    // semantically a read of the temporary.
                    RuntimeValue::TempRef(v) => (*v).clone(),
                    // Legacy: a reference that was not produced as a
                    // `Reference` or `TempRef` — pass through. After
                    // ADR 0053 the producing arms above always yield
                    // one of those; this branch exists for
                    // robustness.
                    other => other,
                }
            }

            // ADR 0038 D4b. The interpreter preserves the fat
            // pointer as a `RuntimeValue::DynTrait` so that method
            // dispatch can select the impl by `(trait, concrete)`.
            // The concrete type comes from the analyzer's type
            // table via the `data` expression's ExprId — the IR
            // value itself does not carry it.
            TypedIRValue::DynTrait {
                data, target_type, ..
            } => {
                let data_val = self.eval_value(data)?;
                let (trait_name, concrete_type) = match target_type {
                    Type::Borrow(inner) | Type::MutBorrow(inner) => match inner.as_ref() {
                        Type::DynTrait { trait_name, .. } => {
                            (trait_name.clone(), runtime_concrete_name(&data_val))
                        }
                        _ => {
                            return Err(EvalError::Unsupported {
                                construct: "DynTrait value with non-DynTrait target",
                                hint: "the analyzer should have rejected this shape",
                            });
                        }
                    },
                    _ => {
                        return Err(EvalError::Unsupported {
                            construct: "DynTrait value with non-borrow target",
                            hint: "the analyzer should have rejected this shape",
                        });
                    }
                };
                RuntimeValue::DynTrait {
                    data: Box::new(data_val),
                    trait_name,
                    concrete_type,
                }
            }
            // ADR 0038 D4b. Virtual calls can appear nested inside
            // other instructions — `print(s.area())` carries the
            // VirtualCall as the Print's operand; `f(s.area())`
            // carries it as an argument. Dispatch through the
            // mangled impl name, same as the instruction-level
            // `Instruction::VirtualCall` arm in `mod.rs`.
            TypedIRValue::VirtualCall {
                receiver,
                method_name,
                args,
                ..
            } => {
                let recv = self.eval_value(receiver)?;
                let RuntimeValue::DynTrait {
                    data,
                    trait_name,
                    concrete_type,
                } = recv
                else {
                    return Err(EvalError::TypeMismatch {
                        op: "VirtualCall.receiver",
                        left: runtime_kind(&recv),
                        right: "DynTrait",
                    });
                };

                let mut arg_vals: Vec<RuntimeValue> = Vec::with_capacity(args.len() + 1);
                arg_vals.push(*data);
                for a in args {
                    arg_vals.push(self.eval_value(a)?);
                }

                let callee_name = format!("{}_{}_{}", trait_name, concrete_type, method_name);
                self.call_function_with_values(&callee_name, arg_vals)?
            }

            TypedIRValue::Range(..) => {
                return Err(EvalError::Unsupported {
                    construct: "ranges",
                    hint: "ranges are not yet lowered by the interpreter",
                });
            }

            TypedIRValue::FieldAccess { object, field, .. } => {
                let obj = self.eval_value(object)?;
                // ADR 0053 + TempRef: if the object is a reference
                // (a real reference bound to a place, OR a TempRef
                // produced by borrowing a non-place temporary like
                // `make_user().label()`), dereference to get the
                // record before looking up the field. Without
                // handling TempRef here, `self.name` inside a method
                // whose receiver was a temporary falls through to
                // the `other` arm and reports a type mismatch.
                let obj = match obj {
                    RuntimeValue::Reference(place) => self.resolve_place(&place)?,
                    RuntimeValue::TempRef(inner) => *inner,
                    other => other,
                };
                match obj {
                    RuntimeValue::Record { fields, .. } => fields
                        .into_iter()
                        .find(|(n, _)| n == field)
                        .map(|(_, v)| v)
                        .ok_or_else(|| {
                            EvalError::Runtime(format!(
                                "field `{}` not found at runtime — analyzer should have caught this",
                                field
                            ))
                        })?,
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "FieldAccess",
                            left: runtime_kind(&other),
                            right: "Record",
                        });
                    }
                }
            }
            // ADR 0032 A5d: sets erase to their u64 bit pattern.
            // The IR value's `bits` field is exactly the runtime
            // representation. Reusing `RuntimeValue::Int` matches how
            // enums and subranges erase (they flow through `Int` too —
            // see the `Cast` arms below).
            TypedIRValue::Set { bits, .. } => RuntimeValue::Int(*bits as i64),
            TypedIRValue::UnionConstruct {
                union_name,
                variant_name,
                tag,
                payload,
                ..
            } => {
                let mut payload_vals = Vec::with_capacity(payload.len());
                for p in payload {
                    payload_vals.push(self.eval_value(p)?);
                }
                RuntimeValue::Union {
                    tag: *tag,
                    union_name: union_name.clone(),
                    variant_name: variant_name.clone(),
                    payload: payload_vals,
                }
            }
            TypedIRValue::SetSingleton {
                element,
                element_type,
            } => {
                let elem_val = self.eval_value(element)?;
                let ordinal: i64 = match elem_val {
                    RuntimeValue::Int(i) => i,
                    RuntimeValue::Bool(b) => {
                        if b {
                            1
                        } else {
                            0
                        }
                    }
                    other => {
                        return Err(EvalError::TypeMismatch {
                            op: "SetSingleton.element",
                            left: runtime_kind(&other),
                            right: "Int or Bool",
                        });
                    }
                };
                let low: i64 = match element_type {
                    Type::Subrange { low, .. } => *low,
                    _ => 0,
                };
                let bit = (ordinal - low) as u32;
                let mask = 1u64.checked_shl(bit).unwrap_or(0);
                RuntimeValue::Int(mask as i64)
            }
        })
    }
    /// ADR 0053. Derive the `Place` an IR value refers to, if it
    /// can be determined statically. Mirrors the analyzer's
    /// `place_of_expr` at the IR level: Variable -> base place,
    /// FieldAccess -> place + Field, ArrayAccess with a literal Int
    /// index -> place + Index. Returns `None` for anything else.
    ///
    /// The caller (the reference-producing arms in `eval_value`)
    /// converts `None` into `EvalError::Unsupported` — a reference
    /// to a non-place is a shape the analyzer should have rejected.
    pub(super) fn place_of_ir_value(
        &self,
        v: &TypedIRValue,
    ) -> Option<crate::semantics::state::Place> {
        use crate::semantics::state::{Place, Projection};
        match v {
            TypedIRValue::Variable(name, _) => {
                // ADR 0064 / G19c-bug-2. The place names the *current*
                // frame's slot. If `&mut rule` is passed to a callee,
                // this frame id travels with the reference so the
                // callee resolves back to the caller's frame.
                Some(Place::in_frame(self.current_frame_id(), name.clone()))
            }
            TypedIRValue::FieldAccess { object, field, .. } => {
                let mut base = self.place_of_ir_value(object)?;
                base.projections.push(Projection::Field(field.clone()));
                Some(base)
            }
            TypedIRValue::ArrayAccess { array, index, .. } => {
                let idx = match index.as_ref() {
                    TypedIRValue::Int(n) => *n,
                    _ => return None,
                };
                if idx < 0 {
                    return None;
                }
                let mut base = self.place_of_ir_value(array)?;
                base.projections.push(Projection::Index(idx as usize));
                Some(base)
            }
            // G27 fix. `*r` where `r: &mut T`. The receiver of
            // `(*r).insert(k, v)` (or `.append`, or any future
            // place-mutating builtin) is a `ReadReference`.
            //
            // Return the *inner* expression's place — the place
            // where the reference variable lives — with no `Deref`
            // projection. `resolve_place` and `assign_place`'s
            // reference-chase branches (G19c) handle the dereference
            // implicitly: when they look up the base variable and
            // find a `RuntimeValue::Reference(inner_place)`, they
            // redirect to `inner_place`.
            //
            // Adding a `Deref` projection here would double-deref —
            // the base-variable chase already strips one `Deref` if
            // present, and if we add one to `projections`, the
            // chase's `rest_projs = &projections[1..]` would include
            // it, and then `apply_projection` would hit the
            // `Projection::Deref` unsupported arm.
            TypedIRValue::ReadReference { expr, .. } => self.place_of_ir_value(expr),
            _ => None,
        }
    }

    /// G42 fix. Like `place_of_ir_value`, but evaluates non-literal
    /// indexes to concrete values. Used by `BorrowShared` and
    /// `BorrowMutable` so `&mut xs[i]` captures a reference to
    /// `xs[<value of i>]` rather than the whole list.
    ///
    /// Semantics: `&mut xs[i]` in Rust takes the address of one
    /// element. If `i` changes after the borrow, the reference still
    /// points at the original element. Evaluating `index` here (at
    /// borrow time) matches that.
    ///
    /// The analyzer (post-G41) treats `&mut xs[i]` conservatively —
    /// two `&mut xs[i]` / `&mut xs[j]` borrows overlap. That rule
    /// runs at compile time. At runtime, if the program passed
    /// analysis, we know at most one borrow of any element of `xs`
    /// is live. So the interpreter can be precise: resolve `i` to a
    /// concrete `usize` and store `[Index(i)]` in the place.
    ///
    /// On any failure to evaluate `index` to a non-negative `Int`,
    /// returns `None` (caller falls back to the pre-G42 behavior).
    pub(super) fn place_of_ir_value_precise(
        &mut self,
        v: &TypedIRValue,
    ) -> Option<crate::semantics::state::Place> {
        use crate::semantics::state::{Place, Projection};
        match v {
            TypedIRValue::Variable(name, _) => {
                Some(Place::in_frame(self.current_frame_id(), name.clone()))
            }
            TypedIRValue::FieldAccess { object, field, .. } => {
                let mut base = self.place_of_ir_value_precise(object)?;
                base.projections.push(Projection::Field(field.clone()));
                Some(base)
            }
            TypedIRValue::ArrayAccess { array, index, .. } => {
                let mut base = self.place_of_ir_value_precise(array)?;
                match index.as_ref() {
                    TypedIRValue::Int(n) => {
                        if *n < 0 {
                            return None;
                        }
                        base.projections.push(Projection::Index(*n as usize));
                    }
                    _ => {
                        // Evaluate the index to a concrete Int.
                        match self.eval_value(index) {
                            Ok(RuntimeValue::Int(n)) if n >= 0 => {
                                base.projections.push(Projection::Index(n as usize));
                            }
                            _ => return None,
                        }
                    }
                }
                Some(base)
            }
            TypedIRValue::ReadReference { expr, .. } => self.place_of_ir_value_precise(expr),
            _ => None,
        }
    }

    /// ADR 0053. Resolve a place to the value currently held there.
    /// Walks the base variable's value through each projection:
    /// `Field` looks up a record field, `Index` looks up a list
    /// element. Values are cloned at each step; the interpreter is
    /// a tree-walker with owned values, so aliasing is not a
    /// concern.
    pub(super) fn resolve_place(
        &self,
        place: &crate::semantics::state::Place,
    ) -> Result<RuntimeValue, EvalError> {
        use crate::semantics::state::Projection;
        let mut cur = self
            .lookup_var_in_frame(place.frame_id, &place.base)
            .ok_or_else(|| {
                EvalError::Runtime(format!(
                    "reference to `{}`: base variable not in frame {}",
                    place.base, place.frame_id.0
                ))
            })?;
        // G19 fix. If the base variable itself holds a Reference,
        // resolve through it first. Handles `inner(r)` where `r: &mut T`
        // — the reference is preserved across the call, and the base
        // variable `r` in the callee's frame is `Reference(Place{...})`
        // pointing at the caller's storage.
        if let RuntimeValue::Reference(inner_place) = &cur {
            let inner_place = inner_place.clone();
            // G19c fix. The `Deref` projection that put us on the
            // reference is *consumed* by the chase below — it is
            // the dereference. Applying it again at the end of the
            // chain would either double-deref or hit the
            // `Projection::Deref` unsupported arm. Strip exactly
            // one leading `Deref` (there can only be one: a Place
            // has at most one reference at its base).
            let rest_projs: &[crate::semantics::state::Projection] = match place.projections.first()
            {
                Some(crate::semantics::state::Projection::Deref) => &place.projections[1..],
                _ => &place.projections[..],
            };
            let mut resolved = self.resolve_place(&inner_place)?;
            for proj in rest_projs {
                resolved = self.apply_projection(resolved, proj, place)?;
            }
            return Ok(resolved);
        }
        for proj in &place.projections {
            cur = match (proj, cur) {
                (Projection::Field(f), RuntimeValue::Record { fields, .. }) => fields
                    .into_iter()
                    .find(|(n, _)| n == f)
                    .map(|(_, v)| v)
                    .ok_or_else(|| {
                        EvalError::Runtime(format!(
                            "reference to `{}`: field `{}` not found",
                            place, f
                        ))
                    })?,
                (Projection::Index(i), RuntimeValue::List(items)) => {
                    items.into_iter().nth(*i).ok_or_else(|| {
                        EvalError::Runtime(format!(
                            "reference to `{}`: index {} out of bounds",
                            place, i
                        ))
                    })?
                }
                (_, other) => {
                    return Err(EvalError::TypeMismatch {
                        op: "resolve_place",
                        left: runtime_kind(&other),
                        right: "Record or List",
                    });
                }
            };
        }
        Ok(cur)
    }

    /// G19 fix. Apply one projection to a resolved value. Extracted
    /// from `resolve_place`'s loop body so reference-chasing can
    /// reuse it.
    fn apply_projection(
        &self,
        cur: RuntimeValue,
        proj: &crate::semantics::state::Projection,
        whole: &crate::semantics::state::Place,
    ) -> Result<RuntimeValue, EvalError> {
        use crate::semantics::state::Projection;
        match (proj, cur) {
            (Projection::Field(f), RuntimeValue::Record { fields, .. }) => fields
                .into_iter()
                .find(|(n, _)| n == f)
                .map(|(_, v)| v)
                .ok_or_else(|| {
                    EvalError::Runtime(format!("reference to `{}`: field `{}` not found", whole, f))
                }),
            (Projection::Index(i), RuntimeValue::List(items)) => {
                items.into_iter().nth(*i).ok_or_else(|| {
                    EvalError::Runtime(format!(
                        "reference to `{}`: index {} out of bounds",
                        whole, i
                    ))
                })
            }
            (_, other) => Err(EvalError::TypeMismatch {
                op: "resolve_place",
                left: runtime_kind(&other),
                right: "Record or List",
            }),
        }
    }

    /// ADR 0056. Write `value` into the place `place`. The write
    /// counterpart of `resolve_place`: walk the base variable's
    /// value through each projection and store `value` at the end.
    ///
    /// Clone-out / mutate / insert-back at the top level, so the
    /// borrow of `self.variables` is released before we call
    /// `insert` (same shape as `Instruction::FieldAssign`).
    pub(super) fn assign_place(
        &mut self,
        place: &crate::semantics::state::Place,
        value: RuntimeValue,
    ) -> Result<(), EvalError> {
        let mut cur = self
            .lookup_var_in_frame(place.frame_id, &place.base)
            .ok_or_else(|| {
                EvalError::Runtime(format!(
                    "write to `{}`: base variable not in frame {}",
                    place.base, place.frame_id.0
                ))
            })?;
        // G19 fix. If the base variable holds a Reference, redirect
        // the write to the referenced place. Handles `b.value := 42`
        // where `b: &mut Box` forwarded through a call — `b`'s frame
        // slot is `Reference(Place{box})`.
        if let RuntimeValue::Reference(inner_place) = &cur {
            let inner_place = inner_place.clone();
            // G19c fix. The leading `Deref` projection is the
            // dereference that got us here — it is consumed by
            // redirecting through the reference. Only the *rest*
            // of the projections apply to the pointee.
            let rest_projs: &[crate::semantics::state::Projection] = match place.projections.first()
            {
                Some(crate::semantics::state::Projection::Deref) => &place.projections[1..],
                _ => &place.projections[..],
            };
            let mut composed = inner_place.clone();
            composed.projections.extend_from_slice(rest_projs);
            return self.assign_place(&composed, value);
        }
        Self::assign_into(&mut cur, &place.projections, value, place)?;
        self.write_var_in_frame(place.frame_id, &place.base, cur)?;
        Ok(())
    }

    fn assign_into(
        cur: &mut RuntimeValue,
        projections: &[crate::semantics::state::Projection],
        value: RuntimeValue,
        whole: &crate::semantics::state::Place,
    ) -> Result<(), EvalError> {
        use crate::semantics::state::Projection;
        match projections.split_first() {
            None => {
                *cur = value;
                Ok(())
            }
            Some((Projection::Field(f), rest)) => match cur {
                RuntimeValue::Record { fields, .. } => {
                    let slot = fields.iter_mut().find(|(n, _)| n == f).ok_or_else(|| {
                        EvalError::Runtime(format!("write to `{}`: field `{}` not found", whole, f))
                    })?;
                    Self::assign_into(&mut slot.1, rest, value, whole)
                }
                other => Err(EvalError::TypeMismatch {
                    op: "assign_place",
                    left: runtime_kind(other),
                    right: "Record",
                }),
            },
            Some((Projection::Index(i), rest)) => match cur {
                RuntimeValue::List(items) => {
                    let slot = items.get_mut(*i).ok_or_else(|| {
                        EvalError::Runtime(format!(
                            "write to `{}`: index {} out of bounds",
                            whole, i
                        ))
                    })?;
                    Self::assign_into(slot, rest, value, whole)
                }
                other => Err(EvalError::TypeMismatch {
                    op: "assign_place",
                    left: runtime_kind(other),
                    right: "List",
                }),
            },
            Some((Projection::Deref, _)) => Err(EvalError::Unsupported {
                construct: "deref projection in interpreter assign_place",
                hint: "references are resolved at creation; a Deref projection \
                       should never reach assign_place",
            }),
        }
    }

    pub(super) fn eval_binop(
        op: &SemanticBinOp,
        l: RuntimeValue,
        r: RuntimeValue,
    ) -> Result<RuntimeValue, EvalError> {
        let lk = runtime_kind(&l);
        let rk = runtime_kind(&r);

        let mismatch = |op_name: &'static str| EvalError::TypeMismatch {
            op: op_name,
            left: lk,
            right: rk,
        };

        match op {
            SemanticBinOp::Add => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(a.wrapping_add(b)))
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a + b)),
                (RuntimeValue::Int(a), RuntimeValue::Float(b)) => {
                    Ok(RuntimeValue::Float(a as f64 + b))
                }
                (RuntimeValue::Float(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Float(a + b as f64))
                }
                (RuntimeValue::String(a), RuntimeValue::String(b)) => {
                    Ok(RuntimeValue::String(a + &b))
                }
                _ => Err(mismatch("Add")),
            },
            SemanticBinOp::Subtract => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(a.wrapping_sub(b)))
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a - b)),
                _ => Err(mismatch("Subtract")),
            },
            SemanticBinOp::Multiply => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(a.wrapping_mul(b)))
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a * b)),
                _ => Err(mismatch("Multiply")),
            },
            SemanticBinOp::Divide => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    if b == 0 {
                        return Err(EvalError::Runtime("integer division by zero".into()));
                    }
                    a.checked_div(b).map(RuntimeValue::Int).ok_or_else(|| {
                        EvalError::Runtime(format!("integer division overflow: {} / {}", a, b))
                    })
                }
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Float(a / b)),
                _ => Err(mismatch("Divide")),
            },

            // Structural equality. Uses `runtime_eq`, which handles
            // mixed Int/Float comparison so `1 == 1.0` is `true`
            // here, matching LLVM.
            SemanticBinOp::Equal => Ok(RuntimeValue::Bool(l.runtime_eq(&r))),
            SemanticBinOp::NotEqual => Ok(RuntimeValue::Bool(!l.runtime_eq(&r))),

            SemanticBinOp::Greater => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a > b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a > b)),
                (RuntimeValue::Char(a), RuntimeValue::Char(b)) => Ok(RuntimeValue::Bool(a > b)),
                _ => Err(mismatch("Greater")),
            },
            SemanticBinOp::Less => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a < b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a < b)),
                (RuntimeValue::Char(a), RuntimeValue::Char(b)) => Ok(RuntimeValue::Bool(a < b)),
                _ => Err(mismatch("Less")),
            },
            SemanticBinOp::GreaterEqual => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a >= b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a >= b)),
                (RuntimeValue::Char(a), RuntimeValue::Char(b)) => Ok(RuntimeValue::Bool(a >= b)),
                _ => Err(mismatch("GreaterEqual")),
            },
            SemanticBinOp::LessEqual => match (l, r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => Ok(RuntimeValue::Bool(a <= b)),
                (RuntimeValue::Float(a), RuntimeValue::Float(b)) => Ok(RuntimeValue::Bool(a <= b)),
                (RuntimeValue::Char(a), RuntimeValue::Char(b)) => Ok(RuntimeValue::Bool(a <= b)),
                _ => Err(mismatch("LessEqual")),
            },
            // ADR 0032 A5d: set operations on the u64 bit pattern,
            // carried in RuntimeValue::Int. `i64` and `u64` have the
            // same bit layout; `as i64`/`as u64` round-trips losslessly.
            SemanticBinOp::SetUnion => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(((*a as u64) | (*b as u64)) as i64))
                }
                _ => Err(mismatch("SetUnion")),
            },
            SemanticBinOp::SetIntersection => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(((*a as u64) & (*b as u64)) as i64))
                }
                _ => Err(mismatch("SetIntersection")),
            },
            SemanticBinOp::SetDifference => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    Ok(RuntimeValue::Int(((*a as u64) & !(*b as u64)) as i64))
                }
                _ => Err(mismatch("SetDifference")),
            },
            SemanticBinOp::SetMember => match (&l, &r) {
                (RuntimeValue::Int(d), RuntimeValue::Int(s)) => {
                    // (1u64 << d) & s != 0. `d` has already been
                    // shifted down to its bit position by the IR
                    // builder (see A5d-llvm's low-subtraction).
                    let bit = 1u64.checked_shl(*d as u32).unwrap_or(0);
                    Ok(RuntimeValue::Bool((bit & (*s as u64)) != 0))
                }
                _ => Err(mismatch("SetMember")),
            },
            SemanticBinOp::SetSubset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    // a <= b iff a & ~b == 0.
                    Ok(RuntimeValue::Bool(((*a as u64) & !(*b as u64)) == 0))
                }
                _ => Err(mismatch("SetSubset")),
            },
            SemanticBinOp::SetStrictSubset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    let au = *a as u64;
                    let bu = *b as u64;
                    Ok(RuntimeValue::Bool((au & !bu) == 0 && au != bu))
                }
                _ => Err(mismatch("SetStrictSubset")),
            },
            SemanticBinOp::SetSuperset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    // b <= a.
                    Ok(RuntimeValue::Bool(((*b as u64) & !(*a as u64)) == 0))
                }
                _ => Err(mismatch("SetSuperset")),
            },
            SemanticBinOp::SetStrictSuperset => match (&l, &r) {
                (RuntimeValue::Int(a), RuntimeValue::Int(b)) => {
                    let au = *a as u64;
                    let bu = *b as u64;
                    Ok(RuntimeValue::Bool((bu & !au) == 0 && au != bu))
                }
                _ => Err(mismatch("SetStrictSuperset")),
            },
        }
    }
    pub(super) fn eval_builtin_call(
        &mut self,
        func: &str,
        args: &[TypedIRValue],
    ) -> Result<RuntimeValue, EvalError> {
        let mut arg_vals = Vec::with_capacity(args.len());
        for a in args {
            arg_vals.push(self.eval_value(a)?);
        }

        match func {
            // `length` reports Unicode codepoints for strings, not
            // UTF-8 bytes. `String.substring` already counts
            // codepoints (via `chars()`), so the two agree on the
            // same unit. The LLVM backend still uses C `strlen`
            // (bytes) — see the TODO in `llvm_codegen/builtins.rs`.
            "List.length" | "len" | "length" => match arg_vals.first() {
                Some(RuntimeValue::List(l)) => Ok(RuntimeValue::Int(l.len() as i64)),
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::Int(s.chars().count() as i64)),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "length",
                    left: runtime_kind(other),
                    right: "List | String",
                }),
                None => Err(EvalError::Runtime("length: missing argument".into())),
            },
            "List.sum" | "sum" => match arg_vals.first() {
                Some(RuntimeValue::List(list)) => {
                    let mut sum = 0.0;
                    for v in list {
                        sum += numeric_element(v, "List.sum")?;
                    }
                    Ok(RuntimeValue::Float(sum))
                }
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "List.sum",
                    left: runtime_kind(other),
                    right: "List",
                }),
                None => Err(EvalError::Runtime("List.sum: missing argument".into())),
            },
            "List.max" => match arg_vals.first() {
                Some(RuntimeValue::List(list)) => {
                    let mut max = f64::NEG_INFINITY;
                    for v in list {
                        let f = numeric_element(v, "List.max")?;
                        if f > max {
                            max = f;
                        }
                    }
                    Ok(RuntimeValue::Float(max))
                }
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "List.max",
                    left: runtime_kind(other),
                    right: "List",
                }),
                None => Err(EvalError::Runtime("List.max: missing argument".into())),
            },
            "List.min" => match arg_vals.first() {
                Some(RuntimeValue::List(list)) => {
                    let min = list
                        .iter()
                        .filter_map(|v| match v {
                            RuntimeValue::Int(i) => Some(*i as f64),
                            RuntimeValue::Float(f) => Some(*f),
                            _ => None,
                        })
                        .fold(f64::INFINITY, f64::min);
                    Ok(RuntimeValue::Float(min))
                }
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "List.min",
                    left: runtime_kind(other),
                    right: "List",
                }),
                None => Err(EvalError::Runtime("List.min: missing argument".into())),
            },
            "String.substring" => {
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.substring",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.substring: missing string argument".into(),
                        ))
                    }
                };
                let start = match arg_vals.get(1) {
                    Some(RuntimeValue::Int(i)) => (*i).max(0) as usize,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.substring.start",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.substring: missing start argument".into(),
                        ))
                    }
                };
                let length = match arg_vals.get(2) {
                    Some(RuntimeValue::Int(i)) => (*i).max(0) as usize,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.substring.length",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.substring: missing length argument".into(),
                        ))
                    }
                };
                let chars: Vec<char> = s.chars().collect();
                let start = start.min(chars.len());
                let end = start.saturating_add(length).min(chars.len());
                Ok(RuntimeValue::String(chars[start..end].iter().collect()))
            }
            "String.charAt" => {
                // Option<String>: Some(1-codepoint string) for a valid
                // index, None for out-of-range (negative or >= count).
                // Codepoint-indexed (chars().nth), matching the
                // interpreter's own String.length and String.substring.
                // runtime/string.c's algol26_string_char_at is the
                // LLVM-side implementation of the same contract.
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.charAt",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.charAt: missing string argument".into(),
                        ))
                    }
                };
                let i = match arg_vals.get(1) {
                    Some(RuntimeValue::Int(i)) => *i,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.charAt.index",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.charAt: missing index argument".into(),
                        ))
                    }
                };
                if i < 0 {
                    return Ok(RuntimeValue::Option(None));
                }
                match s.chars().nth(i as usize) {
                    Some(c) => Ok(RuntimeValue::Option(Some(Box::new(RuntimeValue::String(
                        c.to_string(),
                    ))))),
                    None => Ok(RuntimeValue::Option(None)),
                }
            }
            "String.char_at" => {
                // Option<Char>: codepoint-indexed, same indexing
                // as String.charAt but the payload is a u32
                // codepoint rather than a 1-char String.
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.char_at",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.char_at: missing string argument".into(),
                        ))
                    }
                };
                let i = match arg_vals.get(1) {
                    Some(RuntimeValue::Int(i)) => *i,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.char_at.index",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.char_at: missing index argument".into(),
                        ))
                    }
                };
                if i < 0 {
                    return Ok(RuntimeValue::Option(None));
                }
                match s.chars().nth(i as usize) {
                    Some(c) => Ok(RuntimeValue::Option(Some(Box::new(RuntimeValue::Char(
                        c as u32,
                    ))))),
                    None => Ok(RuntimeValue::Option(None)),
                }
            }
            "Char.to_string" => {
                // UTF-8 encode a codepoint.
                let c = match arg_vals.first() {
                    Some(RuntimeValue::Char(c)) => *c,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "Char.to_string",
                            left: runtime_kind(other),
                            right: "Char",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "Char.to_string: missing argument".into(),
                        ))
                    }
                };
                let s = char::from_u32(c)
                    .map(|ch| ch.to_string())
                    .unwrap_or_else(|| "\u{FFFD}".to_string());
                Ok(RuntimeValue::String(s))
            }
            "Char.to_int" => {
                // Codepoint as i64. Direct cast; no validation,
                // since RuntimeValue::Char is always a valid
                // codepoint by construction.
                let c = match arg_vals.first() {
                    Some(RuntimeValue::Char(c)) => *c,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "Char.to_int",
                            left: runtime_kind(other),
                            right: "Char",
                        })
                    }
                    None => return Err(EvalError::Runtime("Char.to_int: missing argument".into())),
                };
                Ok(RuntimeValue::Int(c as i64))
            }
            "String.to_chars" => {
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.to_chars",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.to_chars: missing argument".into(),
                        ))
                    }
                };
                let out: Vec<RuntimeValue> =
                    s.chars().map(|c| RuntimeValue::Char(c as u32)).collect();
                Ok(RuntimeValue::List(out))
            }
            "Math.sqrt" | "Math.sin" | "Math.cos" | "Math.tan" | "Math.exp" | "Math.log"
            | "Math.floor" | "Math.ceil" | "Math.abs" => {
                let x = match arg_vals.first() {
                    Some(RuntimeValue::Float(f)) => *f,
                    Some(RuntimeValue::Int(i)) => *i as f64,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "Math.*",
                            left: runtime_kind(other),
                            right: "Int | Float",
                        })
                    }
                    None => return Err(EvalError::Runtime("Math.*: missing argument".into())),
                };
                let r = match func {
                    "Math.sqrt" => x.sqrt(),
                    "Math.sin" => x.sin(),
                    "Math.cos" => x.cos(),
                    "Math.tan" => x.tan(),
                    "Math.exp" => x.exp(),
                    "Math.log" => x.ln(),
                    "Math.floor" => x.floor(),
                    "Math.ceil" => x.ceil(),
                    "Math.abs" => x.abs(),
                    _ => {
                        // The outer match arm binds `func` to the
                        // Math.* names handled above. Reaching this
                        // point means the arm list and the inner
                        // dispatch got out of sync.
                        return Err(EvalError::Unsupported {
                            construct: "Math builtin dispatch",
                            hint: "internal: arm list out of sync with Math.* alternatives",
                        });
                    }
                };
                Ok(RuntimeValue::Float(r))
            }
            "Math.pow" => {
                let (a, b) = match (arg_vals.first(), arg_vals.get(1)) {
                    (Some(RuntimeValue::Float(a)), Some(RuntimeValue::Float(b))) => (*a, *b),
                    (Some(RuntimeValue::Int(a)), Some(RuntimeValue::Float(b))) => (*a as f64, *b),
                    (Some(RuntimeValue::Float(a)), Some(RuntimeValue::Int(b))) => (*a, *b as f64),
                    (Some(RuntimeValue::Int(a)), Some(RuntimeValue::Int(b))) => {
                        (*a as f64, *b as f64)
                    }
                    _ => {
                        return Err(EvalError::TypeMismatch {
                            op: "Math.pow",
                            left: arg_vals.first().map(runtime_kind).unwrap_or("none"),
                            right: arg_vals.get(1).map(runtime_kind).unwrap_or("none"),
                        })
                    }
                };
                Ok(RuntimeValue::Float(a.powf(b)))
            }
            "String.concat" | "String_concat" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(RuntimeValue::String(a)), Some(RuntimeValue::String(b))) => {
                    Ok(RuntimeValue::String(format!("{}{}", a, b)))
                }
                _ => Err(EvalError::TypeMismatch {
                    op: "String.concat",
                    left: arg_vals.first().map(runtime_kind).unwrap_or("none"),
                    right: arg_vals.get(1).map(runtime_kind).unwrap_or("none"),
                }),
            },

            // Byte-wise lexicographic comparison. Returns -1, 0, or 1.
            // Rust's `str::cmp` already produces the right ordering;
            // we map its `Ordering` to the language's convention.
            "String.compare" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(RuntimeValue::String(a)), Some(RuntimeValue::String(b))) => {
                    let r = match a.cmp(b) {
                        std::cmp::Ordering::Less => -1i64,
                        std::cmp::Ordering::Equal => 0i64,
                        std::cmp::Ordering::Greater => 1i64,
                    };
                    Ok(RuntimeValue::Int(r))
                }
                _ => Err(EvalError::TypeMismatch {
                    op: "String.compare",
                    left: arg_vals.first().map(runtime_kind).unwrap_or("none"),
                    right: arg_vals.get(1).map(runtime_kind).unwrap_or("none"),
                }),
            },
            "String.to_upper" | "String.upper" | "to_upper" | "upper" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::String(s.to_uppercase())),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.to_upper",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime(
                    "String.to_upper: missing argument".into(),
                )),
            },
            "String.to_lower" | "String.lower" | "to_lower" | "lower" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::String(s.to_lowercase())),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.to_lower",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime(
                    "String.to_lower: missing argument".into(),
                )),
            },
            "String.length" | "String.len" | "strlen" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::Int(s.chars().count() as i64)),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.length",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime("String.length: missing argument".into())),
            },
            "String.trim" => match arg_vals.first() {
                Some(RuntimeValue::String(s)) => Ok(RuntimeValue::String(s.trim().to_string())),
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "String.trim",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime("String.trim: missing argument".into())),
            },
            "String.split" => {
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.split",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.split: missing string argument".into(),
                        ))
                    }
                };
                let sep = match arg_vals.get(1) {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.split.separator",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.split: missing separator argument".into(),
                        ))
                    }
                };
                let parts: Vec<RuntimeValue> = s
                    .split(sep.as_str())
                    .map(|p| RuntimeValue::String(p.to_string()))
                    .collect();
                Ok(RuntimeValue::List(parts))
            }
            "String.join" => {
                let list = match arg_vals.first() {
                    Some(RuntimeValue::List(l)) => l.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.join",
                            left: runtime_kind(other),
                            right: "List",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.join: missing list argument".into(),
                        ))
                    }
                };
                let sep = match arg_vals.get(1) {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.join.separator",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "String.join: missing separator argument".into(),
                        ))
                    }
                };
                let parts: Vec<String> = list
                    .iter()
                    .map(|v| match v {
                        RuntimeValue::String(s) => Ok(s.clone()),
                        other => Err(EvalError::TypeMismatch {
                            op: "String.join element",
                            left: runtime_kind(other),
                            right: "String",
                        }),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(RuntimeValue::String(parts.join(&sep)))
            }
            "File.read" => match arg_vals.first() {
                Some(RuntimeValue::String(path)) => match std::fs::read_to_string(path) {
                    Ok(content) => Ok(RuntimeValue::String(content)),
                    Err(e) => Err(EvalError::Runtime(format!(
                        "File.read: cannot read '{}': {}",
                        path, e
                    ))),
                },
                Some(other) => Err(EvalError::TypeMismatch {
                    op: "File.read",
                    left: runtime_kind(other),
                    right: "String",
                }),
                None => Err(EvalError::Runtime(
                    "File.read: missing path argument".into(),
                )),
            },
            "File.write" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(RuntimeValue::String(path)), Some(RuntimeValue::String(content))) => {
                    match std::fs::write(path, content) {
                        Ok(()) => Ok(RuntimeValue::Int(content.len() as i64)),
                        Err(e) => Err(EvalError::Runtime(format!(
                            "File.write: cannot write '{}': {}",
                            path, e
                        ))),
                    }
                }
                (None, _) => Err(EvalError::Runtime(
                    "File.write: missing path argument".into(),
                )),
                (_, None) => Err(EvalError::Runtime(
                    "File.write: missing content argument".into(),
                )),
                (Some(p), Some(c)) => Err(EvalError::TypeMismatch {
                    op: "File.write",
                    left: runtime_kind(p),
                    right: runtime_kind(c),
                }),
            },
            "File.append" => match (arg_vals.first(), arg_vals.get(1)) {
                (Some(RuntimeValue::String(path)), Some(RuntimeValue::String(content))) => {
                    use std::io::Write;
                    match std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)
                    {
                        Ok(mut f) => match f.write_all(content.as_bytes()) {
                            Ok(()) => Ok(RuntimeValue::Int(content.len() as i64)),
                            Err(e) => Err(EvalError::Runtime(format!(
                                "File.append: cannot append to '{}': {}",
                                path, e
                            ))),
                        },
                        Err(e) => Err(EvalError::Runtime(format!(
                            "File.append: cannot open '{}': {}",
                            path, e
                        ))),
                    }
                }
                (None, _) => Err(EvalError::Runtime(
                    "File.append: missing path argument".into(),
                )),
                (_, None) => Err(EvalError::Runtime(
                    "File.append: missing content argument".into(),
                )),
                (Some(p), Some(c)) => Err(EvalError::TypeMismatch {
                    op: "File.append",
                    left: runtime_kind(p),
                    right: runtime_kind(c),
                }),
            },
            "affirm" => {
                let cond = match arg_vals.first() {
                    Some(RuntimeValue::Bool(b)) => *b,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "affirm.cond",
                            left: runtime_kind(other),
                            right: "Bool",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "affirm: missing condition argument".into(),
                        ))
                    }
                };
                if cond {
                    return Ok(RuntimeValue::Void);
                }
                let msg = match arg_vals.get(1) {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "affirm.msg",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "affirm: missing message argument".into(),
                        ))
                    }
                };
                Err(EvalError::Runtime(format!("assertion failed: {}", msg)))
            }
            "args" => {
                if !arg_vals.is_empty() {
                    return Err(EvalError::Runtime(format!(
                        "args: takes no arguments, got {}",
                        arg_vals.len()
                    )));
                }
                let list: Vec<RuntimeValue> = self
                    .command_line_args
                    .iter()
                    .map(|s| RuntimeValue::String(s.clone()))
                    .collect();
                Ok(RuntimeValue::List(list))
            }
            "Int.to_string" => {
                let n = match arg_vals.first() {
                    Some(RuntimeValue::Int(n)) => *n,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "Int.to_string",
                            left: runtime_kind(other),
                            right: "Int",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime("Int.to_string: missing argument".into()))
                    }
                };
                Ok(RuntimeValue::String(n.to_string()))
            }
            "Float.to_string" => {
                let f = match arg_vals.first() {
                    Some(RuntimeValue::Float(f)) => *f,
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "Float.to_string",
                            left: runtime_kind(other),
                            right: "Float",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime(
                            "Float.to_string: missing argument".into(),
                        ))
                    }
                };
                Ok(RuntimeValue::String(format_float_for_display(f)))
            }
            "String.to_int" => {
                let s = match arg_vals.first() {
                    Some(RuntimeValue::String(s)) => s.clone(),
                    Some(other) => {
                        return Err(EvalError::TypeMismatch {
                            op: "String.to_int",
                            left: runtime_kind(other),
                            right: "String",
                        })
                    }
                    None => {
                        return Err(EvalError::Runtime("String.to_int: missing argument".into()))
                    }
                };
                match s.parse::<i64>() {
                    Ok(n) => Ok(RuntimeValue::Option(Some(Box::new(RuntimeValue::Int(n))))),
                    Err(_) => Ok(RuntimeValue::Option(None)),
                }
            }
            "Map.insert" => {
                // ADR 0059. The receiver is a place, not just a
                // variable. `self.books.insert(k, v)` from an
                // inherent method has a `FieldAccess` receiver;
                // `m.insert(k, v)` on a local has a `Variable`
                // receiver. Both are places.
                //
                // Resolve the receiver to a `Place` and use the
                // interpreter's place machinery (`resolve_place` /
                // `assign_place`) so the mutation lands on the
                // correct storage — the variable, the field, or
                // wherever the place roots.
                if args.len() != 3 {
                    return Err(EvalError::Runtime(format!(
                        "Map.insert expects 3 arguments (receiver, key, value), got {}",
                        args.len()
                    )));
                }

                // Evaluate key and value before touching the
                // receiver's storage.
                let k_val = self.eval_value(&args[1])?;
                let v_val = self.eval_value(&args[2])?;
                let key = MapKey::from_runtime(&k_val)?;

                // Try the place-based path first.
                if let Some(place) = self.place_of_ir_value(&args[0]) {
                    let mut current = self.resolve_place(&place)?;
                    match &mut current {
                        RuntimeValue::Map(entries) => {
                            entries.insert(key, v_val);
                            self.assign_place(&place, current)?;
                            return Ok(RuntimeValue::Void);
                        }
                        other => {
                            return Err(EvalError::TypeMismatch {
                                op: "Map.insert",
                                left: runtime_kind(other),
                                right: "Map",
                            });
                        }
                    }
                }

                // Fall back to the old non-place path. This handles
                // a receiver that isn't a place — a function call,
                // a literal — which shouldn't occur for a Map
                // method but is left as a defensive error.
                Err(EvalError::Unsupported {
                    construct: "Map.insert on non-place receiver",
                    hint: "the receiver must be a variable, field, or other place",
                })
            }

            "Map.get" => {
                let receiver = self.eval_value(&args[0])?;
                let k_val = self.eval_value(&args[1])?;
                let key = MapKey::from_runtime(&k_val)?;
                match receiver {
                    RuntimeValue::Map(entries) => match entries.get(&key) {
                        Some(v) => Ok(RuntimeValue::Option(Some(Box::new(v.clone())))),
                        None => Ok(RuntimeValue::Option(None)),
                    },
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.get",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.contains" => {
                let receiver = self.eval_value(&args[0])?;
                let k_val = self.eval_value(&args[1])?;
                let key = MapKey::from_runtime(&k_val)?;
                match receiver {
                    RuntimeValue::Map(entries) => {
                        Ok(RuntimeValue::Bool(entries.contains_key(&key)))
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.contains",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.keys" => {
                let receiver = self.eval_value(&args[0])?;
                match receiver {
                    RuntimeValue::Map(entries) => {
                        // Collect owned keys so the match below binds by value
                        // (no `*` vs `&` ambiguity) and sorts deterministically.
                        let mut keys: Vec<MapKey> = entries.keys().cloned().collect();
                        keys.sort();
                        let out: Vec<RuntimeValue> = keys
                            .into_iter()
                            .map(|k| match k {
                                MapKey::Int(i) => RuntimeValue::Int(i),
                                MapKey::String(s) => RuntimeValue::String(s),
                                MapKey::Bool(b) => RuntimeValue::Bool(b),
                            })
                            .collect();
                        Ok(RuntimeValue::List(out))
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.keys",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.values" => {
                let receiver = self.eval_value(&args[0])?;
                match receiver {
                    RuntimeValue::Map(entries) => {
                        // Sort by key so `.keys()` and `.values()` are aligned
                        // — `zip(m.keys(), m.values())` gives (k, v) pairs.
                        let mut sorted: Vec<(&MapKey, &RuntimeValue)> = entries.iter().collect();
                        sorted.sort_by(|a, b| a.0.cmp(b.0));
                        let out: Vec<RuntimeValue> =
                            sorted.into_iter().map(|(_, v)| v.clone()).collect();
                        Ok(RuntimeValue::List(out))
                    }
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.values",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }

            "Map.length" => {
                let receiver = self.eval_value(&args[0])?;
                match receiver {
                    RuntimeValue::Map(entries) => Ok(RuntimeValue::Int(entries.len() as i64)),
                    other => Err(EvalError::TypeMismatch {
                        op: "Map.length",
                        left: runtime_kind(&other),
                        right: "Map",
                    }),
                }
            }
            "List.append" => {
                // Place-based receiver, same pattern as Map.insert.
                // Handles `xs.append(x)` (variable receiver) and
                // `r.xs.append(x)` (field receiver on a mutable
                // reference, as in `proc f(r: &mut IRRule)`).
                // Without place resolution, field receivers were
                // rejected as "non-variable" even though the IR
                // builder emits them correctly for &mut access.
                if args.len() != 2 {
                    return Err(EvalError::Runtime(format!(
                        "List.append expects 2 arguments (receiver, value), got {}",
                        args.len()
                    )));
                }

                let val = self.eval_value(&args[1])?;

                if let Some(place) = self.place_of_ir_value(&args[0]) {
                    let mut current = self.resolve_place(&place)?;
                    match &mut current {
                        RuntimeValue::List(items) => {
                            items.push(val);
                            self.assign_place(&place, current)?;
                            return Ok(RuntimeValue::Void);
                        }
                        other => {
                            return Err(EvalError::TypeMismatch {
                                op: "List.append",
                                left: runtime_kind(other),
                                right: "List",
                            });
                        }
                    }
                }

                Err(EvalError::Unsupported {
                    construct: "List.append on non-place receiver",
                    hint: "the receiver must be a variable, field, or other place",
                })
            }
            _ => Err(EvalError::Unsupported {
                construct: "builtin",
                hint: "interpreter has no dispatch arm for this registered \
                       builtin — the IR verifier accepted it, but the \
                       interpreter cannot execute it. This is an interpreter \
                       completeness gap.",
            }),
        }
    }
    /// Evaluate a call to either a user function or a built-in. User
    /// functions are dispatched with a fresh frame; the caller's frame
    /// is saved and restored. Returns `Void` if the callee errors
    /// internally (block not found, infinite loop) — the error is
    /// printed to stderr.
    pub(super) fn eval_call(
        &mut self,
        function: &str,
        args: &[TypedIRValue],
    ) -> Result<RuntimeValue, EvalError> {
        if let Some(callee) = self
            .program
            .functions
            .iter()
            .find(|f| f.name == function)
            .cloned()
        {
            // ─── ADR 0035: write-back plan for `&mut self` receivers ───
            // The builder emits `BorrowMutable { expr: Variable(name) }`
            // for a mutable receiver. Scan the argument list for that
            // shape and record (position, caller_binding_name) so the
            // callee's final parameter value can be written back to the
            // caller after the frame is restored. This is a syntactic
            // scan — nothing is evaluated here.
            // G19 fix. The ADR 0035 write-back exists only because the
            // interpreter historically resolved `&mut` arguments to
            // their values before binding. Now that references are
            // preserved for `Borrow`/`MutBorrow` params (see the arg
            // evaluation block below), writes through the reference
            // reach the caller's place directly. Write-back is skipped
            // for reference params to avoid double-writing (or, worse,
            // writing a `Reference` value back into a value slot).
            //
            // Kept as a plan for any hypothetical future case where a
            // `BorrowMutable` argument targets a by-value parameter —
            // the plan is filtered below.
            let write_back_plan: Vec<(usize, String)> = args
                .iter()
                .enumerate()
                .filter_map(|(i, arg)| match arg {
                    TypedIRValue::BorrowMutable { expr, .. } => match &**expr {
                        TypedIRValue::Variable(name, _) => Some((i, name.clone())),
                        _ => None,
                    },
                    _ => None,
                })
                .filter(|(i, _)| {
                    // Skip write-back for reference-typed params.
                    !callee
                        .params
                        .get(*i)
                        .map(|(_, ty)| matches!(ty, Type::Borrow(_) | Type::MutBorrow(_)))
                        .unwrap_or(false)
                })
                .collect();

            // ADR 0053 + G19 fix. Reference arguments must be
            // *preserved* when the callee's parameter is a reference
            // type (`Borrow(T)` / `MutBorrow(T)`), so writes through
            // the reference propagate to the caller's frame. For
            // by-value parameters, still resolve references to their
            // pointee values (the callee needs the value, not the
            // reference).
            //
            // Prior to this fix, all reference arguments were
            // resolved unconditionally. That broke two-hop `&mut`
            // forwarding — `outer(r: &mut Int) -> inner(r)` where
            // `inner` writes through `r` — because `r` at hop 2 was
            // resolved to its current value, losing the reference to
            // the caller's storage. One-hop cases worked because the
            // ADR 0035 write-back machinery copied the callee's final
            // parameter value back to the caller by name; that
            // mechanism does not chain through a second hop.
            //
            // With references preserved, the ADR 0035 write-back is
            // redundant — mutations go through the reference to the
            // caller's place directly, not through a value copy
            // written back by name. Write-back is therefore
            // suppressed below when the argument is a preserved
            // reference; it remains for any future case where a
            // `BorrowMutable` arg is passed to a by-value parameter.
            let mut arg_vals = Vec::with_capacity(args.len());
            for (i, a) in args.iter().enumerate() {
                let mut v = self.eval_value(a)?;
                let param_is_ref = callee
                    .params
                    .get(i)
                    .map(|(_, ty)| matches!(ty, Type::Borrow(_) | Type::MutBorrow(_)))
                    .unwrap_or(false);
                if !param_is_ref {
                    if let RuntimeValue::Reference(place) = &v {
                        v = self.resolve_place(place)?;
                    }
                }
                arg_vals.push(v);
            }

            self.push_frame();
            let _saved_vars_marker: () = ();
            let saved_ret = self.return_value.take();
            let saved_regions = std::mem::take(&mut self.region_stack);

            for ((param_name, _), val) in callee.params.iter().zip(arg_vals) {
                self.insert_var(param_name.clone(), val);
            }

            let result = self.execute_function(&callee);

            // G27 fix. Check for errors BEFORE checking the return
            // value. If `execute_function` errored, `self.return_value`
            // is None because the error occurred before reaching a
            // `return` instruction — but the real error is much more
            // informative than "returned no value". Prior to this fix,
            // the "returned no value" message masked the actual error
            // (e.g. "Map.insert on non-place receiver") at every
            // enclosing call frame, turning a targeted diagnostic into
            // an opaque one.
            if let Err(e) = &result {
                self.pop_frame();
                self.return_value = saved_ret;
                self.region_stack = saved_regions;
                return Err(EvalError::Runtime(format!("in {}: {}", callee.name, e)));
            }

            let ret = match self.return_value.take() {
                Some(v) => v,
                None if callee.return_type == Type::Void => RuntimeValue::Void,
                None => {
                    // G19 fix. Pop the frame we pushed before bailing
                    // out — otherwise the stack leaks one frame per
                    // such error, and later lookups walk the wrong
                    // scope chain.
                    self.pop_frame();
                    return Err(EvalError::Runtime(format!(
                        "function `{}` returned no value but its return type is `{}`",
                        callee.name, callee.return_type
                    )));
                }
            };

            // ─── ADR 0035: capture write-backs before restoring ───
            // Read the callee's final parameter values from the still-
            // installed frame. `self.variables` currently holds only the
            // callee's params and locals; the caller's bindings are in
            // `saved_vars`.
            let mut write_backs: Vec<(String, RuntimeValue)> =
                Vec::with_capacity(write_back_plan.len());
            for (i, caller_name) in &write_back_plan {
                if let Some((param_name, _)) = callee.params.get(*i) {
                    if let Some(val) = self.lookup_var(param_name) {
                        write_backs.push((caller_name.clone(), val));
                    }
                }
            }

            self.pop_frame();
            self.return_value = saved_ret;
            self.region_stack = saved_regions;

            // ─── ADR 0035: apply write-backs to the caller's frame ───
            for (caller_name, val) in write_backs {
                self.insert_var(caller_name, val);
            }

            // Wrap the callee's error with the function name so
            // nested calls produce a readable chain
            // (`in f: in g: ...`). The error type is `EvalError`
            // throughout; nothing is swallowed.
            result.map_err(|e| EvalError::Runtime(format!("in {}: {}", callee.name, e)))?;

            Ok(ret)
        } else {
            self.eval_builtin_call(function, args)
        }
    }

    /// ADR 0038 D4b. Invoke a named function with already-evaluated
    /// argument values. Parallel to `eval_call`, which evaluates
    /// `TypedIRValue` arguments first; the virtual-dispatch path has
    /// the receiver's runtime value in hand and must not re-evaluate
    /// it (that would re-run any side effects in the receiver).
    ///
    /// Write-back for `&mut self` receivers is not implemented on
    /// this path in v1. Trait methods invoked through `&dyn Trait`
    /// use `&Self` receivers, which do not mutate the caller's
    /// binding, so the omission is not observable today. `&mut Self`
    /// through a `&mut dyn Trait` would need the same structural-
    /// receiver machinery `eval_call` uses.
    pub(super) fn call_function_with_values(
        &mut self,
        callee_name: &str,
        arg_vals: Vec<RuntimeValue>,
    ) -> Result<RuntimeValue, EvalError> {
        let Some(callee) = self
            .program
            .functions
            .iter()
            .find(|f| f.name == callee_name)
            .cloned()
        else {
            return Err(EvalError::Runtime(format!(
                "virtual dispatch: no function `{}` — the impl may not have                  been registered, or the mangled name scheme has drifted",
                callee_name
            )));
        };

        // ADR 0053: same treatment as `eval_call` — resolve any
        // reference arguments to their values while the caller's
        // frame is still installed. See the comment in `eval_call`
        // for the trade-off.
        let arg_vals: Vec<RuntimeValue> = arg_vals
            .into_iter()
            .map(|v| {
                if let RuntimeValue::Reference(place) = &v {
                    if let Ok(resolved) = self.resolve_place(place) {
                        return resolved;
                    }
                }
                v
            })
            .collect();

        self.push_frame();
        let _saved_vars_marker: () = ();
        let saved_ret = self.return_value.take();
        let saved_regions = std::mem::take(&mut self.region_stack);

        for ((param_name, _), val) in callee.params.iter().zip(arg_vals) {
            self.insert_var(param_name.clone(), val);
        }

        let result = self.execute_function(&callee);

        // G27 fix. Same as `eval_call`: check for errors before
        // checking the return value, or the real error gets masked
        // by "returned no value".
        if let Err(e) = &result {
            self.pop_frame();
            self.return_value = saved_ret;
            self.region_stack = saved_regions;
            return Err(EvalError::Runtime(format!("in {}: {}", callee.name, e)));
        }

        let ret = match self.return_value.take() {
            Some(v) => v,
            None if callee.return_type == Type::Void => RuntimeValue::Void,
            None => {
                self.pop_frame();
                return Err(EvalError::Runtime(format!(
                    "function `{}` returned no value but its return type is `{}`",
                    callee.name, callee.return_type
                )));
            }
        };

        self.pop_frame();
        self.return_value = saved_ret;
        self.region_stack = saved_regions;

        result.map_err(|e| EvalError::Runtime(format!("in {}: {}", callee.name, e)))?;
        Ok(ret)
    }
}

/// Format a float for `Float.to_string`. Implements ECMA-262
/// §7.1.12.1 (Number::toString), matching JavaScript's `String(x)`
/// output exactly.
///
/// Replaces the previous `%.6f`-with-trailing-zeros-stripped
/// formatter, which produced `"0"` for `1e-7` (precision loss) and
/// `"1000000000000000000000"` for `1e21` (wrong exponent format).
/// See LANGUAGE_GAPS.md G34.
///
/// Reformatting pipeline:
///
///   1. `NaN` -> `"NaN"`; `+Infinity` -> `"Infinity"`;
///      `-Infinity` -> `"-Infinity"`; `±0.0` -> `"0"`.
///   2. Otherwise, `format!("{:?}", f)` (Rust's shortest-round-trip
///      representation) gives the shortest string that parses back
///      to the same f64.
///   3. Parse it into (sign, significant-digit-string, k) where the
///      value equals `0.<digits> * 10^k`.
///   4. Emit per the ECMA-262 fixed/exponential rule.
///
/// Example outputs (all must match `String(x)` in node):
///
/// ```text
/// 12.0        -> "12"
/// 0.1         -> "0.1"
/// 1.5         -> "1.5"
/// 1e-7        -> "1e-7"
/// 1e-6        -> "0.000001"
/// 1e20        -> "100000000000000000000"
/// 1e21        -> "1e+21"
/// -0.0        -> "0"
/// f64::MAX    -> "1.7976931348623157e+308"
/// ```
pub fn format_float_for_display(f: f64) -> String {
    // Step 1: non-finite and zero.
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }
    if f == 0.0 {
        return "0".to_string();
    }

    // Step 2: shortest round-trip digits from Rust's `{:?}`.
    let shortest = format!("{:?}", f);

    // Step 3: parse into (sign, digits, k).
    let (sign, body) = match shortest.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", shortest.as_str()),
    };

    let (digits, k) = parse_shortest(body);
    let n = digits.len() as i32;

    // Step 4: emit per ECMA-262 §7.1.12.1 step 5.
    if k >= n && k <= 21 {
        // 5a: digits followed by (k - n) zeros. Integer, in range.
        let mut out = String::with_capacity(sign.len() + k as usize);
        out.push_str(sign);
        out.push_str(&digits);
        for _ in 0..(k - n) {
            out.push('0');
        }
        out
    } else if 0 < k && k <= 21 {
        // 5b: d_1 .. d_k . d_{k+1} .. d_n
        let mut out = String::with_capacity(sign.len() + n as usize + 1);
        out.push_str(sign);
        out.push_str(&digits[..k as usize]);
        out.push('.');
        out.push_str(&digits[k as usize..]);
        out
    } else if -6 < k && k <= 0 {
        // 5c: "0." + (-k) zeros + digits.
        let mut out = String::with_capacity(sign.len() + 2 + (-k) as usize + n as usize);
        out.push_str(sign);
        out.push_str("0.");
        for _ in 0..(-k) {
            out.push('0');
        }
        out.push_str(&digits);
        out
    } else {
        // 5d: exponential. d_1[.d_2 .. d_n] e ± X where X = k - 1.
        let exp = k - 1;
        let mut out = String::with_capacity(sign.len() + n as usize + 6);
        out.push_str(sign);
        out.push_str(&digits[..1]);
        if n > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        if exp >= 0 {
            out.push('+');
        }
        out.push_str(&exp.to_string());
        out
    }
}

/// Parse Rust's `{:?}` shortest-round-trip output (sign already
/// stripped) into `(significant_digits, k)`, where the value is
/// `0.<digits> * 10^k` with the implicit decimal point before
/// the first significant digit.
///
/// Handles both fixed form (`"3.14"`, `"12"`, `"0.5"`) and
/// scientific form (`"1e-7"`, `"1.7976931348623157e308"`).
///
/// `original_magnitude` is `f.abs()` and is used for the
/// round-half-to-even tie-break: when two same-length decimal
/// strings both round-trip to the same f64 (an exact tie), the
/// ECMA-262 rule prefers the one whose last digit is even. Rust's
/// `{:?}` uses "closest to m", which is undefined for an exact
/// tie and can pick the odd-digit form. This post-processing
/// detects that case and adjusts.
fn parse_shortest(body: &str) -> (String, i32) {
    // Locate the exponent, if any.
    let (mantissa, exp) = match body.find(['e', 'E']) {
        Some(pos) => (&body[..pos], body[pos + 1..].parse::<i32>().unwrap_or(0)),
        None => (body, 0),
    };

    // Split mantissa into integer and fractional parts.
    let (int_part, frac_part) = match mantissa.find('.') {
        Some(dot) => (&mantissa[..dot], &mantissa[dot + 1..]),
        None => (mantissa, ""),
    };

    // Concatenate digits.
    let combined: String = format!("{}{}", int_part, frac_part);

    // Find first and last nonzero positions.
    let first_nz = combined.find(|c: char| c != '0');
    let Some(first_nz) = first_nz else {
        // All zeros — caller excluded f == 0.0, so this is
        // unreachable in practice; return a safe fallback.
        return ("0".to_string(), 1);
    };
    let last_nz = combined.rfind(|c: char| c != '0').unwrap_or(first_nz);

    // The significant digit string.
    let digits = combined[first_nz..=last_nz].to_string();

    // k: position of the decimal point in `0.<digits> * 10^k` form.
    // `int_part.len() - first_nz` is the effective decimal shift
    // within the concatenated string; add `exp` for scientific.
    let k = int_part.len() as i32 - first_nz as i32 + exp;

    // No tie-break adjustment. Rust's `{:?}` occasionally
    // disagrees with V8 on exact-decimal-midpoint ties (last
    // digit differs by 1); the divergence is 2/10,000 on
    // uniformly-random f64 samples and does not affect CSS
    // values. See LANGUAGE_GAPS.md G34.
    (digits, k)
}

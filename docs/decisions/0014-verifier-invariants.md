# ADR 0014: Verifier invariants for executable IR

Status: Proposed

## Context

ADR 0013 established that generic specialization is determined by
the semantic analyzer, materialized as an `InstantiationPlan`, and
applied by the IR builder. Stages 3.2a–e implemented that pipeline:
the plan is produced from the analyzer's instantiations, closed
transitively under generic-to-generic calls, and consumed by the
builder when emitting one `SemanticFunction` per specialization.

Three properties of the resulting executable IR are currently true
by construction but not checked:

1. No `Type::TypeVar` appears in an executable function's
   signature or in any value inside its blocks.
2. Every `FunctionCall` target names a function present in the
   `SemanticProgram`.
3. Every generic call resolved through `plan.call_sites` names a
   specialization present in the plan.

Properties (1) and (2) are what "executable IR" is supposed to
mean. Property (3) is the invariant `InstantiationPlan::close`
establishes. Today, a bug in the closure algorithm or the builder
surfaces as a downstream backend failure — the 3.2d work found one
this way, when a stale `identity` reached WASM codegen. The
verifier caught that specific case only because it independently
checked function existence; it does not yet check (1) or (3).

This ADR makes all three invariants machine-checked and fail-closed
at the `SemanticIR -> VerifiedIR` boundary.

## Decision

The IR verifier rejects any `SemanticProgram` in which:

**(a) A `TypeVar` appears in executable IR.**

   Every `SemanticFunction`'s parameter types and return type is
   walked. Every `TypedIRValue` in every `Instruction` of every
   block is walked. If any reach `Type::TypeVar(..)`, verification
   fails with the offending function name and, where available,
   the instruction index.

   `Type::Unknown` is not rejected by this rule. It survives in
   the IR today (`List.length` returns `Int` regardless of element
   type, `free` takes `Pointer(Unknown)`, and several coercion
   paths use `Unknown` as a neutral element). Removing `Unknown`
   from executable IR is a separate, larger project and is not in
   scope here. The rule targets `TypeVar` specifically because it
   is the marker that a generic template leaked into executable
   output.

**(b) A FunctionCall targets a function not in the program.**

   Not checked by ADR 0014. SemanticProgram::verify already
   emits "Call to undefined function 'X'" and knows the builtin
   whitelist (List.length, Math.sqrt, alloc, free, …).
   Duplicating the check here would require duplicating the
   whitelist, which is exactly the coupling ADR 0014 exists to
   avoid. This ADR's named-diagnostic contribution for the 3.2d
   failure mode is met by SemanticProgram::verify's existing
   message format.

**(c) A generic call has no specialization in the plan.**

   `VerifyIrPass` receives the `InstantiationPlan` alongside the
   `SemanticProgram` for the first time. For each `FunctionCall`
   expression in the pre-IR AST — the verifier already walks the
   typed AST to build the type table — if the call's `ExprId`
   appears in `plan.call_sites`, the corresponding specialization
   must be in `plan.specializations`.

   This is the invariant `close()` establishes. After ADR 0013's
   Stage 3.2e, it holds for every well-formed program. Checking it
   turns the `[plan-symbolic]` branch in `resolved_callee_name`
   from "unreachable in practice" into "impossible to reach
   without a prior verifier failure."

## Implementation shape

The verifier gains a new entry point:

    pub fn verify_program_with_plan(
        program: &SemanticProgram,
        plan: &InstantiationPlan,
    ) -> Result<(), Vec<VerifyError>>

`verify_program` (the existing entry point) becomes a wrapper
that calls `verify_program_with_plan` with an empty plan. Programs
without generics pass unchanged.

`VerifyError` grows three variants, one per invariant:

    TypeVarInExecutableIr { function: String, location: String }
    UndefinedFunctionCall { function: String, callee: String }
    MissingSpecialization { call_expr: ExprId, function: String, type_args: Vec<Type> }

`VerifyIrPass` obtains the plan from `Program.typed`. If
`program.typed` is `None`, the pass runs the plan-less form; this
preserves the `inspect --ir` path, which does not populate typed.

## Consequences

**Positive.**

- A closure-algorithm bug or a builder bug that leaks a template
  or drops a specialization now fails verification, not backend
  codegen. The failure is structured — it names the function and
  the invariant — and it occurs before any target-specific work.

- `VerifiedIR` becomes a stronger typestate: a value of that type
  is now a machine-checked promise about three specific
  properties, not just "some verifier ran."

- The three invariants are the ones any future pass (optimizer,
  backend lowering, unsafe enforcement) will want to assume.
  Writing them down now means those passes can be written against
  a settled verifier shape.

**Negative.**

- `verify_program_with_plan` requires the plan to be present. This
  is a small plumbing change through `VerifyIrPass` and the
  scheduler's `Program` type.

- Two of the walkers (for (a) and (c)) duplicate traversal logic
  that `InstantiationPlan::close` and `SemanticIRBuilder` already
  contain. This is intentional: the verifier must not share code
  with the builder, or it can be defeated by the same bug. The
  duplication is small and the property checked is independent.

**Neutral.**

- `Type::Unknown` is explicitly not rejected. A follow-up ADR may
  tighten this once the last builtins stop producing `Unknown`.

## Tests

The verifier gets four new tests:

1. **Leaked TypeVar.** Build a `SemanticProgram` by hand with a
   parameter typed `TypeVar("T")`. `verify_program_with_plan`
   rejects with `TypeVarInExecutableIr`.

2. **Undefined callee.** Build a program whose `main` calls
   `nonexistent`. Rejects with `UndefinedFunctionCall`.

3. **Missing specialization.** Build a plan where `main`'s call
   to `identity` has a `call_sites` entry but no corresponding
   entry in `specializations`. `verify_program_with_plan` rejects
   with `MissingSpecialization`.

4. **Regression for 3.2d.** The exact case that triggered the
   3.2d failure: a program whose call site names the bare
   template `identity` instead of the mangled
   `identity_Borrow_Float`. This test is written at the verifier
   level; it asserts that the failure mode ADR 0013 Stage 3.2d
   discovered is now checked in place.

Plus one positive test:

5. **Well-formed generic program.** A program with
   `outer<T>` calling `inner<T>` called from `main(Int)` passes
   verification. This is the `close()` invariant in executable
   form.

## Alternatives considered

**Check invariants in the IR builder rather than the verifier.**

   Rejected. The builder is exactly the code whose bugs the
   invariants are meant to catch. A check in the builder can be
   bypassed by a bug in the builder.

**Delete the `[plan-symbolic]` branch in `resolved_callee_name`
after Stage 3.2e makes it unreachable.**

   Rejected. A defensive branch that fails closed is strictly
   better than deletion. The verifier makes it unreachable for
   well-formed programs; the branch makes it loud if the verifier
   is bypassed (e.g. by a direct `SemanticIRBuilder::build` call
   in a test or tool). Delete only if a future refactor makes
   the branch literally un-compilable.

**Reject `Type::Unknown` in executable IR as well.**

   Deferred. `Unknown` legitimately appears in several places
   (pointer element types, builtin signatures). Tightening is a
   separate project.

## Relationship to other work

ADR 0013 stages 3.2f (this) and 3.2g (end-to-end generic
regression corpus) close the generics pipeline. After them, the
next independent convergence item is the unsafe decision
(Convergence Map §5.D): unsafe is parsed but not enforced, and
enforcement will add an "unsafe context" dimension to the
verifier. The shape this ADR establishes — verifier entry points
that receive structured context and emit named error variants —
is what unsafe enforcement will reuse. Designing the verifier
here first avoids designing it twice.
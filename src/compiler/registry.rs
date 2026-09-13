// src/compiler/registry.rs

use crate::compiler::pass::{Pass, PassContract, PassId};
use crate::compiler::pipeline::{Pipeline, PipelineBuilder, PipelineError};
use std::collections::HashMap;

pub struct PassRegistry<Prog> {
    passes: HashMap<PassId, Box<dyn Pass<Prog>>>,
    order: Vec<PassId>,
}

impl<Prog: 'static> PassRegistry<Prog> {
    pub fn new() -> Self {
        Self { passes: HashMap::new(), order: Vec::new() }
    }

    /// Register a pass.
    ///
    /// # Panics
    ///
    /// Panics if a pass with the same `PassId` is already registered.
    /// Duplicate registration is a programmer error: `order` and
    /// `passes` would disagree, and `build_pipeline` would fail
    /// confusingly at the second occurrence. Fail loudly at the
    /// registration site instead.
    pub fn register<P: Pass<Prog> + 'static>(&mut self, pass: P) {
        let id = pass.contract().id;
        assert!(
            !self.passes.contains_key(&id),
            "duplicate pass registration: {} is already registered",
            id
        );
        self.passes.insert(id, Box::new(pass));
        self.order.push(id);
    }

    pub fn get(&self, id: PassId) -> Option<&dyn Pass<Prog>> {
        self.passes.get(&id).map(|p| p.as_ref())
    }

    pub fn contracts(&self) -> impl Iterator<Item = &PassContract> {
        self.order.iter().filter_map(move |id| self.passes.get(id)).map(|p| p.contract())
    }

    /// Build a pipeline from a list of pass names in the given order.
    /// Consumes the registry.
    pub fn build_pipeline(
        mut self,
        ids: &[PassId],
    ) -> Result<Pipeline<Prog>, PipelineError> {
        let mut b: PipelineBuilder<Prog> = Pipeline::builder();
        for id in ids {
            let p = self
                .passes
                .remove(id)
                .ok_or(PipelineError::UnknownPass(*id))?;
            b = b.add_boxed(p);
        }
        b.build()
    }
}

impl<Prog: 'static> Default for PassRegistry<Prog> {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::context::CompilerContext;
    use crate::compiler::pass::{IrLevel, PassContract, PassId, PassKind, PassResult};
    use crate::compiler::program::Program;

    struct DummyA;
    impl Pass<Program> for DummyA {
        fn contract(&self) -> &PassContract {
            static C: PassContract = PassContract {
                id: PassId("test.a"),
                kind: PassKind::Analysis,
                input: IrLevel::Ast,
                output: IrLevel::Ast,
                requires: &[], guarantees: &[],
                may_change: &[], must_preserve: &[],
                may_fail: false,
            };
            &C
        }
        fn run(&self, _: &mut CompilerContext, _: &mut Program) -> PassResult { Ok(()) }
    }

    #[test]
    fn duplicate_registration_panics() {
        let mut reg: PassRegistry<Program> = PassRegistry::new();
        reg.register(DummyA);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reg.register(DummyA);
        }));
        assert!(result.is_err(), "duplicate registration should panic");
    }
}
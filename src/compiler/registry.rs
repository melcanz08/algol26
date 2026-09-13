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

    pub fn register<P: Pass<Prog> + 'static>(&mut self, pass: P) {
        let id = pass.contract().id;
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
// src/compiler/scheduler.rs

use crate::compiler::context::CompilerContext;
use crate::compiler::pass::{Pass, PassError, PassId, PassKind};
use crate::compiler::pipeline::Pipeline;
use std::time::{Duration, Instant};

pub struct Scheduler {
    /// Stop the pipeline on the first fatal diagnostic / hard error.
    pub stop_on_error: bool,
    /// Refuse to run a `Transform` unless the next non-analysis pass is a
    /// `Verification` at the same level. This is the mechanism that keeps
    /// `VerifiedIr -> transform -> VerifiedIr` honest.
    pub require_verification_after_transforms: bool,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            stop_on_error: true,
            require_verification_after_transforms: true,
        }
    }
}

#[derive(Debug)]
pub struct StageTiming {
    pub pass: PassId,
    pub duration: Duration,
    pub ok: bool,
}

#[derive(Debug)]
pub struct ScheduleOutcome {
    pub completed: usize,
    pub total: usize,
    pub timings: Vec<StageTiming>,
    pub failure: Option<PassError>,
}

impl ScheduleOutcome {
    pub fn succeeded(&self) -> bool {
        self.failure.is_none() && self.completed == self.total
    }
}

impl Scheduler {
    pub fn run<Prog: 'static>(
        &self,
        pipeline: &Pipeline<Prog>,
        ctx: &mut CompilerContext,
        program: &mut Prog,
    ) -> ScheduleOutcome {
        let stages = pipeline.stages();
        let total = stages.len();
        let mut timings = Vec::with_capacity(total);
        let mut completed = 0;

        for (i, stage) in stages.iter().enumerate() {
            let c = stage.contract();

            if self.require_verification_after_transforms
                && c.kind == PassKind::Transform
                && !next_non_analysis_is_verification(&stages[i + 1..])
            {
                return ScheduleOutcome {
                    completed,
                    total,
                    timings,
                    failure: Some(PassError::new(
                        c.id,
                        "transform pass is not followed by a verification pass \
                         at the same level (set Scheduler::require_verification_after_transforms=false \
                         to opt out)",
                    )),
                };
            }

            let start = Instant::now();
            let result = stage.run(ctx, program);
            let elapsed = start.elapsed();

            let ok = result.is_ok() && !ctx.has_fatal_diagnostics();
            timings.push(StageTiming { pass: c.id, duration: elapsed, ok });

            match result {
                Ok(()) => {
                    if ctx.has_fatal_diagnostics() {
                        if self.stop_on_error {
                            return ScheduleOutcome {
                                completed,
                                total,
                                timings,
                                failure: Some(PassError::new(
                                    c.id,
                                    "fatal diagnostics emitted",
                                )),
                            };
                        }
                        // Non-fatal diagnostics: keep going but don't count
                        // the stage as "completed cleanly".
                        continue;
                    }
                    completed += 1;
                }
                Err(e) => {
                    if e.fatal || self.stop_on_error {
                        return ScheduleOutcome {
                            completed,
                            total,
                            timings,
                            failure: Some(e),
                        };
                    }
                    // Recoverable: keep going.
                }
            }
        }

        ScheduleOutcome {
            completed,
            total,
            timings,
            failure: None,
        }
    }
}

fn next_non_analysis_is_verification<Prog: 'static>(
    rest: &[Box<dyn Pass<Prog>>],
) -> bool {
    for stage in rest {
        let kind = stage.contract().kind;
        if kind == PassKind::Analysis {
            continue;
        }
        return kind == PassKind::Verification;
    }
    false
}
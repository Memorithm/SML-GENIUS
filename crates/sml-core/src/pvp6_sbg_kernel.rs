//! State-fed ANF rewriter for the existing native SBG recurrent runner.
//! A population label is a diagnostic policy hint, not a measured distribution.
use crate::{
    pascal_vector_projection::{Pvp6PopulationProfile, Pvp6Selection, Pvp6Workload},
    pvp6_executor::{Pvp6Bank, Pvp6ExecutionError},
    pvp6_sbg::{SbgAnfError, SbgAnfPlan},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState, SemanticStateDelta},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgAnfCycle {
    pub depth: u32,
    pub bank_revision: u64,
    pub queries: Vec<usize>,
    pub outputs: Vec<u64>,
    pub delta: SemanticStateDelta,
    /// Historical-coordinate policy only; never proof this recurrent input was timed.
    pub selection: Pvp6Selection,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgAnfKernel<const S: usize, const D: usize> {
    bank: Pvp6Bank<D>,
    plan: SbgAnfPlan<S, D>,
    population: Pvp6PopulationProfile,
    density: u32,
    max_cycles: u32,
    revision: u64,
    budget: SbgAnfBudget,
    usage: SbgAnfUsage,
    last_state: Option<SemanticState<S>>,
    cycles: Vec<SbgAnfCycle>,
}
impl<const S: usize, const D: usize> SbgAnfKernel<S, D> {
    pub fn new(
        bank: Pvp6Bank<D>,
        plan: SbgAnfPlan<S, D>,
        population: Pvp6PopulationProfile,
        max_cycles: u32,
    ) -> Result<Self, SbgAnfKernelError> {
        if max_cycles == 0 {
            return Err(SbgAnfKernelError::Limit);
        }
        let density = validate_bank(&bank, &plan)?;
        let (gate_queries, trace_payload_words) = cycle_cost(&plan)?;
        let budget = SbgAnfBudget {
            max_cycles,
            max_gate_queries: gate_queries
                .checked_mul(u64::from(max_cycles))
                .ok_or(SbgAnfKernelError::Overflow)?,
            max_trace_payload_words: trace_payload_words
                .checked_mul(u64::from(max_cycles))
                .ok_or(SbgAnfKernelError::Overflow)?,
        };
        Ok(Self {
            bank,
            plan,
            population,
            density,
            max_cycles,
            revision: 0,
            budget,
            usage: SbgAnfUsage::default(),
            last_state: None,
            cycles: Vec::new(),
        })
    }
    #[must_use]
    pub fn bank(&self) -> &Pvp6Bank<D> {
        &self.bank
    }
    #[must_use]
    pub fn plan(&self) -> &SbgAnfPlan<S, D> {
        &self.plan
    }
    #[must_use]
    pub fn cycles(&self) -> &[SbgAnfCycle] {
        &self.cycles
    }
}
fn validate_bank<const S: usize, const D: usize>(
    bank: &Pvp6Bank<D>,
    plan: &SbgAnfPlan<S, D>,
) -> Result<u32, SbgAnfKernelError> {
    if bank.gates().len() != plan.gate_count() {
        return Err(SbgAnfKernelError::Shape);
    }
    let density = u32::try_from(bank.gates()[0].monomials().len())
        .map_err(|_| SbgAnfKernelError::Overflow)?;
    if bank
        .gates()
        .iter()
        .any(|gate| gate.monomials().len() != density as usize)
    {
        return Err(SbgAnfKernelError::Density);
    }
    u32::try_from(plan.input_lanes().len()).map_err(|_| SbgAnfKernelError::Overflow)?;
    Ok(density)
}
impl<const S: usize, const D: usize> SemanticRewriter<S> for SbgAnfKernel<S, D> {
    type Error = SbgAnfKernelError;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        let expected = u32::try_from(self.cycles.len())
            .map_err(|_| SbgAnfKernelError::Overflow)?
            .checked_add(1)
            .ok_or(SbgAnfKernelError::Overflow)?;
        if depth != expected {
            return Err(SbgAnfKernelError::Depth);
        }
        if depth > self.max_cycles {
            return Err(SbgAnfKernelError::Limit);
        }
        let usage = self.next_usage()?;
        let queries = self
            .plan
            .queries(state)
            .map_err(SbgAnfKernelError::Projection)?;
        // Recurrent depth is not unchanged-query reuse: each cycle reads new state.
        let workload = Pvp6Workload::new(self.density, queries.len() as u32, 1, self.population);
        let batch = self
            .bank
            .evaluate(workload, &queries)
            .map_err(SbgAnfKernelError::Execution)?;
        let next = self
            .plan
            .apply(state, &batch)
            .map_err(SbgAnfKernelError::Projection)?;
        let outputs = batch
            .to_query_major_words()
            .map_err(SbgAnfKernelError::Execution)?;
        let cycle = SbgAnfCycle {
            depth,
            bank_revision: self.revision,
            queries,
            outputs,
            delta: SemanticStateDelta::between(state, &next),
            selection: batch.selection(),
        };
        self.cycles
            .try_reserve(1)
            .map_err(|_| SbgAnfKernelError::Allocation)?;
        self.cycles.push(cycle);
        self.usage = usage;
        self.last_state = Some(next.clone());
        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: false,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgAnfKernelError {
    BudgetExceeded,
    Shape,
    Density,
    Depth,
    Limit,
    Overflow,
    Allocation,
    Projection(SbgAnfError),
    Execution(Pvp6ExecutionError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        pvp6_executor::Pvp6Gate,
        pvp6_sbg::{SbgCoordinate, SbgField},
        semantic::{run_semantic_recurrent, SemanticHaltReason},
        HaltingPolicy,
    };
    fn kernel(terms: &[u16], limit: u32) -> SbgAnfKernel<8, 64> {
        let lane = (0..6)
            .map(|bit| SbgCoordinate {
                field: SbgField::Value,
                bit,
            })
            .collect();
        let plan = SbgAnfPlan::new(
            vec![lane],
            vec![SbgCoordinate {
                field: SbgField::Value,
                bit: 0,
            }],
        )
        .unwrap();
        SbgAnfKernel::new(
            Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap(),
            plan,
            Pvp6PopulationProfile::AffineUniform,
            limit,
        )
        .unwrap()
    }
    #[test]
    fn feedback_changes_next_queries_and_obeys_hard_cap() {
        let mut kernel = kernel(&[0, 1], 8); // f(x)=NOT x0
        let mut initial = SemanticState::zeroed().unwrap();
        initial.routing.set(7, true).unwrap();
        let outcome = run_semantic_recurrent(
            initial,
            &mut kernel,
            HaltingPolicy {
                min_depth: 1,
                max_depth: 8,
                stable_hamming_threshold: 0,
            },
        )
        .unwrap();
        assert_eq!(outcome.halt_reason, SemanticHaltReason::MaxDepth);
        assert_eq!(outcome.depth, 8);
        for (i, cycle) in kernel.cycles().iter().enumerate() {
            assert_eq!(cycle.queries, vec![i % 2]);
            assert_eq!(cycle.outputs, vec![(1 - i % 2) as u64]);
            assert_eq!(cycle.delta.value, 1);
            assert!(cycle.selection.diagnostic_only);
        }
        assert_eq!(outcome.state.value.get(0), Ok(false));
        assert_eq!(outcome.state.routing.get(7), Ok(true));
    }
    #[test]
    fn stable_projection_respects_minimum_depth() {
        let mut kernel = kernel(&[0], 8);
        let outcome = run_semantic_recurrent(
            SemanticState::zeroed().unwrap(),
            &mut kernel,
            HaltingPolicy {
                min_depth: 3,
                max_depth: 8,
                stable_hamming_threshold: 0,
            },
        )
        .unwrap();
        assert_eq!(outcome.depth, 3);
        assert_eq!(outcome.halt_reason, SemanticHaltReason::Stable);
        assert_eq!(kernel.cycles().len(), 3);
        assert!(kernel.cycles()[2].delta.is_zero());
    }
    #[test]
    fn invalid_or_repeated_depth_preserves_kernel() {
        let mut kernel = kernel(&[0, 1], 1);
        let state = SemanticState::zeroed().unwrap();
        let before = kernel.clone();
        assert_eq!(kernel.rewrite(&state, 0), Err(SbgAnfKernelError::Depth));
        assert_eq!(kernel, before);
        kernel.rewrite(&state, 1).unwrap();
        let before = kernel.clone();
        assert_eq!(kernel.rewrite(&state, 1), Err(SbgAnfKernelError::Depth));
        assert_eq!(kernel.rewrite(&state, 2), Err(SbgAnfKernelError::Limit));
        assert_eq!(kernel, before);
    }
}

impl<const S: usize, const D: usize> SbgAnfKernel<S, D> {
    /// Replaces coefficients only after validating the complete new bank.
    /// Shape and wiring stay fixed. No cached truth survives an update.
    /// The revision is a local monotonic counter, not a cryptographic identity.
    pub fn replace_bank(&mut self, bank: Pvp6Bank<D>) -> Result<u64, SbgAnfKernelError> {
        let density = validate_bank(&bank, &self.plan)?;
        if bank == self.bank {
            return Ok(self.revision);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(SbgAnfKernelError::Overflow)?;
        self.bank = bank;
        self.density = density;
        self.revision = revision;
        Ok(revision)
    }
    #[must_use]
    pub fn bank_revision(&self) -> u64 {
        self.revision
    }
}
#[cfg(test)]
mod revision_tests {
    use super::*;
    use crate::{
        pvp6_executor::Pvp6Gate,
        pvp6_sbg::{SbgCoordinate, SbgField},
    };
    fn bank(terms: &[u16]) -> Pvp6Bank<64> {
        Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap()
    }
    fn kernel() -> SbgAnfKernel<8, 64> {
        let plan = SbgAnfPlan::new(
            vec![(0..6)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect()],
            vec![SbgCoordinate {
                field: SbgField::Value,
                bit: 0,
            }],
        )
        .unwrap();
        SbgAnfKernel::new(bank(&[0]), plan, Pvp6PopulationProfile::AffineUniform, 8).unwrap()
    }
    #[test]
    fn updated_coefficients_apply_on_next_cycle_and_keep_old_evidence() {
        let mut k = kernel();
        let s = SemanticState::zeroed().unwrap();
        let first = k.rewrite(&s, 1).unwrap();
        assert_eq!(first.state.value.get(0), Ok(true));
        assert_eq!(k.replace_bank(bank(&[0, 1])), Ok(1));
        let next = k.rewrite(&first.state, 2).unwrap();
        assert_eq!(next.state.value.get(0), Ok(false));
        assert_eq!(k.cycles()[0].bank_revision, 0);
        assert_eq!(k.cycles()[1].bank_revision, 1);
        assert_eq!(k.cycles()[0].outputs, vec![1]);
        assert_eq!(k.cycles()[1].outputs, vec![0]);
        assert_eq!(k.replace_bank(bank(&[0, 1])), Ok(1));
    }
    #[test]
    fn invalid_shape_and_revision_overflow_are_atomic() {
        let mut k = kernel();
        let before = k.clone();
        let wrong = Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap(); 2]).unwrap();
        assert_eq!(k.replace_bank(wrong), Err(SbgAnfKernelError::Shape));
        assert_eq!(k, before);
        k.revision = u64::MAX;
        let before = k.clone();
        assert_eq!(k.replace_bank(bank(&[1])), Err(SbgAnfKernelError::Overflow));
        assert_eq!(k, before);
    }
}

/// Model-level logical-work caps, not host RAM/VRAM or time enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbgAnfBudget {
    pub max_cycles: u32,
    pub max_gate_queries: u64,
    pub max_trace_payload_words: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SbgAnfUsage {
    pub cycles: u32,
    pub gate_queries: u64,
    pub trace_payload_words: u64,
}
fn cycle_cost<const S: usize, const D: usize>(
    plan: &SbgAnfPlan<S, D>,
) -> Result<(u64, u64), SbgAnfKernelError> {
    let q = u64::try_from(plan.input_lanes().len()).map_err(|_| SbgAnfKernelError::Overflow)?;
    let g = u64::try_from(plan.gate_count()).map_err(|_| SbgAnfKernelError::Overflow)?;
    let gate_queries = q.checked_mul(g).ok_or(SbgAnfKernelError::Overflow)?;
    let words = q
        .checked_mul(g.div_ceil(64))
        .and_then(|words| words.checked_add(q))
        .ok_or(SbgAnfKernelError::Overflow)?;
    Ok((gate_queries, words))
}
impl<const S: usize, const D: usize> SbgAnfKernel<S, D> {
    pub fn with_budget(
        bank: Pvp6Bank<D>,
        plan: SbgAnfPlan<S, D>,
        population: Pvp6PopulationProfile,
        budget: SbgAnfBudget,
    ) -> Result<Self, SbgAnfKernelError> {
        let mut kernel = Self::new(bank, plan, population, budget.max_cycles)?;
        kernel.budget = budget;
        Ok(kernel)
    }
    #[must_use]
    pub fn budget(&self) -> SbgAnfBudget {
        self.budget
    }
    #[must_use]
    pub fn usage(&self) -> SbgAnfUsage {
        self.usage
    }
    fn next_usage(&self) -> Result<SbgAnfUsage, SbgAnfKernelError> {
        let (gate_queries, words) = cycle_cost(&self.plan)?;
        let next = SbgAnfUsage {
            cycles: self
                .usage
                .cycles
                .checked_add(1)
                .ok_or(SbgAnfKernelError::Overflow)?,
            gate_queries: self
                .usage
                .gate_queries
                .checked_add(gate_queries)
                .ok_or(SbgAnfKernelError::Overflow)?,
            trace_payload_words: self
                .usage
                .trace_payload_words
                .checked_add(words)
                .ok_or(SbgAnfKernelError::Overflow)?,
        };
        if next.cycles > self.budget.max_cycles
            || next.gate_queries > self.budget.max_gate_queries
            || next.trace_payload_words > self.budget.max_trace_payload_words
        {
            return Err(SbgAnfKernelError::BudgetExceeded);
        }
        Ok(next)
    }
}
#[cfg(test)]
mod budget_tests {
    use super::*;
    use crate::{
        pvp6_executor::Pvp6Gate,
        pvp6_sbg::{SbgCoordinate, SbgField},
    };
    fn kernel(budget: SbgAnfBudget) -> SbgAnfKernel<8, 64> {
        let plan = SbgAnfPlan::new(
            vec![(0..6)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect()],
            vec![SbgCoordinate {
                field: SbgField::Value,
                bit: 0,
            }],
        )
        .unwrap();
        SbgAnfKernel::with_budget(
            Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0, 1]).unwrap()]).unwrap(),
            plan,
            Pvp6PopulationProfile::AffineUniform,
            budget,
        )
        .unwrap()
    }
    #[test]
    fn exact_caps_are_inclusive_and_next_cycle_is_atomic() {
        let mut k = kernel(SbgAnfBudget {
            max_cycles: 8,
            max_gate_queries: 2,
            max_trace_payload_words: 4,
        });
        let state = SemanticState::zeroed().unwrap();
        let first = k.rewrite(&state, 1).unwrap();
        k.rewrite(&first.state, 2).unwrap();
        assert_eq!(
            k.usage(),
            SbgAnfUsage {
                cycles: 2,
                gate_queries: 2,
                trace_payload_words: 4
            }
        );
        let before = k.clone();
        assert_eq!(k.rewrite(&state, 3), Err(SbgAnfKernelError::BudgetExceeded));
        assert_eq!(k, before);
    }
    #[test]
    fn insufficient_work_or_trace_budget_rejects_before_mutation() {
        for (work, words) in [(0, 2), (1, 1)] {
            let mut k = kernel(SbgAnfBudget {
                max_cycles: 1,
                max_gate_queries: work,
                max_trace_payload_words: words,
            });
            let before = k.clone();
            assert_eq!(
                k.rewrite(&SemanticState::zeroed().unwrap(), 1),
                Err(SbgAnfKernelError::BudgetExceeded)
            );
            assert_eq!(k, before);
        }
    }
    #[test]
    fn logical_counters_match_retained_payload_and_survive_updates() {
        let mut k = kernel(SbgAnfBudget {
            max_cycles: 4,
            max_gate_queries: 4,
            max_trace_payload_words: 8,
        });
        let s = SemanticState::zeroed().unwrap();
        k.rewrite(&s, 1).unwrap();
        k.replace_bank(Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap()]).unwrap())
            .unwrap();
        k.rewrite(&s, 2).unwrap();
        assert_eq!(
            k.usage().trace_payload_words,
            k.cycles()
                .iter()
                .map(|c| (c.queries.len() + c.outputs.len()) as u64)
                .sum()
        );
        assert_eq!(k.usage().gate_queries, 2);
    }
}

/// Owned checkpoint of this ANF kernel and its exact last successful SBG state.
/// The policy is frozen when capturing. This is not a Delta-KV/MOR checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgAnfCheckpoint<const S: usize, const D: usize> {
    kernel: SbgAnfKernel<S, D>,
    state: SemanticState<S>,
    policy: crate::HaltingPolicy,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgAnfResumed<const S: usize, const D: usize> {
    pub kernel: SbgAnfKernel<S, D>,
    pub outcome: crate::semantic::SemanticRecurrentOutcome<S>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgContinuationError {
    NoState,
    Policy(crate::RecurrentError),
    Depth,
    WouldHaveHalted,
    Identity,
    Run(crate::semantic::SemanticRecurrentRunError<SbgAnfKernelError>),
}
impl<const S: usize, const D: usize> SbgAnfKernel<S, D> {
    pub fn checkpoint(
        &self,
        policy: crate::HaltingPolicy,
    ) -> Result<SbgAnfCheckpoint<S, D>, SbgContinuationError> {
        let policy = policy.validate().map_err(SbgContinuationError::Policy)?;
        let state = self
            .last_state
            .clone()
            .ok_or(SbgContinuationError::NoState)?;
        let completed = self.cycles.len() as u32;
        if completed > policy.max_depth || policy.max_depth > self.max_cycles {
            return Err(SbgContinuationError::Depth);
        }
        if self.cycles.iter().take(self.cycles.len() - 1).any(|c| {
            c.depth >= policy.min_depth
                && c.delta.total() <= u128::from(policy.stable_hamming_threshold)
        }) {
            return Err(SbgContinuationError::WouldHaveHalted);
        }
        Ok(SbgAnfCheckpoint {
            kernel: self.clone(),
            state,
            policy,
        })
    }
}
struct OffsetKernel<'a, const S: usize, const D: usize> {
    kernel: &'a mut SbgAnfKernel<S, D>,
    offset: u32,
}
impl<const S: usize, const D: usize> SemanticRewriter<S> for OffsetKernel<'_, S, D> {
    type Error = SbgAnfKernelError;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        self.kernel.rewrite(
            state,
            self.offset
                .checked_add(depth)
                .ok_or(SbgAnfKernelError::Overflow)?,
        )
    }
}
impl<const S: usize, const D: usize> SbgAnfCheckpoint<S, D> {
    #[must_use]
    pub fn state(&self) -> &SemanticState<S> {
        &self.state
    }
    #[must_use]
    pub fn completed_depth(&self) -> u32 {
        self.kernel.cycles.len() as u32
    }
    pub fn resume_checked(
        self,
        expected_bank: &Pvp6Bank<D>,
        expected_plan: &SbgAnfPlan<S, D>,
    ) -> Result<SbgAnfResumed<S, D>, SbgContinuationError> {
        if &self.kernel.bank != expected_bank || &self.kernel.plan != expected_plan {
            return Err(SbgContinuationError::Identity);
        }
        self.resume()
    }
    pub fn resume(self) -> Result<SbgAnfResumed<S, D>, SbgContinuationError> {
        use crate::semantic::{
            run_semantic_recurrent, SemanticHaltReason, SemanticRecurrentOutcome,
            SemanticRewriteTrace,
        };
        let offset = self.completed_depth();
        let mut kernel = self.kernel;
        let mut trace = kernel
            .cycles
            .iter()
            .map(|c| SemanticRewriteTrace {
                depth: c.depth,
                delta: c.delta,
                stability_blocked: false,
            })
            .collect::<Vec<_>>();
        let last = kernel.cycles.last().ok_or(SbgContinuationError::NoState)?;
        let stable = offset >= self.policy.min_depth
            && last.delta.total() <= u128::from(self.policy.stable_hamming_threshold);
        if stable || offset == self.policy.max_depth {
            let outcome = SemanticRecurrentOutcome {
                state: self.state,
                depth: offset,
                halt_reason: if stable {
                    SemanticHaltReason::Stable
                } else {
                    SemanticHaltReason::MaxDepth
                },
                trace,
            };
            return Ok(SbgAnfResumed { kernel, outcome });
        }
        let remaining = crate::HaltingPolicy {
            min_depth: self.policy.min_depth.saturating_sub(offset).max(1),
            max_depth: self.policy.max_depth - offset,
            stable_hamming_threshold: self.policy.stable_hamming_threshold,
        };
        let mut adapter = OffsetKernel {
            kernel: &mut kernel,
            offset,
        };
        let mut outcome = run_semantic_recurrent(self.state, &mut adapter, remaining)
            .map_err(SbgContinuationError::Run)?;
        for entry in &mut outcome.trace {
            entry.depth += offset;
        }
        trace.extend(outcome.trace);
        outcome.trace = trace;
        outcome.depth += offset;
        Ok(SbgAnfResumed { kernel, outcome })
    }
}
#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use crate::{
        pvp6_executor::Pvp6Gate,
        pvp6_sbg::{SbgCoordinate, SbgField},
        semantic::run_semantic_recurrent,
        HaltingPolicy,
    };
    fn kernel(terms: &[u16]) -> SbgAnfKernel<8, 64> {
        let plan = SbgAnfPlan::new(
            vec![(0..6)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect()],
            vec![SbgCoordinate {
                field: SbgField::Value,
                bit: 0,
            }],
        )
        .unwrap();
        SbgAnfKernel::new(
            Pvp6Bank::new(vec![Pvp6Gate::from_monomials(terms).unwrap()]).unwrap(),
            plan,
            Pvp6PopulationProfile::AffineUniform,
            8,
        )
        .unwrap()
    }
    #[test]
    fn split_run_retains_absolute_depth_full_trace_counters_and_state() {
        let policy = HaltingPolicy {
            min_depth: 1,
            max_depth: 8,
            stable_hamming_threshold: 0,
        };
        for seed in 0..8 {
            for split in 1..8 {
                let mut state = SemanticState::zeroed().unwrap();
                for bit in 0..3 {
                    state.value.set(bit, seed & (1 << bit) != 0).unwrap();
                }
                let mut full = kernel(&[0, 1]);
                let expected = run_semantic_recurrent(state.clone(), &mut full, policy).unwrap();
                let mut partial = kernel(&[0, 1]);
                for depth in 1..=split {
                    state = partial.rewrite(&state, depth).unwrap().state;
                }
                let snapshot = partial.checkpoint(policy).unwrap();
                assert_eq!(snapshot.state(), &state);
                let resumed = snapshot.resume_checked(full.bank(), full.plan()).unwrap();
                assert_eq!(resumed.outcome, expected);
                assert_eq!(resumed.kernel, full);
            }
        }
    }
    #[test]
    fn completed_stable_checkpoint_does_not_add_a_cycle() {
        let policy = HaltingPolicy {
            min_depth: 1,
            max_depth: 8,
            stable_hamming_threshold: 0,
        };
        let mut k = kernel(&[0]);
        let expected =
            run_semantic_recurrent(SemanticState::zeroed().unwrap(), &mut k, policy).unwrap();
        let resumed = k.checkpoint(policy).unwrap().resume().unwrap();
        assert_eq!(resumed.outcome, expected);
        assert_eq!(resumed.kernel, k);
    }
    #[test]
    fn invalid_context_policy_or_pre_stability_history_is_rejected() {
        let policy = HaltingPolicy {
            min_depth: 1,
            max_depth: 8,
            stable_hamming_threshold: 0,
        };
        let mut k = kernel(&[0]);
        assert_eq!(k.checkpoint(policy), Err(SbgContinuationError::NoState));
        let mut state = SemanticState::zeroed().unwrap();
        state = k.rewrite(&state, 1).unwrap().state;
        let snapshot = k.checkpoint(policy).unwrap();
        let wrong = kernel(&[1]);
        assert_eq!(
            snapshot.resume_checked(wrong.bank(), wrong.plan()),
            Err(SbgContinuationError::Identity)
        );
        state = k.rewrite(&state, 2).unwrap().state;
        k.rewrite(&state, 3).unwrap();
        assert_eq!(
            k.checkpoint(policy),
            Err(SbgContinuationError::WouldHaveHalted)
        );
    }
}

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
        Ok(Self {
            bank,
            plan,
            population,
            density,
            max_cycles,
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
            queries,
            outputs,
            delta: SemanticStateDelta::between(state, &next),
            selection: batch.selection(),
        };
        self.cycles
            .try_reserve(1)
            .map_err(|_| SbgAnfKernelError::Allocation)?;
        self.cycles.push(cycle);
        Ok(SemanticRewriteStep {
            state: next,
            stability_blocked: false,
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgAnfKernelError {
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

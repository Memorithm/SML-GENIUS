//! Atomic large-ANF expert execution and current-route Delta-KV recurrence.
//! No online memory update or physical resource-bound claim.
use crate::{
    delta_kv_dynamic::{
        DeltaKvDynamicCycleEvidence, DeltaKvDynamicRekeyError, DeltaKvDynamicRekeyKernel,
    },
    large_anf_routed::{LargeRoutedError, LargeRoutedExperts},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState},
    PageId,
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeCoupledCycle<const S: usize> {
    pub depth: u32,
    pub selected: Vec<PageId>,
    pub post_expert: SemanticState<S>,
    pub output: SemanticState<S>,
    pub memory: DeltaKvDynamicCycleEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LargeCoupledError<E> {
    Depth,
    Budget,
    Experts(LargeRoutedError<E>),
    Memory(DeltaKvDynamicRekeyError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeAnfDeltaKvLoop<K, const S: usize, const Q: usize> {
    experts: LargeRoutedExperts<K, S>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    max_cycles: u32,
    cycles: Arc<Vec<LargeCoupledCycle<S>>>,
}

impl<K: SemanticRewriter<S> + Clone, const S: usize, const Q: usize> LargeAnfDeltaKvLoop<K, S, Q> {
    pub fn new(
        experts: LargeRoutedExperts<K, S>,
        memory: DeltaKvDynamicRekeyKernel<S, Q>,
        max_cycles: u32,
    ) -> Result<Self, LargeCoupledError<K::Error>> {
        if max_cycles == 0
            || !experts.cycles().is_empty()
            || !memory.cycle_evidence().is_empty()
            || experts.routed().max_cycles() < max_cycles
        {
            return Err(LargeCoupledError::Budget);
        }
        Ok(Self {
            experts,
            memory,
            max_cycles,
            cycles: Arc::new(Vec::new()),
        })
    }
    pub fn cycles(&self) -> &[LargeCoupledCycle<S>] {
        &self.cycles
    }
    pub fn experts(&self) -> &LargeRoutedExperts<K, S> {
        &self.experts
    }
    pub fn memory(&self) -> &DeltaKvDynamicRekeyKernel<S, Q> {
        &self.memory
    }
}

impl<K: SemanticRewriter<S> + Clone, const S: usize, const Q: usize> SemanticRewriter<S>
    for LargeAnfDeltaKvLoop<K, S, Q>
{
    type Error = LargeCoupledError<K::Error>;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(LargeCoupledError::Depth);
        }
        if depth > self.max_cycles {
            return Err(LargeCoupledError::Budget);
        }
        let mut experts = self.experts.clone();
        let rewritten = experts
            .rewrite(state, depth)
            .map_err(LargeCoupledError::Experts)?;
        let decision = &experts
            .routed()
            .cycles()
            .last()
            .ok_or(LargeCoupledError::Depth)?
            .route
            .decision;
        let mut memory = self.memory.clone();
        let read = memory
            .rewrite_with_mor(&rewritten.state, depth, decision)
            .map_err(LargeCoupledError::Memory)?;
        let evidence = memory
            .cycle_evidence()
            .last()
            .ok_or(LargeCoupledError::Depth)?
            .clone();
        let mut cycles = self.cycles.clone();
        Arc::make_mut(&mut cycles)
            .try_reserve(1)
            .map_err(|_| LargeCoupledError::Budget)?;
        Arc::make_mut(&mut cycles).push(LargeCoupledCycle {
            depth,
            selected: decision.selected_experts.clone(),
            post_expert: rewritten.state,
            output: read.state.clone(),
            memory: evidence,
        });
        self.experts = experts;
        self.memory = memory;
        self.cycles = cycles;
        Ok(SemanticRewriteStep {
            state: read.state,
            stability_blocked: rewritten.stability_blocked || read.stability_blocked,
        })
    }
}

//! Atomic wide-sparse ANF page execution and current-route Delta-KV recurrence.
//! No online memory update or physical resource-bound claim.
use crate::{
    delta_kv_dynamic::{
        DeltaKvDynamicCycleEvidence, DeltaKvDynamicRekeyError, DeltaKvDynamicRekeyKernel,
    },
    wide_anf_routed::{WideRoutedError, WideRoutedExperts},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState},
    PageId,
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideCoupledCycle<const S: usize> {
    pub depth: u32,
    pub selected: Vec<PageId>,
    pub post_expert: SemanticState<S>,
    pub output: SemanticState<S>,
    pub memory: DeltaKvDynamicCycleEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WideCoupledError<E> {
    Depth,
    Budget,
    Experts(WideRoutedError<E>),
    Memory(DeltaKvDynamicRekeyError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideAnfDeltaKvLoop<K, const S: usize, const Q: usize> {
    experts: WideRoutedExperts<K, S>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    max_cycles: u32,
    cycles: Arc<Vec<WideCoupledCycle<S>>>,
}

impl<K: SemanticRewriter<S> + Clone, const S: usize, const Q: usize> WideAnfDeltaKvLoop<K, S, Q> {
    pub fn new(
        experts: WideRoutedExperts<K, S>,
        memory: DeltaKvDynamicRekeyKernel<S, Q>,
        max_cycles: u32,
    ) -> Result<Self, WideCoupledError<K::Error>> {
        if max_cycles == 0
            || !experts.cycles().is_empty()
            || !memory.cycle_evidence().is_empty()
            || experts.routed().max_cycles() < max_cycles
        {
            return Err(WideCoupledError::Budget);
        }
        Ok(Self {
            experts,
            memory,
            max_cycles,
            cycles: Arc::new(Vec::new()),
        })
    }
    pub fn cycles(&self) -> &[WideCoupledCycle<S>] {
        &self.cycles
    }
    pub fn experts(&self) -> &WideRoutedExperts<K, S> {
        &self.experts
    }
    pub fn memory(&self) -> &DeltaKvDynamicRekeyKernel<S, Q> {
        &self.memory
    }
}

impl<K: SemanticRewriter<S> + Clone, const S: usize, const Q: usize> SemanticRewriter<S>
    for WideAnfDeltaKvLoop<K, S, Q>
{
    type Error = WideCoupledError<K::Error>;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(WideCoupledError::Depth);
        }
        if depth > self.max_cycles {
            return Err(WideCoupledError::Budget);
        }
        let mut experts = self.experts.clone();
        let rewritten = experts
            .rewrite(state, depth)
            .map_err(WideCoupledError::Experts)?;
        let decision = &experts
            .routed()
            .cycles()
            .last()
            .ok_or(WideCoupledError::Depth)?
            .route
            .decision;
        let mut memory = self.memory.clone();
        let read = memory
            .rewrite_with_mor(&rewritten.state, depth, decision)
            .map_err(WideCoupledError::Memory)?;
        let evidence = memory
            .cycle_evidence()
            .last()
            .ok_or(WideCoupledError::Depth)?
            .clone();
        let mut cycles = self.cycles.clone();
        Arc::make_mut(&mut cycles)
            .try_reserve(1)
            .map_err(|_| WideCoupledError::Budget)?;
        Arc::make_mut(&mut cycles).push(WideCoupledCycle {
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

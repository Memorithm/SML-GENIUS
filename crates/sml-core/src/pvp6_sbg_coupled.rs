//! Closed functional cycle: routed expert outputs -> current-state/current-route
//! Delta-KV projection/read -> next SBG state. No trained-utility claim.
use crate::{
    delta_kv_dynamic::{
        DeltaKvDynamicCycleEvidence, DeltaKvDynamicRekeyError, DeltaKvDynamicRekeyKernel,
    },
    delta_kv_semantic::DeltaKvSourceProvenance,
    pvp6_sbg_expert_loop::{RoutedExpertError, RoutedExpertSemantic},
    semantic::{SemanticRewriteStep, SemanticRewriter, SemanticState, SemanticStateDelta},
    PageId,
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgCoupledCycle<const S: usize> {
    pub depth: u32,
    pub selected: Vec<PageId>,
    pub post_expert: SemanticState<S>,
    pub output: SemanticState<S>,
    pub memory: DeltaKvDynamicCycleEvidence,
    pub provenance: Option<DeltaKvSourceProvenance>,
    pub delta: SemanticStateDelta,
    pub stability_blocked: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoupledSbgLoop<K, const S: usize, const D: usize, const Q: usize> {
    experts: RoutedExpertSemantic<K, S, D>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    cycles: Vec<SbgCoupledCycle<S>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoupledSbgError<E> {
    NotFresh,
    Depth,
    Allocation,
    Experts(RoutedExpertError<E>),
    Memory(DeltaKvDynamicRekeyError),
}
impl<K: SemanticRewriter<S> + Clone, const S: usize, const D: usize, const Q: usize>
    CoupledSbgLoop<K, S, D, Q>
{
    pub fn new(
        experts: RoutedExpertSemantic<K, S, D>,
        memory: DeltaKvDynamicRekeyKernel<S, Q>,
    ) -> Result<Self, CoupledSbgError<K::Error>> {
        if !experts.cycles().is_empty() || !memory.cycle_evidence().is_empty() {
            return Err(CoupledSbgError::NotFresh);
        }
        Ok(Self {
            experts,
            memory,
            cycles: vec![],
        })
    }
    #[must_use]
    pub fn experts(&self) -> &RoutedExpertSemantic<K, S, D> {
        &self.experts
    }
    #[must_use]
    pub fn memory(&self) -> &DeltaKvDynamicRekeyKernel<S, Q> {
        &self.memory
    }
    #[must_use]
    pub fn cycles(&self) -> &[SbgCoupledCycle<S>] {
        &self.cycles
    }
}
impl<K: SemanticRewriter<S> + Clone, const S: usize, const D: usize, const Q: usize>
    SemanticRewriter<S> for CoupledSbgLoop<K, S, D, Q>
{
    type Error = CoupledSbgError<K::Error>;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(CoupledSbgError::Depth);
        }
        let mut experts = self.experts.clone();
        let rewritten = experts
            .rewrite(state, depth)
            .map_err(CoupledSbgError::Experts)?;
        let decision = &experts
            .routed()
            .cycles()
            .last()
            .ok_or(CoupledSbgError::Depth)?
            .route
            .decision;
        let mut memory = self.memory.clone();
        let read = memory
            .rewrite_with_mor(&rewritten.state, depth, decision)
            .map_err(CoupledSbgError::Memory)?;
        let stability_blocked = rewritten.stability_blocked || read.stability_blocked;
        let evidence = SbgCoupledCycle {
            depth,
            selected: decision.selected_experts.clone(),
            post_expert: rewritten.state,
            output: read.state.clone(),
            memory: memory
                .cycle_evidence()
                .last()
                .ok_or(CoupledSbgError::Depth)?
                .clone(),
            provenance: memory.provenance().cloned(),
            delta: SemanticStateDelta::between(state, &read.state),
            stability_blocked,
        };
        self.cycles
            .try_reserve(1)
            .map_err(|_| CoupledSbgError::Allocation)?;
        self.cycles.push(evidence);
        self.experts = experts;
        self.memory = memory;
        Ok(SemanticRewriteStep {
            state: read.state,
            stability_blocked,
        })
    }
}
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;
    use crate::{
        delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_SCALE},
        delta_kv_recurrent::DeltaKvRecurrentPolicy,
        delta_kv_semantic::{DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector},
        mor::PreviousRoute,
        pvp6_sbg_expert_loop::fixtures::{self as expert, Identity},
    };
    pub fn projector() -> DeltaKvSemanticProjector<8> {
        DeltaKvSemanticProjector::new(
            DeltaKvSemanticProjectionPolicy {
                semantic_weight: 1,
                mor_weight: 32,
            },
            17,
        )
        .unwrap()
    }
    pub fn memory(invalid_provenance: bool) -> DeltaKvDynamicRekeyKernel<8, 8> {
        let state = SemanticState::zeroed().unwrap();
        let context = expert::context();
        let route = context
            .bridge
            .route(
                &state,
                &context.registry,
                &context.engine,
                &[],
                &PreviousRoute::default(),
            )
            .unwrap()
            .decision;
        let key = projector().project(&state, &route, None).unwrap();
        let mut memory = DeltaKvMemory::new(8, 1).unwrap();
        memory
            .update(
                &key.key_q12,
                &[2000],
                DELTA_KV_SCALE as u16,
                &mut DeltaKvCounters::default(),
            )
            .unwrap();
        DeltaKvDynamicRekeyKernel::new(
            memory,
            projector(),
            route,
            Some(DeltaKvSourceProvenance {
                semantic_record_id: 7,
                raw_segment_ids: if invalid_provenance {
                    vec![]
                } else {
                    vec![11, 13]
                },
            }),
            vec![1],
            DeltaKvRecurrentPolicy::default(),
        )
        .unwrap()
    }
    pub fn coupled() -> CoupledSbgLoop<Identity, 8, 64, 8> {
        CoupledSbgLoop::new(expert::kernel(8, expert::budget()), memory(false)).unwrap()
    }
}
#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::{
        pvp6_sbg_expert_loop::fixtures as expert,
        semantic::{run_semantic_recurrent, SemanticBitField},
        HaltingPolicy,
    };
    #[test]
    fn full_cycle_uses_post_expert_state_and_current_decision_in_every_key() {
        for routing in [false, true] {
            let mut state = SemanticState::zeroed().unwrap();
            if routing {
                state.routing = SemanticBitField::ones().unwrap();
            }
            let mut k = coupled();
            let mut expected_experts = k.experts().clone();
            let mut expected_memory = k.memory().clone();
            for depth in 1..=4 {
                let expected = expected_experts.rewrite(&state, depth).unwrap();
                let route = &expected_experts
                    .routed()
                    .cycles()
                    .last()
                    .unwrap()
                    .route
                    .decision;
                let key = projector().project(&expected.state, route, None).unwrap();
                let expected = expected_memory
                    .rewrite_with_mor(&expected.state, depth, route)
                    .unwrap();
                let result = k.rewrite(&state, depth).unwrap();
                assert_eq!(result, expected);
                assert_eq!(k.cycles().last().unwrap().memory.key_q12, key.key_q12);
                assert_eq!(k.experts(), &expected_experts);
                assert_eq!(k.memory(), &expected_memory);
                assert_eq!(
                    k.cycles()
                        .last()
                        .unwrap()
                        .provenance
                        .as_ref()
                        .unwrap()
                        .raw_segment_ids,
                    vec![11, 13]
                );
                state = result.state;
            }
        }
    }
    #[test]
    fn projection_failure_rolls_back_experts_routes_counters_and_all_traces() {
        let mut k = CoupledSbgLoop::new(expert::kernel(8, expert::budget()), memory(true)).unwrap();
        let before = k.clone();
        assert!(matches!(
            k.rewrite(&SemanticState::zeroed().unwrap(), 1),
            Err(CoupledSbgError::Memory(_))
        ));
        assert_eq!(k, before);
        assert_eq!(k.experts().routed().inner().calls, 0);
        assert_eq!(k.memory().counters().reads, 0);
    }
    #[test]
    fn native_runner_carries_final_memory_state_and_rejects_reused_depth() {
        let mut k = coupled();
        let result = run_semantic_recurrent(
            SemanticState::zeroed().unwrap(),
            &mut k,
            HaltingPolicy {
                min_depth: 1,
                max_depth: 8,
                stable_hamming_threshold: 0,
            },
        )
        .unwrap();
        assert_eq!(result.state, k.cycles().last().unwrap().output);
        assert_eq!(result.depth as usize, k.cycles().len());
        let before = k.clone();
        assert!(matches!(
            k.rewrite(&result.state, result.depth),
            Err(CoupledSbgError::Depth)
        ));
        assert_eq!(k, before);
    }
}

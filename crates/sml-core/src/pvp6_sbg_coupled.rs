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
use std::sync::Arc;
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
    cycles: Arc<Vec<SbgCoupledCycle<S>>>,
    budget: SbgMemoryWork,
    usage: SbgMemoryWork,
    per_cycle: SbgMemoryWork,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoupledSbgError<E> {
    NotFresh,
    Budget,
    Overflow,
    Accounting,
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
        let per_cycle = SbgMemoryWork::for_shape::<S, Q>(memory.value_dim())
            .ok_or(CoupledSbgError::Overflow)?;
        let budget = per_cycle
            .times(experts.routed().max_cycles())
            .ok_or(CoupledSbgError::Overflow)?;
        Ok(Self {
            experts,
            memory,
            cycles: Arc::new(vec![]),
            budget,
            usage: SbgMemoryWork::default(),
            per_cycle,
        })
    }
    /// Configure logical memory work before starting the trajectory.
    pub fn with_memory_budget(
        mut self,
        budget: SbgMemoryWork,
    ) -> Result<Self, CoupledSbgError<K::Error>> {
        if !self.cycles.is_empty() {
            return Err(CoupledSbgError::NotFresh);
        }
        self.budget = budget;
        Ok(self)
    }
    #[must_use]
    pub fn memory_usage(&self) -> SbgMemoryWork {
        self.usage
    }
    #[must_use]
    pub fn memory_budget(&self) -> SbgMemoryWork {
        self.budget
    }
    pub fn replace_expert(
        &mut self,
        id: PageId,
        expected_revision: u64,
        bank: crate::pvp6_executor::Pvp6Bank<D>,
    ) -> Result<u64, CoupledSbgError<K::Error>> {
        self.experts
            .replace_expert(id, expected_revision, bank)
            .map_err(CoupledSbgError::Experts)
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
        let usage = self
            .usage
            .checked_add(self.per_cycle)
            .ok_or(CoupledSbgError::Overflow)?;
        if !usage.within(self.budget) {
            return Err(CoupledSbgError::Budget);
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
        if evidence.memory.read_macs_delta != self.per_cycle.read_macs
            || evidence.memory.projection.semantic_coordinates_read
                != self.per_cycle.projected_coordinates
        {
            return Err(CoupledSbgError::Accounting);
        }
        Arc::make_mut(&mut self.cycles)
            .try_reserve(1)
            .map_err(|_| CoupledSbgError::Allocation)?;
        Arc::make_mut(&mut self.cycles).push(evidence);
        self.experts = experts;
        self.memory = memory;
        self.usage = usage;
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

/// Logical memory work. Retained units charge one unit per state u64 word and
/// one per key/read scalar, NOT bytes. Excludes route/provenance metadata, other
/// stage logs and temporary clones. Host resource enforcement belongs to RemoteOps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SbgMemoryWork {
    pub cycles: u32,
    pub read_macs: u64,
    pub projected_coordinates: u64,
    pub retained_units: u64,
}
impl SbgMemoryWork {
    fn for_shape<const S: usize, const Q: usize>(values: usize) -> Option<Self> {
        let s = u64::try_from(S).ok()?;
        let q = u64::try_from(Q).ok()?;
        let v = u64::try_from(values).ok()?;
        Some(Self {
            cycles: 1,
            read_macs: q.checked_mul(v)?,
            projected_coordinates: s.checked_mul(4)?,
            retained_units: s
                .div_ceil(64)
                .checked_mul(8)?
                .checked_add(q)?
                .checked_add(v)?,
        })
    }
    fn times(self, n: u32) -> Option<Self> {
        Some(Self {
            cycles: self.cycles.checked_mul(n)?,
            read_macs: self.read_macs.checked_mul(u64::from(n))?,
            projected_coordinates: self.projected_coordinates.checked_mul(u64::from(n))?,
            retained_units: self.retained_units.checked_mul(u64::from(n))?,
        })
    }
    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            cycles: self.cycles.checked_add(other.cycles)?,
            read_macs: self.read_macs.checked_add(other.read_macs)?,
            projected_coordinates: self
                .projected_coordinates
                .checked_add(other.projected_coordinates)?,
            retained_units: self.retained_units.checked_add(other.retained_units)?,
        })
    }
    fn within(self, cap: Self) -> bool {
        self.cycles <= cap.cycles
            && self.read_macs <= cap.read_macs
            && self.projected_coordinates <= cap.projected_coordinates
            && self.retained_units <= cap.retained_units
    }
}
#[cfg(test)]
mod memory_budget_tests {
    use super::fixtures::coupled;
    use super::*;
    #[test]
    fn inclusive_work_caps_match_actual_projection_and_reads() {
        let cap = SbgMemoryWork {
            cycles: 2,
            read_macs: 16,
            projected_coordinates: 64,
            retained_units: 34,
        };
        let mut k = coupled().with_memory_budget(cap).unwrap();
        let mut state = SemanticState::zeroed().unwrap();
        for depth in 1..=2 {
            state = k.rewrite(&state, depth).unwrap().state;
        }
        assert_eq!(k.memory_usage(), cap);
        assert_eq!(k.memory().counters().read_macs, 16);
        let before = k.clone();
        assert_eq!(k.rewrite(&state, 3), Err(CoupledSbgError::Budget));
        assert_eq!(k, before);
        assert!(matches!(
            k.clone().with_memory_budget(cap),
            Err(CoupledSbgError::NotFresh)
        ));
    }
    #[test]
    fn every_insufficient_axis_rejects_before_inner_work() {
        let valid = SbgMemoryWork {
            cycles: 1,
            read_macs: 8,
            projected_coordinates: 32,
            retained_units: 17,
        };
        for cap in [
            SbgMemoryWork { cycles: 0, ..valid },
            SbgMemoryWork {
                read_macs: 7,
                ..valid
            },
            SbgMemoryWork {
                projected_coordinates: 31,
                ..valid
            },
            SbgMemoryWork {
                retained_units: 16,
                ..valid
            },
        ] {
            let mut k = coupled().with_memory_budget(cap).unwrap();
            let before = k.clone();
            assert_eq!(
                k.rewrite(&SemanticState::zeroed().unwrap(), 1),
                Err(CoupledSbgError::Budget)
            );
            assert_eq!(k, before);
            assert_eq!(k.experts().routed().inner().calls, 0);
        }
    }
    #[test]
    fn checked_counter_overflow_is_not_silent_saturation() {
        let mut k = coupled();
        k.usage.read_macs = u64::MAX;
        let before = k.clone();
        assert_eq!(
            k.rewrite(&SemanticState::zeroed().unwrap(), 1),
            Err(CoupledSbgError::Overflow)
        );
        assert_eq!(k, before);
    }
}

#[cfg(test)]
mod expert_update_tests {
    use super::fixtures::coupled;
    use super::*;
    use crate::pvp6_executor::{Pvp6Bank, Pvp6Gate};
    #[test]
    fn next_cycle_executes_new_coefficients_without_rewriting_old_evidence() {
        let mut k = coupled();
        let state = k
            .rewrite(&SemanticState::zeroed().unwrap(), 1)
            .unwrap()
            .state;
        let old_cycle = k.cycles()[0].clone();
        let old_expert = k.experts().cycles()[0].clone();
        let old_work = k.memory_usage();
        assert_eq!(
            k.replace_expert(
                PageId(0),
                0,
                Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0, 1]).unwrap()]).unwrap()
            )
            .unwrap(),
            1
        );
        assert_eq!(k.memory_usage(), old_work);
        k.rewrite(&state, 2).unwrap();
        assert_eq!(k.cycles()[0], old_cycle);
        assert_eq!(k.experts().cycles()[0], old_expert);
        assert_eq!(k.experts().cycles()[1].pages[0].revision, 1);
        assert!(!k.cycles()[1].post_expert.value.get(0).unwrap());
        let before = k.clone();
        let bank = k.experts().atlas().page(PageId(0)).unwrap().bank().clone();
        assert!(k.replace_expert(PageId(0), 0, bank.clone()).is_err());
        assert_eq!(k, before);
        assert!(k.replace_expert(PageId(99), 0, bank).is_err());
        assert_eq!(k, before);
    }
}

//! Native end-to-end composition: sparse ANF recurrence -> SBG/MOR routing ->
//! selected SBG expert bodies -> route-conditioned Delta-KV -> next semantic state.
//! This adapter uses the existing transactional coupled kernel; it does not
//! replace the SBG page executor with a large-ANF page implementation.
use crate::{
    anf_large_bank::LargeAnfBank,
    anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError},
    delta_kv_dynamic::DeltaKvDynamicRekeyKernel,
    pvp6_sbg_atlas::{SbgExpertAtlas, SbgExpertWork},
    pvp6_sbg_coupled::{CoupledSbgError, CoupledSbgLoop},
    pvp6_sbg_expert_loop::{RoutedExpertError, RoutedExpertSemantic},
    pvp6_sbg_routing::{RoutedSemantic, RoutedSemanticError, SbgRoutingContext},
};

pub type UnifiedAnfSbgLoop<const S: usize, const D: usize, const Q: usize> =
    CoupledSbgLoop<LargeBankRecurrent<S>, S, D, Q>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnifiedAnfSbgError {
    Anf(LargeRecurrentError),
    Routing(RoutedSemanticError<LargeRecurrentError>),
    Experts(RoutedExpertError<LargeRecurrentError>),
    Coupled(CoupledSbgError<LargeRecurrentError>),
}

/// Construct one bounded recurrent execution pipeline from native components.
/// Every cycle uses ANF-derived state for MOR, executes only selected SBG
/// expert pages, and then projects the resulting state and route into Delta-KV.
pub fn unified_anf_sbg_loop<const S: usize, const D: usize, const Q: usize>(
    anf: LargeAnfBank,
    routing: SbgRoutingContext<S>,
    atlas: SbgExpertAtlas<S, D>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    max_cycles: u32,
    expert_budget: SbgExpertWork,
) -> Result<UnifiedAnfSbgLoop<S, D, Q>, UnifiedAnfSbgError> {
    let recurrent = LargeBankRecurrent::<S>::new(anf, max_cycles as usize)
        .map_err(UnifiedAnfSbgError::Anf)?;
    let routed = RoutedSemantic::new(recurrent, routing, max_cycles)
        .map_err(UnifiedAnfSbgError::Routing)?;
    let experts = RoutedExpertSemantic::new(routed, atlas, expert_budget)
        .map_err(UnifiedAnfSbgError::Experts)?;
    CoupledSbgLoop::new(experts, memory).map_err(UnifiedAnfSbgError::Coupled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        anf_large::LargeAnfGate,
        pvp6_sbg_coupled::{fixtures::memory, SbgMemoryWork},
        pvp6_sbg_expert_loop::fixtures as expert,
        semantic::{SemanticRewriter, SemanticState},
    };

    fn pipeline() -> UnifiedAnfSbgLoop<8, 64, 8> {
        let bank = LargeAnfBank::new(
            (0..8)
                .map(|i| LargeAnfGate::new(20, vec![1_u32 << i]).unwrap())
                .collect(),
        )
        .unwrap();
        let atlas = SbgExpertAtlas::new(
            vec![expert::page(0, &[0]), expert::page(1, &[1])],
            2,
        )
        .unwrap();
        unified_anf_sbg_loop(
            bank,
            expert::context(),
            atlas,
            memory(false),
            8,
            expert::budget(),
        )
        .unwrap()
    }

    #[test]
    fn one_cycle_commits_all_four_stages_with_shared_depth() {
        let mut kernel = pipeline();
        let state = SemanticState::<8>::zeroed().unwrap();
        let step = kernel.rewrite(&state, 1).unwrap();
        assert_eq!(kernel.cycles().len(), 1);
        assert_eq!(kernel.cycles()[0].depth, 1);
        assert_eq!(kernel.experts().cycles().len(), 1);
        assert_eq!(kernel.experts().routed().cycles().len(), 1);
        assert_eq!(kernel.experts().routed().inner().history().len(), 1);
        assert_eq!(kernel.memory().cycle_evidence().len(), 1);
        assert_eq!(kernel.cycles()[0].output, step.state);
        assert!(!kernel.cycles()[0].selected.is_empty());
    }

    #[test]
    fn memory_budget_rejection_does_not_commit_any_stage() {
        let mut kernel = pipeline()
            .with_memory_budget(SbgMemoryWork::default())
            .unwrap();
        let before = kernel.clone();
        let state = SemanticState::<8>::zeroed().unwrap();
        assert!(matches!(
            kernel.rewrite(&state, 1),
            Err(CoupledSbgError::Budget)
        ));
        assert_eq!(kernel, before);
        assert!(kernel.experts().routed().inner().history().is_empty());
    }
}

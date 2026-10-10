//! Native end-to-end composition: sparse ANF recurrence -> SBG/MOR routing ->
//! selected SBG expert bodies -> route-conditioned Delta-KV -> next semantic state.
//! This adapter uses the existing transactional coupled kernel; it does not
//! replace the SBG page executor with a large-ANF page implementation.
use crate::{
    anf_large_bank::LargeAnfBank,
    anf_large_recurrent::{LargeBankRecurrent, LargeRecurrentError, LargeRoutingProjectionError},
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
    Projection(LargeRoutingProjectionError),
    Routing(RoutedSemanticError<LargeRecurrentError>),
    Experts(RoutedExpertError<LargeRecurrentError>),
    Coupled(CoupledSbgError<LargeRecurrentError>),
}

/// Construct the original unprojected control pipeline. ANF changes `value`,
/// while MOR reads the unchanged `routing` field. This is execution composition,
/// not a causal ANF-to-routing projection. Selected expert bodies still execute
/// and the resulting current state/route are projected into Delta-KV.
pub fn unified_anf_sbg_loop<const S: usize, const D: usize, const Q: usize>(
    anf: LargeAnfBank,
    routing: SbgRoutingContext<S>,
    atlas: SbgExpertAtlas<S, D>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    max_cycles: u32,
    expert_budget: SbgExpertWork,
) -> Result<UnifiedAnfSbgLoop<S, D, Q>, UnifiedAnfSbgError> {
    let recurrent =
        LargeBankRecurrent::<S>::new(anf, max_cycles as usize).map_err(UnifiedAnfSbgError::Anf)?;
    couple_recurrent(recurrent, routing, atlas, memory, max_cycles, expert_budget)
}

/// Opt in to an explicit post-ANF value-bit -> routing-bit projection before MOR.
/// Assignments are validated before the trajectory starts. The original
/// constructor remains unchanged as an unprojected control.
pub fn unified_anf_sbg_loop_with_routing_projection<
    const S: usize,
    const D: usize,
    const Q: usize,
>(
    anf: LargeAnfBank,
    routing: SbgRoutingContext<S>,
    atlas: SbgExpertAtlas<S, D>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    max_cycles: u32,
    expert_budget: SbgExpertWork,
    projection: Vec<(usize, usize)>,
) -> Result<UnifiedAnfSbgLoop<S, D, Q>, UnifiedAnfSbgError> {
    let recurrent = LargeBankRecurrent::<S>::new(anf, max_cycles as usize)
        .map_err(UnifiedAnfSbgError::Anf)?
        .with_routing_projection(projection)
        .map_err(UnifiedAnfSbgError::Projection)?;
    couple_recurrent(recurrent, routing, atlas, memory, max_cycles, expert_budget)
}

fn couple_recurrent<const S: usize, const D: usize, const Q: usize>(
    recurrent: LargeBankRecurrent<S>,
    routing: SbgRoutingContext<S>,
    atlas: SbgExpertAtlas<S, D>,
    memory: DeltaKvDynamicRekeyKernel<S, Q>,
    max_cycles: u32,
    expert_budget: SbgExpertWork,
) -> Result<UnifiedAnfSbgLoop<S, D, Q>, UnifiedAnfSbgError> {
    let routed =
        RoutedSemantic::new(recurrent, routing, max_cycles).map_err(UnifiedAnfSbgError::Routing)?;
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
        let atlas =
            SbgExpertAtlas::new(vec![expert::page(0, &[0]), expert::page(1, &[1])], 2).unwrap();
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

#[cfg(test)]
mod projection_tests {
    use super::*;
    use crate::{
        anf_large::LargeAnfGate,
        pvp6_sbg_coupled::{fixtures::memory, fixtures::projector, SbgMemoryWork},
        pvp6_sbg_expert_loop::fixtures as expert,
        semantic::{SemanticBitField, SemanticRewriter, SemanticState},
        PageId,
    };

    fn bank() -> LargeAnfBank {
        LargeAnfBank::new(
            (0..8)
                .map(|bit| {
                    let mut terms = vec![1_u32 << bit];
                    if bit == 7 {
                        terms.push(0);
                    }
                    LargeAnfGate::new(20, terms).unwrap()
                })
                .collect(),
        )
        .unwrap()
    }

    fn atlas() -> SbgExpertAtlas<8, 64> {
        SbgExpertAtlas::new(vec![expert::page(0, &[0]), expert::page(1, &[1])], 2).unwrap()
    }

    fn projected(invalid_memory: bool) -> UnifiedAnfSbgLoop<8, 64, 8> {
        unified_anf_sbg_loop_with_routing_projection(
            bank(),
            expert::context(),
            atlas(),
            memory(invalid_memory),
            8,
            expert::budget(),
            (0..8).map(|destination| (7, destination)).collect(),
        )
        .unwrap()
    }

    #[test]
    fn projection_uses_post_anf_bits_and_preserves_unassigned_fields() {
        let mut kernel = LargeBankRecurrent::<8>::new(bank(), 8)
            .unwrap()
            .with_routing_projection(vec![(7, 0), (7, 5)])
            .unwrap();
        let mut initial = SemanticState::zeroed().unwrap();
        initial.routing.set(2, true).unwrap();
        initial.meaning_mask.set(3, true).unwrap();
        initial.ambiguity.set(4, true).unwrap();
        let result = kernel.rewrite(&initial, 1).unwrap();
        let mut expected = initial.clone();
        expected.value.set(7, true).unwrap();
        expected.routing.set(0, true).unwrap();
        expected.routing.set(5, true).unwrap();
        assert_eq!(result.state, expected);
        assert_eq!(kernel.history(), &[vec![128]]);
        assert_eq!(kernel.routing_projection(), &[(7, 0), (7, 5)]);
        let before = kernel.clone();
        assert_eq!(kernel.rewrite(&result.state, 1), Err(LargeRecurrentError::Depth));
        assert_eq!(kernel, before);
    }

    #[test]
    fn invalid_projection_and_late_reconfiguration_are_rejected() {
        let fresh = LargeBankRecurrent::<8>::new(bank(), 8).unwrap();
        let cases = [
            (vec![], LargeRoutingProjectionError::Empty),
            (vec![(8, 0)], LargeRoutingProjectionError::Width),
            (vec![(0, 8)], LargeRoutingProjectionError::Width),
            (
                vec![(0, 1), (7, 1)],
                LargeRoutingProjectionError::DuplicateDestination(1),
            ),
            (
                vec![(0, 0); 9],
                LargeRoutingProjectionError::TooManyDestinations,
            ),
        ];
        for (plan, error) in cases {
            assert_eq!(fresh.clone().with_routing_projection(plan), Err(error));
        }
        assert!(fresh.history().is_empty());
        assert!(fresh.routing_projection().is_empty());
        let mut active = fresh.with_routing_projection(vec![(7, 0)]).unwrap();
        active.rewrite(&SemanticState::zeroed().unwrap(), 1).unwrap();
        assert_eq!(
            active.with_routing_projection(vec![(7, 1)]),
            Err(LargeRoutingProjectionError::NotFresh)
        );
    }

    #[test]
    fn anf_projection_changes_executed_routes_and_current_route_memory_keys() {
        let mut kernel = projected(false);
        let mut control = unified_anf_sbg_loop(
            bank(),
            expert::context(),
            atlas(),
            memory(false),
            8,
            expert::budget(),
        )
        .unwrap();
        let mut state = SemanticState::zeroed().unwrap();
        let mut control_state = state.clone();
        for depth in 1..=4 {
            let output = kernel.rewrite(&state, depth).unwrap();
            let control_output = control.rewrite(&control_state, depth).unwrap();
            let expected = PageId(u32::from(depth % 2 == 1));
            let cycle = kernel.cycles().last().unwrap();
            let route = &kernel.experts().routed().cycles().last().unwrap().route.decision;
            let control_route = &control.experts().routed().cycles().last().unwrap().route.decision;
            assert_eq!(cycle.selected, vec![expected]);
            assert_eq!(control_route.selected_experts, vec![PageId(0)]);
            let executed = kernel.experts().cycles().last().unwrap();
            assert_eq!(executed.pages.len(), 1);
            assert_eq!(executed.pages[0].page, expected);
            assert_eq!(kernel.memory().mor_decision(), route);
            let key = projector()
                .project(&cycle.post_expert, route, cycle.provenance.as_ref())
                .unwrap();
            assert_eq!(cycle.memory.key_q12, key.key_q12);
            if depth == 1 {
                let frozen = projector()
                    .project(&cycle.post_expert, control_route, cycle.provenance.as_ref())
                    .unwrap();
                assert_ne!(cycle.memory.key_q12, frozen.key_q12);
            }
            let expected_routing = if expected == PageId(1) {
                SemanticBitField::ones().unwrap()
            } else {
                SemanticBitField::zeroed().unwrap()
            };
            assert_eq!(output.state.routing, expected_routing);
            assert_eq!(control_output.state.routing, SemanticBitField::zeroed().unwrap());
            assert_eq!(cycle.output, output.state);
            assert_eq!(kernel.experts().routed().inner().history().len(), depth as usize);
            state = output.state;
            control_state = control_output.state;
        }
        assert_eq!(kernel.experts().usage().pages, 4);
        assert_eq!(kernel.memory().cycle_evidence().len(), 4);
    }

    #[test]
    fn memory_failure_after_projected_routing_rolls_back_every_owned_stage() {
        let mut kernel = projected(true);
        let before = kernel.clone();
        assert!(matches!(
            kernel.rewrite(&SemanticState::zeroed().unwrap(), 1),
            Err(CoupledSbgError::Memory(_))
        ));
        assert_eq!(kernel, before);
        assert!(kernel.experts().routed().inner().history().is_empty());
        assert!(kernel.experts().routed().cycles().is_empty());
        assert!(kernel.memory().cycle_evidence().is_empty());
    }

    #[test]
    fn late_budget_rejection_preserves_the_committed_projected_route() {
        let kernel = projected(false);
        let budget = SbgMemoryWork {
            cycles: 1,
            ..kernel.memory_budget()
        };
        let mut kernel = kernel.with_memory_budget(budget).unwrap();
        let output = kernel.rewrite(&SemanticState::zeroed().unwrap(), 1).unwrap();
        let before = kernel.clone();
        assert!(matches!(
            kernel.rewrite(&output.state, 2),
            Err(CoupledSbgError::Budget)
        ));
        assert_eq!(kernel, before);
        assert_eq!(kernel.cycles()[0].selected, vec![PageId(1)]);
        assert_eq!(kernel.experts().routed().inner().history().len(), 1);
        assert_eq!(kernel.memory().cycle_evidence().len(), 1);
    }
}

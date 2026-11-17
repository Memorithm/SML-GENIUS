//! Transactional state rewrite followed by the existing native SBG/MOR engine.
//! Routing evidence is retained; this does not execute an expert's page body.
use crate::{
    mor::{ExpertRuntimeHint, MixtureOfRouters, PreviousRoute},
    mor_registry::MorRouterRegistry,
    semantic::{
        rae_learned_route::{RaeReplayPolicy, RaeTrainedEventRouter},
        rae_qualification::{
            dispatch_native_shadow, RaeDispatchMode, RaeNativeDispatch, RaeQualificationError,
            RaeShadowQualification,
        },
        rae_runtime_capture::{RaeNativeCaptureError, RaeNativeRuntimeCapture},
        SemanticMorBridge, SemanticMorError, SemanticMorRoute, SemanticRewriteStep,
        SemanticRewriter, SemanticState,
    },
};
use std::sync::Arc;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgRoutingContext<const S: usize> {
    pub bridge: SemanticMorBridge<S>,
    pub registry: MorRouterRegistry,
    pub engine: MixtureOfRouters,
    pub runtime_hints: Vec<ExpertRuntimeHint>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgRouteCycle {
    pub depth: u32,
    pub previous: PreviousRoute,
    pub route: SemanticMorRoute,
}
#[derive(Debug, Clone, PartialEq, Eq)]
/// Frozen shadow configuration. Always observational: native MOR stays active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaeShadowSession {
    trained: RaeTrainedEventRouter,
    qualification: RaeShadowQualification,
    replay: RaeReplayPolicy,
    expected_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RaeShadowSetupError {
    Observation(RaeNativeCaptureError),
    Qualification(RaeQualificationError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutedSemantic<K, const S: usize> {
    inner: K,
    context: SbgRoutingContext<S>,
    previous: PreviousRoute,
    max_cycles: u32,
    cycles: Arc<Vec<SbgRouteCycle>>,
    rae_observer: Option<RaeNativeRuntimeCapture<S>>,
    rae_shadow: Option<RaeShadowSession>,
    rae_shadow_evidence: Vec<RaeNativeDispatch>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutedSemanticError<E> {
    Limit,
    Depth,
    Allocation,
    Kernel(E),
    Routing(SemanticMorError),
    Rae(RaeNativeCaptureError),
    RaeQualification(RaeQualificationError),
}
impl<K: SemanticRewriter<S> + Clone, const S: usize> RoutedSemantic<K, S> {
    pub fn new(
        inner: K,
        context: SbgRoutingContext<S>,
        max_cycles: u32,
    ) -> Result<Self, RoutedSemanticError<K::Error>> {
        if max_cycles == 0 {
            return Err(RoutedSemanticError::Limit);
        }
        Ok(Self {
            inner,
            context,
            previous: PreviousRoute::default(),
            max_cycles,
            cycles: Arc::new(Vec::new()),
            rae_observer: None,
            rae_shadow: None,
            rae_shadow_evidence: Vec::new(),
        })
    }
    #[must_use]
    pub fn max_cycles(&self) -> u32 {
        self.max_cycles
    }

    #[must_use]
    pub fn inner(&self) -> &K {
        &self.inner
    }
    #[must_use]
    pub fn context(&self) -> &SbgRoutingContext<S> {
        &self.context
    }
    #[must_use]
    pub fn cycles(&self) -> &[SbgRouteCycle] {
        &self.cycles
    }
    #[must_use]
    pub fn previous(&self) -> &PreviousRoute {
        &self.previous
    }

    /// Opt-in passive observation at the actual SBG→MOR decision boundary.
    /// Only a fresh kernel may enable it; no routing decision is substituted.
    pub fn enable_rae_observation(
        &mut self,
        baseline: &SemanticState<S>,
    ) -> Result<(), RaeNativeCaptureError> {
        if !self.cycles.is_empty() || self.rae_observer.is_some() {
            return Err(RaeNativeCaptureError::TooManyFrames);
        }
        let expert_count = self.context.registry.policy().expert_count as usize;
        self.rae_observer = Some(RaeNativeRuntimeCapture::new(baseline, 0, expert_count)?);
        Ok(())
    }

    /// Enable a learned candidate only in observational mode.
    /// Any mismatch rejects enablement before touching the live kernel.
    pub fn enable_rae_shadow(
        &mut self,
        baseline: &SemanticState<S>,
        trained: RaeTrainedEventRouter,
        qualification: RaeShadowQualification,
        replay: RaeReplayPolicy,
        expected_revision: &str,
    ) -> Result<(), RaeShadowSetupError> {
        if expected_revision != qualification.pinned_code_revision
            || trained.training_dataset_id() != qualification.train_case_id
        {
            return Err(RaeShadowSetupError::Qualification(
                RaeQualificationError::MismatchedModelRevision,
            ));
        }
        if trained.feature_width() != S
            || trained.expert_count() != self.context.registry.policy().expert_count as usize
        {
            return Err(RaeShadowSetupError::Qualification(
                RaeQualificationError::InvalidModelGeometry,
            ));
        }
        self.enable_rae_observation(baseline)
            .map_err(RaeShadowSetupError::Observation)?;
        self.rae_shadow = Some(RaeShadowSession {
            trained,
            qualification,
            replay,
            expected_revision: expected_revision.to_owned(),
        });
        Ok(())
    }

    #[must_use]
    pub fn rae_observer(&self) -> Option<&RaeNativeRuntimeCapture<S>> {
        self.rae_observer.as_ref()
    }

    #[must_use]
    pub fn rae_shadow_evidence(&self) -> &[RaeNativeDispatch] {
        &self.rae_shadow_evidence
    }
}
impl<K: SemanticRewriter<S> + Clone, const S: usize> SemanticRewriter<S> for RoutedSemantic<K, S> {
    type Error = RoutedSemanticError<K::Error>;
    fn rewrite(
        &mut self,
        state: &SemanticState<S>,
        depth: u32,
    ) -> Result<SemanticRewriteStep<S>, Self::Error> {
        if usize::try_from(depth).ok() != self.cycles.len().checked_add(1) {
            return Err(RoutedSemanticError::Depth);
        }
        if depth > self.max_cycles {
            return Err(RoutedSemanticError::Limit);
        }
        let mut inner = self.inner.clone();
        let step = inner
            .rewrite(state, depth)
            .map_err(RoutedSemanticError::Kernel)?;
        let route = self
            .context
            .bridge
            .route(
                &step.state,
                &self.context.registry,
                &self.context.engine,
                &self.context.runtime_hints,
                &self.previous,
            )
            .map_err(RoutedSemanticError::Routing)?;
        let previous = PreviousRoute {
            primary: route.decision.selected_experts.first().copied(),
            selected: route.decision.selected_experts.clone(),
        };
        // Observe the already-computed native decision, on a tentative
        // observer snapshot. This NEVER calls MOR again or changes selection.
        let mut next_observer = self.rae_observer.clone();
        if let Some(observer) = &mut next_observer {
            observer
                .observe_native(
                    u64::from(depth),
                    &step.state,
                    &route.decision.selected_experts,
                    !self.previous.selected.is_empty(),
                )
                .map_err(RoutedSemanticError::Rae)?;
        }
        let candidate_evidence = if let Some(shadow) = &self.rae_shadow {
            let frame = next_observer
                .as_ref()
                .and_then(|observer| observer.frames().last())
                .ok_or(RoutedSemanticError::RaeQualification(
                    RaeQualificationError::EmptyNativeReference,
                ))?;
            Some(
                dispatch_native_shadow(
                    &shadow.expected_revision,
                    &shadow.qualification,
                    &shadow.trained,
                    frame,
                    RaeDispatchMode::Shadow,
                    shadow.replay,
                )
                .map_err(RoutedSemanticError::RaeQualification)?,
            )
        } else {
            None
        };
        if candidate_evidence.is_some() {
            self.rae_shadow_evidence
                .try_reserve(1)
                .map_err(|_| RoutedSemanticError::Allocation)?;
        }
        Arc::make_mut(&mut self.cycles)
            .try_reserve(1)
            .map_err(|_| RoutedSemanticError::Allocation)?;
        Arc::make_mut(&mut self.cycles).push(SbgRouteCycle {
            depth,
            previous: self.previous.clone(),
            route,
        });
        self.inner = inner;
        self.previous = previous;
        self.rae_observer = next_observer;
        if let Some(evidence) = candidate_evidence {
            // Shadow decisions never replace `route.decision` above.
            self.rae_shadow_evidence.push(evidence);
        }
        Ok(step)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mor::{MorPolicy, RouterId},
        mor_registry::{MorRegistryPolicy, RouterSpecialization},
        pascal_vector_projection::Pvp6PopulationProfile,
        pvp6_executor::{Pvp6Bank, Pvp6Gate},
        pvp6_sbg::{SbgAnfPlan, SbgCoordinate, SbgField},
        pvp6_sbg_kernel::SbgAnfKernel,
        semantic::{
            run_semantic_recurrent, SemanticBitField, SemanticExpertSignature, SemanticMorTopK,
            SemanticRouterSignature,
        },
        HaltingPolicy, PageId,
    };
    fn kernel() -> SbgAnfKernel<8, 64> {
        let plan = SbgAnfPlan::new(
            vec![(0..6)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Value,
                    bit,
                })
                .collect()],
            (0..8)
                .map(|bit| SbgCoordinate {
                    field: SbgField::Routing,
                    bit,
                })
                .collect(),
        )
        .unwrap();
        SbgAnfKernel::new(
            Pvp6Bank::new(vec![Pvp6Gate::from_monomials(&[0]).unwrap(); 8]).unwrap(),
            plan,
            Pvp6PopulationProfile::AffineUniform,
            8,
        )
        .unwrap()
    }
    fn context(missing: bool) -> SbgRoutingContext<8> {
        let registry = MorRouterRegistry::new(
            vec![RouterSpecialization {
                router_id: RouterId(0),
                experts: vec![PageId(0), PageId(1)],
            }],
            MorRegistryPolicy {
                expert_count: 2,
                max_routers: 1,
                max_edges: 2,
                max_fanout: 2,
                require_full_coverage: true,
            },
        )
        .unwrap();
        let mut signatures = vec![SemanticExpertSignature {
            page_id: PageId(0),
            signature: SemanticBitField::zeroed().unwrap(),
        }];
        if !missing {
            signatures.push(SemanticExpertSignature {
                page_id: PageId(1),
                signature: SemanticBitField::ones().unwrap(),
            });
        }
        let bridge = SemanticMorBridge::new(
            vec![SemanticRouterSignature {
                router_id: RouterId(0),
                signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).unwrap(),
            }],
            signatures,
            SemanticMorTopK::One,
        )
        .unwrap();
        SbgRoutingContext {
            bridge,
            registry,
            engine: MixtureOfRouters::new(MorPolicy::default()).unwrap(),
            runtime_hints: vec![],
        }
    }
    #[test]
    fn actual_native_mor_observes_new_routing_and_retains_previous_route() {
        let ctx = context(false);
        let state = SemanticState::zeroed().unwrap();
        let before = ctx
            .bridge
            .route(
                &state,
                &ctx.registry,
                &ctx.engine,
                &[],
                &PreviousRoute::default(),
            )
            .unwrap();
        assert_eq!(before.decision.selected_experts, vec![PageId(0)]);
        let mut routed = RoutedSemantic::new(kernel(), ctx, 8).unwrap();
        let out = run_semantic_recurrent(
            state,
            &mut routed,
            HaltingPolicy {
                min_depth: 1,
                max_depth: 8,
                stable_hamming_threshold: 0,
            },
        )
        .unwrap();
        assert_eq!(out.depth, 2);
        assert_eq!(
            routed.cycles()[0].route.decision.selected_experts,
            vec![PageId(1)]
        );
        assert_eq!(routed.cycles()[1].previous.primary, Some(PageId(1)));
        assert_eq!(routed.inner().cycles().len(), 2);
    }
    #[test]
    fn downstream_route_failure_rolls_back_anf_trace_and_state() {
        let mut routed = RoutedSemantic::new(kernel(), context(true), 8).unwrap();
        let before = routed.clone();
        let state = SemanticState::zeroed().unwrap();
        assert!(matches!(
            routed.rewrite(&state, 1),
            Err(RoutedSemanticError::Routing(_))
        ));
        assert_eq!(routed, before);
        assert_eq!(state.routing.count_ones(), 0);
    }
    #[test]
    fn duplicate_depth_or_limit_cannot_add_route_or_execute_kernel() {
        let mut routed = RoutedSemantic::new(kernel(), context(false), 1).unwrap();
        let state = SemanticState::zeroed().unwrap();
        routed.rewrite(&state, 1).unwrap();
        let before = routed.clone();
        assert!(matches!(
            routed.rewrite(&state, 1),
            Err(RoutedSemanticError::Depth)
        ));
        assert!(matches!(
            routed.rewrite(&state, 2),
            Err(RoutedSemanticError::Limit)
        ));
        assert_eq!(routed, before);
    }
}

//! Model-owned export of sampled SBG routing states and native MOR decisions.
//!
//! Research-only evidence bridge to counterfactual event/page-policy studies.
//! This MUST NOT gate or skip native MOR, expert execution, recurrent depth,
//! memory reads, or attention: SML has no native Transformer Q/K/V semantics.

use core::fmt;

use crate::{
    mor::{ExpertRuntimeHint, PreviousRoute},
    semantic::{
        rising_edge::{SbgNativeRouteContext, SbgRisingMorError, SbgRisingMorProbe},
        SemanticState,
    },
    PageId,
};

pub const SBG_RAE_MAX_EXPERTS: usize = 65_536;
pub const SBG_RAE_MAX_FRAMES: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgRaeTraceError {
    InvalidExpertCapacity,
    ExpertCapacityChanged { expected: usize, actual: usize },
    TooManyFrames,
    Allocation,
    NativeRoute(SbgRisingMorError),
    SelectedPageOutOfRange(PageId),
    DuplicateSelectedPage(PageId),
}

impl fmt::Display for SbgRaeTraceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for SbgRaeTraceError {}

impl From<SbgRisingMorError> for SbgRaeTraceError {
    fn from(error: SbgRisingMorError) -> Self {
        Self::NativeRoute(error)
    }
}

/// One native SML reference row. `required_pages_words` represents the
/// *actual selected MOR experts*, not numerical softmax-relevant K/V pages.
/// This row does not imply native model quality or edge-routing correctness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgRaeNativeFrame {
    pub cycle: u64,
    pub routing_words: Vec<u64>,
    pub required_pages_words: Vec<u64>,
    pub selected_experts: Vec<PageId>,
    pub rising_routing_bits: u64,
    pub falling_routing_bits: u64,
    pub active_routing_bits: u64,
    /// True on explicitly supplied context changes or locally observed
    /// V/M/A, prior-route or runtime-hint changes.
    pub context_invalidated: bool,
}

/// SML-owned trace builder using the genuine SemanticMorBridge and MOR engine.
/// Ingest may allocate to retain research evidence. It is not a hot inference
/// path, not a file format and not a dependency on FLAT-ATTENTION.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgRaeNativeTrace<const BITS: usize> {
    probe: SbgRisingMorProbe<BITS>,
    previous_state: SemanticState<BITS>,
    previous_hints: Option<Vec<ExpertRuntimeHint>>,
    previous_route_input: Option<PreviousRoute>,
    expert_count: usize,
    frames: Vec<SbgRaeNativeFrame>,
}

impl<const BITS: usize> SbgRaeNativeTrace<BITS> {
    pub fn new(
        initial_state: &SemanticState<BITS>,
        initial_cycle: u64,
        expert_count: usize,
    ) -> Result<Self, SbgRaeTraceError> {
        if expert_count == 0 || expert_count > SBG_RAE_MAX_EXPERTS {
            return Err(SbgRaeTraceError::InvalidExpertCapacity);
        }
        Ok(Self {
            probe: SbgRisingMorProbe::new(initial_state, initial_cycle),
            previous_state: initial_state.clone(),
            previous_hints: None,
            previous_route_input: None,
            expert_count,
            frames: Vec::new(),
        })
    }

    #[must_use]
    pub const fn expert_count(&self) -> usize {
        self.expert_count
    }

    #[must_use]
    pub fn frames(&self) -> &[SbgRaeNativeFrame] {
        &self.frames
    }

    #[must_use]
    pub const fn last_cycle(&self) -> u64 {
        self.probe.last_cycle()
    }

    /// Execute the original MOR route and capture its selected expert mask.
    ///
    /// The caller MUST set `external_context_invalidated` whenever relevant
    /// model/expert semantics, bridge signatures, registry, engine policy,
    /// memory generation or request identity changes in a way not observable
    /// from the four SBG fields or supplied hints and previous route.
    ///
    /// No registered model operation is skipped, even without any new front.
    /// On error, every recorder field and prior evidence frame is unchanged.
    pub fn capture(
        &mut self,
        cycle: u64,
        state: &SemanticState<BITS>,
        context: SbgNativeRouteContext<'_, BITS>,
        external_context_invalidated: bool,
    ) -> Result<&SbgRaeNativeFrame, SbgRaeTraceError> {
        let observed_capacity = usize::try_from(context.registry.policy().expert_count)
            .map_err(|_| SbgRaeTraceError::InvalidExpertCapacity)?;
        if observed_capacity != self.expert_count {
            return Err(SbgRaeTraceError::ExpertCapacityChanged {
                expected: self.expert_count,
                actual: observed_capacity,
            });
        }
        if self.frames.len() >= SBG_RAE_MAX_FRAMES {
            return Err(SbgRaeTraceError::TooManyFrames);
        }

        // The exact native route is executed on a private probe snapshot.
        // A subsequent page validation error must never partially advance it.
        let mut next_probe = self.probe.clone();
        let (edge, native) = next_probe.observe_native_route(cycle, state, context)?;
        let mut required_pages_words = vec![0_u64; self.expert_count.div_ceil(64)];
        for page in &native.decision.selected_experts {
            let index = usize::try_from(page.0)
                .map_err(|_| SbgRaeTraceError::SelectedPageOutOfRange(*page))?;
            if index >= self.expert_count {
                return Err(SbgRaeTraceError::SelectedPageOutOfRange(*page));
            }
            let word = &mut required_pages_words[index / 64];
            let bit = 1_u64 << (index % 64);
            if *word & bit != 0 {
                return Err(SbgRaeTraceError::DuplicateSelectedPage(*page));
            }
            *word |= bit;
        }

        let semantic_invalidation = state.value != self.previous_state.value
            || state.meaning_mask != self.previous_state.meaning_mask
            || state.ambiguity != self.previous_state.ambiguity;
        let hints_changed = self
            .previous_hints
            .as_deref()
            .is_some_and(|previous| previous != context.runtime_hints);
        let prior_route_changed = self
            .previous_route_input
            .as_ref()
            .is_some_and(|previous| previous != context.previous_route);
        let frame = SbgRaeNativeFrame {
            cycle,
            routing_words: state.routing.words().to_vec(),
            required_pages_words,
            selected_experts: native.decision.selected_experts,
            rising_routing_bits: edge.rising_routing_bits,
            falling_routing_bits: edge.falling_routing_bits,
            active_routing_bits: edge.active_routing_bits,
            context_invalidated: external_context_invalidated
                || semantic_invalidation
                || hints_changed
                || prior_route_changed,
        };

        self.frames
            .try_reserve(1)
            .map_err(|_| SbgRaeTraceError::Allocation)?;
        self.probe = next_probe;
        self.previous_state = state.clone();
        self.previous_hints = Some(context.runtime_hints.to_vec());
        self.previous_route_input = Some(context.previous_route.clone());
        self.frames.push(frame);
        Ok(self.frames.last().expect("just appended one frame"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mor::{MixtureOfRouters, MorPolicy, RouterId},
        mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
        semantic::{
            SemanticBitField, SemanticExpertSignature, SemanticMorBridge, SemanticMorTopK,
            SemanticRouterSignature,
        },
    };

    fn registry(router_id: u32) -> MorRouterRegistry {
        MorRouterRegistry::new(
            vec![
                RouterSpecialization {
                    router_id: RouterId(router_id),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(router_id + 1),
                    experts: vec![PageId(1)],
                },
            ],
            MorRegistryPolicy {
                expert_count: 2,
                max_routers: 2,
                max_edges: 2,
                max_fanout: 1,
                require_full_coverage: true,
            },
        )
        .unwrap()
    }

    fn bridge() -> SemanticMorBridge<8> {
        SemanticMorBridge::new(
            vec![
                SemanticRouterSignature {
                    router_id: RouterId(0),
                    signature: SemanticBitField::<8>::zeroed().unwrap(),
                },
                SemanticRouterSignature {
                    router_id: RouterId(1),
                    signature: SemanticBitField::<8>::ones().unwrap(),
                },
            ],
            vec![
                SemanticExpertSignature {
                    page_id: PageId(0),
                    signature: SemanticBitField::<8>::zeroed().unwrap(),
                },
                SemanticExpertSignature {
                    page_id: PageId(1),
                    signature: SemanticBitField::<8>::ones().unwrap(),
                },
            ],
            SemanticMorTopK::One,
        )
        .unwrap()
    }

    fn engine() -> MixtureOfRouters {
        MixtureOfRouters::new(MorPolicy::default()).unwrap()
    }

    fn capture<'a>(
        trace: &'a mut SbgRaeNativeTrace<8>,
        cycle: u64,
        state: &SemanticState<8>,
        bridge: &SemanticMorBridge<8>,
        registry: &MorRouterRegistry,
        engine: &MixtureOfRouters,
    ) -> Result<&'a SbgRaeNativeFrame, SbgRaeTraceError> {
        trace.capture(
            cycle,
            state,
            SbgNativeRouteContext {
                bridge,
                registry,
                engine,
                runtime_hints: &[],
                previous_route: &PreviousRoute::default(),
            },
            false,
        )
    }

    #[test]
    fn captures_actual_mor_experts_even_without_new_fronts() {
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let bridge = bridge();
        let registry = registry(0);
        let engine = engine();
        let mut trace = SbgRaeNativeTrace::new(&baseline, 0, 2).unwrap();

        let first = capture(&mut trace, 1, &baseline, &bridge, &registry, &engine).unwrap();
        assert_eq!(first.rising_routing_bits, 0);
        assert_eq!(first.required_pages_words, vec![0b01]);
        assert_eq!(first.selected_experts, vec![PageId(0)]);

        let held = capture(&mut trace, 2, &baseline, &bridge, &registry, &engine).unwrap();
        assert_eq!(held.rising_routing_bits, 0);
        assert_eq!(held.required_pages_words, vec![0b01]);
        assert!(!held.context_invalidated);
        assert_eq!(trace.frames().len(), 2);
    }

    #[test]
    fn non_routing_semantic_change_invalidates_without_a_front() {
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let mut altered = baseline.clone();
        altered.value.set(3, true).unwrap();
        let bridge = bridge();
        let registry = registry(0);
        let engine = engine();
        let mut trace = SbgRaeNativeTrace::new(&baseline, 0, 2).unwrap();

        capture(&mut trace, 1, &baseline, &bridge, &registry, &engine).unwrap();
        let changed = capture(&mut trace, 2, &altered, &bridge, &registry, &engine).unwrap();
        assert_eq!(changed.rising_routing_bits, 0);
        assert_eq!(changed.required_pages_words, vec![0b01]);
        assert!(changed.context_invalidated);
    }

    #[test]
    fn changing_r_field_produces_front_and_real_mor_page_change() {
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let mut altered = baseline.clone();
        altered.routing = SemanticBitField::ones().unwrap();
        let bridge = bridge();
        let registry = registry(0);
        let engine = engine();
        let mut trace = SbgRaeNativeTrace::new(&baseline, 0, 2).unwrap();

        capture(&mut trace, 1, &baseline, &bridge, &registry, &engine).unwrap();
        let changed = capture(&mut trace, 2, &altered, &bridge, &registry, &engine).unwrap();
        assert_eq!(changed.rising_routing_bits, 8);
        assert_eq!(changed.routing_words, vec![0xff]);
        assert_eq!(changed.required_pages_words, vec![0b10]);
        assert_eq!(changed.selected_experts, vec![PageId(1)]);
    }

    #[test]
    fn malformed_route_and_time_leave_all_trace_state_unchanged() {
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let bridge = bridge();
        let engine = engine();
        let mut trace = SbgRaeNativeTrace::new(&baseline, 5, 2).unwrap();
        let before = trace.clone();

        assert!(matches!(
            capture(&mut trace, 5, &baseline, &bridge, &registry(0), &engine),
            Err(SbgRaeTraceError::NativeRoute(
                SbgRisingMorError::NonIncreasingCycle { .. }
            ))
        ));
        assert!(matches!(
            capture(&mut trace, 6, &baseline, &bridge, &registry(1), &engine),
            Err(SbgRaeTraceError::NativeRoute(_))
        ));
        assert_eq!(trace, before);
    }

    #[test]
    fn wrong_page_capacity_fails_before_any_native_execution() {
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let mut trace = SbgRaeNativeTrace::new(&baseline, 0, 1).unwrap();
        let before = trace.clone();
        assert_eq!(
            capture(&mut trace, 1, &baseline, &bridge(), &registry(0), &engine()),
            Err(SbgRaeTraceError::ExpertCapacityChanged {
                expected: 1,
                actual: 2
            })
        );
        assert_eq!(trace, before);
    }
}

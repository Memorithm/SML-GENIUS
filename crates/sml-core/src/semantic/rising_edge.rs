//! SML-owned SBG rising-front observation alongside the existing native MOR.
//!
//! Research-only sidecar: the native MOR decision is ALWAYS evaluated,
//! including cycles with no routing-field rising front. A zero front is not
//! permission to skip a model decision or a memory read.

use crate::{
    mor::{ExpertRuntimeHint, MixtureOfRouters, PreviousRoute},
    mor_registry::MorRouterRegistry,
    semantic::{
        SemanticBitField, SemanticMorBridge, SemanticMorError, SemanticMorRoute, SemanticState,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbgRisingMorError {
    NonIncreasingCycle { previous: u64, next: u64 },
    NativeRouting(SemanticMorError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SbgRisingMorEvidence {
    pub cycle: u64,
    pub rising_routing_bits: u64,
    pub falling_routing_bits: u64,
    pub active_routing_bits: u64,
}

/// Store a previous SBG R-field snapshot, plus two current event masks.
/// The model remains sovereign: no FLAT runtime dependency or Transformer
/// attention score path enters this native non-attention observer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbgRisingMorProbe<const BITS: usize> {
    previous_routing: SemanticBitField<BITS>,
    rising: Vec<u64>,
    falling: Vec<u64>,
    last_cycle: u64,
}

impl<const BITS: usize> SbgRisingMorProbe<BITS> {
    /// The baseline never emits synthetic initialization spikes.
    #[must_use]
    pub fn new(baseline: &SemanticState<BITS>, baseline_cycle: u64) -> Self {
        let words = baseline.routing.words().len();
        Self {
            previous_routing: baseline.routing.clone(),
            rising: vec![0; words],
            falling: vec![0; words],
            last_cycle: baseline_cycle,
        }
    }

    #[must_use]
    pub const fn last_cycle(&self) -> u64 {
        self.last_cycle
    }

    #[must_use]
    pub fn rising_words(&self) -> &[u64] {
        &self.rising
    }

    #[must_use]
    pub fn falling_words(&self) -> &[u64] {
        &self.falling
    }

    #[must_use]
    pub fn previous_routing(&self) -> &SemanticBitField<BITS> {
        &self.previous_routing
    }

    /// Run real native SBG->MOR routing, then transactionally record sampled
    /// rising and falling edges of the SBG R field. There is no optimization
    /// claim: MOR scores must still be computed for all eligible decisions.
    ///
    /// Any invalid cycle or native MOR error preserves this observer.
    pub fn observe_native_route(
        &mut self,
        cycle: u64,
        state: &SemanticState<BITS>,
        bridge: &SemanticMorBridge<BITS>,
        registry: &MorRouterRegistry,
        engine: &MixtureOfRouters,
        runtime_hints: &[ExpertRuntimeHint],
        previous_route: &PreviousRoute,
    ) -> Result<(SbgRisingMorEvidence, SemanticMorRoute), SbgRisingMorError> {
        if cycle <= self.last_cycle {
            return Err(SbgRisingMorError::NonIncreasingCycle {
                previous: self.last_cycle,
                next: cycle,
            });
        }
        // Validate and execute native routing *before* mutating edge state.
        let native_route = bridge
            .route(state, registry, engine, runtime_hints, previous_route)
            .map_err(SbgRisingMorError::NativeRouting)?;

        let mut rising_count = 0_u64;
        let mut falling_count = 0_u64;
        for (((rise, fall), previous), next) in self
            .rising
            .iter_mut()
            .zip(&mut self.falling)
            .zip(self.previous_routing.words())
            .zip(state.routing.words())
        {
            *rise = *next & !*previous;
            *fall = *previous & !*next;
            rising_count += u64::from(rise.count_ones());
            falling_count += u64::from(fall.count_ones());
        }

        self.previous_routing.clear();
        self.previous_routing.or_assign(&state.routing);
        self.last_cycle = cycle;
        Ok((
            SbgRisingMorEvidence {
                cycle,
                rising_routing_bits: rising_count,
                falling_routing_bits: falling_count,
                active_routing_bits: state.routing.count_ones(),
            },
            native_route,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        mor::{MorPolicy, RouterId},
        mor_registry::{MorRegistryPolicy, RouterSpecialization},
        semantic::{SemanticExpertSignature, SemanticMorTopK, SemanticRouterSignature},
        PageId,
    };

    fn registry(router_id: u32) -> MorRouterRegistry {
        MorRouterRegistry::new(
            vec![RouterSpecialization {
                router_id: RouterId(router_id),
                experts: vec![PageId(0)],
            }],
            MorRegistryPolicy {
                expert_count: 1,
                max_routers: 1,
                max_edges: 1,
                max_fanout: 1,
                require_full_coverage: true,
            },
        )
        .unwrap()
    }

    fn bridge<const BITS: usize>() -> SemanticMorBridge<BITS> {
        SemanticMorBridge::new(
            vec![SemanticRouterSignature {
                router_id: RouterId(0),
                signature: SemanticBitField::zeroed().unwrap(),
            }],
            vec![SemanticExpertSignature {
                page_id: PageId(0),
                signature: SemanticBitField::zeroed().unwrap(),
            }],
            SemanticMorTopK::One,
        )
        .unwrap()
    }

    fn engine() -> MixtureOfRouters {
        MixtureOfRouters::new(MorPolicy::default()).unwrap()
    }

    fn observe<const BITS: usize>(
        probe: &mut SbgRisingMorProbe<BITS>,
        cycle: u64,
        state: &SemanticState<BITS>,
        bridge: &SemanticMorBridge<BITS>,
        registry: &MorRouterRegistry,
        engine: &MixtureOfRouters,
    ) -> Result<(SbgRisingMorEvidence, SemanticMorRoute), SbgRisingMorError> {
        probe.observe_native_route(
            cycle,
            state,
            bridge,
            registry,
            engine,
            &[],
            &PreviousRoute::default(),
        )
    }

    #[test]
    fn held_high_sbg_routing_state_does_not_emit_repeated_events() {
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let mut state = baseline.clone();
        state.routing.set(3, true).unwrap();
        let mut probe = SbgRisingMorProbe::new(&baseline, 0);
        let bridge = bridge::<8>();
        let registry = registry(0);
        let engine = engine();

        let (first, route) =
            observe(&mut probe, 1, &state, &bridge, &registry, &engine).unwrap();
        assert_eq!(first.rising_routing_bits, 1);
        assert_eq!(probe.rising_words(), &[0b1000]);
        assert_eq!(route.router_inputs.len(), 1);

        // A second native MOR decision is still computed with no new front.
        let (held, route) =
            observe(&mut probe, 2, &state, &bridge, &registry, &engine).unwrap();
        assert_eq!(held.rising_routing_bits, 0);
        assert_eq!(held.active_routing_bits, 1);
        assert_eq!(route.router_inputs.len(), 1);
        assert_eq!(probe.rising_words(), &[0]);
    }

    #[test]
    fn falling_then_rising_rearms_and_keeps_padding_zero() {
        let baseline = SemanticState::<65>::zeroed().unwrap();
        let mut state = baseline.clone();
        state.routing.set(64, true).unwrap();
        let mut probe = SbgRisingMorProbe::new(&baseline, 0);
        let bridge = bridge::<65>();
        let registry = registry(0);
        let engine = engine();

        let (rise, _) =
            observe(&mut probe, 1, &state, &bridge, &registry, &engine).unwrap();
        assert_eq!(rise.rising_routing_bits, 1);
        assert_eq!(probe.rising_words(), &[0, 1]);

        let (fall, _) =
            observe(&mut probe, 2, &baseline, &bridge, &registry, &engine).unwrap();
        assert_eq!(fall.falling_routing_bits, 1);
        assert_eq!(probe.falling_words(), &[0, 1]);

        let (rearm, _) =
            observe(&mut probe, 3, &state, &bridge, &registry, &engine).unwrap();
        assert_eq!(rearm.rising_routing_bits, 1);
        assert_eq!(probe.rising_words(), &[0, 1]);
    }

    #[test]
    fn bad_cycle_and_failed_native_route_are_transactional() {
        let baseline = SemanticState::<8>::zeroed().unwrap();
        let mut probe = SbgRisingMorProbe::new(&baseline, 3);
        let before = probe.clone();
        let bridge = bridge::<8>();
        let engine = engine();

        assert_eq!(
            observe(&mut probe, 3, &baseline, &bridge, &registry(0), &engine),
            Err(SbgRisingMorError::NonIncreasingCycle {
                previous: 3,
                next: 3
            })
        );
        assert!(matches!(
            observe(&mut probe, 4, &baseline, &bridge, &registry(1), &engine),
            Err(SbgRisingMorError::NativeRouting(_))
        ));
        assert_eq!(probe, before);
    }
}

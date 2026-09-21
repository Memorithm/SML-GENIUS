//! Semantic Boolean Geometry adapter for the native SML Mixture-of-Routers.
//!
//! The SBG routing field (R) is compared directly against Boolean router and
//! expert signatures using XNOR/POPCOUNT. Scores are normalized into the
//! integer parts-per-million domain already used by MOR. The resulting
//! `RouterInput` values are then validated and routed by the existing native
//! `MorRouterRegistry` and `MixtureOfRouters`; this module does not create a
//! second routing engine.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorDecision, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM,
    },
    mor_registry::{MorRegistryError, MorRouterRegistry},
    PageId,
};

use super::{SemanticBitField, SemanticState};

/// Sparse proposal fanout tested by the SBG->MOR bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticMorTopK {
    One,
    Two,
    Four,
}

impl SemanticMorTopK {
    #[must_use]
    pub const fn as_usize(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }
}

/// Boolean semantic signature attached to one native MOR router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticRouterSignature<const BITS: usize> {
    pub router_id: RouterId,
    pub signature: SemanticBitField<BITS>,
}

/// Boolean semantic signature attached to one native MOR expert page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticExpertSignature<const BITS: usize> {
    pub page_id: PageId,
    pub signature: SemanticBitField<BITS>,
}

/// Deterministic adapter from SBG routing state into native MOR inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorBridge<const BITS: usize> {
    router_signatures: BTreeMap<RouterId, SemanticBitField<BITS>>,
    expert_signatures: BTreeMap<u32, SemanticBitField<BITS>>,
    proposal_top_k: SemanticMorTopK,
}

impl<const BITS: usize> SemanticMorBridge<BITS> {
    /// Builds a bridge and rejects duplicate semantic signatures fail-closed.
    pub fn new(
        router_signatures: Vec<SemanticRouterSignature<BITS>>,
        expert_signatures: Vec<SemanticExpertSignature<BITS>>,
        proposal_top_k: SemanticMorTopK,
    ) -> Result<Self, SemanticMorError> {
        if router_signatures.is_empty() {
            return Err(SemanticMorError::EmptyRouterSignatures);
        }
        if expert_signatures.is_empty() {
            return Err(SemanticMorError::EmptyExpertSignatures);
        }

        let mut routers = BTreeMap::new();
        for signature in router_signatures {
            if routers
                .insert(signature.router_id, signature.signature)
                .is_some()
            {
                return Err(SemanticMorError::DuplicateRouterSignature(
                    signature.router_id,
                ));
            }
        }

        let mut experts = BTreeMap::new();
        for signature in expert_signatures {
            if experts
                .insert(signature.page_id.0, signature.signature)
                .is_some()
            {
                return Err(SemanticMorError::DuplicateExpertSignature(
                    signature.page_id,
                ));
            }
        }

        Ok(Self {
            router_signatures: routers,
            expert_signatures: experts,
            proposal_top_k,
        })
    }

    #[must_use]
    pub const fn proposal_top_k(&self) -> SemanticMorTopK {
        self.proposal_top_k
    }

    #[must_use]
    pub fn router_signature_count(&self) -> usize {
        self.router_signatures.len()
    }

    #[must_use]
    pub fn expert_signature_count(&self) -> usize {
        self.expert_signatures.len()
    }

    /// Converts one SBG state into the complete native MOR router-input set.
    ///
    /// Router affinity and expert proposal scores are both derived from exact
    /// XNOR similarity with the state's routing field. Reliability and
    /// confidence remain neutral at 1.0 ppm scale in this bridge; existing MOR
    /// policy layers may still apply denoising, consensus, hysteresis, balance,
    /// residency and precision-escalation logic.
    pub fn build_inputs(
        &self,
        state: &SemanticState<BITS>,
        registry: &MorRouterRegistry,
    ) -> Result<Vec<RouterInput>, SemanticMorError> {
        self.validate_against_registry(registry)?;

        let mut inputs = Vec::with_capacity(registry.specializations().len());
        for specialization in registry.specializations() {
            let router_signature = self
                .router_signatures
                .get(&specialization.router_id)
                .ok_or(SemanticMorError::MissingRouterSignature(
                    specialization.router_id,
                ))?;
            let affinity_ppm = semantic_similarity_ppm(&state.routing, router_signature)?;

            let mut proposals = specialization
                .experts
                .iter()
                .map(|page_id| {
                    let signature = self
                        .expert_signatures
                        .get(&page_id.0)
                        .ok_or(SemanticMorError::MissingExpertSignature(*page_id))?;
                    Ok(ExpertProposal {
                        page_id: *page_id,
                        score_ppm: semantic_similarity_ppm(&state.routing, signature)?,
                    })
                })
                .collect::<Result<Vec<_>, SemanticMorError>>()?;

            proposals.sort_by(|left, right| {
                right
                    .score_ppm
                    .cmp(&left.score_ppm)
                    .then_with(|| left.page_id.0.cmp(&right.page_id.0))
            });
            proposals.truncate(self.proposal_top_k.as_usize().min(proposals.len()));

            inputs.push(RouterInput {
                router_id: specialization.router_id,
                affinity_ppm,
                reliability_ppm: MOR_SCALE_PPM,
                confidence_ppm: MOR_SCALE_PPM,
                balance_bias_ppm: 0,
                proposals,
            });
        }

        Ok(inputs)
    }

    /// Routes one SBG state through the existing native MOR registry and engine.
    pub fn route(
        &self,
        state: &SemanticState<BITS>,
        registry: &MorRouterRegistry,
        engine: &MixtureOfRouters,
        runtime_hints: &[ExpertRuntimeHint],
        previous: &PreviousRoute,
    ) -> Result<SemanticMorRoute, SemanticMorError> {
        let router_inputs = self.build_inputs(state, registry)?;
        let decision = registry
            .route(engine, &router_inputs, runtime_hints, previous)
            .map_err(SemanticMorError::Registry)?;

        Ok(SemanticMorRoute {
            router_inputs,
            decision,
        })
    }

    fn validate_against_registry(
        &self,
        registry: &MorRouterRegistry,
    ) -> Result<(), SemanticMorError> {
        let required_routers = registry
            .specializations()
            .iter()
            .map(|specialization| specialization.router_id)
            .collect::<BTreeSet<_>>();

        for router_id in &required_routers {
            if !self.router_signatures.contains_key(router_id) {
                return Err(SemanticMorError::MissingRouterSignature(*router_id));
            }
        }
        for router_id in self.router_signatures.keys() {
            if !required_routers.contains(router_id) {
                return Err(SemanticMorError::UnknownRouterSignature(*router_id));
            }
        }

        let expert_count = registry.policy().expert_count;
        for page in self.expert_signatures.keys().copied() {
            if page >= expert_count {
                return Err(SemanticMorError::ExpertSignatureOutOfRange {
                    page_id: PageId(page),
                    expert_count,
                });
            }
        }
        for specialization in registry.specializations() {
            for page_id in &specialization.experts {
                if !self.expert_signatures.contains_key(&page_id.0) {
                    return Err(SemanticMorError::MissingExpertSignature(*page_id));
                }
            }
        }

        Ok(())
    }
}

/// Auditable bridge result retaining the exact generated MOR inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorRoute {
    pub router_inputs: Vec<RouterInput>,
    pub decision: MorDecision,
}

fn semantic_similarity_ppm<const BITS: usize>(
    routing: &SemanticBitField<BITS>,
    signature: &SemanticBitField<BITS>,
) -> Result<u32, SemanticMorError> {
    if BITS == 0 {
        return Err(SemanticMorError::ZeroSemanticWidth);
    }

    let equal = routing.xnor_popcount(signature) as u128;
    let ppm = equal
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(SemanticMorError::ArithmeticOverflow)?
        / BITS as u128;
    u32::try_from(ppm).map_err(|_| SemanticMorError::ArithmeticOverflow)
}

/// Invalid SBG->MOR integration request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMorError {
    EmptyRouterSignatures,
    EmptyExpertSignatures,
    DuplicateRouterSignature(RouterId),
    DuplicateExpertSignature(PageId),
    MissingRouterSignature(RouterId),
    UnknownRouterSignature(RouterId),
    MissingExpertSignature(PageId),
    ExpertSignatureOutOfRange { page_id: PageId, expert_count: u32 },
    ZeroSemanticWidth,
    ArithmeticOverflow,
    Registry(MorRegistryError),
}

impl fmt::Display for SemanticMorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMorError {}

#[cfg(test)]
mod tests {
    use super::{
        SemanticExpertSignature, SemanticMorBridge, SemanticMorError, SemanticMorTopK,
        SemanticRouterSignature,
    };
    use crate::{
        mor::{MixtureOfRouters, MorPolicy, PreviousRoute, RouterId, MOR_SCALE_PPM},
        mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
        semantic::{SemanticBitField, SemanticState},
        PageId,
    };

    fn registry_one_router_four_experts() -> MorRouterRegistry {
        MorRouterRegistry::new(
            vec![RouterSpecialization {
                router_id: RouterId(7),
                experts: vec![PageId(0), PageId(1), PageId(2), PageId(3)],
            }],
            MorRegistryPolicy {
                expert_count: 4,
                max_routers: 1,
                max_edges: 4,
                max_fanout: 4,
                require_full_coverage: true,
            },
        )
        .expect("registry")
    }

    fn top_k_bridge(top_k: SemanticMorTopK) -> SemanticMorBridge<8> {
        SemanticMorBridge::new(
            vec![SemanticRouterSignature {
                router_id: RouterId(7),
                signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3])
                    .expect("router signature"),
            }],
            vec![
                SemanticExpertSignature {
                    page_id: PageId(0),
                    signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).expect("expert 0"),
                },
                SemanticExpertSignature {
                    page_id: PageId(1),
                    signature: SemanticBitField::from_set_indices(&[0, 1, 2, 4]).expect("expert 1"),
                },
                SemanticExpertSignature {
                    page_id: PageId(2),
                    signature: SemanticBitField::from_set_indices(&[0, 1, 4, 5]).expect("expert 2"),
                },
                SemanticExpertSignature {
                    page_id: PageId(3),
                    signature: SemanticBitField::from_set_indices(&[4, 5, 6, 7]).expect("expert 3"),
                },
            ],
            top_k,
        )
        .expect("bridge")
    }

    fn state_with_routing<const BITS: usize>(
        routing: SemanticBitField<BITS>,
    ) -> SemanticState<BITS> {
        let mut state = SemanticState::zeroed().expect("state");
        state.routing = routing;
        state
    }

    #[test]
    fn top_k_controls_sparse_expert_proposal_fanout() {
        let registry = registry_one_router_four_experts();
        let state = state_with_routing(
            SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("routing"),
        );

        for (top_k, expected) in [
            (SemanticMorTopK::One, 1_usize),
            (SemanticMorTopK::Two, 2_usize),
            (SemanticMorTopK::Four, 4_usize),
        ] {
            let bridge = top_k_bridge(top_k);
            let inputs = bridge.build_inputs(&state, &registry).expect("inputs");
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].proposals.len(), expected);
            assert_eq!(inputs[0].proposals[0].page_id, PageId(0));
            assert_eq!(inputs[0].proposals[0].score_ppm, MOR_SCALE_PPM);
            if expected >= 2 {
                assert_eq!(inputs[0].proposals[1].page_id, PageId(1));
                assert_eq!(inputs[0].proposals[1].score_ppm, 750_000);
            }
        }
    }

    #[test]
    fn exact_boolean_similarity_drives_native_mor_selection() {
        let registry = MorRouterRegistry::new(
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
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
        .expect("registry");
        let bridge = SemanticMorBridge::new(
            vec![
                SemanticRouterSignature {
                    router_id: RouterId(0),
                    signature: SemanticBitField::from_set_indices(&[4, 5, 6, 7]).expect("router 0"),
                },
                SemanticRouterSignature {
                    router_id: RouterId(1),
                    signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).expect("router 1"),
                },
            ],
            vec![
                SemanticExpertSignature {
                    page_id: PageId(0),
                    signature: SemanticBitField::from_set_indices(&[4, 5, 6, 7]).expect("expert 0"),
                },
                SemanticExpertSignature {
                    page_id: PageId(1),
                    signature: SemanticBitField::from_set_indices(&[0, 1, 2, 3]).expect("expert 1"),
                },
            ],
            SemanticMorTopK::One,
        )
        .expect("bridge");

        let state = state_with_routing(
            SemanticBitField::<8>::from_set_indices(&[0, 1, 2, 3]).expect("routing"),
        );
        let engine = MixtureOfRouters::new(MorPolicy::default()).expect("engine");
        let routed = bridge
            .route(&state, &registry, &engine, &[], &PreviousRoute::default())
            .expect("route");

        assert_eq!(routed.router_inputs[0].affinity_ppm, 0);
        assert_eq!(routed.router_inputs[1].affinity_ppm, MOR_SCALE_PPM);
        assert_eq!(routed.decision.selected_experts, vec![PageId(1)]);
        assert_eq!(routed.decision.active_routers[0].router_id, RouterId(1));
    }

    #[test]
    fn missing_or_duplicate_signatures_fail_closed() {
        let duplicate = SemanticMorBridge::new(
            vec![
                SemanticRouterSignature {
                    router_id: RouterId(1),
                    signature: SemanticBitField::<8>::zeroed().expect("router"),
                },
                SemanticRouterSignature {
                    router_id: RouterId(1),
                    signature: SemanticBitField::<8>::ones().expect("router"),
                },
            ],
            vec![SemanticExpertSignature {
                page_id: PageId(0),
                signature: SemanticBitField::<8>::zeroed().expect("expert"),
            }],
            SemanticMorTopK::One,
        );
        assert_eq!(
            duplicate,
            Err(SemanticMorError::DuplicateRouterSignature(RouterId(1)))
        );

        let registry = registry_one_router_four_experts();
        let bridge = SemanticMorBridge::new(
            vec![SemanticRouterSignature {
                router_id: RouterId(7),
                signature: SemanticBitField::<8>::zeroed().expect("router"),
            }],
            vec![SemanticExpertSignature {
                page_id: PageId(0),
                signature: SemanticBitField::<8>::zeroed().expect("expert"),
            }],
            SemanticMorTopK::One,
        )
        .expect("bridge");
        let state = SemanticState::<8>::zeroed().expect("state");

        assert_eq!(
            bridge.build_inputs(&state, &registry),
            Err(SemanticMorError::MissingExpertSignature(PageId(1)))
        );
    }
}

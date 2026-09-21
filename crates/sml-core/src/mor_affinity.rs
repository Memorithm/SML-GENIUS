//! Native SML state fingerprints and specialized-router affinity scoring.
//!
//! MOR routing no longer needs caller-computed affinities on this path. A
//! declared sparse fingerprint plan samples the bounded SML Boolean state, and
//! each specialized router owns a weighted Boolean signature over that
//! fingerprint. Affinity is exact weighted agreement in integer ppm.

use core::fmt;
use std::collections::BTreeSet;

use crate::{
    mor::{
        ExpertProposal, ExpertRuntimeHint, MixtureOfRouters, MorDecision, MorError, PreviousRoute,
        RouterId, RouterInput, MOR_SCALE_PPM,
    },
    BooleanWorkspace, PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorAffinityPolicy {
    pub max_fingerprint_bits: usize,
    pub max_routers: usize,
    pub max_atoms_per_router: usize,
    pub max_total_atoms: usize,
}

impl Default for MorAffinityPolicy {
    fn default() -> Self {
        Self {
            max_fingerprint_bits: 256,
            max_routers: 64,
            max_atoms_per_router: 64,
            max_total_atoms: 2_048,
        }
    }
}

impl MorAffinityPolicy {
    pub fn validate(self) -> Result<Self, MorAffinityError> {
        if self.max_fingerprint_bits == 0 {
            return Err(MorAffinityError::ZeroFingerprintBudget);
        }
        if self.max_routers == 0 {
            return Err(MorAffinityError::ZeroRouterBudget);
        }
        if self.max_atoms_per_router == 0 {
            return Err(MorAffinityError::ZeroAtomBudget);
        }
        if self.max_total_atoms == 0 {
            return Err(MorAffinityError::ZeroTotalAtomBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmlFingerprintPlan {
    positions: Vec<u32>,
}

impl SmlFingerprintPlan {
    pub fn new(
        positions: Vec<u32>,
        policy: MorAffinityPolicy,
    ) -> Result<Self, MorAffinityError> {
        policy.validate()?;
        if positions.is_empty() {
            return Err(MorAffinityError::EmptyFingerprint);
        }
        if positions.len() > policy.max_fingerprint_bits {
            return Err(MorAffinityError::FingerprintBudget {
                actual: positions.len(),
                maximum: policy.max_fingerprint_bits,
            });
        }

        let mut seen = BTreeSet::new();
        for position in &positions {
            if !seen.insert(*position) {
                return Err(MorAffinityError::DuplicateFingerprintPosition(*position));
            }
        }
        Ok(Self { positions })
    }

    #[must_use]
    pub fn positions(&self) -> &[u32] {
        &self.positions
    }

    #[must_use]
    pub fn width(&self) -> usize {
        self.positions.len()
    }

    pub fn fingerprint(
        &self,
        workspace: &BooleanWorkspace,
    ) -> Result<SmlStateFingerprint, MorAffinityError> {
        let mut bits = Vec::with_capacity(self.positions.len());
        for position in &self.positions {
            let index = usize::try_from(*position).map_err(|_| MorAffinityError::IndexOverflow)?;
            let bit = workspace
                .get(index)
                .ok_or(MorAffinityError::WorkspaceTooSmall {
                    required_index: index,
                    actual_bits: workspace.len(),
                })?;
            bits.push(bit);
        }
        Ok(SmlStateFingerprint { bits })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmlStateFingerprint {
    bits: Vec<bool>,
}

impl SmlStateFingerprint {
    #[must_use]
    pub fn len(&self) -> usize {
        self.bits.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<bool> {
        self.bits.get(index).copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterSignatureAtom {
    pub fingerprint_bit: u16,
    pub expected: bool,
    pub weight_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeRouterSpec {
    pub router_id: RouterId,
    pub atoms: Vec<RouterSignatureAtom>,
    pub reliability_ppm: u32,
    pub confidence_ppm: u32,
    pub balance_bias_ppm: i32,
    pub proposals: Vec<ExpertProposal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeAffinityAccounting {
    pub fingerprint_bits: usize,
    pub router_count: usize,
    pub signature_atoms: usize,
    pub expected_value_bits: usize,
    pub weight_bits: u64,
    pub position_metadata_bits: u64,
    pub average_atoms_milli: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterAffinityEvidence {
    pub router_id: RouterId,
    pub matched_atoms: usize,
    pub atom_count: usize,
    pub matched_weight: u64,
    pub total_weight: u64,
    pub affinity_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAffinityBatch {
    pub fingerprint: SmlStateFingerprint,
    pub routers: Vec<RouterInput>,
    pub evidence: Vec<RouterAffinityEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAffinityBank {
    policy: MorAffinityPolicy,
    fingerprint_plan: SmlFingerprintPlan,
    routers: Vec<NativeRouterSpec>,
    accounting: NativeAffinityAccounting,
}

impl NativeAffinityBank {
    pub fn new(
        fingerprint_plan: SmlFingerprintPlan,
        routers: Vec<NativeRouterSpec>,
        policy: MorAffinityPolicy,
    ) -> Result<Self, MorAffinityError> {
        let policy = policy.validate()?;
        if fingerprint_plan.width() > policy.max_fingerprint_bits {
            return Err(MorAffinityError::FingerprintBudget {
                actual: fingerprint_plan.width(),
                maximum: policy.max_fingerprint_bits,
            });
        }
        if routers.is_empty() {
            return Err(MorAffinityError::EmptyRouterBank);
        }
        if routers.len() > policy.max_routers {
            return Err(MorAffinityError::RouterBudget {
                actual: routers.len(),
                maximum: policy.max_routers,
            });
        }

        let mut router_ids = BTreeSet::new();
        let mut total_atoms = 0_usize;
        for router in &routers {
            if !router_ids.insert(router.router_id) {
                return Err(MorAffinityError::DuplicateRouter(router.router_id));
            }
            validate_ppm(router.reliability_ppm)?;
            validate_ppm(router.confidence_ppm)?;
            if router.atoms.is_empty() {
                return Err(MorAffinityError::EmptySignature(router.router_id));
            }
            if router.atoms.len() > policy.max_atoms_per_router {
                return Err(MorAffinityError::AtomBudget {
                    router_id: router.router_id,
                    actual: router.atoms.len(),
                    maximum: policy.max_atoms_per_router,
                });
            }
            if router.proposals.is_empty() {
                return Err(MorAffinityError::EmptyProposals(router.router_id));
            }

            let mut atom_bits = BTreeSet::new();
            let mut total_weight = 0_u64;
            for atom in &router.atoms {
                let bit = atom.fingerprint_bit as usize;
                if bit >= fingerprint_plan.width() {
                    return Err(MorAffinityError::SignatureBitOutOfRange {
                        router_id: router.router_id,
                        bit,
                        fingerprint_width: fingerprint_plan.width(),
                    });
                }
                if !atom_bits.insert(atom.fingerprint_bit) {
                    return Err(MorAffinityError::DuplicateSignatureBit {
                        router_id: router.router_id,
                        bit,
                    });
                }
                if atom.weight_ppm == 0 || atom.weight_ppm > MOR_SCALE_PPM {
                    return Err(MorAffinityError::InvalidAtomWeight {
                        router_id: router.router_id,
                        bit,
                        weight_ppm: atom.weight_ppm,
                    });
                }
                total_weight = total_weight
                    .checked_add(atom.weight_ppm as u64)
                    .ok_or(MorAffinityError::ArithmeticOverflow)?;
            }
            if total_weight == 0 {
                return Err(MorAffinityError::ZeroSignatureWeight(router.router_id));
            }

            let mut proposal_pages = BTreeSet::new();
            for proposal in &router.proposals {
                if proposal.score_ppm > MOR_SCALE_PPM {
                    return Err(MorAffinityError::ProposalPpmOutOfRange {
                        router_id: router.router_id,
                        page_id: proposal.page_id,
                        score_ppm: proposal.score_ppm,
                    });
                }
                if !proposal_pages.insert(proposal.page_id.0) {
                    return Err(MorAffinityError::DuplicateProposal {
                        router_id: router.router_id,
                        page_id: proposal.page_id,
                    });
                }
            }

            total_atoms = total_atoms
                .checked_add(router.atoms.len())
                .ok_or(MorAffinityError::ArithmeticOverflow)?;
            if total_atoms > policy.max_total_atoms {
                return Err(MorAffinityError::TotalAtomBudget {
                    actual: total_atoms,
                    maximum: policy.max_total_atoms,
                });
            }
        }

        let position_bits_per_atom = bits_required(fingerprint_plan.width()) as u64;
        let accounting = NativeAffinityAccounting {
            fingerprint_bits: fingerprint_plan.width(),
            router_count: routers.len(),
            signature_atoms: total_atoms,
            expected_value_bits: total_atoms,
            weight_bits: (total_atoms as u64)
                .checked_mul(u32::BITS as u64)
                .ok_or(MorAffinityError::ArithmeticOverflow)?,
            position_metadata_bits: (total_atoms as u64)
                .checked_mul(position_bits_per_atom)
                .ok_or(MorAffinityError::ArithmeticOverflow)?,
            average_atoms_milli: ((total_atoms as u128 * 1_000) / routers.len() as u128) as u32,
        };

        Ok(Self {
            policy,
            fingerprint_plan,
            routers,
            accounting,
        })
    }

    #[must_use]
    pub const fn policy(&self) -> MorAffinityPolicy {
        self.policy
    }

    #[must_use]
    pub fn fingerprint_plan(&self) -> &SmlFingerprintPlan {
        &self.fingerprint_plan
    }

    #[must_use]
    pub fn routers(&self) -> &[NativeRouterSpec] {
        &self.routers
    }

    #[must_use]
    pub const fn accounting(&self) -> NativeAffinityAccounting {
        self.accounting
    }

    pub fn materialize(
        &self,
        workspace: &BooleanWorkspace,
    ) -> Result<NativeAffinityBatch, MorAffinityError> {
        let fingerprint = self.fingerprint_plan.fingerprint(workspace)?;
        let mut router_inputs = Vec::with_capacity(self.routers.len());
        let mut evidence = Vec::with_capacity(self.routers.len());

        for router in &self.routers {
            let affinity = score_signature(router, &fingerprint)?;
            router_inputs.push(RouterInput {
                router_id: router.router_id,
                affinity_ppm: affinity.affinity_ppm,
                reliability_ppm: router.reliability_ppm,
                confidence_ppm: router.confidence_ppm,
                balance_bias_ppm: router.balance_bias_ppm,
                proposals: router.proposals.clone(),
            });
            evidence.push(affinity);
        }

        Ok(NativeAffinityBatch {
            fingerprint,
            routers: router_inputs,
            evidence,
        })
    }

    pub fn route(
        &self,
        engine: &MixtureOfRouters,
        workspace: &BooleanWorkspace,
        runtime_hints: &[ExpertRuntimeHint],
        previous: &PreviousRoute,
    ) -> Result<(MorDecision, NativeAffinityBatch), MorAffinityError> {
        let batch = self.materialize(workspace)?;
        let decision = engine
            .route(&batch.routers, runtime_hints, previous)
            .map_err(MorAffinityError::Mor)?;
        Ok((decision, batch))
    }
}

fn score_signature(
    router: &NativeRouterSpec,
    fingerprint: &SmlStateFingerprint,
) -> Result<RouterAffinityEvidence, MorAffinityError> {
    let mut matched_atoms = 0_usize;
    let mut matched_weight = 0_u64;
    let mut total_weight = 0_u64;

    for atom in &router.atoms {
        let bit = atom.fingerprint_bit as usize;
        let observed = fingerprint
            .get(bit)
            .ok_or(MorAffinityError::SignatureBitOutOfRange {
                router_id: router.router_id,
                bit,
                fingerprint_width: fingerprint.len(),
            })?;
        total_weight = total_weight
            .checked_add(atom.weight_ppm as u64)
            .ok_or(MorAffinityError::ArithmeticOverflow)?;
        if observed == atom.expected {
            matched_atoms = matched_atoms.saturating_add(1);
            matched_weight = matched_weight
                .checked_add(atom.weight_ppm as u64)
                .ok_or(MorAffinityError::ArithmeticOverflow)?;
        }
    }

    if total_weight == 0 {
        return Err(MorAffinityError::ZeroSignatureWeight(router.router_id));
    }

    let affinity_ppm = ((matched_weight as u128 * MOR_SCALE_PPM as u128)
        / total_weight as u128) as u32;

    Ok(RouterAffinityEvidence {
        router_id: router.router_id,
        matched_atoms,
        atom_count: router.atoms.len(),
        matched_weight,
        total_weight,
        affinity_ppm,
    })
}

fn validate_ppm(value: u32) -> Result<(), MorAffinityError> {
    if value > MOR_SCALE_PPM {
        return Err(MorAffinityError::InputPpmOutOfRange(value));
    }
    Ok(())
}

fn bits_required(cardinality: usize) -> u32 {
    if cardinality <= 1 {
        0
    } else {
        usize::BITS - (cardinality - 1).leading_zeros()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorAffinityError {
    ZeroFingerprintBudget,
    ZeroRouterBudget,
    ZeroAtomBudget,
    ZeroTotalAtomBudget,
    EmptyFingerprint,
    FingerprintBudget {
        actual: usize,
        maximum: usize,
    },
    DuplicateFingerprintPosition(u32),
    WorkspaceTooSmall {
        required_index: usize,
        actual_bits: usize,
    },
    IndexOverflow,
    EmptyRouterBank,
    RouterBudget {
        actual: usize,
        maximum: usize,
    },
    DuplicateRouter(RouterId),
    EmptySignature(RouterId),
    AtomBudget {
        router_id: RouterId,
        actual: usize,
        maximum: usize,
    },
    EmptyProposals(RouterId),
    SignatureBitOutOfRange {
        router_id: RouterId,
        bit: usize,
        fingerprint_width: usize,
    },
    DuplicateSignatureBit {
        router_id: RouterId,
        bit: usize,
    },
    InvalidAtomWeight {
        router_id: RouterId,
        bit: usize,
        weight_ppm: u32,
    },
    ZeroSignatureWeight(RouterId),
    ProposalPpmOutOfRange {
        router_id: RouterId,
        page_id: PageId,
        score_ppm: u32,
    },
    DuplicateProposal {
        router_id: RouterId,
        page_id: PageId,
    },
    TotalAtomBudget {
        actual: usize,
        maximum: usize,
    },
    InputPpmOutOfRange(u32),
    ArithmeticOverflow,
    Mor(MorError),
}

impl fmt::Display for MorAffinityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorAffinityError {}

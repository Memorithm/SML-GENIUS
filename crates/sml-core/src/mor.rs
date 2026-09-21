//! Deterministic Mixture-of-Routers control plane for SML.
//!
//! This reference implementation keeps routing evidence in integer parts per
//! million. It separates sparse router selection, singleton-noise suppression,
//! coherent disagreement, bounded hysteresis and residency-aware tie breaking.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::PageId;

pub const MOR_SCALE_PPM: u32 = 1_000_000;
pub const MOR_SCORE_CEILING_PPM: u32 = 2_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RouterId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorPolicy {
    pub min_routers: usize,
    pub max_routers: usize,
    pub router_mass_threshold_ppm: u32,
    pub min_experts: usize,
    pub max_experts: usize,
    pub base_expert_mass_threshold_ppm: u32,
    pub disagreement_extra_mass_ppm: u32,
    pub singleton_denoise_ppm: u32,
    pub singleton_bypass_credibility_ppm: u32,
    pub singleton_bypass_score_ppm: u32,
    pub consensus_bonus_per_router_ppm: u32,
    pub residency_penalty_cap_ppm: u32,
    pub recurrent_hysteresis_bonus_ppm: u32,
    pub primary_switch_margin_ppm: u32,
    pub precision_escalation_margin_ppm: u32,
    pub disagreement_precision_threshold_ppm: u32,
}

impl Default for MorPolicy {
    fn default() -> Self {
        Self {
            min_routers: 1,
            max_routers: 4,
            router_mass_threshold_ppm: 850_000,
            min_experts: 1,
            max_experts: 4,
            base_expert_mass_threshold_ppm: 700_000,
            disagreement_extra_mass_ppm: 200_000,
            singleton_denoise_ppm: 250_000,
            singleton_bypass_credibility_ppm: 850_000,
            singleton_bypass_score_ppm: 900_000,
            consensus_bonus_per_router_ppm: 20_000,
            residency_penalty_cap_ppm: 100_000,
            recurrent_hysteresis_bonus_ppm: 15_000,
            primary_switch_margin_ppm: 30_000,
            precision_escalation_margin_ppm: 50_000,
            disagreement_precision_threshold_ppm: 250_000,
        }
    }
}

impl MorPolicy {
    pub fn validate(self) -> Result<Self, MorError> {
        if self.min_routers == 0 || self.max_routers < self.min_routers {
            return Err(MorError::InvalidPolicy("router count range"));
        }
        if self.min_experts == 0 || self.max_experts < self.min_experts {
            return Err(MorError::InvalidPolicy("expert count range"));
        }
        let ppm_values = [
            self.router_mass_threshold_ppm,
            self.base_expert_mass_threshold_ppm,
            self.disagreement_extra_mass_ppm,
            self.singleton_denoise_ppm,
            self.singleton_bypass_credibility_ppm,
            self.singleton_bypass_score_ppm,
            self.consensus_bonus_per_router_ppm,
            self.residency_penalty_cap_ppm,
            self.recurrent_hysteresis_bonus_ppm,
            self.primary_switch_margin_ppm,
            self.precision_escalation_margin_ppm,
            self.disagreement_precision_threshold_ppm,
        ];
        if ppm_values.iter().any(|value| *value > MOR_SCALE_PPM) {
            return Err(MorError::PolicyPpmOutOfRange);
        }
        if self.singleton_denoise_ppm == 0 {
            return Err(MorError::InvalidPolicy("zero singleton evidence"));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpertProposal {
    pub page_id: PageId,
    pub score_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterInput {
    pub router_id: RouterId,
    pub affinity_ppm: u32,
    pub reliability_ppm: u32,
    pub confidence_ppm: u32,
    pub balance_bias_ppm: i32,
    pub proposals: Vec<ExpertProposal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpertRuntimeHint {
    pub page_id: PageId,
    pub balance_bias_ppm: i32,
    pub residency_penalty_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreviousRoute {
    pub primary: Option<PageId>,
    pub selected: Vec<PageId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveRouterEvidence {
    pub router_id: RouterId,
    pub eligibility_ppm: u32,
    pub effective_weight_ppm: u32,
    pub denoise_multiplier_ppm: u32,
    pub top_expert: PageId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpertEvidence {
    pub page_id: PageId,
    pub consensus_ppm: u32,
    pub support_routers: u32,
    pub support_weight_ppm: u32,
    pub consensus_bonus_ppm: u32,
    pub residency_penalty_ppm: u32,
    pub hysteresis_bonus_ppm: u32,
    pub final_score_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorDecision {
    pub active_routers: Vec<ActiveRouterEvidence>,
    pub ranked_experts: Vec<ExpertEvidence>,
    pub selected_experts: Vec<PageId>,
    pub noise_ppm: u32,
    pub structural_disagreement_ppm: u32,
    pub expert_mass_threshold_ppm: u32,
    pub request_precision_escalation: bool,
    pub hysteresis_retained_previous_primary: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixtureOfRouters {
    policy: MorPolicy,
}

impl MixtureOfRouters {
    pub fn new(policy: MorPolicy) -> Result<Self, MorError> {
        Ok(Self {
            policy: policy.validate()?,
        })
    }

    pub fn route(
        &self,
        routers: &[RouterInput],
        runtime_hints: &[ExpertRuntimeHint],
        previous: &PreviousRoute,
    ) -> Result<MorDecision, MorError> {
        self.validate_inputs(routers, runtime_hints)?;
        let mut active = self.select_routers(routers)?;
        let groups = top_groups(&active)?;
        let total_top = groups.values().map(|group| group.weight).sum::<u128>();
        if total_top == 0 {
            return Err(MorError::ZeroRouterMass);
        }

        let singleton_mass = groups
            .values()
            .filter(|group| group.count == 1)
            .map(|group| group.weight)
            .sum::<u128>();
        let noise_ppm = ratio_ppm(singleton_mass, total_top)?;

        let coherent = groups
            .values()
            .filter(|group| group.count >= 2)
            .collect::<Vec<_>>();
        let structural_disagreement_ppm = if coherent.len() < 2 {
            0
        } else {
            let coherent_total = coherent.iter().map(|group| group.weight).sum::<u128>();
            let primary = coherent.iter().map(|group| group.weight).max().unwrap_or(0);
            ratio_ppm(coherent_total.saturating_sub(primary), total_top)?
        };

        self.denoise(&mut active, &groups)?;
        let active_routers = active
            .iter()
            .map(|router| {
                let top = top_proposal(&router.input.proposals)
                    .ok_or(MorError::EmptyProposals(router.input.router_id))?;
                Ok(ActiveRouterEvidence {
                    router_id: router.input.router_id,
                    eligibility_ppm: router.eligibility_ppm,
                    effective_weight_ppm: router.weight_ppm,
                    denoise_multiplier_ppm: router.denoise_multiplier_ppm,
                    top_expert: top.page_id,
                })
            })
            .collect::<Result<Vec<_>, MorError>>()?;

        let hints = hint_map(runtime_hints)?;
        let raw = self.rank_experts(&active, &hints, previous, false)?;
        let raw_primary = raw.first().map(|expert| expert.page_id);
        let ranked_experts = self.rank_experts(&active, &hints, previous, true)?;
        let final_primary = ranked_experts.first().map(|expert| expert.page_id);
        let hysteresis_retained_previous_primary = previous.primary.is_some()
            && raw_primary != final_primary
            && final_primary == previous.primary;

        let extra = mul_ppm(
            structural_disagreement_ppm,
            self.policy.disagreement_extra_mass_ppm,
        );
        let expert_mass_threshold_ppm = self
            .policy
            .base_expert_mass_threshold_ppm
            .saturating_add(extra)
            .min(MOR_SCALE_PPM);
        let selected_experts = self.select_experts(&ranked_experts, expert_mass_threshold_ppm)?;

        let margin = match ranked_experts.as_slice() {
            [first, second, ..] => first.final_score_ppm.saturating_sub(second.final_score_ppm),
            [_] => MOR_SCORE_CEILING_PPM,
            [] => return Err(MorError::ZeroExpertMass),
        };
        let request_precision_escalation = margin <= self.policy.precision_escalation_margin_ppm
            || structural_disagreement_ppm >= self.policy.disagreement_precision_threshold_ppm;

        Ok(MorDecision {
            active_routers,
            ranked_experts,
            selected_experts,
            noise_ppm,
            structural_disagreement_ppm,
            expert_mass_threshold_ppm,
            request_precision_escalation,
            hysteresis_retained_previous_primary,
        })
    }

    fn validate_inputs(
        &self,
        routers: &[RouterInput],
        runtime_hints: &[ExpertRuntimeHint],
    ) -> Result<(), MorError> {
        if routers.len() < self.policy.min_routers {
            return Err(MorError::InsufficientRouters {
                required: self.policy.min_routers,
                actual: routers.len(),
            });
        }
        let mut ids = BTreeSet::new();
        for router in routers {
            if !ids.insert(router.router_id) {
                return Err(MorError::DuplicateRouter(router.router_id));
            }
            validate_ppm(router.affinity_ppm)?;
            validate_ppm(router.reliability_ppm)?;
            validate_ppm(router.confidence_ppm)?;
            if router.proposals.is_empty() {
                return Err(MorError::EmptyProposals(router.router_id));
            }
            let mut pages = BTreeSet::new();
            for proposal in &router.proposals {
                validate_ppm(proposal.score_ppm)?;
                if !pages.insert(proposal.page_id.0) {
                    return Err(MorError::DuplicateProposal {
                        router_id: router.router_id,
                        page_id: proposal.page_id,
                    });
                }
            }
        }
        let mut hint_pages = BTreeSet::new();
        for hint in runtime_hints {
            if !hint_pages.insert(hint.page_id.0) {
                return Err(MorError::DuplicateRuntimeHint(hint.page_id));
            }
        }
        Ok(())
    }

    fn select_routers<'a>(
        &self,
        routers: &'a [RouterInput],
    ) -> Result<Vec<SelectedRouter<'a>>, MorError> {
        let mut ranked = routers
            .iter()
            .map(|input| {
                let base = mul_ppm(input.affinity_ppm, input.reliability_ppm);
                let eligibility_ppm = apply_bias(base, input.balance_bias_ppm, MOR_SCALE_PPM);
                SelectedRouter {
                    input,
                    eligibility_ppm,
                    weight_ppm: mul_ppm(eligibility_ppm, input.confidence_ppm),
                    denoise_multiplier_ppm: MOR_SCALE_PPM,
                }
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            right
                .eligibility_ppm
                .cmp(&left.eligibility_ppm)
                .then_with(|| left.input.router_id.cmp(&right.input.router_id))
        });

        let total = ranked
            .iter()
            .map(|router| router.eligibility_ppm as u128)
            .sum::<u128>();
        if total == 0 {
            return Err(MorError::ZeroRouterMass);
        }
        let target = mass_target(total, self.policy.router_mass_threshold_ppm)?;
        let mut cumulative = 0_u128;
        let mut selected = Vec::new();
        for router in ranked {
            if selected.len() >= self.policy.max_routers {
                break;
            }
            if selected.len() >= self.policy.min_routers && cumulative >= target {
                break;
            }
            cumulative += router.eligibility_ppm as u128;
            selected.push(router);
        }
        if selected.len() < self.policy.min_routers {
            return Err(MorError::InsufficientRouters {
                required: self.policy.min_routers,
                actual: selected.len(),
            });
        }
        if selected.iter().all(|router| router.weight_ppm == 0) {
            return Err(MorError::ZeroRouterMass);
        }
        Ok(selected)
    }

    fn denoise(
        &self,
        active: &mut [SelectedRouter<'_>],
        groups: &BTreeMap<u32, TopGroup>,
    ) -> Result<(), MorError> {
        let count = active.len();
        for router in active {
            let top = top_proposal(&router.input.proposals)
                .ok_or(MorError::EmptyProposals(router.input.router_id))?;
            let group = groups
                .get(&top.page_id.0)
                .ok_or(MorError::InternalInvariant)?;
            let credibility = mul_ppm(router.input.reliability_ppm, router.input.confidence_ppm);
            let bypass = credibility >= self.policy.singleton_bypass_credibility_ppm
                && top.score_ppm >= self.policy.singleton_bypass_score_ppm;
            if count >= 3 && group.count == 1 && !bypass {
                router.denoise_multiplier_ppm = self.policy.singleton_denoise_ppm;
                router.weight_ppm = mul_ppm(router.weight_ppm, router.denoise_multiplier_ppm);
            }
        }
        if active.iter().all(|router| router.weight_ppm == 0) {
            return Err(MorError::ZeroRouterMass);
        }
        Ok(())
    }

    fn rank_experts(
        &self,
        active: &[SelectedRouter<'_>],
        hints: &BTreeMap<u32, ExpertRuntimeHint>,
        previous: &PreviousRoute,
        use_hysteresis: bool,
    ) -> Result<Vec<ExpertEvidence>, MorError> {
        let total_weight = active
            .iter()
            .map(|router| router.weight_ppm as u128)
            .sum::<u128>();
        if total_weight == 0 {
            return Err(MorError::ZeroRouterMass);
        }
        let mut pages = BTreeSet::new();
        for router in active {
            for proposal in &router.input.proposals {
                pages.insert(proposal.page_id.0);
            }
        }

        let mut ranked = Vec::with_capacity(pages.len());
        for page in pages {
            let mut weighted_score = 0_u128;
            let mut support_weight = 0_u128;
            let mut support_routers = 0_u32;
            for router in active {
                if let Some(proposal) = router
                    .input
                    .proposals
                    .iter()
                    .find(|proposal| proposal.page_id.0 == page)
                {
                    weighted_score += router.weight_ppm as u128 * proposal.score_ppm as u128;
                    support_weight += router.weight_ppm as u128;
                    support_routers = support_routers.saturating_add(1);
                }
            }
            let consensus_ppm = u32::try_from(weighted_score / total_weight)
                .map_err(|_| MorError::ArithmeticOverflow)?;
            let support_weight_ppm = ratio_ppm(support_weight, total_weight)?;
            let consensus_bonus_ppm = self
                .policy
                .consensus_bonus_per_router_ppm
                .saturating_mul(support_routers.saturating_sub(1))
                .min(MOR_SCALE_PPM);
            let hint = hints.get(&page).copied().unwrap_or(ExpertRuntimeHint {
                page_id: PageId(page),
                balance_bias_ppm: 0,
                residency_penalty_ppm: 0,
            });
            let residency_penalty_ppm = hint
                .residency_penalty_ppm
                .min(self.policy.residency_penalty_cap_ppm);
            let mut hysteresis_bonus_ppm = 0_u32;
            if use_hysteresis {
                if previous.selected.iter().any(|selected| selected.0 == page) {
                    hysteresis_bonus_ppm = hysteresis_bonus_ppm
                        .saturating_add(self.policy.recurrent_hysteresis_bonus_ppm);
                }
                if previous.primary.map(|primary| primary.0) == Some(page) {
                    hysteresis_bonus_ppm =
                        hysteresis_bonus_ppm.saturating_add(self.policy.primary_switch_margin_ppm);
                }
            }
            let final_score_ppm = (consensus_ppm as i64)
                .saturating_add(consensus_bonus_ppm as i64)
                .saturating_add(hint.balance_bias_ppm as i64)
                .saturating_add(hysteresis_bonus_ppm as i64)
                .saturating_sub(residency_penalty_ppm as i64)
                .clamp(0, MOR_SCORE_CEILING_PPM as i64) as u32;
            ranked.push(ExpertEvidence {
                page_id: PageId(page),
                consensus_ppm,
                support_routers,
                support_weight_ppm,
                consensus_bonus_ppm,
                residency_penalty_ppm,
                hysteresis_bonus_ppm,
                final_score_ppm,
            });
        }
        ranked.sort_by(|left, right| {
            right
                .final_score_ppm
                .cmp(&left.final_score_ppm)
                .then_with(|| left.page_id.0.cmp(&right.page_id.0))
        });
        if ranked
            .first()
            .map(|expert| expert.final_score_ppm)
            .unwrap_or(0)
            == 0
        {
            return Err(MorError::ZeroExpertMass);
        }
        Ok(ranked)
    }

    fn select_experts(
        &self,
        ranked: &[ExpertEvidence],
        threshold_ppm: u32,
    ) -> Result<Vec<PageId>, MorError> {
        let total = ranked
            .iter()
            .map(|expert| expert.final_score_ppm as u128)
            .sum::<u128>();
        if total == 0 {
            return Err(MorError::ZeroExpertMass);
        }
        let target = mass_target(total, threshold_ppm)?;
        let mut cumulative = 0_u128;
        let mut selected = Vec::new();
        for expert in ranked {
            if selected.len() >= self.policy.max_experts {
                break;
            }
            if selected.len() >= self.policy.min_experts && cumulative >= target {
                break;
            }
            cumulative += expert.final_score_ppm as u128;
            selected.push(expert.page_id);
        }
        if selected.len() < self.policy.min_experts {
            return Err(MorError::InsufficientExperts);
        }
        Ok(selected)
    }
}

#[derive(Debug, Clone, Copy)]
struct SelectedRouter<'a> {
    input: &'a RouterInput,
    eligibility_ppm: u32,
    weight_ppm: u32,
    denoise_multiplier_ppm: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct TopGroup {
    count: u32,
    weight: u128,
}

fn top_groups(active: &[SelectedRouter<'_>]) -> Result<BTreeMap<u32, TopGroup>, MorError> {
    let mut groups = BTreeMap::new();
    for router in active {
        let top = top_proposal(&router.input.proposals)
            .ok_or(MorError::EmptyProposals(router.input.router_id))?;
        let group = groups
            .entry(top.page_id.0)
            .or_insert_with(TopGroup::default);
        group.count = group.count.saturating_add(1);
        group.weight = group.weight.saturating_add(router.weight_ppm as u128);
    }
    Ok(groups)
}

fn top_proposal(proposals: &[ExpertProposal]) -> Option<ExpertProposal> {
    proposals.iter().copied().max_by(|left, right| {
        left.score_ppm
            .cmp(&right.score_ppm)
            .then_with(|| right.page_id.0.cmp(&left.page_id.0))
    })
}

fn hint_map(hints: &[ExpertRuntimeHint]) -> Result<BTreeMap<u32, ExpertRuntimeHint>, MorError> {
    let mut map = BTreeMap::new();
    for hint in hints {
        if map.insert(hint.page_id.0, *hint).is_some() {
            return Err(MorError::DuplicateRuntimeHint(hint.page_id));
        }
    }
    Ok(map)
}

fn validate_ppm(value: u32) -> Result<(), MorError> {
    if value > MOR_SCALE_PPM {
        return Err(MorError::InputPpmOutOfRange(value));
    }
    Ok(())
}

fn mul_ppm(left: u32, right: u32) -> u32 {
    ((left as u64 * right as u64) / MOR_SCALE_PPM as u64) as u32
}

fn apply_bias(base: u32, bias: i32, ceiling: u32) -> u32 {
    (base as i64 + bias as i64).clamp(0, ceiling as i64) as u32
}

fn ratio_ppm(numerator: u128, denominator: u128) -> Result<u32, MorError> {
    let scaled = numerator
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(MorError::ArithmeticOverflow)?
        / denominator;
    u32::try_from(scaled).map_err(|_| MorError::ArithmeticOverflow)
}

fn mass_target(total: u128, threshold_ppm: u32) -> Result<u128, MorError> {
    total
        .checked_mul(threshold_ppm as u128)
        .ok_or(MorError::ArithmeticOverflow)
        .map(|scaled| scaled.div_ceil(MOR_SCALE_PPM as u128))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorError {
    InvalidPolicy(&'static str),
    PolicyPpmOutOfRange,
    InputPpmOutOfRange(u32),
    InsufficientRouters {
        required: usize,
        actual: usize,
    },
    InsufficientExperts,
    DuplicateRouter(RouterId),
    EmptyProposals(RouterId),
    DuplicateProposal {
        router_id: RouterId,
        page_id: PageId,
    },
    DuplicateRuntimeHint(PageId),
    ZeroRouterMass,
    ZeroExpertMass,
    ArithmeticOverflow,
    InternalInvariant,
}

impl fmt::Display for MorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorError {}

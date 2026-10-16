//! SML-local Elastic Activation Controller (EAC) reference.
//!
//! The controller adjusts the active weight budget and recurrent depth using
//! inference-visible evidence. It delegates RAM/VRAM placement to the existing
//! ElasticWeightPlanner. Planning is read-only; a rejected planner commit does
//! not advance controller state. No learned-quality or physical-speed claim.

use core::fmt;
use std::collections::BTreeSet;

use crate::{
    elastic_weights::{ElasticWeightError, ElasticWeightPlan, ElasticWeightPlanner},
    PageId,
};

pub const EAC_PPM: u32 = 1_000_000;
pub const EAC_CONTRACT_V1: &str = "sml.elastic-activation@1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EacPolicy {
    pub initial_active_ppm: u32,
    pub min_active_ppm: u32,
    pub max_active_ppm: u32,
    pub width_step_ppm: u32,
    pub min_depth: u8,
    pub max_depth: u8,
    pub ambiguous_margin_ppm: u32,
    pub disagreement_threshold_ppm: u32,
    pub pressure_threshold_ppm: u32,
    pub stable_cycles_to_contract: u32,
    pub cooldown_cycles: u32,
    /// Physical envelopes, including explicit non-weight headroom.
    pub ram_ceiling_bytes: u64,
    pub vram_ceiling_bytes: u64,
    pub ram_reserved_bytes: u64,
    pub vram_reserved_bytes: u64,
}

impl EacPolicy {
    pub fn validate(self) -> Result<Self, EacError> {
        if self.min_active_ppm == 0
            || self.min_active_ppm > self.initial_active_ppm
            || self.initial_active_ppm > self.max_active_ppm
            || self.max_active_ppm > EAC_PPM
            || self.width_step_ppm == 0
            || self.width_step_ppm > EAC_PPM
            || self.min_depth == 0
            || self.min_depth > self.max_depth
            || self.stable_cycles_to_contract == 0
            || self.ambiguous_margin_ppm > EAC_PPM
            || self.disagreement_threshold_ppm > EAC_PPM
            || self.pressure_threshold_ppm > EAC_PPM
            || self.ram_reserved_bytes > self.ram_ceiling_bytes
            || self.vram_reserved_bytes > self.vram_ceiling_bytes
        {
            return Err(EacError::InvalidPolicy);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EacEvidence {
    /// MOR top-1/top-2 margin, not a calibrated correctness probability.
    pub route_margin_ppm: u32,
    pub structural_disagreement_ppm: u32,
    /// Independent, inference-visible verification failure.
    pub verification_failed: bool,
    /// Stability observation from recurrence, not a protected target label.
    pub stable: bool,
    /// Observed memory pressure (parts per million).
    pub memory_pressure_ppm: u32,
}

impl EacEvidence {
    fn validate(self) -> Result<(), EacError> {
        if [
            self.route_margin_ppm,
            self.structural_disagreement_ppm,
            self.memory_pressure_ppm,
        ]
        .iter()
        .any(|&part| part > EAC_PPM)
        {
            return Err(EacError::InvalidEvidence);
        }
        Ok(())
    }

    fn ambiguous(self, policy: EacPolicy) -> bool {
        self.verification_failed
            || self.route_margin_ppm <= policy.ambiguous_margin_ppm
            || self.structural_disagreement_ppm >= policy.disagreement_threshold_ppm
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EacAction {
    Hold,
    ExpandWidth,
    ExpandDepth,
    ContractWidth,
    ContractDepth,
    ContractForPressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EacNext {
    active_ppm: u32,
    depth: u8,
    stable_streak: u32,
    cooldown: u32,
    epoch: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EacProposal {
    /// Version of the proposed SML-only controller contract.
    pub contract: &'static str,
    pub action: EacAction,
    pub active_ceiling_parameters: u64,
    pub selected_pages: Vec<PageId>,
    pub recurrent_depth: u8,
    pub weight_plan: ElasticWeightPlan,
    base_generation: u64,
    next: EacNext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElasticActivationController {
    policy: EacPolicy,
    state: EacNext,
    generation: u64,
}

impl ElasticActivationController {
    pub fn new(policy: EacPolicy) -> Result<Self, EacError> {
        let policy = policy.validate()?;
        Ok(Self {
            state: EacNext {
                active_ppm: policy.initial_active_ppm,
                depth: policy.min_depth,
                stable_streak: 0,
                cooldown: 0,
                epoch: 0,
            },
            policy,
            generation: 0,
        })
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn active_ppm(&self) -> u32 {
        self.state.active_ppm
    }

    #[must_use]
    pub const fn recurrent_depth(&self) -> u8 {
        self.state.depth
    }

    /// Candidate pages must be supplied in MOR-ranked order. This method
    /// never widens the registry or invents expert eligibility.
    pub fn propose(
        &self,
        planner: &ElasticWeightPlanner,
        ranked_candidates: &[PageId],
        evidence: EacEvidence,
        epoch: u64,
    ) -> Result<EacProposal, EacError> {
        evidence.validate()?;
        if epoch <= self.state.epoch {
            return Err(EacError::EpochNotIncreasing);
        }

        let mut next = self.state;
        next.epoch = epoch;
        next.cooldown = next.cooldown.saturating_sub(1);
        next.stable_streak = if evidence.stable && !evidence.ambiguous(self.policy) {
            self.state.stable_streak.saturating_add(1)
        } else {
            0
        };

        let mut action = EacAction::Hold;
        let pressure = evidence.memory_pressure_ppm >= self.policy.pressure_threshold_ppm;
        if pressure {
            if next.depth > self.policy.min_depth {
                next.depth -= 1;
                action = EacAction::ContractForPressure;
            } else if next.active_ppm > self.policy.min_active_ppm {
                next.active_ppm = next
                    .active_ppm
                    .saturating_sub(self.policy.width_step_ppm)
                    .max(self.policy.min_active_ppm);
                action = EacAction::ContractForPressure;
            }
        } else if self.state.cooldown == 0 && evidence.ambiguous(self.policy) {
            if next.active_ppm < self.policy.max_active_ppm {
                next.active_ppm = next
                    .active_ppm
                    .saturating_add(self.policy.width_step_ppm)
                    .min(self.policy.max_active_ppm);
                action = EacAction::ExpandWidth;
            } else if next.depth < self.policy.max_depth {
                next.depth += 1;
                action = EacAction::ExpandDepth;
            }
        } else if self.state.cooldown == 0
            && next.stable_streak >= self.policy.stable_cycles_to_contract
        {
            if next.depth > self.policy.min_depth {
                next.depth -= 1;
                action = EacAction::ContractDepth;
            } else if next.active_ppm > self.policy.min_active_ppm {
                next.active_ppm = next
                    .active_ppm
                    .saturating_sub(self.policy.width_step_ppm)
                    .max(self.policy.min_active_ppm);
                action = EacAction::ContractWidth;
            }
        }
        if action != EacAction::Hold {
            next.cooldown = self.policy.cooldown_cycles;
            next.stable_streak = 0;
        }

        let total = planner.pages().iter().try_fold(0_u64, |sum, p| {
            sum.checked_add(p.parameters).ok_or(EacError::Overflow)
        })?;
        if total == 0 {
            return Err(EacError::NoModelParameters);
        }
        let ceiling = ppm_floor(total, next.active_ppm)?;
        let minimum = ppm_ceil(total, self.policy.min_active_ppm)?;
        if ceiling < minimum {
            return Err(EacError::ActiveFloorUnavailable {
                required: minimum,
                selected: ceiling,
            });
        }

        let mut known = BTreeSet::new();
        let mut selected = Vec::new();
        let mut selected_parameters = 0_u64;
        for &page_id in ranked_candidates {
            if !known.insert(page_id) {
                return Err(EacError::DuplicateCandidate(page_id));
            }
            let page = planner
                .pages()
                .iter()
                .find(|p| p.page_id == page_id)
                .ok_or(EacError::UnknownCandidate(page_id))?;
            if page.parameters <= ceiling.saturating_sub(selected_parameters) {
                selected_parameters = selected_parameters
                    .checked_add(page.parameters)
                    .ok_or(EacError::Overflow)?;
                selected.push(page_id);
            }
        }
        if selected_parameters < minimum {
            return Err(EacError::ActiveFloorUnavailable {
                required: minimum,
                selected: selected_parameters,
            });
        }

        // The existing planner independently validates active, RAM, VRAM and
        // residency constraints. EAC has additional explicit headroom limits.
        let weight_plan = planner
            .plan_cycle(&selected, epoch)
            .map_err(EacError::Planner)?;
        if weight_plan.precision_changes != 0 {
            return Err(EacError::UnmaterializedPrecisionTransition);
        }
        if weight_plan
            .ram_bytes
            .checked_add(self.policy.ram_reserved_bytes)
            .ok_or(EacError::Overflow)?
            > self.policy.ram_ceiling_bytes
            || weight_plan
                .vram_bytes
                .checked_add(self.policy.vram_reserved_bytes)
                .ok_or(EacError::Overflow)?
                > self.policy.vram_ceiling_bytes
        {
            return Err(EacError::MemoryEnvelopeExceeded);
        }

        Ok(EacProposal {
            contract: EAC_CONTRACT_V1,
            action,
            active_ceiling_parameters: ceiling,
            selected_pages: selected,
            recurrent_depth: next.depth,
            weight_plan,
            base_generation: self.generation,
            next,
        })
    }

    /// Atomically advances controller state only after the native planner
    /// accepts the exact plan. Rejected or stale plans leave both unchanged.
    pub fn commit(
        &mut self,
        planner: &mut ElasticWeightPlanner,
        proposal: &EacProposal,
    ) -> Result<(), EacError> {
        if proposal.contract != EAC_CONTRACT_V1
            || proposal.base_generation != self.generation
            || proposal.next.epoch <= self.state.epoch
        {
            return Err(EacError::StaleOrInvalidProposal);
        }
        // The complete weight plan is validated again by planner.commit().
        planner
            .commit(&proposal.weight_plan)
            .map_err(EacError::Planner)?;
        self.state = proposal.next;
        self.generation = self.generation.saturating_add(1);
        Ok(())
    }
}

fn ppm_floor(parameters: u64, ppm: u32) -> Result<u64, EacError> {
    u64::try_from((parameters as u128 * ppm as u128) / EAC_PPM as u128)
        .map_err(|_| EacError::Overflow)
}

fn ppm_ceil(parameters: u64, ppm: u32) -> Result<u64, EacError> {
    let scaled = parameters as u128 * ppm as u128;
    u64::try_from(scaled.div_ceil(EAC_PPM as u128)).map_err(|_| EacError::Overflow)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EacError {
    InvalidPolicy,
    InvalidEvidence,
    EpochNotIncreasing,
    NoModelParameters,
    Overflow,
    UnknownCandidate(PageId),
    DuplicateCandidate(PageId),
    ActiveFloorUnavailable { required: u64, selected: u64 },
    UnmaterializedPrecisionTransition,
    MemoryEnvelopeExceeded,
    StaleOrInvalidProposal,
    Planner(ElasticWeightError),
}

impl fmt::Display for EacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for EacError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elastic_weights::{
        ElasticWeightPage, ElasticWeightPolicy, ResidencyTier, WeightPrecision,
    };

    fn policy() -> EacPolicy {
        EacPolicy {
            initial_active_ppm: 250_000,
            min_active_ppm: 250_000,
            max_active_ppm: 1_000_000,
            width_step_ppm: 250_000,
            min_depth: 1,
            max_depth: 3,
            ambiguous_margin_ppm: 100_000,
            disagreement_threshold_ppm: 500_000,
            pressure_threshold_ppm: 900_000,
            stable_cycles_to_contract: 2,
            cooldown_cycles: 0,
            ram_ceiling_bytes: 400,
            vram_ceiling_bytes: 0,
            ram_reserved_bytes: 100,
            vram_reserved_bytes: 0,
        }
    }

    fn planner() -> ElasticWeightPlanner {
        let pages = (0..4)
            .map(|id| {
                ElasticWeightPage::new(
                    PageId(id),
                    100,
                    WeightPrecision::Boolean,
                    ResidencyTier::Disk,
                    400_000,
                )
                .unwrap()
            })
            .collect();
        ElasticWeightPlanner::new(
            pages,
            ElasticWeightPolicy {
                ram_limit_bytes: 400,
                vram_limit_bytes: 0,
                max_active_parameters: 400,
                ternary_threshold_ppm: 1_000_000,
                residual4_threshold_ppm: 1_000_000,
                vram_threshold_ppm: 1_000_000,
            },
        )
        .unwrap()
    }

    fn candidates() -> [PageId; 4] {
        [PageId(0), PageId(1), PageId(2), PageId(3)]
    }

    fn ambiguous() -> EacEvidence {
        EacEvidence {
            route_margin_ppm: 0,
            structural_disagreement_ppm: 800_000,
            verification_failed: false,
            stable: false,
            memory_pressure_ppm: 0,
        }
    }

    fn stable() -> EacEvidence {
        EacEvidence {
            route_margin_ppm: 900_000,
            structural_disagreement_ppm: 0,
            verification_failed: false,
            stable: true,
            memory_pressure_ppm: 0,
        }
    }

    #[test]
    fn first_plan_is_read_only_and_commit_is_exact() {
        let mut eac = ElasticActivationController::new(policy()).unwrap();
        let mut weights = planner();
        let proposal = eac.propose(&weights, &candidates(), stable(), 1).unwrap();
        assert_eq!(proposal.action, EacAction::Hold);
        assert_eq!(proposal.active_ceiling_parameters, 100);
        assert_eq!(proposal.selected_pages, vec![PageId(0)]);
        assert_eq!(proposal.weight_plan.active_parameters, 100);
        assert_eq!(weights.accounting().unwrap().active_parameters, 0);
        assert_eq!(eac.generation(), 0);
        eac.commit(&mut weights, &proposal).unwrap();
        assert_eq!(eac.generation(), 1);
        assert_eq!(weights.accounting().unwrap().active_parameters, 100);
        assert_eq!(weights.generation(), 1);
    }

    #[test]
    fn ambiguity_widens_to_full_then_deepens() {
        let mut eac = ElasticActivationController::new(policy()).unwrap();
        let mut weights = planner();
        for (epoch, expected) in [(1, 200), (2, 300), (3, 400)] {
            let proposal = eac
                .propose(&weights, &candidates(), ambiguous(), epoch)
                .unwrap();
            assert_eq!(proposal.action, EacAction::ExpandWidth);
            assert_eq!(proposal.weight_plan.active_parameters, expected);
            eac.commit(&mut weights, &proposal).unwrap();
        }
        assert_eq!(eac.active_ppm(), EAC_PPM);
        let proposal = eac
            .propose(&weights, &candidates(), ambiguous(), 4)
            .unwrap();
        assert_eq!(proposal.action, EacAction::ExpandDepth);
        assert_eq!(proposal.recurrent_depth, 2);
        eac.commit(&mut weights, &proposal).unwrap();
        assert_eq!(eac.recurrent_depth(), 2);
    }

    #[test]
    fn stable_signal_contracts_after_hysteresis() {
        let mut eac = ElasticActivationController::new(policy()).unwrap();
        let mut weights = planner();
        let expand = eac
            .propose(&weights, &candidates(), ambiguous(), 1)
            .unwrap();
        eac.commit(&mut weights, &expand).unwrap();
        let hold = eac.propose(&weights, &candidates(), stable(), 2).unwrap();
        assert_eq!(hold.action, EacAction::Hold);
        eac.commit(&mut weights, &hold).unwrap();
        let shrink = eac.propose(&weights, &candidates(), stable(), 3).unwrap();
        assert_eq!(shrink.action, EacAction::ContractWidth);
        assert_eq!(shrink.weight_plan.active_parameters, 100);
    }

    #[test]
    fn pressure_contracts_even_while_ambiguous() {
        let mut p = policy();
        p.cooldown_cycles = 3;
        let mut eac = ElasticActivationController::new(p).unwrap();
        let mut weights = planner();
        let expand = eac
            .propose(&weights, &candidates(), ambiguous(), 1)
            .unwrap();
        eac.commit(&mut weights, &expand).unwrap();
        let mut pressured = ambiguous();
        pressured.memory_pressure_ppm = EAC_PPM;
        let shrink = eac.propose(&weights, &candidates(), pressured, 2).unwrap();
        assert_eq!(shrink.action, EacAction::ContractForPressure);
        assert_eq!(shrink.weight_plan.active_parameters, 100);
    }

    #[test]
    fn controller_and_weight_generation_block_replay() {
        let mut eac = ElasticActivationController::new(policy()).unwrap();
        let mut weights = planner();
        let proposal = eac.propose(&weights, &candidates(), stable(), 1).unwrap();
        eac.commit(&mut weights, &proposal).unwrap();
        assert!(matches!(
            eac.commit(&mut weights, &proposal),
            Err(EacError::StaleOrInvalidProposal)
        ));
        assert_eq!(weights.generation(), 1);
    }

    #[test]
    fn headroom_rejection_does_not_mutate() {
        let mut p = policy();
        p.ram_ceiling_bytes = 110;
        p.ram_reserved_bytes = 100;
        let eac = ElasticActivationController::new(p).unwrap();
        let weights = planner();
        assert_eq!(
            eac.propose(&weights, &candidates(), stable(), 1),
            Err(EacError::MemoryEnvelopeExceeded)
        );
        assert_eq!(weights.generation(), 0);
        assert_eq!(eac.generation(), 0);
    }

    #[test]
    fn unknown_duplicate_and_missing_floor_fail_closed() {
        let eac = ElasticActivationController::new(policy()).unwrap();
        let weights = planner();
        assert_eq!(
            eac.propose(&weights, &[PageId(0), PageId(0)], stable(), 1),
            Err(EacError::DuplicateCandidate(PageId(0)))
        );
        assert_eq!(
            eac.propose(&weights, &[PageId(9)], stable(), 1),
            Err(EacError::UnknownCandidate(PageId(9)))
        );
        assert!(matches!(
            eac.propose(&weights, &[], stable(), 1),
            Err(EacError::ActiveFloorUnavailable { .. })
        ));
    }

    #[test]
    fn no_unearned_precision_promotion() {
        let mut weights = planner();
        weights.record_importance(PageId(0), 1_000_000).unwrap();
        let eac = ElasticActivationController::new(policy()).unwrap();
        assert_eq!(
            eac.propose(&weights, &candidates(), stable(), 1),
            Err(EacError::UnmaterializedPrecisionTransition)
        );
    }

    #[test]
    fn invalid_signals_and_policies_fail_closed() {
        let mut p = policy();
        p.max_active_ppm = EAC_PPM + 1;
        assert_eq!(
            ElasticActivationController::new(p),
            Err(EacError::InvalidPolicy)
        );
        let eac = ElasticActivationController::new(policy()).unwrap();
        let weights = planner();
        let mut signal = stable();
        signal.route_margin_ppm = EAC_PPM + 1;
        assert_eq!(
            eac.propose(&weights, &candidates(), signal, 1),
            Err(EacError::InvalidEvidence)
        );
    }
}

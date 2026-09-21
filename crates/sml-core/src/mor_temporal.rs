//! Temporal denoising for SML Mixture-of-Routers.
//!
//! A one-cycle route change may be useful evidence without being a stable
//! transition. This module keeps a confirmed recurrent route, requires bounded
//! persistence for ordinary changes, and allows high-margin urgent bypasses.

use core::fmt;

use crate::{
    mor::{MorDecision, PreviousRoute, MOR_SCALE_PPM, MOR_SCORE_CEILING_PPM},
    mor_controller::{MorController, MorControllerError, MorControllerPolicy, MorCycleReport},
    PageId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorTemporalPolicy {
    pub confirmation_cycles: u32,
    pub noise_extra_confirmation_cycles: u32,
    pub disagreement_extra_confirmation_cycles: u32,
    pub max_confirmation_cycles: u32,
    pub noise_threshold_ppm: u32,
    pub disagreement_threshold_ppm: u32,
    pub urgent_margin_ppm: u32,
}

impl Default for MorTemporalPolicy {
    fn default() -> Self {
        Self {
            confirmation_cycles: 2,
            noise_extra_confirmation_cycles: 1,
            disagreement_extra_confirmation_cycles: 1,
            max_confirmation_cycles: 4,
            noise_threshold_ppm: 300_000,
            disagreement_threshold_ppm: 250_000,
            urgent_margin_ppm: 250_000,
        }
    }
}

impl MorTemporalPolicy {
    pub fn validate(self) -> Result<Self, MorTemporalError> {
        if self.confirmation_cycles == 0 {
            return Err(MorTemporalError::ZeroConfirmationCycles);
        }
        if self.max_confirmation_cycles < self.confirmation_cycles {
            return Err(MorTemporalError::InvalidConfirmationRange {
                base: self.confirmation_cycles,
                maximum: self.max_confirmation_cycles,
            });
        }
        for value in [
            self.noise_threshold_ppm,
            self.disagreement_threshold_ppm,
            self.urgent_margin_ppm,
        ] {
            if value > MOR_SCALE_PPM {
                return Err(MorTemporalError::PolicyPpmOutOfRange(value));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorTemporalTransition {
    Initialized,
    Stable,
    Held,
    Confirmed,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorTemporalReport {
    pub transition: MorTemporalTransition,
    pub observed_primary: PageId,
    pub stable_primary: PageId,
    pub pending_primary: Option<PageId>,
    pub pending_streak: u32,
    pub required_streak: u32,
    pub route_margin_ppm: u32,
    pub noise_ppm: u32,
    pub structural_disagreement_ppm: u32,
    pub accepted_route_switch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MorTemporalReportContext {
    transition: MorTemporalTransition,
    observed_primary: PageId,
    stable_primary: PageId,
    required_streak: u32,
    route_margin_ppm: u32,
    accepted_route_switch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorTemporalDenoiser {
    policy: MorTemporalPolicy,
    stable: PreviousRoute,
    pending_primary: Option<PageId>,
    pending_streak: u32,
}

impl MorTemporalDenoiser {
    pub fn new(policy: MorTemporalPolicy) -> Result<Self, MorTemporalError> {
        Ok(Self {
            policy: policy.validate()?,
            stable: PreviousRoute::default(),
            pending_primary: None,
            pending_streak: 0,
        })
    }

    #[must_use]
    pub fn stable_route(&self) -> &PreviousRoute {
        &self.stable
    }

    #[must_use]
    pub const fn pending_primary(&self) -> Option<PageId> {
        self.pending_primary
    }

    #[must_use]
    pub const fn pending_streak(&self) -> u32 {
        self.pending_streak
    }

    pub fn reset(&mut self) {
        self.stable = PreviousRoute::default();
        self.pending_primary = None;
        self.pending_streak = 0;
    }

    pub fn observe(
        &mut self,
        decision: &MorDecision,
    ) -> Result<MorTemporalReport, MorTemporalError> {
        let observed_primary = decision
            .ranked_experts
            .first()
            .map(|expert| expert.page_id)
            .ok_or(MorTemporalError::EmptyDecision)?;
        let route_margin_ppm = route_margin(decision);
        let candidate = PreviousRoute {
            primary: Some(observed_primary),
            selected: decision.selected_experts.clone(),
        };

        let Some(stable_primary) = self.stable.primary else {
            self.stable = candidate;
            self.clear_pending();
            return Ok(self.report(
                MorTemporalReportContext {
                    transition: MorTemporalTransition::Initialized,
                    observed_primary,
                    stable_primary: observed_primary,
                    required_streak: 1,
                    route_margin_ppm,
                    accepted_route_switch: false,
                },
                decision,
            ));
        };

        if observed_primary == stable_primary {
            self.stable.selected = decision.selected_experts.clone();
            self.clear_pending();
            return Ok(self.report(
                MorTemporalReportContext {
                    transition: MorTemporalTransition::Stable,
                    observed_primary,
                    stable_primary,
                    required_streak: 0,
                    route_margin_ppm,
                    accepted_route_switch: false,
                },
                decision,
            ));
        }

        if route_margin_ppm >= self.policy.urgent_margin_ppm {
            self.stable = candidate;
            self.clear_pending();
            return Ok(self.report(
                MorTemporalReportContext {
                    transition: MorTemporalTransition::Urgent,
                    observed_primary,
                    stable_primary: observed_primary,
                    required_streak: 1,
                    route_margin_ppm,
                    accepted_route_switch: true,
                },
                decision,
            ));
        }

        let required_streak = self.required_streak(decision);
        if self.pending_primary == Some(observed_primary) {
            self.pending_streak = self.pending_streak.saturating_add(1);
        } else {
            self.pending_primary = Some(observed_primary);
            self.pending_streak = 1;
        }

        if self.pending_streak >= required_streak {
            self.stable = candidate;
            self.clear_pending();
            return Ok(self.report(
                MorTemporalReportContext {
                    transition: MorTemporalTransition::Confirmed,
                    observed_primary,
                    stable_primary: observed_primary,
                    required_streak,
                    route_margin_ppm,
                    accepted_route_switch: true,
                },
                decision,
            ));
        }

        Ok(self.report(
            MorTemporalReportContext {
                transition: MorTemporalTransition::Held,
                observed_primary,
                stable_primary,
                required_streak,
                route_margin_ppm,
                accepted_route_switch: false,
            },
            decision,
        ))
    }

    fn required_streak(&self, decision: &MorDecision) -> u32 {
        let mut required = self.policy.confirmation_cycles;
        if decision.noise_ppm >= self.policy.noise_threshold_ppm {
            required = required.saturating_add(self.policy.noise_extra_confirmation_cycles);
        }
        if decision.structural_disagreement_ppm >= self.policy.disagreement_threshold_ppm {
            required = required.saturating_add(self.policy.disagreement_extra_confirmation_cycles);
        }
        required.min(self.policy.max_confirmation_cycles)
    }

    fn clear_pending(&mut self) {
        self.pending_primary = None;
        self.pending_streak = 0;
    }

    fn report(
        &self,
        context: MorTemporalReportContext,
        decision: &MorDecision,
    ) -> MorTemporalReport {
        MorTemporalReport {
            transition: context.transition,
            observed_primary: context.observed_primary,
            stable_primary: context.stable_primary,
            pending_primary: self.pending_primary,
            pending_streak: self.pending_streak,
            required_streak: context.required_streak,
            route_margin_ppm: context.route_margin_ppm,
            noise_ppm: decision.noise_ppm,
            structural_disagreement_ppm: decision.structural_disagreement_ppm,
            accepted_route_switch: context.accepted_route_switch,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalMorCycleReport {
    pub cycle: MorCycleReport,
    pub temporal: MorTemporalReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalMorController {
    controller: MorController,
    denoiser: MorTemporalDenoiser,
}

impl TemporalMorController {
    pub fn new(
        controller_policy: MorControllerPolicy,
        temporal_policy: MorTemporalPolicy,
    ) -> Result<Self, TemporalMorControllerError> {
        Ok(Self {
            controller: MorController::new(controller_policy)
                .map_err(TemporalMorControllerError::Controller)?,
            denoiser: MorTemporalDenoiser::new(temporal_policy)
                .map_err(TemporalMorControllerError::Temporal)?,
        })
    }

    #[must_use]
    pub fn controller(&self) -> &MorController {
        &self.controller
    }

    #[must_use]
    pub fn denoiser(&self) -> &MorTemporalDenoiser {
        &self.denoiser
    }

    pub fn reset(&mut self) {
        self.controller.reset();
        self.denoiser.reset();
    }

    pub fn route_cycle(
        &mut self,
        engine: &crate::mor::MixtureOfRouters,
        routers: &[crate::mor::RouterInput],
        runtime_hints: &[crate::mor::ExpertRuntimeHint],
    ) -> Result<TemporalMorCycleReport, TemporalMorControllerError> {
        let cycle = self
            .controller
            .route_cycle(engine, routers, runtime_hints)
            .map_err(TemporalMorControllerError::Controller)?;
        let temporal = self
            .denoiser
            .observe(&cycle.decision)
            .map_err(TemporalMorControllerError::Temporal)?;
        self.controller
            .replace_previous_route(self.denoiser.stable_route().clone());

        Ok(TemporalMorCycleReport { cycle, temporal })
    }
}

fn route_margin(decision: &MorDecision) -> u32 {
    match decision.ranked_experts.as_slice() {
        [first, second, ..] => first.final_score_ppm.saturating_sub(second.final_score_ppm),
        [_] => MOR_SCORE_CEILING_PPM,
        [] => 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorTemporalError {
    ZeroConfirmationCycles,
    InvalidConfirmationRange { base: u32, maximum: u32 },
    PolicyPpmOutOfRange(u32),
    EmptyDecision,
}

impl fmt::Display for MorTemporalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorTemporalError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemporalMorControllerError {
    Controller(MorControllerError),
    Temporal(MorTemporalError),
}

impl fmt::Display for TemporalMorControllerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for TemporalMorControllerError {}

//! Fully native SML Mixture-of-Routers runtime composition.
//!
//! This runtime connects the already-qualified SML-local mechanisms:
//! state fingerprinting and affinity, router specialization validation,
//! stateful balancing, and temporal denoising. Callers provide an SML Boolean
//! state and optional runtime hints; they no longer assemble RouterInput values.

use core::fmt;

use crate::{
    mor::{ExpertRuntimeHint, MixtureOfRouters, RouterInput},
    mor_affinity::{
        MorAffinityError, NativeAffinityAccounting, NativeAffinityBank, NativeAffinityBatch,
    },
    mor_controller::MorControllerPolicy,
    mor_registry::{MorRegistryAccounting, MorRegistryError, MorRouterRegistry},
    mor_temporal::{
        MorTemporalPolicy, TemporalMorController, TemporalMorControllerError,
        TemporalMorCycleReport,
    },
    BooleanWorkspace,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeMorRuntimeAccounting {
    pub affinity: NativeAffinityAccounting,
    pub registry: MorRegistryAccounting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntimeReport {
    pub affinity: NativeAffinityBatch,
    pub cycle: TemporalMorCycleReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMorRuntime {
    engine: MixtureOfRouters,
    affinity_bank: NativeAffinityBank,
    registry: MorRouterRegistry,
    controller: TemporalMorController,
}

impl NativeMorRuntime {
    pub fn new(
        engine: MixtureOfRouters,
        affinity_bank: NativeAffinityBank,
        registry: MorRouterRegistry,
        controller_policy: MorControllerPolicy,
        temporal_policy: MorTemporalPolicy,
    ) -> Result<Self, NativeMorRuntimeError> {
        validate_static_compatibility(&affinity_bank, &registry)?;

        let controller = TemporalMorController::new(controller_policy, temporal_policy)
            .map_err(NativeMorRuntimeError::Temporal)?;

        Ok(Self {
            engine,
            affinity_bank,
            registry,
            controller,
        })
    }

    #[must_use]
    pub fn affinity_bank(&self) -> &NativeAffinityBank {
        &self.affinity_bank
    }

    #[must_use]
    pub fn registry(&self) -> &MorRouterRegistry {
        &self.registry
    }

    #[must_use]
    pub fn controller(&self) -> &TemporalMorController {
        &self.controller
    }

    #[must_use]
    pub fn accounting(&self) -> NativeMorRuntimeAccounting {
        NativeMorRuntimeAccounting {
            affinity: self.affinity_bank.accounting(),
            registry: self.registry.accounting(),
        }
    }

    pub fn reset(&mut self) {
        self.controller.reset();
    }

    pub fn route_state(
        &mut self,
        state: &BooleanWorkspace,
        runtime_hints: &[ExpertRuntimeHint],
    ) -> Result<NativeMorRuntimeReport, NativeMorRuntimeError> {
        let affinity = self
            .affinity_bank
            .materialize(state)
            .map_err(NativeMorRuntimeError::Affinity)?;

        self.registry
            .validate_complete_inputs(&affinity.routers, runtime_hints)
            .map_err(NativeMorRuntimeError::Registry)?;

        let cycle = self
            .controller
            .route_cycle(&self.engine, &affinity.routers, runtime_hints)
            .map_err(NativeMorRuntimeError::Temporal)?;

        Ok(NativeMorRuntimeReport { affinity, cycle })
    }
}

fn validate_static_compatibility(
    affinity_bank: &NativeAffinityBank,
    registry: &MorRouterRegistry,
) -> Result<(), NativeMorRuntimeError> {
    let routers = affinity_bank
        .routers()
        .iter()
        .map(|router| RouterInput {
            router_id: router.router_id,
            affinity_ppm: 0,
            reliability_ppm: router.reliability_ppm,
            confidence_ppm: router.confidence_ppm,
            balance_bias_ppm: router.balance_bias_ppm,
            proposals: router.proposals.clone(),
        })
        .collect::<Vec<_>>();

    registry
        .validate_complete_inputs(&routers, &[])
        .map_err(NativeMorRuntimeError::Registry)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeMorRuntimeError {
    Affinity(MorAffinityError),
    Registry(MorRegistryError),
    Temporal(TemporalMorControllerError),
}

impl fmt::Display for NativeMorRuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for NativeMorRuntimeError {}

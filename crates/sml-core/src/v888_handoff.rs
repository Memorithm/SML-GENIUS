//! Fail-closed coordination contract for the CSP / V888-BOOL consumer handoff.
//!
//! This module does not implement a connectome simulator and does not qualify a
//! V888-derived topology. It encodes the evidence boundaries required by
//! SML-GENIUS issue #59 before a future V888-BOOL-4.0 consumer experiment may
//! be admitted.

use core::fmt;

pub const V888_BOOL_STRATEGY_REVISION: &str = "2026-09-22.1";
pub const V888_BOOL_STRATEGY_COMMIT: &str = "7f8f08e764dd5f36688533e9f7e7fb2f290f6e17";
pub const V888_BOOL_HANDOFF_STAGE: &str = "V888-BOOL-4.0";
pub const V888_BOOL_INTERFACE_ID: &str = "sml.v888-bool-handoff.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceCountKind {
    MetadataNodes,
    DirectedPairs,
    Contacts,
    RawRecords,
}

impl SourceCountKind {
    const fn bit(self) -> u8 {
        match self {
            Self::MetadataNodes => 1,
            Self::DirectedPairs => 2,
            Self::Contacts => 4,
            Self::RawRecords => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCount {
    pub kind: SourceCountKind,
    pub value: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceScope<'a> {
    pub scope_id: &'a str,
    pub artifact_sha256: &'a str,
    pub counts: &'a [SourceCount],
}

impl SourceScope<'_> {
    pub fn validate(self) -> Result<(), V888ContractError> {
        if self.scope_id.is_empty() {
            return Err(V888ContractError::MissingSourceScope);
        }
        if !is_lower_hex(self.artifact_sha256, 64) {
            return Err(V888ContractError::InvalidSourceArtifactSha256);
        }
        if self.counts.is_empty() {
            return Err(V888ContractError::MissingScopedCounts);
        }

        let mut seen = 0_u8;
        for count in self.counts {
            if count.value == 0 {
                return Err(V888ContractError::ZeroScopedCount(count.kind));
            }
            let bit = count.kind.bit();
            if seen & bit != 0 {
                return Err(V888ContractError::DuplicateScopedCount(count.kind));
            }
            seen |= bit;
        }

        let required = SourceCountKind::MetadataNodes.bit() | SourceCountKind::DirectedPairs.bit();
        if seen & required != required {
            return Err(V888ContractError::MissingExecutableGraphCounts);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DelayedXorProtocol {
    pub delay_steps: u32,
    pub bounded_readout_bits: u32,
    pub fixed_recurrence: bool,
    pub exact_sequential_logic_control: bool,
    pub no_reservoir_control: bool,
    pub topology_matched_control: bool,
    pub simplified_lif_rail: bool,
    pub integer_event_rail: bool,
    pub boolean_recurrent_rail: bool,
    pub hidden_input_history: bool,
    pub cross_trial_state_leakage: bool,
}

impl DelayedXorProtocol {
    pub fn validate(self) -> Result<(), V888ContractError> {
        if self.delay_steps == 0 {
            return Err(V888ContractError::ZeroDelay);
        }
        if self.bounded_readout_bits == 0 {
            return Err(V888ContractError::UnboundedReadout);
        }
        if !self.fixed_recurrence {
            return Err(V888ContractError::RecurrenceNotFrozen);
        }
        if !self.exact_sequential_logic_control
            || !self.no_reservoir_control
            || !self.topology_matched_control
        {
            return Err(V888ContractError::MissingDelayedXorControl);
        }
        if !self.simplified_lif_rail || !self.integer_event_rail || !self.boolean_recurrent_rail {
            return Err(V888ContractError::MissingDynamicsRail);
        }
        if self.hidden_input_history {
            return Err(V888ContractError::HiddenInputHistory);
        }
        if self.cross_trial_state_leakage {
            return Err(V888ContractError::CrossTrialStateLeakage);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffAblations {
    pub graph_only: bool,
    pub existing_router_only: bool,
    pub hybrid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExistingStackReuse {
    pub gf2_anf: bool,
    pub bounded_memory: bool,
    pub routing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndependentValidation {
    pub scirust_executable_graph_qualified: bool,
    pub sml_quality_qualified: bool,
    pub sml_resource_qualified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V888BoolHandoff<'a> {
    pub stage: &'a str,
    pub strategy_revision: &'a str,
    pub strategy_commit: &'a str,
    pub rust_interface_id: &'a str,
    pub rust_source_commit: &'a str,
    pub graph_artifact_sha256: &'a str,
    pub source: SourceScope<'a>,
    pub ablations: HandoffAblations,
    pub existing_stack: ExistingStackReuse,
    pub validation: IndependentValidation,
    pub max_memory_bytes: u64,
    pub max_readout_bits: u32,
    pub hidden_input_history_bypass: bool,
    pub raw_v888_inference_dependency: bool,
    pub protected_final_access: bool,
    pub model_promotion_requested: bool,
    pub default_runtime_policy_change: bool,
}

impl V888BoolHandoff<'_> {
    pub fn validate(self) -> Result<(), V888ContractError> {
        if self.stage != V888_BOOL_HANDOFF_STAGE {
            return Err(V888ContractError::WrongHandoffStage);
        }
        if self.strategy_revision != V888_BOOL_STRATEGY_REVISION
            || self.strategy_commit != V888_BOOL_STRATEGY_COMMIT
        {
            return Err(V888ContractError::WrongStrategyIdentity);
        }
        if self.rust_interface_id != V888_BOOL_INTERFACE_ID {
            return Err(V888ContractError::WrongRustInterfaceIdentity);
        }
        if !is_lower_hex(self.rust_source_commit, 40) {
            return Err(V888ContractError::InvalidRustSourceCommit);
        }
        if !is_lower_hex(self.graph_artifact_sha256, 64) {
            return Err(V888ContractError::InvalidGraphArtifactSha256);
        }
        self.source.validate()?;

        if !self.ablations.graph_only
            || !self.ablations.existing_router_only
            || !self.ablations.hybrid
        {
            return Err(V888ContractError::MissingHandoffAblation);
        }
        if !self.existing_stack.gf2_anf
            || !self.existing_stack.bounded_memory
            || !self.existing_stack.routing
        {
            return Err(V888ContractError::ExistingBooleanStackNotReused);
        }
        if self.max_memory_bytes == 0 || self.max_readout_bits == 0 {
            return Err(V888ContractError::MissingBoundedResources);
        }
        if self.hidden_input_history_bypass {
            return Err(V888ContractError::HiddenInputHistory);
        }
        if self.raw_v888_inference_dependency {
            return Err(V888ContractError::RawV888InferenceDependency);
        }
        if !self.validation.scirust_executable_graph_qualified {
            return Err(V888ContractError::SciRustGraphGateOpen);
        }
        if !self.validation.sml_quality_qualified || !self.validation.sml_resource_qualified {
            return Err(V888ContractError::IndependentSmlValidationMissing);
        }
        if self.protected_final_access {
            return Err(V888ContractError::ProtectedFinalAccess);
        }
        if self.model_promotion_requested {
            return Err(V888ContractError::ModelPromotionRequested);
        }
        if self.default_runtime_policy_change {
            return Err(V888ContractError::DefaultRuntimePolicyChange);
        }
        Ok(())
    }
}

fn is_lower_hex(value: &str, expected_len: usize) -> bool {
    value.len() == expected_len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V888ContractError {
    MissingSourceScope,
    InvalidSourceArtifactSha256,
    MissingScopedCounts,
    ZeroScopedCount(SourceCountKind),
    DuplicateScopedCount(SourceCountKind),
    MissingExecutableGraphCounts,
    ZeroDelay,
    UnboundedReadout,
    RecurrenceNotFrozen,
    MissingDelayedXorControl,
    MissingDynamicsRail,
    HiddenInputHistory,
    CrossTrialStateLeakage,
    WrongHandoffStage,
    WrongStrategyIdentity,
    WrongRustInterfaceIdentity,
    InvalidRustSourceCommit,
    InvalidGraphArtifactSha256,
    MissingHandoffAblation,
    ExistingBooleanStackNotReused,
    MissingBoundedResources,
    RawV888InferenceDependency,
    SciRustGraphGateOpen,
    IndependentSmlValidationMissing,
    ProtectedFinalAccess,
    ModelPromotionRequested,
    DefaultRuntimePolicyChange,
}

impl fmt::Display for V888ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for V888ContractError {}

#[cfg(test)]
mod tests {
    use super::*;

    const COUNTS: [SourceCount; 4] = [
        SourceCount {
            kind: SourceCountKind::MetadataNodes,
            value: 188_508,
        },
        SourceCount {
            kind: SourceCountKind::DirectedPairs,
            value: 13_620_865,
        },
        SourceCount {
            kind: SourceCountKind::Contacts,
            value: 42_309_621,
        },
        SourceCount {
            kind: SourceCountKind::RawRecords,
            value: 198_816_365,
        },
    ];

    fn source() -> SourceScope<'static> {
        SourceScope {
            scope_id: "banc-v888-detector-v3-audited-scope",
            artifact_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            counts: &COUNTS,
        }
    }

    fn handoff() -> V888BoolHandoff<'static> {
        V888BoolHandoff {
            stage: V888_BOOL_HANDOFF_STAGE,
            strategy_revision: V888_BOOL_STRATEGY_REVISION,
            strategy_commit: V888_BOOL_STRATEGY_COMMIT,
            rust_interface_id: V888_BOOL_INTERFACE_ID,
            rust_source_commit: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            graph_artifact_sha256:
                "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            source: source(),
            ablations: HandoffAblations {
                graph_only: true,
                existing_router_only: true,
                hybrid: true,
            },
            existing_stack: ExistingStackReuse {
                gf2_anf: true,
                bounded_memory: true,
                routing: true,
            },
            validation: IndependentValidation {
                scirust_executable_graph_qualified: true,
                sml_quality_qualified: true,
                sml_resource_qualified: true,
            },
            max_memory_bytes: 64 * 1024 * 1024,
            max_readout_bits: 4096,
            hidden_input_history_bypass: false,
            raw_v888_inference_dependency: false,
            protected_final_access: false,
            model_promotion_requested: false,
            default_runtime_policy_change: false,
        }
    }

    #[test]
    fn scoped_source_counts_keep_graph_and_contact_semantics_distinct() {
        source().validate().unwrap();

        let duplicate = [
            SourceCount {
                kind: SourceCountKind::MetadataNodes,
                value: 1,
            },
            SourceCount {
                kind: SourceCountKind::DirectedPairs,
                value: 2,
            },
            SourceCount {
                kind: SourceCountKind::DirectedPairs,
                value: 3,
            },
        ];
        let invalid = SourceScope {
            scope_id: "duplicate-directed-pairs",
            artifact_sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            counts: &duplicate,
        };
        assert_eq!(
            invalid.validate(),
            Err(V888ContractError::DuplicateScopedCount(
                SourceCountKind::DirectedPairs
            ))
        );
    }

    #[test]
    fn delayed_xor_requires_controls_rails_and_no_hidden_history() {
        let protocol = DelayedXorProtocol {
            delay_steps: 4,
            bounded_readout_bits: 256,
            fixed_recurrence: true,
            exact_sequential_logic_control: true,
            no_reservoir_control: true,
            topology_matched_control: true,
            simplified_lif_rail: true,
            integer_event_rail: true,
            boolean_recurrent_rail: true,
            hidden_input_history: false,
            cross_trial_state_leakage: false,
        };
        protocol.validate().unwrap();

        assert_eq!(
            DelayedXorProtocol {
                hidden_input_history: true,
                ..protocol
            }
            .validate(),
            Err(V888ContractError::HiddenInputHistory)
        );
        assert_eq!(
            DelayedXorProtocol {
                boolean_recurrent_rail: false,
                ..protocol
            }
            .validate(),
            Err(V888ContractError::MissingDynamicsRail)
        );
    }

    #[test]
    fn v888_bool_4_handoff_requires_all_ablation_and_validation_gates() {
        handoff().validate().unwrap();

        assert_eq!(
            V888BoolHandoff {
                validation: IndependentValidation {
                    scirust_executable_graph_qualified: false,
                    ..handoff().validation
                },
                ..handoff()
            }
            .validate(),
            Err(V888ContractError::SciRustGraphGateOpen)
        );

        assert_eq!(
            V888BoolHandoff {
                raw_v888_inference_dependency: true,
                ..handoff()
            }
            .validate(),
            Err(V888ContractError::RawV888InferenceDependency)
        );

        assert_eq!(
            V888BoolHandoff {
                ablations: HandoffAblations {
                    hybrid: false,
                    ..handoff().ablations
                },
                ..handoff()
            }
            .validate(),
            Err(V888ContractError::MissingHandoffAblation)
        );
    }
}

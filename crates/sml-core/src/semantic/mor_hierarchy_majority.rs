//! Bounded majority-bit control for learned MOR cluster prototypes.
//!
//! This is a matched MOR-H4a control arm. Cluster IDs and expert membership are
//! frozen. Unlike the observed-state prototype search, this trainer builds a
//! candidate by changing only strongly supported seed bits toward the per-cluster
//! Boolean majority, under a hard mutation budget.
//!
//! Development examples are evaluated only after the candidate is frozen.

use core::fmt;
use std::collections::BTreeMap;

use crate::{
    mor::MOR_SCALE_PPM,
    mor_specialization::{MorClusterId, MorSpecializationError, MorSpecializationRegistry},
    PageId,
};

use super::{
    mor_hierarchy_train::SemanticMorHierarchyTrainingExample, SemanticBitFieldError,
    SemanticHierarchicalMorBridge, SemanticMorHierarchyError, SemanticState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorMajorityTrainingConfig {
    pub min_examples_per_cluster: usize,
    pub min_bit_margin_ppm: u32,
    pub max_mutated_bits_per_cluster: usize,
    pub min_development_gain_ppm: u32,
}

impl Default for SemanticMorMajorityTrainingConfig {
    fn default() -> Self {
        Self {
            min_examples_per_cluster: 2,
            min_bit_margin_ppm: 250_000,
            max_mutated_bits_per_cluster: 256,
            min_development_gain_ppm: 0,
        }
    }
}

impl SemanticMorMajorityTrainingConfig {
    pub fn validate(self) -> Result<Self, SemanticMorMajorityTrainingError> {
        if self.min_examples_per_cluster == 0 {
            return Err(SemanticMorMajorityTrainingError::ZeroExamplesPerCluster);
        }
        if self.min_bit_margin_ppm == 0 || self.min_bit_margin_ppm > MOR_SCALE_PPM {
            return Err(SemanticMorMajorityTrainingError::InvalidBitMargin(
                self.min_bit_margin_ppm,
            ));
        }
        if self.max_mutated_bits_per_cluster == 0 {
            return Err(SemanticMorMajorityTrainingError::ZeroMutationBudget);
        }
        if self.min_development_gain_ppm > MOR_SCALE_PPM {
            return Err(SemanticMorMajorityTrainingError::InvalidDevelopmentGain(
                self.min_development_gain_ppm,
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorMajorityClusterReport {
    pub cluster_id: MorClusterId,
    pub positive_examples: usize,
    pub supported_bits: usize,
    pub proposed_mutations: usize,
    pub mutated_bits: usize,
    pub budget_dropped_mutations: usize,
    pub strongest_supported_margin_ppm: u32,
    pub weakest_mutated_margin_ppm: u32,
    pub mean_mutated_margin_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorMajorityTrainingReport {
    pub training_examples: usize,
    pub development_examples: usize,
    pub cluster_count: usize,
    pub mutated_bits: usize,
    pub baseline_training_accuracy_ppm: u32,
    pub candidate_training_accuracy_ppm: u32,
    pub baseline_development_accuracy_ppm: u32,
    pub candidate_development_accuracy_ppm: u32,
    pub development_gain_ppm: i32,
    pub promotion_eligible: bool,
    pub clusters: Vec<SemanticMorMajorityClusterReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorMajorityTrainingArtifact<const BITS: usize> {
    pub candidate: SemanticHierarchicalMorBridge<BITS>,
    pub report: SemanticMorMajorityTrainingReport,
}

pub struct SemanticMorMajorityPrototypeTrainer;

impl SemanticMorMajorityPrototypeTrainer {
    pub fn train<const BITS: usize>(
        baseline: &SemanticHierarchicalMorBridge<BITS>,
        training: &[SemanticMorHierarchyTrainingExample<BITS>],
        development: &[SemanticMorHierarchyTrainingExample<BITS>],
        config: SemanticMorMajorityTrainingConfig,
    ) -> Result<SemanticMorMajorityTrainingArtifact<BITS>, SemanticMorMajorityTrainingError> {
        let config = config.validate()?;
        if training.is_empty() {
            return Err(SemanticMorMajorityTrainingError::EmptyTrainingSet);
        }
        if development.is_empty() {
            return Err(SemanticMorMajorityTrainingError::EmptyDevelopmentSet);
        }

        let registry = baseline
            .specialization_registry()
            .map_err(SemanticMorMajorityTrainingError::Specialization)?;
        validate_targets(&registry, training)?;
        validate_targets(&registry, development)?;

        if baseline.cluster_specs().len() < 2 {
            return Err(SemanticMorMajorityTrainingError::NeedMultipleClusters);
        }

        let mut learned_signatures = BTreeMap::new();
        let mut reports = Vec::with_capacity(baseline.cluster_specs().len());
        let mut total_mutated_bits = 0_usize;

        for seed in baseline.cluster_specs() {
            let cluster_id = MorClusterId(seed.cluster_id);
            let positives = training
                .iter()
                .filter_map(
                    |example| match target_cluster(&registry, example.target_expert) {
                        Ok(target) if target == cluster_id => Some(Ok(&example.routing)),
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    },
                )
                .collect::<Result<Vec<_>, _>>()?;

            if positives.len() < config.min_examples_per_cluster {
                return Err(
                    SemanticMorMajorityTrainingError::InsufficientPositiveExamples {
                        cluster_id,
                        actual: positives.len(),
                        minimum: config.min_examples_per_cluster,
                    },
                );
            }

            let mut ones = vec![0_usize; BITS];
            for routing in &positives {
                for (bit, count) in ones.iter_mut().enumerate() {
                    if routing.get(bit)? {
                        *count = count.saturating_add(1);
                    }
                }
            }

            let mut supported_bits = 0_usize;
            let mut strongest_supported_margin_ppm = 0_u32;
            let mut candidates = Vec::new();
            for (bit, ones_count) in ones.iter().copied().enumerate() {
                let zeros_count = positives.len().saturating_sub(ones_count);
                if ones_count == zeros_count {
                    continue;
                }
                let margin_ppm = ratio_ppm(ones_count.abs_diff(zeros_count), positives.len())?;
                if margin_ppm < config.min_bit_margin_ppm {
                    continue;
                }

                supported_bits = supported_bits.saturating_add(1);
                strongest_supported_margin_ppm = strongest_supported_margin_ppm.max(margin_ppm);
                let learned_value = ones_count > zeros_count;
                let seed_value = seed.signature.get(bit)?;
                if learned_value != seed_value {
                    candidates.push(BitMutation {
                        bit,
                        value: learned_value,
                        margin_ppm,
                    });
                }
            }

            candidates.sort_by(|left, right| {
                right
                    .margin_ppm
                    .cmp(&left.margin_ppm)
                    .then_with(|| left.bit.cmp(&right.bit))
            });
            let proposed_mutations = candidates.len();
            candidates.truncate(config.max_mutated_bits_per_cluster);
            let budget_dropped_mutations = proposed_mutations.saturating_sub(candidates.len());

            let mut signature = seed.signature.clone();
            let mut margin_sum = 0_u64;
            for mutation in &candidates {
                signature.set(mutation.bit, mutation.value)?;
                margin_sum = margin_sum.saturating_add(mutation.margin_ppm as u64);
            }

            let mutated_bits = candidates.len();
            let weakest_mutated_margin_ppm = candidates
                .last()
                .map(|mutation| mutation.margin_ppm)
                .unwrap_or(0);
            let mean_mutated_margin_ppm = if mutated_bits == 0 {
                0
            } else {
                (margin_sum / mutated_bits as u64) as u32
            };

            total_mutated_bits = total_mutated_bits.saturating_add(mutated_bits);
            learned_signatures.insert(seed.cluster_id, signature);
            reports.push(SemanticMorMajorityClusterReport {
                cluster_id,
                positive_examples: positives.len(),
                supported_bits,
                proposed_mutations,
                mutated_bits,
                budget_dropped_mutations,
                strongest_supported_margin_ppm,
                weakest_mutated_margin_ppm,
                mean_mutated_margin_ppm,
            });
        }

        let candidate = baseline
            .rebuild_with_cluster_signatures(&learned_signatures)
            .map_err(SemanticMorMajorityTrainingError::Hierarchy)?;

        let baseline_training_accuracy_ppm = evaluate_accuracy_ppm(baseline, &registry, training)?;
        let candidate_training_accuracy_ppm =
            evaluate_accuracy_ppm(&candidate, &registry, training)?;
        let baseline_development_accuracy_ppm =
            evaluate_accuracy_ppm(baseline, &registry, development)?;
        let candidate_development_accuracy_ppm =
            evaluate_accuracy_ppm(&candidate, &registry, development)?;
        let development_gain_ppm =
            candidate_development_accuracy_ppm as i32 - baseline_development_accuracy_ppm as i32;
        let promotion_eligible = candidate_training_accuracy_ppm >= baseline_training_accuracy_ppm
            && candidate_development_accuracy_ppm as u64
                >= baseline_development_accuracy_ppm as u64
                    + config.min_development_gain_ppm as u64;

        Ok(SemanticMorMajorityTrainingArtifact {
            candidate,
            report: SemanticMorMajorityTrainingReport {
                training_examples: training.len(),
                development_examples: development.len(),
                cluster_count: baseline.cluster_specs().len(),
                mutated_bits: total_mutated_bits,
                baseline_training_accuracy_ppm,
                candidate_training_accuracy_ppm,
                baseline_development_accuracy_ppm,
                candidate_development_accuracy_ppm,
                development_gain_ppm,
                promotion_eligible,
                clusters: reports,
            },
        })
    }
}

fn validate_targets<const BITS: usize>(
    registry: &MorSpecializationRegistry,
    examples: &[SemanticMorHierarchyTrainingExample<BITS>],
) -> Result<(), SemanticMorMajorityTrainingError> {
    for example in examples {
        target_cluster(registry, example.target_expert)?;
    }
    Ok(())
}

fn target_cluster(
    registry: &MorSpecializationRegistry,
    page_id: PageId,
) -> Result<MorClusterId, SemanticMorMajorityTrainingError> {
    registry
        .cluster_of(page_id)
        .ok_or(SemanticMorMajorityTrainingError::UnknownTargetExpert(
            page_id,
        ))
}

fn evaluate_accuracy_ppm<const BITS: usize>(
    bridge: &SemanticHierarchicalMorBridge<BITS>,
    registry: &MorSpecializationRegistry,
    examples: &[SemanticMorHierarchyTrainingExample<BITS>],
) -> Result<u32, SemanticMorMajorityTrainingError> {
    let mut correct = 0_usize;
    for example in examples {
        let mut state =
            SemanticState::<BITS>::zeroed().map_err(SemanticMorMajorityTrainingError::BitField)?;
        state.routing = example.routing.clone();
        let predicted = bridge
            .rank_clusters(&state)
            .map_err(SemanticMorMajorityTrainingError::Hierarchy)?
            .first()
            .map(|evidence| MorClusterId(evidence.cluster_id))
            .ok_or(SemanticMorMajorityTrainingError::InternalInvariant)?;
        if predicted == target_cluster(registry, example.target_expert)? {
            correct = correct.saturating_add(1);
        }
    }
    ratio_ppm(correct, examples.len())
}

fn ratio_ppm(
    numerator: usize,
    denominator: usize,
) -> Result<u32, SemanticMorMajorityTrainingError> {
    if denominator == 0 {
        return Err(SemanticMorMajorityTrainingError::InternalInvariant);
    }
    let scaled = (numerator as u128)
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(SemanticMorMajorityTrainingError::ArithmeticOverflow)?;
    u32::try_from(scaled / denominator as u128)
        .map_err(|_| SemanticMorMajorityTrainingError::ArithmeticOverflow)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BitMutation {
    bit: usize,
    value: bool,
    margin_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMorMajorityTrainingError {
    ZeroExamplesPerCluster,
    InvalidBitMargin(u32),
    ZeroMutationBudget,
    InvalidDevelopmentGain(u32),
    EmptyTrainingSet,
    EmptyDevelopmentSet,
    NeedMultipleClusters,
    UnknownTargetExpert(PageId),
    InsufficientPositiveExamples {
        cluster_id: MorClusterId,
        actual: usize,
        minimum: usize,
    },
    ArithmeticOverflow,
    InternalInvariant,
    Hierarchy(SemanticMorHierarchyError),
    Specialization(MorSpecializationError),
    BitField(SemanticBitFieldError),
}

impl fmt::Display for SemanticMorMajorityTrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMorMajorityTrainingError {}

impl From<SemanticBitFieldError> for SemanticMorMajorityTrainingError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SemanticMorMajorityPrototypeTrainer, SemanticMorMajorityTrainingConfig,
        SemanticMorMajorityTrainingError,
    };
    use crate::{
        mor::{RouterId, MOR_SCALE_PPM},
        semantic::{
            SemanticBitField, SemanticHierarchicalMorBridge, SemanticMorClusterSignature,
            SemanticMorHierarchyPolicy, SemanticMorHierarchyTrainingExample, SemanticMorTopK,
        },
        PageId,
    };

    fn bits(indices: &[usize]) -> SemanticBitField<8> {
        SemanticBitField::from_set_indices(indices).expect("bits")
    }

    fn bridge(swapped: bool) -> SemanticHierarchicalMorBridge<8> {
        let low = bits(&[0, 1, 2, 3]);
        let high = bits(&[4, 5, 6, 7]);
        let (cluster_10, cluster_20) = if swapped {
            (high.clone(), low.clone())
        } else {
            (low.clone(), high.clone())
        };

        SemanticHierarchicalMorBridge::new(
            vec![(RouterId(0), low.clone()), (RouterId(1), high.clone())],
            vec![(PageId(0), low), (PageId(1), high)],
            vec![
                SemanticMorClusterSignature {
                    cluster_id: 10,
                    signature: cluster_10,
                    experts: vec![PageId(0)],
                },
                SemanticMorClusterSignature {
                    cluster_id: 20,
                    signature: cluster_20,
                    experts: vec![PageId(1)],
                },
            ],
            SemanticMorHierarchyPolicy {
                min_clusters: 1,
                max_clusters: 2,
                cluster_mass_threshold_ppm: MOR_SCALE_PPM,
                max_candidate_experts: 2,
                max_candidate_edges: 2,
                proposal_top_k: SemanticMorTopK::One,
            },
        )
        .expect("bridge")
    }

    fn training() -> Vec<SemanticMorHierarchyTrainingExample<8>> {
        vec![
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[0, 1, 2, 3]),
                target_expert: PageId(0),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[0, 1, 2]),
                target_expert: PageId(0),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[4, 5, 6, 7]),
                target_expert: PageId(1),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[4, 5, 6]),
                target_expert: PageId(1),
            },
        ]
    }

    fn development() -> Vec<SemanticMorHierarchyTrainingExample<8>> {
        vec![
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[0, 1, 3]),
                target_expert: PageId(0),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[0, 2, 3]),
                target_expert: PageId(0),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[4, 5, 7]),
                target_expert: PageId(1),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[4, 6, 7]),
                target_expert: PageId(1),
            },
        ]
    }

    fn config() -> SemanticMorMajorityTrainingConfig {
        SemanticMorMajorityTrainingConfig {
            min_examples_per_cluster: 2,
            min_bit_margin_ppm: 500_000,
            max_mutated_bits_per_cluster: 8,
            min_development_gain_ppm: 500_000,
        }
    }

    #[test]
    fn majority_control_recovers_swapped_geometry_with_development_gain() {
        let artifact = SemanticMorMajorityPrototypeTrainer::train(
            &bridge(true),
            &training(),
            &development(),
            config(),
        )
        .expect("train");

        assert_eq!(artifact.report.baseline_training_accuracy_ppm, 0);
        assert_eq!(
            artifact.report.candidate_training_accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert_eq!(artifact.report.baseline_development_accuracy_ppm, 0);
        assert_eq!(
            artifact.report.candidate_development_accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert!(artifact.report.promotion_eligible);
        assert!(artifact.report.mutated_bits > 0);
    }

    #[test]
    fn development_gate_blocks_zero_gain_on_correct_seed() {
        let artifact = SemanticMorMajorityPrototypeTrainer::train(
            &bridge(false),
            &training(),
            &development(),
            SemanticMorMajorityTrainingConfig {
                min_development_gain_ppm: 1,
                ..config()
            },
        )
        .expect("train");

        assert_eq!(
            artifact.report.baseline_development_accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert_eq!(
            artifact.report.candidate_development_accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert!(!artifact.report.promotion_eligible);
    }

    #[test]
    fn hard_mutation_budget_drops_supported_changes_deterministically() {
        let artifact = SemanticMorMajorityPrototypeTrainer::train(
            &bridge(true),
            &training(),
            &development(),
            SemanticMorMajorityTrainingConfig {
                max_mutated_bits_per_cluster: 1,
                min_development_gain_ppm: 0,
                ..config()
            },
        )
        .expect("train");

        assert_eq!(artifact.report.clusters[0].mutated_bits, 1);
        assert!(artifact.report.clusters[0].budget_dropped_mutations > 0);
        assert_eq!(artifact.report.clusters[1].mutated_bits, 1);
    }

    #[test]
    fn low_margin_coordinates_retain_seed_bits() {
        let training = vec![
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[0]),
                target_expert: PageId(0),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[]),
                target_expert: PageId(0),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[4, 5, 6, 7]),
                target_expert: PageId(1),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[4, 5, 6, 7]),
                target_expert: PageId(1),
            },
        ];
        let baseline = bridge(false);
        let artifact = SemanticMorMajorityPrototypeTrainer::train(
            &baseline,
            &training,
            &development(),
            SemanticMorMajorityTrainingConfig {
                min_bit_margin_ppm: 250_000,
                min_development_gain_ppm: 0,
                ..config()
            },
        )
        .expect("train");

        let baseline_bit = baseline.cluster_specs()[0]
            .signature
            .get(0)
            .expect("seed bit");
        let candidate_bit = artifact.candidate.cluster_specs()[0]
            .signature
            .get(0)
            .expect("candidate bit");
        assert_eq!(candidate_bit, baseline_bit);
    }

    #[test]
    fn unknown_expert_and_invalid_config_fail_closed() {
        let mut training = training();
        training[0].target_expert = PageId(99);
        assert_eq!(
            SemanticMorMajorityPrototypeTrainer::train(
                &bridge(true),
                &training,
                &development(),
                config(),
            ),
            Err(SemanticMorMajorityTrainingError::UnknownTargetExpert(
                PageId(99)
            ))
        );

        assert_eq!(
            SemanticMorMajorityTrainingConfig {
                max_mutated_bits_per_cluster: 0,
                ..SemanticMorMajorityTrainingConfig::default()
            }
            .validate(),
            Err(SemanticMorMajorityTrainingError::ZeroMutationBudget)
        );
    }
}

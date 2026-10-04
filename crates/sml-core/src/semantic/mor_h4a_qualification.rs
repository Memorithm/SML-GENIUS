//! Matched development controls for MOR-H4a cluster-prototype learning.
//!
//! This module compares the merged discriminative-medoid H4a trainer against
//! three frozen-topology controls: the original fixed prototypes, a training-only
//! Boolean majority prototype, and deterministic random observed medoids.
//! Development examples are evaluated only after every arm is frozen.

use core::fmt;
use std::collections::BTreeMap;

use crate::{
    mor::MOR_SCALE_PPM,
    mor_specialization::{MorClusterId, MorSpecializationError, MorSpecializationRegistry},
    PageId,
};

use super::{
    SemanticBitField, SemanticBitFieldError, SemanticHierarchicalMorBridge,
    SemanticMorHierarchyError, SemanticMorHierarchyTrainer, SemanticMorHierarchyTrainingArtifact,
    SemanticMorHierarchyTrainingConfig, SemanticMorHierarchyTrainingError,
    SemanticMorHierarchyTrainingExample, SemanticMorHierarchyTrainingReport, SemanticState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorH4aQualificationConfig {
    pub random_seeds: Vec<u64>,
    pub max_random_seeds: usize,
    pub min_gain_over_fixed_ppm: u32,
    pub min_gain_over_majority_ppm: u32,
    pub min_gain_over_random_mean_ppm: u32,
}

impl Default for SemanticMorH4aQualificationConfig {
    fn default() -> Self {
        Self {
            random_seeds: vec![1, 2, 3, 4],
            max_random_seeds: 16,
            min_gain_over_fixed_ppm: 0,
            min_gain_over_majority_ppm: 0,
            min_gain_over_random_mean_ppm: 0,
        }
    }
}

impl SemanticMorH4aQualificationConfig {
    pub fn validate(mut self) -> Result<Self, SemanticMorH4aQualificationError> {
        if self.max_random_seeds == 0 {
            return Err(SemanticMorH4aQualificationError::ZeroRandomSeedBudget);
        }
        if self.random_seeds.is_empty() {
            return Err(SemanticMorH4aQualificationError::EmptyRandomSeeds);
        }
        self.random_seeds.sort_unstable();
        self.random_seeds.dedup();
        if self.random_seeds.len() > self.max_random_seeds {
            return Err(SemanticMorH4aQualificationError::RandomSeedBudget {
                actual: self.random_seeds.len(),
                maximum: self.max_random_seeds,
            });
        }
        for gain in [
            self.min_gain_over_fixed_ppm,
            self.min_gain_over_majority_ppm,
            self.min_gain_over_random_mean_ppm,
        ] {
            if gain > MOR_SCALE_PPM {
                return Err(SemanticMorH4aQualificationError::GainOutOfRange(gain));
            }
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorH4aRandomControl {
    pub seed: u64,
    pub development_accuracy_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorH4aQualificationReport {
    pub training_examples: usize,
    pub development_examples: usize,
    pub fixed_development_accuracy_ppm: u32,
    pub learned_development_accuracy_ppm: u32,
    pub majority_development_accuracy_ppm: u32,
    pub random_mean_development_accuracy_ppm: u32,
    pub learned_gain_over_fixed_ppm: i32,
    pub learned_gain_over_majority_ppm: i32,
    pub learned_gain_over_random_mean_ppm: i32,
    pub random_controls: Vec<SemanticMorH4aRandomControl>,
    pub learned_training: SemanticMorHierarchyTrainingReport,
    pub qualification_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorH4aQualificationArtifact<const BITS: usize> {
    pub learned: SemanticHierarchicalMorBridge<BITS>,
    pub report: SemanticMorH4aQualificationReport,
}

pub struct SemanticMorH4aQualifier;

impl SemanticMorH4aQualifier {
    pub fn qualify<const BITS: usize>(
        baseline: &SemanticHierarchicalMorBridge<BITS>,
        training: &[SemanticMorHierarchyTrainingExample<BITS>],
        development: &[SemanticMorHierarchyTrainingExample<BITS>],
        training_config: SemanticMorHierarchyTrainingConfig,
        qualification_config: SemanticMorH4aQualificationConfig,
    ) -> Result<SemanticMorH4aQualificationArtifact<BITS>, SemanticMorH4aQualificationError> {
        let qualification_config = qualification_config.validate()?;
        let learned_artifact =
            SemanticMorHierarchyTrainer::train(baseline, training, development, training_config)
                .map_err(SemanticMorH4aQualificationError::Training)?;

        let registry = baseline
            .specialization_registry()
            .map_err(SemanticMorH4aQualificationError::Specialization)?;
        validate_targets(&registry, training)?;
        validate_targets(&registry, development)?;

        let fixed_development_accuracy_ppm =
            evaluate_accuracy_ppm(baseline, &registry, development)?;
        let majority = majority_control(baseline, &registry, training)?;
        let majority_development_accuracy_ppm =
            evaluate_accuracy_ppm(&majority, &registry, development)?;

        let mut random_controls = Vec::with_capacity(qualification_config.random_seeds.len());
        for seed in &qualification_config.random_seeds {
            let control = random_medoid_control(baseline, &registry, training, *seed)?;
            random_controls.push(SemanticMorH4aRandomControl {
                seed: *seed,
                development_accuracy_ppm: evaluate_accuracy_ppm(&control, &registry, development)?,
            });
        }
        let random_mean_development_accuracy_ppm = mean_ppm(
            &random_controls
                .iter()
                .map(|control| control.development_accuracy_ppm)
                .collect::<Vec<_>>(),
        )?;

        let learned_development_accuracy_ppm =
            learned_artifact.report.candidate_development_accuracy_ppm;
        let learned_gain_over_fixed_ppm = signed_gain(
            learned_development_accuracy_ppm,
            fixed_development_accuracy_ppm,
        );
        let learned_gain_over_majority_ppm = signed_gain(
            learned_development_accuracy_ppm,
            majority_development_accuracy_ppm,
        );
        let learned_gain_over_random_mean_ppm = signed_gain(
            learned_development_accuracy_ppm,
            random_mean_development_accuracy_ppm,
        );

        let qualification_passed = learned_artifact.report.promotion_eligible
            && meets_gain(
                learned_development_accuracy_ppm,
                fixed_development_accuracy_ppm,
                qualification_config.min_gain_over_fixed_ppm,
            )
            && meets_gain(
                learned_development_accuracy_ppm,
                majority_development_accuracy_ppm,
                qualification_config.min_gain_over_majority_ppm,
            )
            && meets_gain(
                learned_development_accuracy_ppm,
                random_mean_development_accuracy_ppm,
                qualification_config.min_gain_over_random_mean_ppm,
            );

        Ok(SemanticMorH4aQualificationArtifact {
            learned: learned_artifact.candidate,
            report: SemanticMorH4aQualificationReport {
                training_examples: training.len(),
                development_examples: development.len(),
                fixed_development_accuracy_ppm,
                learned_development_accuracy_ppm,
                majority_development_accuracy_ppm,
                random_mean_development_accuracy_ppm,
                learned_gain_over_fixed_ppm,
                learned_gain_over_majority_ppm,
                learned_gain_over_random_mean_ppm,
                random_controls,
                learned_training: learned_artifact.report,
                qualification_passed,
            },
        })
    }
}

fn majority_control<const BITS: usize>(
    baseline: &SemanticHierarchicalMorBridge<BITS>,
    registry: &MorSpecializationRegistry,
    training: &[SemanticMorHierarchyTrainingExample<BITS>],
) -> Result<SemanticHierarchicalMorBridge<BITS>, SemanticMorH4aQualificationError> {
    let mut signatures = BTreeMap::new();
    for cluster in baseline.cluster_specs() {
        let cluster_id = MorClusterId(cluster.cluster_id);
        let positives = examples_for_cluster(registry, training, cluster_id)?;
        if positives.is_empty() {
            return Err(SemanticMorH4aQualificationError::NoTrainingExamples(
                cluster_id,
            ));
        }

        let mut signature = cluster.signature.clone();
        for bit in 0..BITS {
            let mut ones = 0_usize;
            for example in &positives {
                if example
                    .routing
                    .get(bit)
                    .map_err(SemanticMorH4aQualificationError::BitField)?
                {
                    ones = ones.saturating_add(1);
                }
            }
            let zeros = positives.len().saturating_sub(ones);
            if ones > zeros {
                signature
                    .set(bit, true)
                    .map_err(SemanticMorH4aQualificationError::BitField)?;
            } else if zeros > ones {
                signature
                    .set(bit, false)
                    .map_err(SemanticMorH4aQualificationError::BitField)?;
            }
        }
        signatures.insert(cluster.cluster_id, signature);
    }

    baseline
        .rebuild_with_cluster_signatures(&signatures)
        .map_err(SemanticMorH4aQualificationError::Hierarchy)
}

fn random_medoid_control<const BITS: usize>(
    baseline: &SemanticHierarchicalMorBridge<BITS>,
    registry: &MorSpecializationRegistry,
    training: &[SemanticMorHierarchyTrainingExample<BITS>],
    seed: u64,
) -> Result<SemanticHierarchicalMorBridge<BITS>, SemanticMorH4aQualificationError> {
    let mut signatures = BTreeMap::new();
    for cluster in baseline.cluster_specs() {
        let cluster_id = MorClusterId(cluster.cluster_id);
        let positives = examples_for_cluster(registry, training, cluster_id)?;
        if positives.is_empty() {
            return Err(SemanticMorH4aQualificationError::NoTrainingExamples(
                cluster_id,
            ));
        }
        let mut candidates = positives
            .iter()
            .map(|example| example.routing.clone())
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.words().cmp(right.words()));
        candidates.dedup_by(|left, right| left == right);
        let mixed = splitmix64(seed ^ u64::from(cluster.cluster_id));
        let index = (mixed as usize) % candidates.len();
        signatures.insert(cluster.cluster_id, candidates[index].clone());
    }

    baseline
        .rebuild_with_cluster_signatures(&signatures)
        .map_err(SemanticMorH4aQualificationError::Hierarchy)
}

fn examples_for_cluster<'a, const BITS: usize>(
    registry: &MorSpecializationRegistry,
    examples: &'a [SemanticMorHierarchyTrainingExample<BITS>],
    cluster_id: MorClusterId,
) -> Result<Vec<&'a SemanticMorHierarchyTrainingExample<BITS>>, SemanticMorH4aQualificationError> {
    let mut selected = Vec::new();
    for example in examples {
        if target_cluster(registry, example.target_expert)? == cluster_id {
            selected.push(example);
        }
    }
    Ok(selected)
}

fn validate_targets<const BITS: usize>(
    registry: &MorSpecializationRegistry,
    examples: &[SemanticMorHierarchyTrainingExample<BITS>],
) -> Result<(), SemanticMorH4aQualificationError> {
    for example in examples {
        target_cluster(registry, example.target_expert)?;
    }
    Ok(())
}

fn target_cluster(
    registry: &MorSpecializationRegistry,
    page_id: PageId,
) -> Result<MorClusterId, SemanticMorH4aQualificationError> {
    registry
        .cluster_of(page_id)
        .ok_or(SemanticMorH4aQualificationError::UnknownTargetExpert(
            page_id,
        ))
}

fn evaluate_accuracy_ppm<const BITS: usize>(
    bridge: &SemanticHierarchicalMorBridge<BITS>,
    registry: &MorSpecializationRegistry,
    examples: &[SemanticMorHierarchyTrainingExample<BITS>],
) -> Result<u32, SemanticMorH4aQualificationError> {
    if examples.is_empty() {
        return Err(SemanticMorH4aQualificationError::EmptyEvaluationSet);
    }
    let mut correct = 0_usize;
    for example in examples {
        let mut state =
            SemanticState::<BITS>::zeroed().map_err(SemanticMorH4aQualificationError::BitField)?;
        state.routing = example.routing.clone();
        let ranked = bridge
            .rank_clusters(&state)
            .map_err(SemanticMorH4aQualificationError::Hierarchy)?;
        let predicted = ranked
            .first()
            .map(|evidence| MorClusterId(evidence.cluster_id))
            .ok_or(SemanticMorH4aQualificationError::InternalInvariant)?;
        if predicted == target_cluster(registry, example.target_expert)? {
            correct = correct.saturating_add(1);
        }
    }
    ratio_ppm(correct, examples.len())
}

fn mean_ppm(values: &[u32]) -> Result<u32, SemanticMorH4aQualificationError> {
    if values.is_empty() {
        return Err(SemanticMorH4aQualificationError::EmptyRandomSeeds);
    }
    let sum = values.iter().map(|value| u64::from(*value)).sum::<u64>();
    Ok((sum / values.len() as u64) as u32)
}

fn ratio_ppm(
    numerator: usize,
    denominator: usize,
) -> Result<u32, SemanticMorH4aQualificationError> {
    if denominator == 0 {
        return Err(SemanticMorH4aQualificationError::EmptyEvaluationSet);
    }
    let scaled = (numerator as u128)
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(SemanticMorH4aQualificationError::ArithmeticOverflow)?;
    u32::try_from(scaled / denominator as u128)
        .map_err(|_| SemanticMorH4aQualificationError::ArithmeticOverflow)
}

fn signed_gain(candidate: u32, control: u32) -> i32 {
    candidate as i32 - control as i32
}

fn meets_gain(candidate: u32, control: u32, minimum_gain: u32) -> bool {
    candidate as u64 >= control as u64 + minimum_gain as u64
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMorH4aQualificationError {
    ZeroRandomSeedBudget,
    EmptyRandomSeeds,
    RandomSeedBudget { actual: usize, maximum: usize },
    GainOutOfRange(u32),
    UnknownTargetExpert(PageId),
    NoTrainingExamples(MorClusterId),
    EmptyEvaluationSet,
    ArithmeticOverflow,
    InternalInvariant,
    Training(SemanticMorHierarchyTrainingError),
    Specialization(MorSpecializationError),
    Hierarchy(SemanticMorHierarchyError),
    BitField(SemanticBitFieldError),
}

impl fmt::Display for SemanticMorH4aQualificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMorH4aQualificationError {}

#[cfg(test)]
mod tests {
    use super::{
        SemanticMorH4aQualificationConfig, SemanticMorH4aQualificationError,
        SemanticMorH4aQualifier,
    };
    use crate::{
        mor::{RouterId, MOR_SCALE_PPM},
        semantic::{
            SemanticBitField, SemanticHierarchicalMorBridge, SemanticMorClusterSignature,
            SemanticMorHierarchyPolicy, SemanticMorHierarchyTrainingConfig,
            SemanticMorHierarchyTrainingExample, SemanticMorTopK,
        },
        PageId,
    };

    fn bits(indices: &[usize]) -> SemanticBitField<8> {
        SemanticBitField::from_set_indices(indices).expect("bits")
    }

    fn bridge(swapped: bool) -> SemanticHierarchicalMorBridge<8> {
        let low = bits(&[0, 1, 2, 3]);
        let high = bits(&[4, 5, 6, 7]);
        let (a, b) = if swapped {
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
                    signature: a,
                    experts: vec![PageId(0)],
                },
                SemanticMorClusterSignature {
                    cluster_id: 20,
                    signature: b,
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

    fn training_config() -> SemanticMorHierarchyTrainingConfig {
        SemanticMorHierarchyTrainingConfig {
            max_candidates_per_cluster: 8,
            min_examples_per_cluster: 2,
            min_prototype_margin_ppm: 250_000,
            min_development_gain_ppm: 0,
        }
    }

    #[test]
    fn matched_controls_share_data_and_frozen_membership() {
        let artifact = SemanticMorH4aQualifier::qualify(
            &bridge(true),
            &training(),
            &development(),
            training_config(),
            SemanticMorH4aQualificationConfig::default(),
        )
        .expect("qualification");

        assert_eq!(artifact.report.fixed_development_accuracy_ppm, 0);
        assert_eq!(
            artifact.report.learned_development_accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert_eq!(
            artifact.report.majority_development_accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert_eq!(
            artifact.report.random_mean_development_accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert!(artifact.report.qualification_passed);

        let registry = artifact
            .learned
            .specialization_registry()
            .expect("registry");
        assert_eq!(
            registry.cluster_of(PageId(0)),
            Some(crate::mor_specialization::MorClusterId(10))
        );
        assert_eq!(
            registry.cluster_of(PageId(1)),
            Some(crate::mor_specialization::MorClusterId(20))
        );
    }

    #[test]
    fn superiority_requirement_can_keep_h4b_blocked() {
        let artifact = SemanticMorH4aQualifier::qualify(
            &bridge(true),
            &training(),
            &development(),
            training_config(),
            SemanticMorH4aQualificationConfig {
                min_gain_over_majority_ppm: 1,
                ..SemanticMorH4aQualificationConfig::default()
            },
        )
        .expect("qualification");

        assert_eq!(
            artifact.report.learned_development_accuracy_ppm,
            artifact.report.majority_development_accuracy_ppm
        );
        assert!(!artifact.report.qualification_passed);
    }

    #[test]
    fn random_controls_are_deterministic_for_frozen_seeds() {
        let config = SemanticMorH4aQualificationConfig {
            random_seeds: vec![9, 3, 9, 5],
            ..SemanticMorH4aQualificationConfig::default()
        };
        let first = SemanticMorH4aQualifier::qualify(
            &bridge(true),
            &training(),
            &development(),
            training_config(),
            config.clone(),
        )
        .expect("first");
        let second = SemanticMorH4aQualifier::qualify(
            &bridge(true),
            &training(),
            &development(),
            training_config(),
            config,
        )
        .expect("second");

        assert_eq!(first.report.random_controls, second.report.random_controls);
        assert_eq!(
            first
                .report
                .random_controls
                .iter()
                .map(|control| control.seed)
                .collect::<Vec<_>>(),
            vec![3, 5, 9]
        );
    }

    #[test]
    fn invalid_random_control_budget_fails_closed() {
        assert_eq!(
            SemanticMorH4aQualificationConfig {
                random_seeds: vec![],
                ..SemanticMorH4aQualificationConfig::default()
            }
            .validate(),
            Err(SemanticMorH4aQualificationError::EmptyRandomSeeds)
        );
        assert_eq!(
            SemanticMorH4aQualificationConfig {
                random_seeds: vec![1, 2, 3],
                max_random_seeds: 2,
                ..SemanticMorH4aQualificationConfig::default()
            }
            .validate(),
            Err(SemanticMorH4aQualificationError::RandomSeedBudget {
                actual: 3,
                maximum: 2,
            })
        );
    }
}

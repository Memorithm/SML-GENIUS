//! Deterministic learning of semantic MOR cluster prototypes.
//!
//! MOR-H4a learns only the Boolean prototype attached to each already-declared
//! H1/H2 cluster. Expert membership is frozen. Candidate prototypes are drawn
//! from training-only routing states and selected by positive-vs-negative mean
//! XNOR separation. Development holdout data is used only for an explicit
//! promotion-eligibility gate; it never changes the learned prototype.
//!
//! This is a bounded research trainer, not evidence that learned hierarchy
//! improves model quality.

use core::fmt;
use std::collections::BTreeMap;

use crate::{
    mor::MOR_SCALE_PPM,
    mor_specialization::{MorClusterId, MorSpecializationError, MorSpecializationRegistry},
    PageId,
};

use super::{
    SemanticBitField, SemanticBitFieldError, SemanticHierarchicalMorBridge,
    SemanticMorHierarchyError, SemanticState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorHierarchyTrainingExample<const BITS: usize> {
    pub routing: SemanticBitField<BITS>,
    pub target_expert: PageId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorHierarchyTrainingConfig {
    pub max_candidates_per_cluster: usize,
    pub min_examples_per_cluster: usize,
    pub min_prototype_margin_ppm: u32,
    pub min_development_gain_ppm: u32,
}

impl Default for SemanticMorHierarchyTrainingConfig {
    fn default() -> Self {
        Self {
            max_candidates_per_cluster: 32,
            min_examples_per_cluster: 2,
            min_prototype_margin_ppm: 50_000,
            min_development_gain_ppm: 0,
        }
    }
}

impl SemanticMorHierarchyTrainingConfig {
    pub fn validate(self) -> Result<Self, SemanticMorHierarchyTrainingError> {
        if self.max_candidates_per_cluster == 0 {
            return Err(SemanticMorHierarchyTrainingError::ZeroCandidateBudget);
        }
        if self.min_examples_per_cluster == 0 {
            return Err(SemanticMorHierarchyTrainingError::ZeroExamplesPerCluster);
        }
        if self.min_prototype_margin_ppm == 0 || self.min_prototype_margin_ppm > MOR_SCALE_PPM {
            return Err(SemanticMorHierarchyTrainingError::InvalidPrototypeMargin(
                self.min_prototype_margin_ppm,
            ));
        }
        if self.min_development_gain_ppm > MOR_SCALE_PPM {
            return Err(SemanticMorHierarchyTrainingError::InvalidDevelopmentGain(
                self.min_development_gain_ppm,
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorClusterPrototypeReport {
    pub cluster_id: MorClusterId,
    pub positive_examples: usize,
    pub negative_examples: usize,
    pub candidate_prototypes: usize,
    pub candidate_evaluations: usize,
    pub selected_positive_mean_ppm: u32,
    pub selected_negative_mean_ppm: u32,
    pub selected_margin_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorHierarchyTrainingReport {
    pub training_examples: usize,
    pub development_examples: usize,
    pub cluster_count: usize,
    pub candidate_evaluations: usize,
    pub baseline_training_accuracy_ppm: u32,
    pub candidate_training_accuracy_ppm: u32,
    pub baseline_development_accuracy_ppm: u32,
    pub candidate_development_accuracy_ppm: u32,
    pub development_gain_ppm: i32,
    pub promotion_eligible: bool,
    pub clusters: Vec<SemanticMorClusterPrototypeReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorHierarchyTrainingArtifact<const BITS: usize> {
    pub candidate: SemanticHierarchicalMorBridge<BITS>,
    pub report: SemanticMorHierarchyTrainingReport,
}

pub struct SemanticMorHierarchyTrainer;

impl SemanticMorHierarchyTrainer {
    pub fn train<const BITS: usize>(
        baseline: &SemanticHierarchicalMorBridge<BITS>,
        training: &[SemanticMorHierarchyTrainingExample<BITS>],
        development: &[SemanticMorHierarchyTrainingExample<BITS>],
        config: SemanticMorHierarchyTrainingConfig,
    ) -> Result<SemanticMorHierarchyTrainingArtifact<BITS>, SemanticMorHierarchyTrainingError> {
        let config = config.validate()?;
        if training.is_empty() {
            return Err(SemanticMorHierarchyTrainingError::EmptyTrainingSet);
        }
        if development.is_empty() {
            return Err(SemanticMorHierarchyTrainingError::EmptyDevelopmentSet);
        }

        let registry = baseline
            .specialization_registry()
            .map_err(SemanticMorHierarchyTrainingError::Specialization)?;
        validate_targets(&registry, training)?;
        validate_targets(&registry, development)?;

        if baseline.cluster_specs().len() < 2 {
            return Err(SemanticMorHierarchyTrainingError::NeedMultipleClusters);
        }

        let mut learned_signatures = BTreeMap::new();
        let mut cluster_reports = Vec::with_capacity(baseline.cluster_specs().len());
        let mut candidate_evaluations = 0_usize;

        for cluster in baseline.cluster_specs() {
            let cluster_id = MorClusterId(cluster.cluster_id);
            let mut positives = Vec::new();
            let mut negatives = Vec::new();
            for example in training {
                let target_cluster = target_cluster(&registry, example.target_expert)?;
                if target_cluster == cluster_id {
                    positives.push(&example.routing);
                } else {
                    negatives.push(&example.routing);
                }
            }

            if positives.len() < config.min_examples_per_cluster {
                return Err(
                    SemanticMorHierarchyTrainingError::InsufficientPositiveExamples {
                        cluster_id,
                        actual: positives.len(),
                        minimum: config.min_examples_per_cluster,
                    },
                );
            }
            if negatives.len() < config.min_examples_per_cluster {
                return Err(
                    SemanticMorHierarchyTrainingError::InsufficientNegativeExamples {
                        cluster_id,
                        actual: negatives.len(),
                        minimum: config.min_examples_per_cluster,
                    },
                );
            }

            let mut candidates = positives
                .iter()
                .map(|routing| (*routing).clone())
                .collect::<Vec<_>>();
            candidates.sort_by(|left, right| left.words().cmp(right.words()));
            candidates.dedup_by(|left, right| left == right);
            candidates.truncate(config.max_candidates_per_cluster);

            let mut best: Option<(SemanticBitField<BITS>, u32, u32, u32)> = None;
            for candidate in &candidates {
                let positive_mean_ppm = mean_similarity_ppm(candidate, &positives)?;
                let negative_mean_ppm = mean_similarity_ppm(candidate, &negatives)?;
                let margin_ppm = positive_mean_ppm.saturating_sub(negative_mean_ppm);
                candidate_evaluations = candidate_evaluations.saturating_add(1);

                let replace = best
                    .as_ref()
                    .is_none_or(|(_, _, _, best_margin)| margin_ppm > *best_margin);
                if replace {
                    best = Some((
                        candidate.clone(),
                        positive_mean_ppm,
                        negative_mean_ppm,
                        margin_ppm,
                    ));
                }
            }

            let (prototype, positive_mean_ppm, negative_mean_ppm, margin_ppm) = best.ok_or(
                SemanticMorHierarchyTrainingError::NoPrototypeCandidates(cluster_id),
            )?;
            if margin_ppm < config.min_prototype_margin_ppm {
                return Err(
                    SemanticMorHierarchyTrainingError::InsufficientPrototypeSeparation {
                        cluster_id,
                        actual: margin_ppm,
                        minimum: config.min_prototype_margin_ppm,
                    },
                );
            }

            learned_signatures.insert(cluster.cluster_id, prototype);
            cluster_reports.push(SemanticMorClusterPrototypeReport {
                cluster_id,
                positive_examples: positives.len(),
                negative_examples: negatives.len(),
                candidate_prototypes: candidates.len(),
                candidate_evaluations: candidates.len(),
                selected_positive_mean_ppm: positive_mean_ppm,
                selected_negative_mean_ppm: negative_mean_ppm,
                selected_margin_ppm: margin_ppm,
            });
        }

        let candidate = baseline
            .rebuild_with_cluster_signatures(&learned_signatures)
            .map_err(SemanticMorHierarchyTrainingError::Hierarchy)?;

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

        Ok(SemanticMorHierarchyTrainingArtifact {
            candidate,
            report: SemanticMorHierarchyTrainingReport {
                training_examples: training.len(),
                development_examples: development.len(),
                cluster_count: baseline.cluster_specs().len(),
                candidate_evaluations,
                baseline_training_accuracy_ppm,
                candidate_training_accuracy_ppm,
                baseline_development_accuracy_ppm,
                candidate_development_accuracy_ppm,
                development_gain_ppm,
                promotion_eligible,
                clusters: cluster_reports,
            },
        })
    }
}

fn validate_targets<const BITS: usize>(
    registry: &MorSpecializationRegistry,
    examples: &[SemanticMorHierarchyTrainingExample<BITS>],
) -> Result<(), SemanticMorHierarchyTrainingError> {
    for example in examples {
        target_cluster(registry, example.target_expert)?;
    }
    Ok(())
}

fn target_cluster(
    registry: &MorSpecializationRegistry,
    page_id: PageId,
) -> Result<MorClusterId, SemanticMorHierarchyTrainingError> {
    registry
        .cluster_of(page_id)
        .ok_or(SemanticMorHierarchyTrainingError::UnknownTargetExpert(
            page_id,
        ))
}

fn mean_similarity_ppm<const BITS: usize>(
    prototype: &SemanticBitField<BITS>,
    examples: &[&SemanticBitField<BITS>],
) -> Result<u32, SemanticMorHierarchyTrainingError> {
    if BITS == 0 {
        return Err(SemanticMorHierarchyTrainingError::ZeroSemanticWidth);
    }
    if examples.is_empty() {
        return Err(SemanticMorHierarchyTrainingError::InternalInvariant);
    }

    let total_equal = examples
        .iter()
        .map(|example| prototype.xnor_popcount(example) as u128)
        .sum::<u128>();
    let denominator = (BITS as u128)
        .checked_mul(examples.len() as u128)
        .ok_or(SemanticMorHierarchyTrainingError::ArithmeticOverflow)?;
    let scaled = total_equal
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(SemanticMorHierarchyTrainingError::ArithmeticOverflow)?;
    u32::try_from(scaled / denominator)
        .map_err(|_| SemanticMorHierarchyTrainingError::ArithmeticOverflow)
}

fn evaluate_accuracy_ppm<const BITS: usize>(
    bridge: &SemanticHierarchicalMorBridge<BITS>,
    registry: &MorSpecializationRegistry,
    examples: &[SemanticMorHierarchyTrainingExample<BITS>],
) -> Result<u32, SemanticMorHierarchyTrainingError> {
    let mut correct = 0_usize;
    for example in examples {
        let mut state =
            SemanticState::<BITS>::zeroed().map_err(SemanticMorHierarchyTrainingError::BitField)?;
        state.routing = example.routing.clone();
        let ranked = bridge
            .rank_clusters(&state)
            .map_err(SemanticMorHierarchyTrainingError::Hierarchy)?;
        let predicted = ranked
            .first()
            .map(|evidence| MorClusterId(evidence.cluster_id))
            .ok_or(SemanticMorHierarchyTrainingError::InternalInvariant)?;
        if predicted == target_cluster(registry, example.target_expert)? {
            correct = correct.saturating_add(1);
        }
    }

    ratio_ppm(correct, examples.len())
}

fn ratio_ppm(
    numerator: usize,
    denominator: usize,
) -> Result<u32, SemanticMorHierarchyTrainingError> {
    if denominator == 0 {
        return Err(SemanticMorHierarchyTrainingError::InternalInvariant);
    }
    let scaled = (numerator as u128)
        .checked_mul(MOR_SCALE_PPM as u128)
        .ok_or(SemanticMorHierarchyTrainingError::ArithmeticOverflow)?;
    u32::try_from(scaled / denominator as u128)
        .map_err(|_| SemanticMorHierarchyTrainingError::ArithmeticOverflow)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMorHierarchyTrainingError {
    ZeroCandidateBudget,
    ZeroExamplesPerCluster,
    InvalidPrototypeMargin(u32),
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
    InsufficientNegativeExamples {
        cluster_id: MorClusterId,
        actual: usize,
        minimum: usize,
    },
    NoPrototypeCandidates(MorClusterId),
    InsufficientPrototypeSeparation {
        cluster_id: MorClusterId,
        actual: u32,
        minimum: u32,
    },
    ZeroSemanticWidth,
    ArithmeticOverflow,
    InternalInvariant,
    Hierarchy(SemanticMorHierarchyError),
    Specialization(MorSpecializationError),
    BitField(SemanticBitFieldError),
}

impl fmt::Display for SemanticMorHierarchyTrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMorHierarchyTrainingError {}

#[cfg(test)]
mod tests {
    use super::{
        SemanticMorHierarchyTrainer, SemanticMorHierarchyTrainingConfig,
        SemanticMorHierarchyTrainingError, SemanticMorHierarchyTrainingExample,
    };
    use crate::{
        mor::{RouterId, MOR_SCALE_PPM},
        semantic::{
            SemanticBitField, SemanticHierarchicalMorBridge, SemanticMorClusterSignature,
            SemanticMorHierarchyPolicy, SemanticMorTopK,
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

    fn examples() -> Vec<SemanticMorHierarchyTrainingExample<8>> {
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

    fn config() -> SemanticMorHierarchyTrainingConfig {
        SemanticMorHierarchyTrainingConfig {
            max_candidates_per_cluster: 8,
            min_examples_per_cluster: 2,
            min_prototype_margin_ppm: 250_000,
            min_development_gain_ppm: 500_000,
        }
    }

    #[test]
    fn learned_prototypes_recover_swapped_cluster_geometry() {
        let baseline = bridge(true);
        let artifact =
            SemanticMorHierarchyTrainer::train(&baseline, &examples(), &development(), config())
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
        assert_eq!(artifact.report.development_gain_ppm, MOR_SCALE_PPM as i32);
        assert!(artifact.report.promotion_eligible);

        let registry = artifact
            .candidate
            .specialization_registry()
            .expect("candidate registry");
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
    fn development_gate_does_not_promote_without_required_gain() {
        let baseline = bridge(false);
        let artifact = SemanticMorHierarchyTrainer::train(
            &baseline,
            &examples(),
            &development(),
            SemanticMorHierarchyTrainingConfig {
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
    fn unknown_target_expert_fails_closed() {
        let baseline = bridge(true);
        let mut training = examples();
        training[0].target_expert = PageId(99);

        assert_eq!(
            SemanticMorHierarchyTrainer::train(&baseline, &training, &development(), config()),
            Err(SemanticMorHierarchyTrainingError::UnknownTargetExpert(
                PageId(99)
            ))
        );
    }

    #[test]
    fn insufficient_cluster_examples_fail_closed() {
        let baseline = bridge(true);
        let training = vec![
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[0, 1, 2, 3]),
                target_expert: PageId(0),
            },
            SemanticMorHierarchyTrainingExample {
                routing: bits(&[4, 5, 6, 7]),
                target_expert: PageId(1),
            },
        ];

        assert!(matches!(
            SemanticMorHierarchyTrainer::train(&baseline, &training, &development(), config()),
            Err(
                SemanticMorHierarchyTrainingError::InsufficientPositiveExamples {
                    actual: 1,
                    minimum: 2,
                    ..
                }
            )
        ));
    }

    #[test]
    fn invalid_training_policy_fails_closed() {
        assert_eq!(
            SemanticMorHierarchyTrainingConfig {
                max_candidates_per_cluster: 0,
                ..SemanticMorHierarchyTrainingConfig::default()
            }
            .validate(),
            Err(SemanticMorHierarchyTrainingError::ZeroCandidateBudget)
        );
        assert_eq!(
            SemanticMorHierarchyTrainingConfig {
                min_prototype_margin_ppm: MOR_SCALE_PPM + 1,
                ..SemanticMorHierarchyTrainingConfig::default()
            }
            .validate(),
            Err(SemanticMorHierarchyTrainingError::InvalidPrototypeMargin(
                MOR_SCALE_PPM + 1
            ))
        );
    }
}

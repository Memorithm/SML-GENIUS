//! Bounded fitting of Boolean semantic cluster prototypes for MOR-H4a.
//!
//! Expert membership and cluster identifiers remain fixed. Only the Boolean
//! routing signature of each existing MOR-H1/H2 cluster may change.
//! Low-margin coordinates retain the seed signature, and a hard mutation budget
//! limits how many coordinates may change per cluster.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::mor::MOR_SCALE_PPM;

use super::{
    SemanticBitField, SemanticBitFieldError, SemanticMorClusterSignature,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorClusterTrainingExample<const BITS: usize> {
    pub routing: SemanticBitField<BITS>,
    pub target_cluster_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorClusterPrototypeTrainingConfig {
    pub min_examples_per_cluster: usize,
    pub min_bit_margin_ppm: u32,
    pub max_mutated_bits_per_cluster: usize,
}

impl Default for SemanticMorClusterPrototypeTrainingConfig {
    fn default() -> Self {
        Self {
            min_examples_per_cluster: 8,
            min_bit_margin_ppm: 250_000,
            max_mutated_bits_per_cluster: 256,
        }
    }
}

impl SemanticMorClusterPrototypeTrainingConfig {
    pub fn validate(self) -> Result<Self, SemanticMorClusterPrototypeError> {
        if self.min_examples_per_cluster == 0 {
            return Err(SemanticMorClusterPrototypeError::ZeroExamplesPerCluster);
        }
        if self.min_bit_margin_ppm == 0 || self.min_bit_margin_ppm > MOR_SCALE_PPM {
            return Err(SemanticMorClusterPrototypeError::InvalidBitMargin(
                self.min_bit_margin_ppm,
            ));
        }
        if self.max_mutated_bits_per_cluster == 0 {
            return Err(SemanticMorClusterPrototypeError::ZeroMutationBudget);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorClusterPrototypeReport {
    pub cluster_id: u32,
    pub examples: usize,
    pub supported_bits: usize,
    pub proposed_mutations: usize,
    pub mutated_bits: usize,
    pub budget_dropped_mutations: usize,
    pub strongest_supported_margin_ppm: u32,
    pub weakest_mutated_margin_ppm: u32,
    pub mean_mutated_margin_ppm: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemanticMorClusterEvaluationRow {
    pub cluster_id: u32,
    pub examples: usize,
    pub correct: usize,
    pub accuracy_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorClusterEvaluationReport {
    pub examples: usize,
    pub correct: usize,
    pub accuracy_ppm: u32,
    pub clusters: Vec<SemanticMorClusterEvaluationRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorClusterPrototypeTrainingReport {
    pub examples: usize,
    pub cluster_count: usize,
    pub mutated_bits: usize,
    pub baseline_training: SemanticMorClusterEvaluationReport,
    pub fitted_training: SemanticMorClusterEvaluationReport,
    pub clusters: Vec<SemanticMorClusterPrototypeReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticMorClusterPrototypeArtifact<const BITS: usize> {
    pub clusters: Vec<SemanticMorClusterSignature<BITS>>,
    pub report: SemanticMorClusterPrototypeTrainingReport,
}

pub struct SemanticMorClusterPrototypeTrainer;

impl SemanticMorClusterPrototypeTrainer {
    pub fn fit<const BITS: usize>(
        seeds: &[SemanticMorClusterSignature<BITS>],
        examples: &[SemanticMorClusterTrainingExample<BITS>],
        config: SemanticMorClusterPrototypeTrainingConfig,
    ) -> Result<SemanticMorClusterPrototypeArtifact<BITS>, SemanticMorClusterPrototypeError> {
        let config = config.validate()?;
        validate_seed_clusters(seeds)?;
        if examples.is_empty() {
            return Err(SemanticMorClusterPrototypeError::EmptyTrainingSet);
        }

        let seed_ids = seeds
            .iter()
            .map(|cluster| cluster.cluster_id)
            .collect::<BTreeSet<_>>();
        let mut counts = seed_ids
            .iter()
            .copied()
            .map(|cluster_id| (cluster_id, 0_usize))
            .collect::<BTreeMap<_, _>>();

        for example in examples {
            let count = counts
                .get_mut(&example.target_cluster_id)
                .ok_or(SemanticMorClusterPrototypeError::UnknownTargetCluster(
                    example.target_cluster_id,
                ))?;
            *count = count.saturating_add(1);
        }

        for (cluster_id, count) in &counts {
            if *count < config.min_examples_per_cluster {
                return Err(
                    SemanticMorClusterPrototypeError::InsufficientClusterExamples {
                        cluster_id: *cluster_id,
                        actual: *count,
                        minimum: config.min_examples_per_cluster,
                    },
                );
            }
        }

        let baseline_training = evaluate_cluster_prototypes(seeds, examples)?;
        let mut fitted = Vec::with_capacity(seeds.len());
        let mut cluster_reports = Vec::with_capacity(seeds.len());
        let mut mutated_bits_total = 0_usize;

        for seed in seeds {
            let cluster_examples = examples
                .iter()
                .filter(|example| example.target_cluster_id == seed.cluster_id)
                .collect::<Vec<_>>();
            let example_count = cluster_examples.len();
            let mut ones = vec![0_usize; BITS];

            for example in &cluster_examples {
                for (bit, count) in ones.iter_mut().enumerate() {
                    if example.routing.get(bit)? {
                        *count = count.saturating_add(1);
                    }
                }
            }

            let mut supported_bits = 0_usize;
            let mut strongest_supported_margin_ppm = 0_u32;
            let mut candidates = Vec::new();

            for (bit, ones_count) in ones.iter().copied().enumerate() {
                let zeros_count = example_count.saturating_sub(ones_count);
                if ones_count == zeros_count {
                    continue;
                }
                let margin_ppm = ratio_ppm(ones_count.abs_diff(zeros_count), example_count);
                if margin_ppm < config.min_bit_margin_ppm {
                    continue;
                }

                supported_bits = supported_bits.saturating_add(1);
                strongest_supported_margin_ppm =
                    strongest_supported_margin_ppm.max(margin_ppm);
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
            mutated_bits_total = mutated_bits_total.saturating_add(mutated_bits);

            fitted.push(SemanticMorClusterSignature {
                cluster_id: seed.cluster_id,
                signature,
                experts: seed.experts.clone(),
            });
            cluster_reports.push(SemanticMorClusterPrototypeReport {
                cluster_id: seed.cluster_id,
                examples: example_count,
                supported_bits,
                proposed_mutations,
                mutated_bits,
                budget_dropped_mutations,
                strongest_supported_margin_ppm,
                weakest_mutated_margin_ppm,
                mean_mutated_margin_ppm,
            });
        }

        let fitted_training = evaluate_cluster_prototypes(&fitted, examples)?;

        Ok(SemanticMorClusterPrototypeArtifact {
            clusters: fitted,
            report: SemanticMorClusterPrototypeTrainingReport {
                examples: examples.len(),
                cluster_count: seeds.len(),
                mutated_bits: mutated_bits_total,
                baseline_training,
                fitted_training,
                clusters: cluster_reports,
            },
        })
    }
}

pub fn evaluate_cluster_prototypes<const BITS: usize>(
    clusters: &[SemanticMorClusterSignature<BITS>],
    examples: &[SemanticMorClusterTrainingExample<BITS>],
) -> Result<SemanticMorClusterEvaluationReport, SemanticMorClusterPrototypeError> {
    validate_seed_clusters(clusters)?;

    let cluster_ids = clusters
        .iter()
        .map(|cluster| cluster.cluster_id)
        .collect::<BTreeSet<_>>();
    let mut totals = cluster_ids
        .iter()
        .copied()
        .map(|cluster_id| (cluster_id, (0_usize, 0_usize)))
        .collect::<BTreeMap<_, _>>();
    let mut correct = 0_usize;

    for example in examples {
        if !cluster_ids.contains(&example.target_cluster_id) {
            return Err(SemanticMorClusterPrototypeError::UnknownTargetCluster(
                example.target_cluster_id,
            ));
        }

        let predicted = predict_cluster(clusters, &example.routing)?;
        let row = totals
            .get_mut(&example.target_cluster_id)
            .ok_or(SemanticMorClusterPrototypeError::InternalInvariant)?;
        row.0 = row.0.saturating_add(1);
        if predicted == example.target_cluster_id {
            row.1 = row.1.saturating_add(1);
            correct = correct.saturating_add(1);
        }
    }

    let rows = totals
        .into_iter()
        .map(|(cluster_id, (cluster_examples, cluster_correct))| {
            SemanticMorClusterEvaluationRow {
                cluster_id,
                examples: cluster_examples,
                correct: cluster_correct,
                accuracy_ppm: ratio_ppm(cluster_correct, cluster_examples),
            }
        })
        .collect();

    Ok(SemanticMorClusterEvaluationReport {
        examples: examples.len(),
        correct,
        accuracy_ppm: ratio_ppm(correct, examples.len()),
        clusters: rows,
    })
}

fn predict_cluster<const BITS: usize>(
    clusters: &[SemanticMorClusterSignature<BITS>],
    routing: &SemanticBitField<BITS>,
) -> Result<u32, SemanticMorClusterPrototypeError> {
    let mut best: Option<(u64, u32)> = None;
    for cluster in clusters {
        let score = routing.xnor_popcount(&cluster.signature);
        match best {
            None => best = Some((score, cluster.cluster_id)),
            Some((best_score, best_id))
                if score > best_score || (score == best_score && cluster.cluster_id < best_id) =>
            {
                best = Some((score, cluster.cluster_id));
            }
            Some(_) => {}
        }
    }
    best.map(|(_, cluster_id)| cluster_id)
        .ok_or(SemanticMorClusterPrototypeError::NeedMultipleClusters)
}

fn validate_seed_clusters<const BITS: usize>(
    clusters: &[SemanticMorClusterSignature<BITS>],
) -> Result<(), SemanticMorClusterPrototypeError> {
    if clusters.len() < 2 {
        return Err(SemanticMorClusterPrototypeError::NeedMultipleClusters);
    }

    let mut ids = BTreeSet::new();
    for cluster in clusters {
        if !ids.insert(cluster.cluster_id) {
            return Err(SemanticMorClusterPrototypeError::DuplicateCluster(
                cluster.cluster_id,
            ));
        }
        if cluster.experts.is_empty() {
            return Err(SemanticMorClusterPrototypeError::EmptyClusterExperts(
                cluster.cluster_id,
            ));
        }
    }
    Ok(())
}

fn ratio_ppm(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128 * MOR_SCALE_PPM as u128) / denominator as u128) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BitMutation {
    bit: usize,
    value: bool,
    margin_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SemanticMorClusterPrototypeError {
    ZeroExamplesPerCluster,
    InvalidBitMargin(u32),
    ZeroMutationBudget,
    NeedMultipleClusters,
    EmptyTrainingSet,
    DuplicateCluster(u32),
    EmptyClusterExperts(u32),
    UnknownTargetCluster(u32),
    InsufficientClusterExamples {
        cluster_id: u32,
        actual: usize,
        minimum: usize,
    },
    InternalInvariant,
    BitField(SemanticBitFieldError),
}

impl fmt::Display for SemanticMorClusterPrototypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SemanticMorClusterPrototypeError {}

impl From<SemanticBitFieldError> for SemanticMorClusterPrototypeError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_cluster_prototypes, SemanticMorClusterPrototypeError,
        SemanticMorClusterPrototypeTrainer, SemanticMorClusterPrototypeTrainingConfig,
        SemanticMorClusterTrainingExample,
    };
    use crate::{
        mor::MOR_SCALE_PPM,
        semantic::{SemanticBitField, SemanticMorClusterSignature},
        PageId,
    };

    fn bits(indices: &[usize]) -> SemanticBitField<8> {
        SemanticBitField::from_set_indices(indices).expect("bits")
    }

    fn seeds_reversed() -> Vec<SemanticMorClusterSignature<8>> {
        vec![
            SemanticMorClusterSignature {
                cluster_id: 10,
                signature: SemanticBitField::ones().expect("ones"),
                experts: vec![PageId(0)],
            },
            SemanticMorClusterSignature {
                cluster_id: 20,
                signature: SemanticBitField::zeroed().expect("zeros"),
                experts: vec![PageId(1)],
            },
        ]
    }

    fn separated_training() -> Vec<SemanticMorClusterTrainingExample<8>> {
        vec![
            SemanticMorClusterTrainingExample {
                routing: bits(&[]),
                target_cluster_id: 10,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[0]),
                target_cluster_id: 10,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[0, 1, 2, 3, 4, 5, 6, 7]),
                target_cluster_id: 20,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[1, 2, 3, 4, 5, 6, 7]),
                target_cluster_id: 20,
            },
        ]
    }

    #[test]
    fn fitted_prototypes_reverse_bad_seeds_and_improve_training_and_holdout() {
        let config = SemanticMorClusterPrototypeTrainingConfig {
            min_examples_per_cluster: 2,
            min_bit_margin_ppm: 500_000,
            max_mutated_bits_per_cluster: 8,
        };
        let artifact =
            SemanticMorClusterPrototypeTrainer::fit(&seeds_reversed(), &separated_training(), config)
                .expect("fit");

        assert_eq!(artifact.report.baseline_training.accuracy_ppm, 0);
        assert_eq!(
            artifact.report.fitted_training.accuracy_ppm,
            MOR_SCALE_PPM
        );
        assert_eq!(artifact.report.mutated_bits, 16);

        let holdout = vec![
            SemanticMorClusterTrainingExample {
                routing: bits(&[1]),
                target_cluster_id: 10,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[0, 2, 3, 4, 5, 6, 7]),
                target_cluster_id: 20,
            },
        ];
        let evaluated = evaluate_cluster_prototypes(&artifact.clusters, &holdout).expect("holdout");
        assert_eq!(evaluated.accuracy_ppm, MOR_SCALE_PPM);
    }

    #[test]
    fn low_margin_tie_retains_seed_coordinate() {
        let seeds = vec![
            SemanticMorClusterSignature {
                cluster_id: 1,
                signature: bits(&[0]),
                experts: vec![PageId(0)],
            },
            SemanticMorClusterSignature {
                cluster_id: 2,
                signature: bits(&[7]),
                experts: vec![PageId(1)],
            },
        ];
        let examples = vec![
            SemanticMorClusterTrainingExample {
                routing: bits(&[0]),
                target_cluster_id: 1,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[]),
                target_cluster_id: 1,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[7]),
                target_cluster_id: 2,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[7]),
                target_cluster_id: 2,
            },
        ];
        let artifact = SemanticMorClusterPrototypeTrainer::fit(
            &seeds,
            &examples,
            SemanticMorClusterPrototypeTrainingConfig {
                min_examples_per_cluster: 2,
                min_bit_margin_ppm: 250_000,
                max_mutated_bits_per_cluster: 8,
            },
        )
        .expect("fit");

        let cluster1 = artifact
            .clusters
            .iter()
            .find(|cluster| cluster.cluster_id == 1)
            .expect("cluster 1");
        assert_eq!(cluster1.signature.get(0), Ok(true));
    }

    #[test]
    fn mutation_budget_prefers_strongest_supported_coordinate() {
        let seeds = vec![
            SemanticMorClusterSignature {
                cluster_id: 1,
                signature: bits(&[]),
                experts: vec![PageId(0)],
            },
            SemanticMorClusterSignature {
                cluster_id: 2,
                signature: bits(&[7]),
                experts: vec![PageId(1)],
            },
        ];
        let examples = vec![
            SemanticMorClusterTrainingExample {
                routing: bits(&[0, 1]),
                target_cluster_id: 1,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[0, 1]),
                target_cluster_id: 1,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[0, 1]),
                target_cluster_id: 1,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[0]),
                target_cluster_id: 1,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[7]),
                target_cluster_id: 2,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[7]),
                target_cluster_id: 2,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[7]),
                target_cluster_id: 2,
            },
            SemanticMorClusterTrainingExample {
                routing: bits(&[7]),
                target_cluster_id: 2,
            },
        ];

        let artifact = SemanticMorClusterPrototypeTrainer::fit(
            &seeds,
            &examples,
            SemanticMorClusterPrototypeTrainingConfig {
                min_examples_per_cluster: 4,
                min_bit_margin_ppm: 400_000,
                max_mutated_bits_per_cluster: 1,
            },
        )
        .expect("fit");

        let cluster1 = artifact
            .clusters
            .iter()
            .find(|cluster| cluster.cluster_id == 1)
            .expect("cluster 1");
        assert_eq!(cluster1.signature.get(0), Ok(true));
        assert_eq!(cluster1.signature.get(1), Ok(false));

        let report = artifact
            .report
            .clusters
            .iter()
            .find(|report| report.cluster_id == 1)
            .expect("cluster report");
        assert_eq!(report.proposed_mutations, 2);
        assert_eq!(report.mutated_bits, 1);
        assert_eq!(report.budget_dropped_mutations, 1);
    }

    #[test]
    fn unknown_labels_and_insufficient_examples_fail_closed() {
        let seeds = seeds_reversed();
        let unknown = vec![SemanticMorClusterTrainingExample {
            routing: bits(&[]),
            target_cluster_id: 999,
        }];
        assert_eq!(
            SemanticMorClusterPrototypeTrainer::fit(
                &seeds,
                &unknown,
                SemanticMorClusterPrototypeTrainingConfig {
                    min_examples_per_cluster: 1,
                    min_bit_margin_ppm: 1,
                    max_mutated_bits_per_cluster: 1,
                },
            ),
            Err(SemanticMorClusterPrototypeError::UnknownTargetCluster(999))
        );

        assert_eq!(
            SemanticMorClusterPrototypeTrainer::fit(
                &seeds,
                &separated_training(),
                SemanticMorClusterPrototypeTrainingConfig {
                    min_examples_per_cluster: 3,
                    min_bit_margin_ppm: 1,
                    max_mutated_bits_per_cluster: 8,
                },
            ),
            Err(
                SemanticMorClusterPrototypeError::InsufficientClusterExamples {
                    cluster_id: 10,
                    actual: 2,
                    minimum: 3,
                }
            )
        );
    }

    #[test]
    fn prediction_ties_are_deterministic_by_cluster_id() {
        let clusters = vec![
            SemanticMorClusterSignature {
                cluster_id: 7,
                signature: bits(&[]),
                experts: vec![PageId(0)],
            },
            SemanticMorClusterSignature {
                cluster_id: 3,
                signature: bits(&[]),
                experts: vec![PageId(1)],
            },
        ];
        let examples = vec![SemanticMorClusterTrainingExample {
            routing: bits(&[]),
            target_cluster_id: 3,
        }];
        let report = evaluate_cluster_prototypes(&clusters, &examples).expect("evaluate");
        assert_eq!(report.correct, 1);
    }
}

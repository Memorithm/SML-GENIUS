//! Deterministic supervised synthesis of native MOR router signatures.
//!
//! This slice learns hard, inspectable router signatures from labelled SML
//! states. For each router and fingerprint bit, it measures the difference
//! between positive-class and negative-class one-rates, keeps the strongest
//! discriminants, and freezes them into the native inference representation.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use crate::{
    mor::{ExpertProposal, RouterId, MOR_SCALE_PPM},
    mor_affinity::{
        MorAffinityError, MorAffinityPolicy, NativeAffinityBank, NativeRouterSpec,
        RouterSignatureAtom, SmlFingerprintPlan,
    },
    BooleanWorkspace,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeRouterTemplate {
    pub router_id: RouterId,
    pub reliability_ppm: u32,
    pub confidence_ppm: u32,
    pub balance_bias_ppm: i32,
    pub proposals: Vec<ExpertProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorAffinityTrainingExample {
    pub state: BooleanWorkspace,
    pub target_router: RouterId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MorAffinityTrainingConfig {
    pub max_atoms_per_router: usize,
    pub min_separation_ppm: u32,
    pub min_examples_per_router: usize,
}

impl Default for MorAffinityTrainingConfig {
    fn default() -> Self {
        Self {
            max_atoms_per_router: 16,
            min_separation_ppm: 100_000,
            min_examples_per_router: 2,
        }
    }
}

impl MorAffinityTrainingConfig {
    pub fn validate(self) -> Result<Self, MorAffinityTrainingError> {
        if self.max_atoms_per_router == 0 {
            return Err(MorAffinityTrainingError::ZeroAtomBudget);
        }
        if self.min_separation_ppm == 0 || self.min_separation_ppm > MOR_SCALE_PPM {
            return Err(MorAffinityTrainingError::InvalidSeparationThreshold(
                self.min_separation_ppm,
            ));
        }
        if self.min_examples_per_router == 0 {
            return Err(MorAffinityTrainingError::ZeroExamplesPerRouter);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouterTrainingReport {
    pub router_id: RouterId,
    pub positive_examples: usize,
    pub negative_examples: usize,
    pub selected_atoms: usize,
    pub strongest_separation_ppm: u32,
    pub weakest_selected_separation_ppm: u32,
    pub mean_selected_separation_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorAffinityTrainingReport {
    pub examples: usize,
    pub router_count: usize,
    pub fingerprint_bits: usize,
    pub selected_atoms: usize,
    pub training_correct: usize,
    pub training_accuracy_ppm: u32,
    pub routers: Vec<RouterTrainingReport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MorAffinityTrainingArtifact {
    pub bank: NativeAffinityBank,
    pub report: MorAffinityTrainingReport,
}

pub struct MorAffinityTrainer;

impl MorAffinityTrainer {
    pub fn train(
        fingerprint_plan: SmlFingerprintPlan,
        templates: Vec<NativeRouterTemplate>,
        examples: &[MorAffinityTrainingExample],
        bank_policy: MorAffinityPolicy,
        config: MorAffinityTrainingConfig,
    ) -> Result<MorAffinityTrainingArtifact, MorAffinityTrainingError> {
        let config = config.validate()?;
        bank_policy
            .validate()
            .map_err(MorAffinityTrainingError::Affinity)?;
        if config.max_atoms_per_router > bank_policy.max_atoms_per_router {
            return Err(MorAffinityTrainingError::TrainingAtomBudgetExceedsBank {
                training: config.max_atoms_per_router,
                bank: bank_policy.max_atoms_per_router,
            });
        }
        if templates.len() < 2 {
            return Err(MorAffinityTrainingError::NeedMultipleRouters);
        }
        if examples.is_empty() {
            return Err(MorAffinityTrainingError::EmptyTrainingSet);
        }

        let mut template_ids = BTreeSet::new();
        for template in &templates {
            if !template_ids.insert(template.router_id) {
                return Err(MorAffinityTrainingError::DuplicateRouter(
                    template.router_id,
                ));
            }
        }
        for example in examples {
            if !template_ids.contains(&example.target_router) {
                return Err(MorAffinityTrainingError::UnknownTargetRouter(
                    example.target_router,
                ));
            }
        }

        let fingerprints = examples
            .iter()
            .map(|example| {
                fingerprint_plan
                    .fingerprint(&example.state)
                    .map_err(MorAffinityTrainingError::Affinity)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut positive_counts = BTreeMap::new();
        for template in &templates {
            positive_counts.insert(template.router_id, 0_usize);
        }
        for example in examples {
            if let Some(count) = positive_counts.get_mut(&example.target_router) {
                *count = count.saturating_add(1);
            }
        }
        for (router_id, count) in &positive_counts {
            if *count < config.min_examples_per_router {
                return Err(MorAffinityTrainingError::InsufficientPositiveExamples {
                    router_id: *router_id,
                    actual: *count,
                    minimum: config.min_examples_per_router,
                });
            }
            let negatives = examples.len().saturating_sub(*count);
            if negatives < config.min_examples_per_router {
                return Err(MorAffinityTrainingError::InsufficientNegativeExamples {
                    router_id: *router_id,
                    actual: negatives,
                    minimum: config.min_examples_per_router,
                });
            }
        }

        let mut specs = Vec::with_capacity(templates.len());
        let mut router_reports = Vec::with_capacity(templates.len());
        let mut selected_atoms_total = 0_usize;

        for template in templates {
            let positive_examples = positive_counts[&template.router_id];
            let negative_examples = examples.len() - positive_examples;
            let mut candidates = Vec::with_capacity(fingerprint_plan.width());

            for bit in 0..fingerprint_plan.width() {
                let bit_u16 = u16::try_from(bit)
                    .map_err(|_| MorAffinityTrainingError::FingerprintIndexTooLarge(bit))?;
                let mut positive_ones = 0_usize;
                let mut negative_ones = 0_usize;

                for (example, fingerprint) in examples.iter().zip(&fingerprints) {
                    let value = fingerprint
                        .get(bit)
                        .ok_or(MorAffinityTrainingError::InternalInvariant)?;
                    if example.target_router == template.router_id {
                        positive_ones = positive_ones.saturating_add(value as usize);
                    } else {
                        negative_ones = negative_ones.saturating_add(value as usize);
                    }
                }

                let positive_one_ppm = ratio_ppm(positive_ones, positive_examples);
                let negative_one_ppm = ratio_ppm(negative_ones, negative_examples);
                let separation_ppm = positive_one_ppm.abs_diff(negative_one_ppm);
                if separation_ppm < config.min_separation_ppm {
                    continue;
                }
                candidates.push(BitCandidate {
                    bit: bit_u16,
                    expected: positive_one_ppm > negative_one_ppm,
                    separation_ppm,
                });
            }

            candidates.sort_by(|left, right| {
                right
                    .separation_ppm
                    .cmp(&left.separation_ppm)
                    .then_with(|| left.bit.cmp(&right.bit))
            });
            candidates.truncate(config.max_atoms_per_router);
            if candidates.is_empty() {
                return Err(MorAffinityTrainingError::UndiscriminatedRouter(
                    template.router_id,
                ));
            }

            let strongest_separation_ppm = candidates[0].separation_ppm;
            let weakest_selected_separation_ppm =
                candidates.last().map(|candidate| candidate.separation_ppm).unwrap_or(0);
            let separation_sum = candidates
                .iter()
                .map(|candidate| candidate.separation_ppm as u64)
                .sum::<u64>();
            let mean_selected_separation_ppm =
                (separation_sum / candidates.len() as u64) as u32;

            let atoms = candidates
                .iter()
                .map(|candidate| RouterSignatureAtom {
                    fingerprint_bit: candidate.bit,
                    expected: candidate.expected,
                    weight_ppm: candidate.separation_ppm,
                })
                .collect::<Vec<_>>();
            selected_atoms_total = selected_atoms_total.saturating_add(atoms.len());

            router_reports.push(RouterTrainingReport {
                router_id: template.router_id,
                positive_examples,
                negative_examples,
                selected_atoms: atoms.len(),
                strongest_separation_ppm,
                weakest_selected_separation_ppm,
                mean_selected_separation_ppm,
            });
            specs.push(NativeRouterSpec {
                router_id: template.router_id,
                atoms,
                reliability_ppm: template.reliability_ppm,
                confidence_ppm: template.confidence_ppm,
                balance_bias_ppm: template.balance_bias_ppm,
                proposals: template.proposals,
            });
        }

        let bank = NativeAffinityBank::new(fingerprint_plan, specs, bank_policy)
            .map_err(MorAffinityTrainingError::Affinity)?;
        let training_correct = count_training_correct(&bank, examples)?;
        let training_accuracy_ppm = ratio_ppm(training_correct, examples.len());

        Ok(MorAffinityTrainingArtifact {
            report: MorAffinityTrainingReport {
                examples: examples.len(),
                router_count: bank.routers().len(),
                fingerprint_bits: bank.fingerprint_plan().width(),
                selected_atoms: selected_atoms_total,
                training_correct,
                training_accuracy_ppm,
                routers: router_reports,
            },
            bank,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BitCandidate {
    bit: u16,
    expected: bool,
    separation_ppm: u32,
}

fn count_training_correct(
    bank: &NativeAffinityBank,
    examples: &[MorAffinityTrainingExample],
) -> Result<usize, MorAffinityTrainingError> {
    let mut correct = 0_usize;
    for example in examples {
        let batch = bank
            .materialize(&example.state)
            .map_err(MorAffinityTrainingError::Affinity)?;
        let mut best: Option<(u32, RouterId)> = None;
        for evidence in &batch.evidence {
            match best {
                None => best = Some((evidence.affinity_ppm, evidence.router_id)),
                Some((score, router_id))
                    if evidence.affinity_ppm > score
                        || (evidence.affinity_ppm == score && evidence.router_id < router_id) =>
                {
                    best = Some((evidence.affinity_ppm, evidence.router_id));
                }
                Some(_) => {}
            }
        }
        let predicted = best
            .map(|(_, router_id)| router_id)
            .ok_or(MorAffinityTrainingError::InternalInvariant)?;
        if predicted == example.target_router {
            correct = correct.saturating_add(1);
        }
    }
    Ok(correct)
}

fn ratio_ppm(numerator: usize, denominator: usize) -> u32 {
    if denominator == 0 {
        return 0;
    }
    ((numerator as u128 * MOR_SCALE_PPM as u128) / denominator as u128) as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MorAffinityTrainingError {
    ZeroAtomBudget,
    InvalidSeparationThreshold(u32),
    ZeroExamplesPerRouter,
    TrainingAtomBudgetExceedsBank {
        training: usize,
        bank: usize,
    },
    NeedMultipleRouters,
    EmptyTrainingSet,
    DuplicateRouter(RouterId),
    UnknownTargetRouter(RouterId),
    InsufficientPositiveExamples {
        router_id: RouterId,
        actual: usize,
        minimum: usize,
    },
    InsufficientNegativeExamples {
        router_id: RouterId,
        actual: usize,
        minimum: usize,
    },
    FingerprintIndexTooLarge(usize),
    UndiscriminatedRouter(RouterId),
    InternalInvariant,
    Affinity(MorAffinityError),
}

impl fmt::Display for MorAffinityTrainingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MorAffinityTrainingError {}

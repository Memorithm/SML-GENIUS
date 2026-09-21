//! Deterministic benchmark of Boolean parameter-search strategies.
//!
//! Wall-clock timing is intentionally excluded from this reference benchmark.
//! It measures objective evaluations and validated candidate quality so results
//! are reproducible across machines and runners.

use core::fmt;

use crate::{
    boolean_hierarchical::{
        BooleanGroupObservation, BooleanPageGroup, HierarchicalBooleanError,
        HierarchicalBooleanPolicy, HierarchicalBooleanSearch,
    },
    boolean_integral::BOOLEAN_INTEGRAL_SCALE_PPM,
    boolean_overlapping::{
        OverlappingProbeError, OverlappingProbeObservation, OverlappingProbePlan,
        OverlappingProbePolicy,
    },
    boolean_page::{BooleanPageError, BooleanPageRead, PackedBooleanPage},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanSearchStrategy {
    Coordinate,
    ContiguousHierarchy,
    HierarchyOverlapping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BooleanSearchBenchmarkPolicy {
    /// Contiguous hierarchy used for exact bit discovery. leaf_width must be one.
    pub contiguous: HierarchicalBooleanPolicy,
    /// Coarser hierarchy used before overlapping probes.
    pub overlap_hierarchy: HierarchicalBooleanPolicy,
    pub overlapping: OverlappingProbePolicy,
    pub rescue_groups_per_round: usize,
    pub max_candidates: usize,
    pub max_hierarchy_rounds: u16,
}

impl Default for BooleanSearchBenchmarkPolicy {
    fn default() -> Self {
        Self {
            contiguous: HierarchicalBooleanPolicy {
                root_group_width: 1_048_576,
                leaf_width: 1,
                min_improvement_ppm: 1,
                max_next_groups: 4_096,
                max_depth: 32,
            },
            overlap_hierarchy: HierarchicalBooleanPolicy::default(),
            overlapping: OverlappingProbePolicy::default(),
            rescue_groups_per_round: 2,
            max_candidates: 8,
            max_hierarchy_rounds: 32,
        }
    }
}

impl BooleanSearchBenchmarkPolicy {
    pub fn validate(self) -> Result<Self, BooleanSearchBenchmarkPolicyError> {
        self.contiguous
            .validate()
            .map_err(BooleanSearchBenchmarkPolicyError::Hierarchical)?;
        self.overlap_hierarchy
            .validate()
            .map_err(BooleanSearchBenchmarkPolicyError::Hierarchical)?;
        self.overlapping
            .validate()
            .map_err(BooleanSearchBenchmarkPolicyError::Overlapping)?;
        if self.contiguous.leaf_width != 1 {
            return Err(BooleanSearchBenchmarkPolicyError::ContiguousLeafMustBeOne(
                self.contiguous.leaf_width,
            ));
        }
        if self.overlap_hierarchy.leaf_width > self.overlapping.max_group_width {
            return Err(BooleanSearchBenchmarkPolicyError::OverlapLeafTooWide {
                leaf_width: self.overlap_hierarchy.leaf_width,
                maximum: self.overlapping.max_group_width,
            });
        }
        if self.max_candidates == 0 {
            return Err(BooleanSearchBenchmarkPolicyError::ZeroCandidateBudget);
        }
        if self.max_hierarchy_rounds == 0 {
            return Err(BooleanSearchBenchmarkPolicyError::ZeroHierarchyRounds);
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedBooleanCandidate {
    pub parameter_index: u64,
    pub candidate_loss_ppm: u32,
    pub improvement_ppm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanSearchMeasurement {
    pub strategy: BooleanSearchStrategy,
    pub baseline_loss_ppm: u32,
    pub objective_evaluations: u64,
    pub groups_evaluated: u64,
    pub probes_evaluated: u64,
    pub rescued_groups: u64,
    pub raw_candidates: u64,
    pub validated_candidates: Vec<ValidatedBooleanCandidate>,
}

impl BooleanSearchMeasurement {
    #[must_use]
    pub fn best_candidate(&self) -> Option<ValidatedBooleanCandidate> {
        self.validated_candidates.first().copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BooleanSearchBenchmarkReport {
    pub coordinate: BooleanSearchMeasurement,
    pub contiguous: BooleanSearchMeasurement,
    pub overlapping: BooleanSearchMeasurement,
}

pub struct BooleanSearchBenchmark;

impl BooleanSearchBenchmark {
    pub fn compare<E, Objective>(
        page: &PackedBooleanPage,
        policy: BooleanSearchBenchmarkPolicy,
        objective: Objective,
    ) -> Result<BooleanSearchBenchmarkReport, BooleanSearchBenchmarkError<E>>
    where
        Objective: Fn(&dyn BooleanPageRead) -> Result<u32, E>,
    {
        let policy = policy
            .validate()
            .map_err(BooleanSearchBenchmarkError::Policy)?;
        Ok(BooleanSearchBenchmarkReport {
            coordinate: run_coordinate(page, policy.max_candidates, &objective)?,
            contiguous: run_contiguous(page, policy, &objective)?,
            overlapping: run_overlapping(page, policy, &objective)?,
        })
    }
}

fn run_coordinate<E, Objective>(
    page: &PackedBooleanPage,
    max_candidates: usize,
    objective: &Objective,
) -> Result<BooleanSearchMeasurement, BooleanSearchBenchmarkError<E>>
where
    Objective: Fn(&dyn BooleanPageRead) -> Result<u32, E>,
{
    let baseline = evaluate(page, objective)?;
    let mut evaluations = 1_u64;
    let mut candidates = Vec::new();

    for parameter_index in 0..page.parameter_count() {
        let view = page
            .index_flip_view(&[parameter_index])
            .map_err(BooleanSearchBenchmarkError::Page)?;
        let loss = evaluate(&view, objective)?;
        evaluations = evaluations.saturating_add(1);
        if loss < baseline {
            candidates.push(ValidatedBooleanCandidate {
                parameter_index,
                candidate_loss_ppm: loss,
                improvement_ppm: baseline - loss,
            });
        }
    }

    rank_validated(&mut candidates);
    let raw_candidates = candidates.len() as u64;
    candidates.truncate(max_candidates);

    Ok(BooleanSearchMeasurement {
        strategy: BooleanSearchStrategy::Coordinate,
        baseline_loss_ppm: baseline,
        objective_evaluations: evaluations,
        groups_evaluated: 0,
        probes_evaluated: 0,
        rescued_groups: 0,
        raw_candidates,
        validated_candidates: candidates,
    })
}

fn run_contiguous<E, Objective>(
    page: &PackedBooleanPage,
    policy: BooleanSearchBenchmarkPolicy,
    objective: &Objective,
) -> Result<BooleanSearchMeasurement, BooleanSearchBenchmarkError<E>>
where
    Objective: Fn(&dyn BooleanPageRead) -> Result<u32, E>,
{
    let baseline = evaluate(page, objective)?;
    let mut evaluations = 1_u64;
    let mut groups_evaluated = 0_u64;
    let mut groups = HierarchicalBooleanSearch::initial_groups(
        page.page_id(),
        page.parameter_count(),
        policy.contiguous,
    )
    .map_err(BooleanSearchBenchmarkError::Hierarchical)?;
    let mut leaves = Vec::new();
    let mut rounds = 0_u16;

    while !groups.is_empty() && rounds < policy.max_hierarchy_rounds {
        let mut observations = Vec::with_capacity(groups.len());
        for group in groups.iter().copied() {
            let view = page
                .group_view(group)
                .map_err(BooleanSearchBenchmarkError::Page)?;
            let loss = evaluate(&view, objective)?;
            evaluations = evaluations.saturating_add(1);
            groups_evaluated = groups_evaluated.saturating_add(1);
            observations.push(BooleanGroupObservation {
                group,
                baseline_loss_ppm: baseline,
                perturbed_loss_ppm: loss,
            });
        }
        let round = HierarchicalBooleanSearch::refine(&observations, policy.contiguous)
            .map_err(BooleanSearchBenchmarkError::Hierarchical)?;
        leaves.extend(round.leaf_candidates);
        groups = round.next_groups;
        rounds = rounds.saturating_add(1);
    }

    let mut candidates = Vec::new();
    for leaf in leaves {
        if leaf.len != 1 {
            return Err(BooleanSearchBenchmarkError::NonUnitContiguousLeaf(leaf));
        }
        let view = page
            .index_flip_view(&[leaf.start])
            .map_err(BooleanSearchBenchmarkError::Page)?;
        let loss = evaluate(&view, objective)?;
        evaluations = evaluations.saturating_add(1);
        if loss < baseline {
            candidates.push(ValidatedBooleanCandidate {
                parameter_index: leaf.start,
                candidate_loss_ppm: loss,
                improvement_ppm: baseline - loss,
            });
        }
    }

    rank_validated(&mut candidates);
    let raw_candidates = candidates.len() as u64;
    candidates.truncate(policy.max_candidates);

    Ok(BooleanSearchMeasurement {
        strategy: BooleanSearchStrategy::ContiguousHierarchy,
        baseline_loss_ppm: baseline,
        objective_evaluations: evaluations,
        groups_evaluated,
        probes_evaluated: 0,
        rescued_groups: 0,
        raw_candidates,
        validated_candidates: candidates,
    })
}

fn run_overlapping<E, Objective>(
    page: &PackedBooleanPage,
    policy: BooleanSearchBenchmarkPolicy,
    objective: &Objective,
) -> Result<BooleanSearchMeasurement, BooleanSearchBenchmarkError<E>>
where
    Objective: Fn(&dyn BooleanPageRead) -> Result<u32, E>,
{
    let baseline = evaluate(page, objective)?;
    let mut evaluations = 1_u64;
    let mut groups_evaluated = 0_u64;
    let mut probes_evaluated = 0_u64;
    let mut rescued_groups = 0_u64;
    let mut groups = HierarchicalBooleanSearch::initial_groups(
        page.page_id(),
        page.parameter_count(),
        policy.overlap_hierarchy,
    )
    .map_err(BooleanSearchBenchmarkError::Hierarchical)?;
    let mut leaves = Vec::new();
    let mut rounds = 0_u16;

    while !groups.is_empty() && rounds < policy.max_hierarchy_rounds {
        let mut observations = Vec::with_capacity(groups.len());
        for group in groups.iter().copied() {
            let view = page
                .group_view(group)
                .map_err(BooleanSearchBenchmarkError::Page)?;
            let loss = evaluate(&view, objective)?;
            evaluations = evaluations.saturating_add(1);
            groups_evaluated = groups_evaluated.saturating_add(1);
            observations.push(BooleanGroupObservation {
                group,
                baseline_loss_ppm: baseline,
                perturbed_loss_ppm: loss,
            });
        }
        let round = HierarchicalBooleanSearch::refine_with_rescue(
            &observations,
            policy.overlap_hierarchy,
            policy.rescue_groups_per_round,
        )
        .map_err(BooleanSearchBenchmarkError::Hierarchical)?;
        rescued_groups = rescued_groups.saturating_add(round.rescued_groups);
        leaves.extend(round.leaf_candidates);
        groups = round.next_groups;
        rounds = rounds.saturating_add(1);
    }

    let mut scored = Vec::new();
    for leaf in leaves {
        if leaf.len == 1 {
            scored.push((leaf.start, i64::from(BOOLEAN_INTEGRAL_SCALE_PPM)));
            continue;
        }

        let plan = OverlappingProbePlan::new(leaf, policy.overlapping)
            .map_err(BooleanSearchBenchmarkError::Overlapping)?;
        let mut observations = Vec::with_capacity(plan.probe_count());
        for probe in plan.probes().iter().copied() {
            let view = page
                .probe_view(probe)
                .map_err(BooleanSearchBenchmarkError::Page)?;
            let loss = evaluate(&view, objective)?;
            evaluations = evaluations.saturating_add(1);
            probes_evaluated = probes_evaluated.saturating_add(1);
            observations.push(OverlappingProbeObservation {
                probe,
                baseline_loss_ppm: baseline,
                perturbed_loss_ppm: loss,
            });
        }
        let report = plan
            .score(&observations)
            .map_err(BooleanSearchBenchmarkError::Overlapping)?;
        scored.extend(
            report
                .candidates
                .into_iter()
                .map(|candidate| (candidate.parameter_index, candidate.net_score_ppm)),
        );
    }

    scored.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    scored.dedup_by_key(|candidate| candidate.0);
    let raw_candidates = scored.len() as u64;
    scored.truncate(policy.max_candidates);

    let mut candidates = Vec::new();
    for (parameter_index, _) in scored {
        let view = page
            .index_flip_view(&[parameter_index])
            .map_err(BooleanSearchBenchmarkError::Page)?;
        let loss = evaluate(&view, objective)?;
        evaluations = evaluations.saturating_add(1);
        if loss < baseline {
            candidates.push(ValidatedBooleanCandidate {
                parameter_index,
                candidate_loss_ppm: loss,
                improvement_ppm: baseline - loss,
            });
        }
    }
    rank_validated(&mut candidates);

    Ok(BooleanSearchMeasurement {
        strategy: BooleanSearchStrategy::HierarchyOverlapping,
        baseline_loss_ppm: baseline,
        objective_evaluations: evaluations,
        groups_evaluated,
        probes_evaluated,
        rescued_groups,
        raw_candidates,
        validated_candidates: candidates,
    })
}

fn rank_validated(candidates: &mut [ValidatedBooleanCandidate]) {
    candidates.sort_by(|left, right| {
        right
            .improvement_ppm
            .cmp(&left.improvement_ppm)
            .then_with(|| left.parameter_index.cmp(&right.parameter_index))
    });
}

fn evaluate<E, Objective>(
    read: &dyn BooleanPageRead,
    objective: &Objective,
) -> Result<u32, BooleanSearchBenchmarkError<E>>
where
    Objective: Fn(&dyn BooleanPageRead) -> Result<u32, E>,
{
    let loss = objective(read).map_err(BooleanSearchBenchmarkError::Objective)?;
    if loss > BOOLEAN_INTEGRAL_SCALE_PPM {
        return Err(BooleanSearchBenchmarkError::LossOutOfRange(loss));
    }
    Ok(loss)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BooleanSearchBenchmarkPolicyError {
    ContiguousLeafMustBeOne(u64),
    OverlapLeafTooWide { leaf_width: u64, maximum: u64 },
    ZeroCandidateBudget,
    ZeroHierarchyRounds,
    Hierarchical(HierarchicalBooleanError),
    Overlapping(OverlappingProbeError),
}

impl fmt::Display for BooleanSearchBenchmarkPolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for BooleanSearchBenchmarkPolicyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BooleanSearchBenchmarkError<E> {
    Policy(BooleanSearchBenchmarkPolicyError),
    Page(BooleanPageError),
    Hierarchical(HierarchicalBooleanError),
    Overlapping(OverlappingProbeError),
    NonUnitContiguousLeaf(BooleanPageGroup),
    LossOutOfRange(u32),
    Objective(E),
}

impl<E: fmt::Debug> fmt::Display for BooleanSearchBenchmarkError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<E: fmt::Debug> std::error::Error for BooleanSearchBenchmarkError<E> {}

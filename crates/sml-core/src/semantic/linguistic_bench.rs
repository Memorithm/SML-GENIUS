//! Deterministic linguistic micro-benchmark for Semantic Boolean Geometry.
//!
//! The fixture labels are mnemonic only. These probes test executable SBG
//! invariants under controlled Boolean constructions; they are not evidence of
//! natural-language understanding or language-model quality.

use core::fmt;

use crate::{
    memory::{BoundedAssociativeMemory, MemoryError, ReadOutcome, ReplacementPolicy, WriteOutcome},
    BooleanWorkspace, ExecutionCounters, WorkspaceError,
};

use super::{
    bind, compose_relation, derive_context_mask, masked_hamming_distance, unbind, ContextMaskError,
    SemanticAlgebraError, SemanticBitField, SemanticBitFieldError, SemanticCompositionError,
    SemanticMetricError, SemanticPolarity, SemanticRoleSet,
};

/// Fixed width used only by the deterministic SBG-0.11 fixture suite.
pub const LINGUISTIC_MICRO_BENCH_BITS: usize = 128;

/// One property exercised by the controlled linguistic fixture suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinguisticProbe {
    SynonymAntonymControl,
    RoleReversal,
    Negation,
    ReferenceCorrespondence,
    ControlledPolysemy,
    SemanticComposition,
    AssociativeRetrieval,
}

/// Named integer evidence attached to one probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinguisticMetric {
    pub name: &'static str,
    pub value: u64,
}

/// Result of one deterministic probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguisticProbeResult {
    pub probe: LinguisticProbe,
    pub passed: bool,
    pub metrics: Vec<LinguisticMetric>,
}

/// Aggregate SBG-0.11 micro-benchmark report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguisticMicroBenchmarkReport {
    pub cases: Vec<LinguisticProbeResult>,
}

impl LinguisticMicroBenchmarkReport {
    #[must_use]
    pub fn passed_cases(&self) -> usize {
        self.cases.iter().filter(|case| case.passed).count()
    }

    #[must_use]
    pub fn failed_cases(&self) -> usize {
        self.cases.len().saturating_sub(self.passed_cases())
    }

    #[must_use]
    pub fn all_passed(&self) -> bool {
        !self.cases.is_empty() && self.failed_cases() == 0
    }
}

/// Runs all controlled SBG linguistic probes.
///
/// Every metric is exact and integer-valued. The suite deliberately avoids
/// corpus statistics, learned claims, and floating-point semantic scores.
pub fn run_linguistic_micro_benchmark(
) -> Result<LinguisticMicroBenchmarkReport, LinguisticMicroBenchmarkError> {
    let roles = fixture_roles()?;
    let cases = vec![
        probe_synonym_antonym()?,
        probe_role_reversal(&roles)?,
        probe_negation(&roles)?,
        probe_reference_correspondence()?,
        probe_controlled_polysemy()?,
        probe_semantic_composition(&roles)?,
        probe_associative_retrieval()?,
    ];

    Ok(LinguisticMicroBenchmarkReport { cases })
}

fn probe_synonym_antonym() -> Result<LinguisticProbeResult, LinguisticMicroBenchmarkError> {
    // Mnemonic fixture: "rapid" and "quick" are constructed near one another;
    // "slow" is a deliberately distant control. This tests metric ordering.
    let rapid = field(&[0, 4, 8, 12, 16, 20, 64, 68, 72, 76, 80, 84])?;
    let quick = field(&[0, 4, 8, 12, 16, 21, 64, 68, 72, 76, 80, 84])?;
    let slow = field(&[1, 5, 9, 13, 17, 21, 65, 69, 73, 77, 81, 85])?;

    let synonym_distance = rapid.xor_popcount(&quick);
    let antonym_control_distance = rapid.xor_popcount(&slow);

    Ok(LinguisticProbeResult {
        probe: LinguisticProbe::SynonymAntonymControl,
        passed: synonym_distance < antonym_control_distance,
        metrics: vec![
            metric("synonym_distance", synonym_distance),
            metric("antonym_control_distance", antonym_control_distance),
        ],
    })
}

fn probe_role_reversal(
    roles: &SemanticRoleSet<LINGUISTIC_MICRO_BENCH_BITS>,
) -> Result<LinguisticProbeResult, LinguisticMicroBenchmarkError> {
    let cat = fixture_cat()?;
    let chase = fixture_chase()?;
    let mouse = fixture_mouse()?;

    let cat_chases_mouse =
        compose_relation(roles, &cat, &chase, &mouse, SemanticPolarity::Affirmed)?;
    let mouse_chases_cat =
        compose_relation(roles, &mouse, &chase, &cat, SemanticPolarity::Affirmed)?;

    let content_distance = cat_chases_mouse.content_distance(&mouse_chases_cat);
    let relational_distance = cat_chases_mouse.relational_distance(&mouse_chases_cat);

    Ok(LinguisticProbeResult {
        probe: LinguisticProbe::RoleReversal,
        passed: content_distance == 0 && relational_distance > 0,
        metrics: vec![
            metric("content_distance", content_distance),
            metric("relational_distance", relational_distance),
        ],
    })
}

fn probe_negation(
    roles: &SemanticRoleSet<LINGUISTIC_MICRO_BENCH_BITS>,
) -> Result<LinguisticProbeResult, LinguisticMicroBenchmarkError> {
    let cat = fixture_cat()?;
    let chase = fixture_chase()?;
    let mouse = fixture_mouse()?;

    let affirmed = compose_relation(roles, &cat, &chase, &mouse, SemanticPolarity::Affirmed)?;
    let negated = compose_relation(roles, &cat, &chase, &mouse, SemanticPolarity::Negated)?;

    let content_distance = affirmed.content_distance(&negated);
    let relational_distance = affirmed.relational_distance(&negated);
    let expected_negation_distance = roles.negation.count_ones();

    Ok(LinguisticProbeResult {
        probe: LinguisticProbe::Negation,
        passed: content_distance == 0 && relational_distance == expected_negation_distance,
        metrics: vec![
            metric("content_distance", content_distance),
            metric("relational_distance", relational_distance),
            metric("expected_negation_distance", expected_negation_distance),
        ],
    })
}

fn probe_reference_correspondence() -> Result<LinguisticProbeResult, LinguisticMicroBenchmarkError>
{
    // A reference role binds one entity and must recover it exactly with the
    // matching role while a deliberately wrong role is a negative control.
    let reference_role = field(&[1, 7, 19, 67])?;
    let wrong_role = field(&[3, 13, 53, 101])?;
    let entity = field(&[0, 4, 16, 20, 64, 68, 80, 84])?;

    let bound = bind(&reference_role, &entity)?;
    let recovered = unbind(&reference_role, &bound)?;
    let wrong = unbind(&wrong_role, &bound)?;

    let recovered_distance = entity.xor_popcount(&recovered);
    let wrong_role_distance = entity.xor_popcount(&wrong);

    Ok(LinguisticProbeResult {
        probe: LinguisticProbe::ReferenceCorrespondence,
        passed: recovered_distance == 0 && wrong_role_distance > 0,
        metrics: vec![
            metric("recovered_distance", recovered_distance),
            metric("wrong_role_distance", wrong_role_distance),
        ],
    })
}

fn probe_controlled_polysemy() -> Result<LinguisticProbeResult, LinguisticMicroBenchmarkError> {
    // Mnemonic fixture: one anchor is compared to "finance" and "river"
    // candidates. Independently derived context masks must reverse the nearest
    // candidate. This is a geometry control, not a lexical-language claim.
    let anchor = field(&[0, 2, 4, 6])?;
    let finance = field(&[0, 2, 5, 7])?;
    let river = field(&[1, 3, 4, 6])?;

    let f1 = field(&[0, 1, 2, 3])?;
    let f2 = field(&[0, 1, 2, 3])?;
    let f3 = field(&[0, 1, 2, 3])?;
    let finance_mask = derive_context_mask(&[&f1, &f2, &f3], 2)?;

    let r1 = field(&[4, 5, 6, 7])?;
    let r2 = field(&[4, 5, 6, 7])?;
    let r3 = field(&[4, 5, 6, 7])?;
    let river_mask = derive_context_mask(&[&r1, &r2, &r3], 2)?;

    let finance_target = masked_hamming_distance(&anchor, &finance, &finance_mask)?;
    let finance_control = masked_hamming_distance(&anchor, &river, &finance_mask)?;
    let river_target = masked_hamming_distance(&anchor, &river, &river_mask)?;
    let river_control = masked_hamming_distance(&anchor, &finance, &river_mask)?;

    Ok(LinguisticProbeResult {
        probe: LinguisticProbe::ControlledPolysemy,
        passed: finance_target < finance_control && river_target < river_control,
        metrics: vec![
            metric("finance_target_distance", finance_target),
            metric("finance_control_distance", finance_control),
            metric("river_target_distance", river_target),
            metric("river_control_distance", river_control),
        ],
    })
}

fn probe_semantic_composition(
    roles: &SemanticRoleSet<LINGUISTIC_MICRO_BENCH_BITS>,
) -> Result<LinguisticProbeResult, LinguisticMicroBenchmarkError> {
    let cat = fixture_cat()?;
    let chase = fixture_chase()?;
    let observe = field(&[1, 5, 10, 14, 18, 22, 65, 69, 74, 78, 82, 86])?;
    let mouse = fixture_mouse()?;

    let chase_relation = compose_relation(roles, &cat, &chase, &mouse, SemanticPolarity::Affirmed)?;
    let observe_relation =
        compose_relation(roles, &cat, &observe, &mouse, SemanticPolarity::Affirmed)?;

    let changed_content = chase_relation.content_distance(&observe_relation);
    let changed_relation = chase_relation.relational_distance(&observe_relation);

    Ok(LinguisticProbeResult {
        probe: LinguisticProbe::SemanticComposition,
        passed: changed_content > 0 && changed_relation > 0,
        metrics: vec![
            metric("changed_action_content_distance", changed_content),
            metric("changed_action_relational_distance", changed_relation),
        ],
    })
}

fn probe_associative_retrieval() -> Result<LinguisticProbeResult, LinguisticMicroBenchmarkError> {
    let concept = field(&[0, 3, 8, 13, 21, 34, 55, 89])?;
    let expected = field_to_workspace(&concept)?;

    let mut memory = BoundedAssociativeMemory::new(
        16,
        LINGUISTIC_MICRO_BENCH_BITS,
        ReplacementPolicy::RejectCollision,
    )?;
    let mut counters = ExecutionCounters::default();
    let key = 0x5342_4700_0011_u64;

    let write = memory.write(key, expected.clone(), &mut counters)?;
    let write_ok = matches!(
        write,
        WriteOutcome::Inserted { .. } | WriteOutcome::Updated { .. }
    );

    let read = memory.read(key, &mut counters);
    let retrieval_distance = match read {
        ReadOutcome::Hit { value, .. } => value.hamming_distance(&expected)?,
        ReadOutcome::EmptyMiss { .. } | ReadOutcome::KeyMismatch { .. } => {
            return Err(LinguisticMicroBenchmarkError::UnexpectedMemoryRead)
        }
    };

    Ok(LinguisticProbeResult {
        probe: LinguisticProbe::AssociativeRetrieval,
        passed: write_ok
            && retrieval_distance == 0
            && counters.memory_reads == 1
            && counters.memory_hits == 1,
        metrics: vec![
            metric("retrieval_distance", retrieval_distance),
            metric("memory_reads", counters.memory_reads),
            metric("memory_hits", counters.memory_hits),
            metric("memory_writes", counters.memory_writes),
        ],
    })
}

fn fixture_roles(
) -> Result<SemanticRoleSet<LINGUISTIC_MICRO_BENCH_BITS>, LinguisticMicroBenchmarkError> {
    Ok(SemanticRoleSet::new(
        field(&[1, 7, 19, 67])?,
        field(&[2, 11, 41, 83])?,
        field(&[3, 13, 53, 101])?,
        field(&[5, 17, 61, 109])?,
    )?)
}

fn fixture_cat(
) -> Result<SemanticBitField<LINGUISTIC_MICRO_BENCH_BITS>, LinguisticMicroBenchmarkError> {
    field(&[0, 4, 8, 12, 16, 20, 64, 68, 72, 76, 80, 84])
}

fn fixture_chase(
) -> Result<SemanticBitField<LINGUISTIC_MICRO_BENCH_BITS>, LinguisticMicroBenchmarkError> {
    field(&[0, 4, 9, 13, 17, 21, 64, 68, 73, 77, 81, 85])
}

fn fixture_mouse(
) -> Result<SemanticBitField<LINGUISTIC_MICRO_BENCH_BITS>, LinguisticMicroBenchmarkError> {
    field(&[0, 5, 8, 12, 18, 22, 64, 69, 72, 76, 82, 86])
}

fn field(
    indices: &[usize],
) -> Result<SemanticBitField<LINGUISTIC_MICRO_BENCH_BITS>, LinguisticMicroBenchmarkError> {
    SemanticBitField::from_set_indices(indices).map_err(LinguisticMicroBenchmarkError::BitField)
}

fn field_to_workspace(
    field: &SemanticBitField<LINGUISTIC_MICRO_BENCH_BITS>,
) -> Result<BooleanWorkspace, LinguisticMicroBenchmarkError> {
    let mut bits = Vec::with_capacity(LINGUISTIC_MICRO_BENCH_BITS);
    for index in 0..LINGUISTIC_MICRO_BENCH_BITS {
        bits.push(field.get(index)?);
    }
    Ok(BooleanWorkspace::from_bits(&bits))
}

const fn metric(name: &'static str, value: u64) -> LinguisticMetric {
    LinguisticMetric { name, value }
}

/// Construction or execution failure in the controlled benchmark suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinguisticMicroBenchmarkError {
    BitField(SemanticBitFieldError),
    Algebra(SemanticAlgebraError),
    Composition(SemanticCompositionError),
    Context(ContextMaskError),
    Metric(SemanticMetricError),
    Memory(MemoryError),
    Workspace(WorkspaceError),
    UnexpectedMemoryRead,
}

impl fmt::Display for LinguisticMicroBenchmarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for LinguisticMicroBenchmarkError {}

impl From<SemanticBitFieldError> for LinguisticMicroBenchmarkError {
    fn from(value: SemanticBitFieldError) -> Self {
        Self::BitField(value)
    }
}

impl From<SemanticAlgebraError> for LinguisticMicroBenchmarkError {
    fn from(value: SemanticAlgebraError) -> Self {
        Self::Algebra(value)
    }
}

impl From<SemanticCompositionError> for LinguisticMicroBenchmarkError {
    fn from(value: SemanticCompositionError) -> Self {
        Self::Composition(value)
    }
}

impl From<ContextMaskError> for LinguisticMicroBenchmarkError {
    fn from(value: ContextMaskError) -> Self {
        Self::Context(value)
    }
}

impl From<SemanticMetricError> for LinguisticMicroBenchmarkError {
    fn from(value: SemanticMetricError) -> Self {
        Self::Metric(value)
    }
}

impl From<MemoryError> for LinguisticMicroBenchmarkError {
    fn from(value: MemoryError) -> Self {
        Self::Memory(value)
    }
}

impl From<WorkspaceError> for LinguisticMicroBenchmarkError {
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{run_linguistic_micro_benchmark, LinguisticProbe};

    #[test]
    fn all_seven_controlled_linguistic_probes_pass() {
        let report = run_linguistic_micro_benchmark().expect("benchmark");

        assert_eq!(report.cases.len(), 7);
        assert_eq!(report.passed_cases(), 7);
        assert_eq!(report.failed_cases(), 0);
        assert!(report.all_passed());

        let expected = [
            LinguisticProbe::SynonymAntonymControl,
            LinguisticProbe::RoleReversal,
            LinguisticProbe::Negation,
            LinguisticProbe::ReferenceCorrespondence,
            LinguisticProbe::ControlledPolysemy,
            LinguisticProbe::SemanticComposition,
            LinguisticProbe::AssociativeRetrieval,
        ];
        let actual = report
            .cases
            .iter()
            .map(|case| case.probe)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }
}

use core::fmt;
use std::collections::BTreeSet;

pub const RSI_CANDIDATE_MANIFEST_SCHEMA_V1: &str = "sml.rsi-candidate@1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CandidateClass {
    DataCurriculum,
    ModelOrExpert,
    RoutingMemoryContext,
    TopologyOrWiring,
    RuntimePolicy,
    EvaluatorOrLoop,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EvidenceKind {
    Build,
    RequiredTests,
    Quality,
    Calibration,
    Context,
    Benchmark,
    ResourceBudget,
    Provenance,
    Determinism,
    Policy,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRef {
    pub kind: EvidenceKind,
    pub name: String,
    /// Stable evidence locator (for example a CI run/check URL or immutable
    /// artifact identifier). This is metadata only; verification is RSI-0b.
    pub locator: String,
    /// Lowercase SHA-256 of the evidence payload or immutable manifest.
    pub sha256: String,
}

impl EvidenceRef {
    pub fn new(
        kind: EvidenceKind,
        name: impl Into<String>,
        locator: impl Into<String>,
        sha256: impl Into<String>,
    ) -> Result<Self, CandidateManifestError> {
        let item = Self {
            kind,
            name: name.into(),
            locator: locator.into(),
            sha256: sha256.into(),
        };
        item.validate()?;
        Ok(item)
    }

    fn validate(&self) -> Result<(), CandidateManifestError> {
        validate_text("evidence.name", &self.name)?;
        validate_text("evidence.locator", &self.locator)?;
        validate_sha256("evidence.sha256", &self.sha256)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateBudget {
    pub max_wall_seconds: u64,
    pub max_candidates: u64,
    pub max_training_examples: u64,
    pub max_storage_bytes: u64,
}

impl CandidateBudget {
    pub fn validate(self) -> Result<(), CandidateManifestError> {
        if self.max_wall_seconds == 0 || self.max_candidates == 0 || self.max_storage_bytes == 0 {
            return Err(CandidateManifestError::InvalidBudget);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricDirection {
    HigherIsBetter,
    LowerIsBetter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricComparison {
    pub metric: String,
    pub direction: MetricDirection,
    /// Fixed-point micro-units chosen by the metric contract.
    pub parent_value: i128,
    pub candidate_value: i128,
    pub hard_gate: bool,
}

impl MetricComparison {
    fn validate(&self) -> Result<(), CandidateManifestError> {
        validate_text("metric", &self.metric)
    }

    #[must_use]
    pub const fn regressed(&self) -> bool {
        match self.direction {
            MetricDirection::HigherIsBetter => self.candidate_value < self.parent_value,
            MetricDirection::LowerIsBetter => self.candidate_value > self.parent_value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegressionSeverity {
    Informational,
    Material,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownRegression {
    pub area: String,
    pub description: String,
    pub severity: RegressionSeverity,
}

impl KnownRegression {
    fn validate(&self) -> Result<(), CandidateManifestError> {
        validate_text("regression.area", &self.area)?;
        validate_text("regression.description", &self.description)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackRef {
    /// For RSI-0a the rollback point is the exact parent champion.
    pub target_sha: String,
    pub note: String,
}

impl RollbackRef {
    fn validate(&self, parent_main_sha: &str) -> Result<(), CandidateManifestError> {
        validate_git_sha("rollback.target_sha", &self.target_sha)?;
        validate_text("rollback.note", &self.note)?;
        if self.target_sha != parent_main_sha {
            return Err(CandidateManifestError::RollbackNotParent);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RsiCandidateManifest {
    pub schema: String,
    pub parent_main_sha: String,
    pub candidate_head_sha: String,
    pub branch: String,
    pub class: CandidateClass,
    pub objective: String,
    pub budget: CandidateBudget,
    pub evidence: Vec<EvidenceRef>,
    pub comparisons: Vec<MetricComparison>,
    pub known_regressions: Vec<KnownRegression>,
    pub rollback: RollbackRef,
}

impl RsiCandidateManifest {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parent_main_sha: impl Into<String>,
        candidate_head_sha: impl Into<String>,
        branch: impl Into<String>,
        class: CandidateClass,
        objective: impl Into<String>,
        budget: CandidateBudget,
        evidence: Vec<EvidenceRef>,
        comparisons: Vec<MetricComparison>,
        known_regressions: Vec<KnownRegression>,
        rollback_note: impl Into<String>,
    ) -> Result<Self, CandidateManifestError> {
        let parent_main_sha = parent_main_sha.into();
        let candidate_head_sha = candidate_head_sha.into();
        let item = Self {
            schema: RSI_CANDIDATE_MANIFEST_SCHEMA_V1.to_string(),
            rollback: RollbackRef {
                target_sha: parent_main_sha.clone(),
                note: rollback_note.into(),
            },
            parent_main_sha,
            candidate_head_sha,
            branch: branch.into(),
            class,
            objective: objective.into(),
            budget,
            evidence,
            comparisons,
            known_regressions,
        };
        item.validate()?;
        Ok(item)
    }

    pub fn validate(&self) -> Result<(), CandidateManifestError> {
        if self.schema != RSI_CANDIDATE_MANIFEST_SCHEMA_V1 {
            return Err(CandidateManifestError::UnsupportedSchema);
        }
        validate_git_sha("parent_main_sha", &self.parent_main_sha)?;
        validate_git_sha("candidate_head_sha", &self.candidate_head_sha)?;
        if self.parent_main_sha == self.candidate_head_sha {
            return Err(CandidateManifestError::CandidateEqualsParent);
        }
        validate_branch(&self.branch)?;
        validate_text("objective", &self.objective)?;
        self.budget.validate()?;
        if self.evidence.is_empty() {
            return Err(CandidateManifestError::MissingEvidence);
        }

        let mut evidence_names = BTreeSet::new();
        let mut evidence_digests = BTreeSet::new();
        for evidence in &self.evidence {
            evidence.validate()?;
            if !evidence_names.insert(evidence.name.as_str()) {
                return Err(CandidateManifestError::DuplicateEvidenceName);
            }
            if !evidence_digests.insert(evidence.sha256.as_str()) {
                return Err(CandidateManifestError::DuplicateEvidenceDigest);
            }
        }

        let mut metrics = BTreeSet::new();
        for comparison in &self.comparisons {
            comparison.validate()?;
            if !metrics.insert(comparison.metric.as_str()) {
                return Err(CandidateManifestError::DuplicateMetric);
            }
        }

        for regression in &self.known_regressions {
            regression.validate()?;
        }
        self.rollback.validate(&self.parent_main_sha)
    }

    #[must_use]
    pub fn hard_gate_regressions(&self) -> usize {
        self.comparisons
            .iter()
            .filter(|comparison| comparison.hard_gate && comparison.regressed())
            .count()
    }

    /// Deterministic, human-readable canonical representation used for review
    /// and future hashing/serialization work. This is not JSON and not a
    /// cryptographic identity.
    pub fn canonical_text(&self) -> Result<String, CandidateManifestError> {
        self.validate()?;
        let mut out = String::new();
        use std::fmt::Write as _;
        writeln!(&mut out, "schema={}", self.schema).map_err(|_| CandidateManifestError::Format)?;
        writeln!(&mut out, "parent={}", self.parent_main_sha)
            .map_err(|_| CandidateManifestError::Format)?;
        writeln!(&mut out, "head={}", self.candidate_head_sha)
            .map_err(|_| CandidateManifestError::Format)?;
        writeln!(&mut out, "branch={}", escape(&self.branch))
            .map_err(|_| CandidateManifestError::Format)?;
        writeln!(&mut out, "class={:?}", self.class).map_err(|_| CandidateManifestError::Format)?;
        writeln!(&mut out, "objective={}", escape(&self.objective))
            .map_err(|_| CandidateManifestError::Format)?;
        writeln!(
            &mut out,
            "budget={},{},{},{}",
            self.budget.max_wall_seconds,
            self.budget.max_candidates,
            self.budget.max_training_examples,
            self.budget.max_storage_bytes
        )
        .map_err(|_| CandidateManifestError::Format)?;

        for evidence in &self.evidence {
            writeln!(
                &mut out,
                "evidence={:?}|{}|{}|{}",
                evidence.kind,
                escape(&evidence.name),
                escape(&evidence.locator),
                evidence.sha256
            )
            .map_err(|_| CandidateManifestError::Format)?;
        }
        for comparison in &self.comparisons {
            writeln!(
                &mut out,
                "metric={}|{:?}|{}|{}|{}",
                escape(&comparison.metric),
                comparison.direction,
                comparison.parent_value,
                comparison.candidate_value,
                comparison.hard_gate
            )
            .map_err(|_| CandidateManifestError::Format)?;
        }
        for regression in &self.known_regressions {
            writeln!(
                &mut out,
                "regression={:?}|{}|{}",
                regression.severity,
                escape(&regression.area),
                escape(&regression.description)
            )
            .map_err(|_| CandidateManifestError::Format)?;
        }
        writeln!(
            &mut out,
            "rollback={}|{}",
            self.rollback.target_sha,
            escape(&self.rollback.note)
        )
        .map_err(|_| CandidateManifestError::Format)?;
        Ok(out)
    }
}

fn validate_git_sha(field: &'static str, value: &str) -> Result<(), CandidateManifestError> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CandidateManifestError::InvalidGitSha(field));
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), CandidateManifestError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CandidateManifestError::InvalidSha256(field));
    }
    Ok(())
}

fn validate_branch(value: &str) -> Result<(), CandidateManifestError> {
    validate_text("branch", value)?;
    if matches!(value, "main" | "master")
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains("..")
        || value.contains(char::is_whitespace)
    {
        return Err(CandidateManifestError::InvalidBranch);
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str) -> Result<(), CandidateManifestError> {
    if value.trim().is_empty()
        || value.contains('\0')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(CandidateManifestError::InvalidText(field));
    }
    Ok(())
}

fn escape(value: &str) -> String {
    value
        .replace('%', "%25")
        .replace('|', "%7c")
        .replace('=', "%3d")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidateManifestError {
    UnsupportedSchema,
    InvalidGitSha(&'static str),
    InvalidSha256(&'static str),
    InvalidText(&'static str),
    InvalidBranch,
    InvalidBudget,
    CandidateEqualsParent,
    MissingEvidence,
    DuplicateEvidenceName,
    DuplicateEvidenceDigest,
    DuplicateMetric,
    RollbackNotParent,
    Format,
}

impl fmt::Display for CandidateManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for CandidateManifestError {}

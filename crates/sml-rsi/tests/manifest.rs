use sml_rsi::manifest::{
    CandidateBudget, CandidateClass, CandidateManifestError, EvidenceKind, EvidenceRef,
    KnownRegression, MetricComparison, MetricDirection, RegressionSeverity, RsiCandidateManifest,
    RSI_CANDIDATE_MANIFEST_SCHEMA_V1,
};

const PARENT: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";
const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn budget() -> CandidateBudget {
    CandidateBudget {
        max_wall_seconds: 3600,
        max_candidates: 32,
        max_training_examples: 10_000,
        max_storage_bytes: 1 << 30,
    }
}

fn evidence() -> Vec<EvidenceRef> {
    vec![
        EvidenceRef::new(
            EvidenceKind::RequiredTests,
            "workspace-tests",
            "github-run:123/job:456",
            SHA_A,
        )
        .unwrap(),
        EvidenceRef::new(
            EvidenceKind::ResourceBudget,
            "resource-report",
            "artifact:resource-v1",
            SHA_B,
        )
        .unwrap(),
    ]
}

fn manifest() -> RsiCandidateManifest {
    RsiCandidateManifest::new(
        PARENT,
        HEAD,
        "research/sml-rsi-candidate",
        CandidateClass::EvaluatorOrLoop,
        "add exact-head RSI evidence binding",
        budget(),
        evidence(),
        vec![
            MetricComparison {
                metric: "quality_ppm".into(),
                direction: MetricDirection::HigherIsBetter,
                parent_value: 900_000,
                candidate_value: 910_000,
                hard_gate: true,
            },
            MetricComparison {
                metric: "latency_us".into(),
                direction: MetricDirection::LowerIsBetter,
                parent_value: 100,
                candidate_value: 105,
                hard_gate: false,
            },
        ],
        vec![KnownRegression {
            area: "latency".into(),
            description: "fixture latency increased by five microseconds".into(),
            severity: RegressionSeverity::Informational,
        }],
        "revert candidate to declared parent champion",
    )
    .unwrap()
}

#[test]
fn valid_manifest_binds_parent_head_evidence_budget_and_rollback() {
    let item = manifest();
    item.validate().unwrap();
    assert_eq!(item.schema, RSI_CANDIDATE_MANIFEST_SCHEMA_V1);
    assert_eq!(item.parent_main_sha, PARENT);
    assert_eq!(item.candidate_head_sha, HEAD);
    assert_eq!(item.rollback.target_sha, PARENT);
    assert_eq!(item.evidence.len(), 2);
    assert_eq!(item.hard_gate_regressions(), 0);
}

#[test]
fn hard_gate_regression_is_counted_but_not_silently_hidden() {
    let mut item = manifest();
    item.comparisons[0].candidate_value = 899_999;
    assert_eq!(item.hard_gate_regressions(), 1);
    item.validate().unwrap();
}

#[test]
fn canonical_text_is_deterministic_and_escapes_delimiters() {
    let mut item = manifest();
    item.objective = "improve x=y|z%".into();
    let left = item.canonical_text().unwrap();
    let right = item.canonical_text().unwrap();
    assert_eq!(left, right);
    assert!(left.contains("objective=improve x%3dy%7cz%25"));
    assert!(left.contains(&format!("parent={PARENT}")));
    assert!(left.contains(&format!("head={HEAD}")));
}

#[test]
fn parent_and_head_must_be_distinct_valid_git_shas() {
    let mut same = manifest();
    same.candidate_head_sha = same.parent_main_sha.clone();
    assert_eq!(
        same.validate(),
        Err(CandidateManifestError::CandidateEqualsParent)
    );

    let mut short = manifest();
    short.parent_main_sha = "abc".into();
    assert_eq!(
        short.validate(),
        Err(CandidateManifestError::InvalidGitSha("parent_main_sha"))
    );
}

#[test]
fn branch_cannot_target_default_branch_or_ambiguous_path() {
    for bad in ["main", "master", "/candidate", "candidate/", "a..b", "has space"] {
        let mut item = manifest();
        item.branch = bad.into();
        assert_eq!(item.validate(), Err(CandidateManifestError::InvalidBranch));
    }
}

#[test]
fn evidence_requires_unique_names_and_digests() {
    let mut names = manifest();
    names.evidence[1].name = names.evidence[0].name.clone();
    assert_eq!(
        names.validate(),
        Err(CandidateManifestError::DuplicateEvidenceName)
    );

    let mut digests = manifest();
    digests.evidence[1].sha256 = digests.evidence[0].sha256.clone();
    assert_eq!(
        digests.validate(),
        Err(CandidateManifestError::DuplicateEvidenceDigest)
    );
}

#[test]
fn evidence_digest_must_be_lowercase_sha256() {
    assert_eq!(
        EvidenceRef::new(
            EvidenceKind::Build,
            "build",
            "run:1",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        ),
        Err(CandidateManifestError::InvalidSha256("evidence.sha256"))
    );
}

#[test]
fn budget_and_evidence_are_mandatory() {
    let mut bad_budget = manifest();
    bad_budget.budget.max_candidates = 0;
    assert_eq!(
        bad_budget.validate(),
        Err(CandidateManifestError::InvalidBudget)
    );

    let mut no_evidence = manifest();
    no_evidence.evidence.clear();
    assert_eq!(
        no_evidence.validate(),
        Err(CandidateManifestError::MissingEvidence)
    );
}

#[test]
fn rollback_target_is_forced_to_exact_parent_champion() {
    let mut item = manifest();
    item.rollback.target_sha = HEAD.into();
    assert_eq!(
        item.validate(),
        Err(CandidateManifestError::RollbackNotParent)
    );
}

#[test]
fn duplicate_metric_names_fail_closed() {
    let mut item = manifest();
    item.comparisons.push(MetricComparison {
        metric: "quality_ppm".into(),
        direction: MetricDirection::HigherIsBetter,
        parent_value: 1,
        candidate_value: 2,
        hard_gate: false,
    });
    assert_eq!(
        item.validate(),
        Err(CandidateManifestError::DuplicateMetric)
    );
}

#[test]
fn multiline_review_fields_are_rejected() {
    let mut item = manifest();
    item.objective = "first line\nsecond line".into();
    assert_eq!(
        item.validate(),
        Err(CandidateManifestError::InvalidText("objective"))
    );
}

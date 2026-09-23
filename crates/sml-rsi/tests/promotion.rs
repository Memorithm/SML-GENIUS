use std::collections::BTreeSet;

use sml_rsi::{
    manifest::{
        CandidateBudget, CandidateClass, EvidenceKind, EvidenceRef, KnownRegression,
        MetricComparison, MetricDirection, RegressionSeverity, RsiCandidateManifest,
    },
    promotion::{
        evaluate_promotion_eligibility, PromotionIneligibility, PromotionPolicy,
        PromotionReviewContext, PromotionStatus,
    },
};

const PARENT: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";
const SHA_BUILD: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_TESTS: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_DET: &str =
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SHA_QUALITY: &str =
    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

fn evidence(kind: EvidenceKind, name: &str, digest: &str) -> EvidenceRef {
    EvidenceRef::new(kind, name, format!("evidence:{name}"), digest).unwrap()
}

fn manifest() -> RsiCandidateManifest {
    RsiCandidateManifest::new(
        PARENT,
        HEAD,
        "research/rsi-candidate",
        CandidateClass::ModelOrExpert,
        "improve bounded decision expert",
        CandidateBudget {
            max_wall_seconds: 3600,
            max_candidates: 16,
            max_training_examples: 1000,
            max_storage_bytes: 1 << 28,
        },
        vec![
            evidence(EvidenceKind::Build, "build", SHA_BUILD),
            evidence(EvidenceKind::RequiredTests, "tests", SHA_TESTS),
            evidence(EvidenceKind::Determinism, "determinism", SHA_DET),
            evidence(EvidenceKind::Quality, "protected-quality", SHA_QUALITY),
        ],
        vec![MetricComparison {
            metric: "heldout_quality_ppm".into(),
            direction: MetricDirection::HigherIsBetter,
            parent_value: 800_000,
            candidate_value: 810_000,
            hard_gate: true,
        }],
        Vec::new(),
        "revert to exact parent",
    )
    .unwrap()
}

fn context() -> PromotionReviewContext {
    PromotionReviewContext {
        current_main_sha: PARENT.into(),
        pr_head_sha: HEAD.into(),
        selection_evidence_sha256: [SHA_BUILD.to_string(), SHA_TESTS.to_string()]
            .into_iter()
            .collect(),
        protected_evidence_sha256: [SHA_QUALITY.to_string()].into_iter().collect(),
    }
}

#[test]
fn exact_head_candidate_with_disjoint_protected_evidence_is_eligible_for_human_review() {
    let report =
        evaluate_promotion_eligibility(&manifest(), &context(), &PromotionPolicy::baseline());
    assert_eq!(report.status, PromotionStatus::EligibleForHumanReview);
    assert!(report.reasons.is_empty());
    assert_eq!(report.checked_evidence, 4);
    assert_eq!(report.protected_evidence, 1);
    assert!(!report.is_merge_authorization());
}

#[test]
fn stale_parent_is_ineligible() {
    let mut ctx = context();
    ctx.current_main_sha = "3333333333333333333333333333333333333333".into();
    let report = evaluate_promotion_eligibility(&manifest(), &ctx, &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::StaleParent));
}

#[test]
fn exact_pr_head_binding_is_required() {
    let mut ctx = context();
    ctx.pr_head_sha = "3333333333333333333333333333333333333333".into();
    let report = evaluate_promotion_eligibility(&manifest(), &ctx, &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::HeadMismatch));
}

#[test]
fn required_evidence_kind_must_be_present() {
    let mut policy = PromotionPolicy::baseline();
    policy.required_evidence.insert(EvidenceKind::Policy);
    let report = evaluate_promotion_eligibility(&manifest(), &context(), &policy);
    assert!(report.reasons.contains(
        &PromotionIneligibility::MissingRequiredEvidence(EvidenceKind::Policy)
    ));
}

#[test]
fn hard_gate_metric_regression_blocks_eligibility() {
    let mut candidate = manifest();
    candidate.comparisons[0].candidate_value = 799_999;
    let report =
        evaluate_promotion_eligibility(&candidate, &context(), &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::HardGateRegression));
}

#[test]
fn critical_known_regression_blocks_baseline_policy() {
    let mut candidate = manifest();
    candidate.known_regressions.push(KnownRegression {
        area: "correctness".into(),
        description: "known protected task regression".into(),
        severity: RegressionSeverity::Critical,
    });
    let report =
        evaluate_promotion_eligibility(&candidate, &context(), &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::CriticalKnownRegression));
}

#[test]
fn protected_evidence_is_required_and_must_not_select_candidate() {
    let mut missing = context();
    missing.protected_evidence_sha256.clear();
    let report =
        evaluate_promotion_eligibility(&manifest(), &missing, &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::MissingProtectedEvaluation));

    let mut contaminated = context();
    contaminated
        .selection_evidence_sha256
        .insert(SHA_QUALITY.to_string());
    let report =
        evaluate_promotion_eligibility(&manifest(), &contaminated, &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::ProtectedEvidenceUsedForSelection));
}

#[test]
fn review_context_cannot_reference_evidence_absent_from_manifest() {
    let unknown = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let mut selection = context();
    selection
        .selection_evidence_sha256
        .insert(unknown.to_string());
    let report =
        evaluate_promotion_eligibility(&manifest(), &selection, &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::UnknownSelectionEvidence));

    let mut protected = context();
    protected.protected_evidence_sha256 = [unknown.to_string()].into_iter().collect();
    let report =
        evaluate_promotion_eligibility(&manifest(), &protected, &PromotionPolicy::baseline());
    assert!(report
        .reasons
        .contains(&PromotionIneligibility::UnknownProtectedEvidence));
}

#[test]
fn policy_can_require_additional_typed_evidence_without_changing_manifest_schema() {
    let mut candidate = manifest();
    let digest = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    candidate
        .evidence
        .push(evidence(EvidenceKind::Policy, "policy", digest));
    let mut policy = PromotionPolicy::baseline();
    policy.required_evidence.insert(EvidenceKind::Policy);
    let report = evaluate_promotion_eligibility(&candidate, &context(), &policy);
    assert!(report.eligible_for_human_review());
}

#[test]
fn report_reasons_are_deterministically_sorted() {
    let mut candidate = manifest();
    candidate.comparisons[0].candidate_value = 0;
    let mut ctx = context();
    ctx.current_main_sha = "3333333333333333333333333333333333333333".into();
    ctx.protected_evidence_sha256 = BTreeSet::new();

    let left =
        evaluate_promotion_eligibility(&candidate, &ctx, &PromotionPolicy::baseline());
    let right =
        evaluate_promotion_eligibility(&candidate, &ctx, &PromotionPolicy::baseline());
    assert_eq!(left.reasons, right.reasons);
    assert_eq!(left.status, PromotionStatus::Ineligible);
}

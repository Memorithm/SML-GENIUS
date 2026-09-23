use std::collections::BTreeSet;

use sml_core::tasks::TaskKind;
use sml_rsi::{
    curriculum::{
        build_verified_curriculum, CurriculumPolicy, CurriculumProposal, CurriculumReport,
    },
    cycle::{CycleError, CyclePhase, RsiCycle},
    manifest::{
        CandidateBudget, CandidateClass, EvidenceKind, EvidenceRef, MetricComparison,
        MetricDirection, RsiCandidateManifest,
    },
    promotion::{PromotionPolicy, PromotionReviewContext, PromotionStatus},
};

const PARENT: &str = "1111111111111111111111111111111111111111";
const HEAD: &str = "2222222222222222222222222222222222222222";
const SHA_BUILD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_TESTS: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_DET: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const SHA_QUALITY: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

fn curriculum() -> CurriculumReport {
    build_verified_curriculum(
        &[CurriculumProposal {
            proposal_id: "self-generated-recall".into(),
            family: TaskKind::DelayedBitRecall,
            case_id_start: 1_000_000,
            case_count: 32,
            difficulty: 7,
        }],
        CurriculumPolicy::default(),
    )
    .unwrap()
}

fn evidence(kind: EvidenceKind, name: &str, digest: &str) -> EvidenceRef {
    EvidenceRef::new(kind, name, format!("evidence:{name}"), digest).unwrap()
}

fn manifest() -> RsiCandidateManifest {
    RsiCandidateManifest::new(
        PARENT,
        HEAD,
        "research/rsi-cycle-candidate",
        CandidateClass::ModelOrExpert,
        "train bounded recall candidate",
        CandidateBudget {
            max_wall_seconds: 3600,
            max_candidates: 8,
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
            metric: "protected_quality_ppm".into(),
            direction: MetricDirection::HigherIsBetter,
            parent_value: 700_000,
            candidate_value: 710_000,
            hard_gate: true,
        }],
        Vec::new(),
        "revert to frozen champion",
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

fn cycle_to_evidence_bound() -> RsiCycle {
    let mut cycle = RsiCycle::begin("cycle-0001", "improve recall", PARENT).unwrap();
    cycle.attach_curriculum(&curriculum()).unwrap();
    cycle.freeze_candidate(HEAD).unwrap();
    cycle.bind_manifest(manifest()).unwrap();
    cycle
}

#[test]
fn happy_path_reaches_human_review_without_self_promotion() {
    let mut cycle = cycle_to_evidence_bound();
    let report = cycle
        .review(&context(), &PromotionPolicy::baseline())
        .unwrap();

    assert_eq!(report.status, PromotionStatus::EligibleForHumanReview);
    assert_eq!(cycle.phase(), CyclePhase::ReviewedEligible);
    assert!(cycle.human_merge_gate_required());
    assert!(!cycle.self_promotion_authorized());
    assert_eq!(cycle.champion_sha(), PARENT);
    assert_eq!(cycle.candidate_head_sha(), Some(HEAD));
}

#[test]
fn phase_order_is_fail_closed() {
    let mut cycle = RsiCycle::begin("cycle", "objective", PARENT).unwrap();
    assert_eq!(
        cycle.freeze_candidate(HEAD),
        Err(CycleError::InvalidPhase {
            expected: CyclePhase::CurriculumVerified,
            actual: CyclePhase::ChampionFrozen,
        })
    );

    cycle.attach_curriculum(&curriculum()).unwrap();
    assert_eq!(
        cycle.bind_manifest(manifest()),
        Err(CycleError::InvalidPhase {
            expected: CyclePhase::CandidateFrozen,
            actual: CyclePhase::CurriculumVerified,
        })
    );
}

#[test]
fn candidate_cannot_equal_frozen_champion() {
    let mut cycle = RsiCycle::begin("cycle", "objective", PARENT).unwrap();
    cycle.attach_curriculum(&curriculum()).unwrap();
    assert_eq!(
        cycle.freeze_candidate(PARENT),
        Err(CycleError::CandidateEqualsChampion)
    );
}

#[test]
fn manifest_must_bind_exact_cycle_parent_and_candidate() {
    let mut cycle = RsiCycle::begin("cycle", "objective", PARENT).unwrap();
    cycle.attach_curriculum(&curriculum()).unwrap();
    cycle.freeze_candidate(HEAD).unwrap();

    let mut wrong_parent = manifest();
    wrong_parent.parent_main_sha = "3333333333333333333333333333333333333333".into();
    wrong_parent.rollback.target_sha = wrong_parent.parent_main_sha.clone();
    assert_eq!(
        cycle.bind_manifest(wrong_parent),
        Err(CycleError::ManifestParentMismatch)
    );

    let mut cycle = RsiCycle::begin("cycle", "objective", PARENT).unwrap();
    cycle.attach_curriculum(&curriculum()).unwrap();
    cycle.freeze_candidate(HEAD).unwrap();
    let mut wrong_head = manifest();
    wrong_head.candidate_head_sha = "3333333333333333333333333333333333333333".into();
    assert_eq!(
        cycle.bind_manifest(wrong_head),
        Err(CycleError::ManifestHeadMismatch)
    );
}

#[test]
fn review_rejects_if_main_moved_after_champion_freeze() {
    let mut cycle = cycle_to_evidence_bound();
    let mut ctx = context();
    ctx.current_main_sha = "3333333333333333333333333333333333333333".into();
    assert_eq!(
        cycle.review(&ctx, &PromotionPolicy::baseline()),
        Err(CycleError::ChampionMoved)
    );
    assert_eq!(cycle.phase(), CyclePhase::EvidenceBound);
}

#[test]
fn review_rejects_if_pr_head_changed_after_candidate_freeze() {
    let mut cycle = cycle_to_evidence_bound();
    let mut ctx = context();
    ctx.pr_head_sha = "3333333333333333333333333333333333333333".into();
    assert_eq!(
        cycle.review(&ctx, &PromotionPolicy::baseline()),
        Err(CycleError::ReviewHeadMismatch)
    );
    assert_eq!(cycle.phase(), CyclePhase::EvidenceBound);
}

#[test]
fn ineligible_review_is_recorded_without_mutating_champion() {
    let mut cycle = cycle_to_evidence_bound();
    let mut ctx = context();
    ctx.protected_evidence_sha256 = BTreeSet::new();
    let report = cycle.review(&ctx, &PromotionPolicy::baseline()).unwrap();

    assert_eq!(report.status, PromotionStatus::Ineligible);
    assert_eq!(cycle.phase(), CyclePhase::ReviewedIneligible);
    assert_eq!(cycle.champion_sha(), PARENT);
    assert!(!cycle.self_promotion_authorized());
}

#[test]
fn pr_summary_is_deterministic_and_records_human_gate() {
    let cycle = cycle_to_evidence_bound();
    let left = cycle.pr_summary().unwrap();
    let right = cycle.pr_summary().unwrap();
    assert_eq!(left, right);
    assert!(left.contains("parent=1111111111111111111111111111111111111111"));
    assert!(left.contains("head=2222222222222222222222222222222222222222"));
    assert!(left.contains("human_merge_gate=true"));
    assert!(left.contains("self_promotion=false"));
    assert!(left.contains("verified_tasks=32"));
}

#[test]
fn empty_or_invalid_identity_cannot_start_cycle() {
    assert_eq!(
        RsiCycle::begin("", "objective", PARENT),
        Err(CycleError::InvalidText)
    );
    assert_eq!(
        RsiCycle::begin("cycle", "objective", "bad-sha"),
        Err(CycleError::InvalidGitSha)
    );
}

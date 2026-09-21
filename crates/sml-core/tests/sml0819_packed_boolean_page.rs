use sml_core::{
    boolean_hierarchical::{
        BooleanCommitDecision, BooleanCommitEvidence, BooleanCommitPolicy, BooleanPageGroup,
    },
    boolean_overlapping::{OverlappingProbePlan, OverlappingProbePolicy},
    boolean_page::{
        evaluate_overlapping_plan, BooleanPageError, BooleanPageRead, PackedBooleanPage,
    },
    PageId,
};

fn probe_policy(max_candidates: usize) -> OverlappingProbePolicy {
    OverlappingProbePolicy {
        rounds: 4,
        max_group_width: 256,
        min_confidence_ppm: 250_000,
        min_net_score_ppm: 1,
        max_candidates,
        seed: 1,
    }
}

#[test]
fn packed_page_accounts_one_physical_bit_per_boolean_parameter() {
    let page = PackedBooleanPage::zeros(PageId(0), 130).unwrap();

    assert_eq!(page.parameter_count(), 130);
    assert_eq!(page.payload_bits(), 130);
    assert_eq!(page.payload_bytes_ceil(), 17);
    assert_eq!(
        PackedBooleanPage::packed_bytes_for_parameters(62_500_000),
        7_812_500
    );
}

#[test]
fn probe_execution_is_ephemeral_and_recovers_cancelled_signal() {
    let page = PackedBooleanPage::zeros(PageId(7), 8).unwrap();
    let group = BooleanPageGroup::new(PageId(7), 0, 8, 0).unwrap();
    let plan = OverlappingProbePlan::new(group, probe_policy(1)).unwrap();

    // Parameter 0 is useful when toggled; parameter 1 is equally harmful.
    // Toggling both together has zero net effect, so a whole-group probe cannot
    // distinguish them. Parameters 2..7 are deliberately objective-neutral.
    let objective = |read: &dyn BooleanPageRead| -> Result<u32, BooleanPageError> {
        let mut loss = 500_000_i64;
        if read.bit(0)? {
            loss -= 100_000;
        }
        if read.bit(1)? {
            loss += 100_000;
        }
        Ok(loss as u32)
    };

    let execution = evaluate_overlapping_plan(&page, &plan, objective).unwrap();
    assert_eq!(execution.baseline_loss_ppm, 500_000);
    assert_eq!(execution.observations.len(), plan.probe_count());

    let scored = plan.score(&execution.observations).unwrap();
    assert_eq!(scored.candidates.len(), 1);
    assert_eq!(scored.candidates[0].parameter_index, 0);
    assert!(scored.candidates[0].net_score_ppm > 0);

    // Probe evaluation must not mutate the hard page.
    assert!(!page.bit(0).unwrap());
    assert!(!page.bit(1).unwrap());
    assert_eq!(page.generation(), 0);
}

#[test]
fn holdout_regression_rejects_commit_without_mutating_page() {
    let mut page = PackedBooleanPage::zeros(PageId(1), 16).unwrap();
    let evidence = BooleanCommitEvidence {
        train_baseline_loss_ppm: 500_000,
        train_candidate_loss_ppm: 400_000,
        holdout_baseline_loss_ppm: 500_000,
        holdout_candidate_loss_ppm: 600_000,
    };

    let report = page
        .commit_verified_flips(&[0], 0, evidence, BooleanCommitPolicy::default())
        .unwrap();

    assert_eq!(report.decision, BooleanCommitDecision::RejectHoldout);
    assert_eq!(report.committed_flips, 0);
    assert_eq!(report.generation_before, 0);
    assert_eq!(report.generation_after, 0);
    assert!(!page.bit(0).unwrap());
}

#[test]
fn verified_commit_toggles_bits_atomically_and_advances_generation() {
    let mut page = PackedBooleanPage::zeros(PageId(2), 16).unwrap();
    let evidence = BooleanCommitEvidence {
        train_baseline_loss_ppm: 500_000,
        train_candidate_loss_ppm: 400_000,
        holdout_baseline_loss_ppm: 500_000,
        holdout_candidate_loss_ppm: 400_000,
    };

    let report = page
        .commit_verified_flips(&[0, 5], 0, evidence, BooleanCommitPolicy::default())
        .unwrap();

    assert_eq!(report.decision, BooleanCommitDecision::Commit);
    assert_eq!(report.committed_flips, 2);
    assert_eq!(page.generation(), 1);
    assert!(page.bit(0).unwrap());
    assert!(page.bit(5).unwrap());
    assert!(!page.bit(1).unwrap());
}

#[test]
fn stale_or_duplicate_commits_fail_closed() {
    let mut page = PackedBooleanPage::zeros(PageId(3), 8).unwrap();
    let evidence = BooleanCommitEvidence {
        train_baseline_loss_ppm: 500_000,
        train_candidate_loss_ppm: 400_000,
        holdout_baseline_loss_ppm: 500_000,
        holdout_candidate_loss_ppm: 400_000,
    };

    page.commit_verified_flips(&[0], 0, evidence, BooleanCommitPolicy::default())
        .unwrap();

    assert_eq!(
        page.commit_verified_flips(&[1], 0, evidence, BooleanCommitPolicy::default())
            .unwrap_err(),
        BooleanPageError::StaleGeneration {
            expected: 1,
            actual: 0,
        }
    );

    assert_eq!(
        page.commit_verified_flips(&[1, 1], 1, evidence, BooleanCommitPolicy::default())
            .unwrap_err(),
        BooleanPageError::DuplicateCommitIndex(1)
    );
    assert!(!page.bit(1).unwrap());
}

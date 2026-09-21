use sml_core::{
    boolean_hierarchical::{
        BooleanCommitDecision, BooleanCommitPolicy, BooleanGroupObservation, BooleanPageGroup,
        HierarchicalBooleanPolicy, HierarchicalBooleanSearch,
    },
    boolean_learning::{BooleanLearningOutcome, BooleanLearningPolicy, BooleanLearningTransaction},
    boolean_overlapping::OverlappingProbePolicy,
    boolean_page::{BooleanPageError, BooleanPageRead, PackedBooleanPage},
    PageId,
};

fn transaction_policy(rescue_groups_per_round: usize) -> BooleanLearningPolicy {
    BooleanLearningPolicy {
        hierarchical: HierarchicalBooleanPolicy {
            root_group_width: 8,
            leaf_width: 8,
            min_improvement_ppm: 1,
            max_next_groups: 32,
            max_depth: 8,
        },
        rescue_groups_per_round,
        overlapping: OverlappingProbePolicy {
            rounds: 4,
            max_group_width: 8,
            min_confidence_ppm: 250_000,
            min_net_score_ppm: 1,
            max_candidates: 1,
            seed: 1,
        },
        commit: BooleanCommitPolicy::default(),
        max_hierarchy_rounds: 8,
        max_candidate_flips: 1,
    }
}

fn cancelled_train_objective(read: &dyn BooleanPageRead) -> Result<u32, BooleanPageError> {
    let mut loss = 500_000_i64;
    if read.bit(0)? {
        loss -= 100_000;
    }
    if read.bit(1)? {
        loss += 100_000;
    }
    Ok(loss as u32)
}

fn adverse_holdout_objective(read: &dyn BooleanPageRead) -> Result<u32, BooleanPageError> {
    let mut loss = 500_000_i64;
    if read.bit(0)? {
        loss += 100_000;
    }
    Ok(loss as u32)
}

#[test]
fn rescue_selects_neutral_group_before_more_negative_group() {
    let policy = HierarchicalBooleanPolicy {
        root_group_width: 8,
        leaf_width: 8,
        min_improvement_ppm: 1,
        max_next_groups: 16,
        max_depth: 8,
    };
    let neutral = BooleanPageGroup::new(PageId(0), 0, 8, 0).unwrap();
    let negative = BooleanPageGroup::new(PageId(0), 8, 8, 0).unwrap();

    let round = HierarchicalBooleanSearch::refine_with_rescue(
        &[
            BooleanGroupObservation {
                group: neutral,
                baseline_loss_ppm: 500_000,
                perturbed_loss_ppm: 500_000,
            },
            BooleanGroupObservation {
                group: negative,
                baseline_loss_ppm: 500_000,
                perturbed_loss_ppm: 550_000,
            },
        ],
        policy,
        1,
    )
    .unwrap();

    assert_eq!(round.rejected_groups, 2);
    assert_eq!(round.rescued_groups, 1);
    assert_eq!(round.leaf_candidates, vec![neutral]);
}

#[test]
fn end_to_end_transaction_recovers_signal_hidden_by_group_cancellation() {
    let mut page = PackedBooleanPage::zeros(PageId(1), 8).unwrap();

    let report = BooleanLearningTransaction::train_page(
        &mut page,
        transaction_policy(1),
        cancelled_train_objective,
        cancelled_train_objective,
    )
    .unwrap();

    assert_eq!(report.train_baseline_loss_ppm, 500_000);
    assert_eq!(report.train_candidate_loss_ppm, Some(400_000));
    assert_eq!(report.holdout_candidate_loss_ppm, Some(400_000));
    assert_eq!(report.rescued_groups, 1);
    assert!(report.probe_evaluations > 0);
    assert_eq!(report.proposed_flips, vec![0]);
    assert_eq!(report.committed_flips, 1);
    assert_eq!(
        report.outcome,
        BooleanLearningOutcome::CommitGate(BooleanCommitDecision::Commit)
    );
    assert_eq!(page.generation(), 1);
    assert!(page.bit(0).unwrap());
    assert!(!page.bit(1).unwrap());
}

#[test]
fn disabling_rescue_preserves_fail_closed_no_candidate_behavior() {
    let mut page = PackedBooleanPage::zeros(PageId(2), 8).unwrap();

    let report = BooleanLearningTransaction::train_page(
        &mut page,
        transaction_policy(0),
        cancelled_train_objective,
        cancelled_train_objective,
    )
    .unwrap();

    assert_eq!(report.outcome, BooleanLearningOutcome::NoCandidate);
    assert_eq!(report.committed_flips, 0);
    assert!(report.proposed_flips.is_empty());
    assert_eq!(page.generation(), 0);
    assert!(!page.bit(0).unwrap());
}

#[test]
fn holdout_gate_rejects_candidate_discovered_by_boolean_search() {
    let mut page = PackedBooleanPage::zeros(PageId(3), 8).unwrap();

    let report = BooleanLearningTransaction::train_page(
        &mut page,
        transaction_policy(1),
        cancelled_train_objective,
        adverse_holdout_objective,
    )
    .unwrap();

    assert_eq!(report.proposed_flips, vec![0]);
    assert_eq!(report.train_candidate_loss_ppm, Some(400_000));
    assert_eq!(report.holdout_baseline_loss_ppm, 500_000);
    assert_eq!(report.holdout_candidate_loss_ppm, Some(600_000));
    assert_eq!(
        report.outcome,
        BooleanLearningOutcome::CommitGate(BooleanCommitDecision::RejectHoldout)
    );
    assert_eq!(report.committed_flips, 0);
    assert_eq!(page.generation(), 0);
    assert!(!page.bit(0).unwrap());
}

#[test]
fn complete_transaction_is_deterministic() {
    let mut first_page = PackedBooleanPage::zeros(PageId(4), 8).unwrap();
    let mut second_page = PackedBooleanPage::zeros(PageId(4), 8).unwrap();

    let first = BooleanLearningTransaction::train_page(
        &mut first_page,
        transaction_policy(1),
        cancelled_train_objective,
        cancelled_train_objective,
    )
    .unwrap();
    let second = BooleanLearningTransaction::train_page(
        &mut second_page,
        transaction_policy(1),
        cancelled_train_objective,
        cancelled_train_objective,
    )
    .unwrap();

    assert_eq!(first, second);
    for index in 0..8 {
        assert_eq!(
            first_page.bit(index).unwrap(),
            second_page.bit(index).unwrap()
        );
    }
}

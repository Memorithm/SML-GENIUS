use sml_core::{
    boolean_hierarchical::{
        BooleanCommitDecision, BooleanCommitEvidence, BooleanCommitPolicy, BooleanGroupObservation,
        BooleanPageCreditSummary, BooleanPageGroup, HierarchicalBooleanPolicy,
        HierarchicalBooleanSearch,
    },
    elastic_weights::{
        ElasticWeightPage, ElasticWeightPlanner, ElasticWeightPolicy, ResidencyTier,
        WeightPrecision,
    },
    PageId,
};

fn policy() -> HierarchicalBooleanPolicy {
    HierarchicalBooleanPolicy {
        root_group_width: 1_024,
        leaf_width: 128,
        min_improvement_ppm: 1,
        max_next_groups: 128,
        max_depth: 16,
    }
}

#[test]
fn active_page_is_partitioned_without_materializing_parameter_indexes() {
    let groups = HierarchicalBooleanSearch::initial_groups(PageId(7), 10_000, policy()).unwrap();

    assert_eq!(groups.len(), 10);
    assert_eq!(groups[0], BooleanPageGroup::new(PageId(7), 0, 1_024, 0).unwrap());
    assert_eq!(
        *groups.last().unwrap(),
        BooleanPageGroup::new(PageId(7), 9_216, 784, 0).unwrap()
    );
    assert_eq!(
        groups.iter().map(|group| group.len).sum::<u64>(),
        10_000
    );
}

#[test]
fn favorable_groups_refine_and_unfavorable_groups_are_pruned() {
    let favorable = BooleanPageGroup::new(PageId(2), 0, 1_024, 0).unwrap();
    let rejected = BooleanPageGroup::new(PageId(2), 1_024, 1_024, 0).unwrap();

    let round = HierarchicalBooleanSearch::refine(
        &[
            BooleanGroupObservation {
                group: favorable,
                baseline_loss_ppm: 600_000,
                perturbed_loss_ppm: 500_000,
            },
            BooleanGroupObservation {
                group: rejected,
                baseline_loss_ppm: 600_000,
                perturbed_loss_ppm: 650_000,
            },
        ],
        policy(),
    )
    .unwrap();

    assert_eq!(round.favorable_groups, 1);
    assert_eq!(round.rejected_groups, 1);
    assert_eq!(round.next_groups.len(), 2);
    assert_eq!(round.next_groups[0].start, 0);
    assert_eq!(round.next_groups[0].len, 512);
    assert_eq!(round.next_groups[1].start, 512);
    assert_eq!(round.next_groups[1].len, 512);
    assert!(round.leaf_candidates.is_empty());
}

#[test]
fn small_favorable_group_becomes_leaf_candidate() {
    let leaf = BooleanPageGroup::new(PageId(3), 256, 128, 3).unwrap();

    let round = HierarchicalBooleanSearch::refine(
        &[BooleanGroupObservation {
            group: leaf,
            baseline_loss_ppm: 500_000,
            perturbed_loss_ppm: 499_999,
        }],
        policy(),
    )
    .unwrap();

    assert_eq!(round.leaf_candidates, vec![leaf]);
    assert!(round.next_groups.is_empty());
}

#[test]
fn holdout_gate_rejects_train_only_improvement() {
    let evidence = BooleanCommitEvidence {
        train_baseline_loss_ppm: 500_000,
        train_candidate_loss_ppm: 400_000,
        holdout_baseline_loss_ppm: 300_000,
        holdout_candidate_loss_ppm: 310_000,
    };

    assert_eq!(
        evidence.decide(BooleanCommitPolicy::default()).unwrap(),
        BooleanCommitDecision::RejectHoldout
    );

    let verified = BooleanCommitEvidence {
        holdout_candidate_loss_ppm: 300_000,
        ..evidence
    };
    assert_eq!(
        verified.decide(BooleanCommitPolicy::default()).unwrap(),
        BooleanCommitDecision::Commit
    );
}

#[test]
fn page_credit_updates_elastic_importance_without_faking_runtime_access() {
    let pages = vec![ElasticWeightPage::new(
        PageId(0),
        1_024,
        WeightPrecision::Boolean,
        ResidencyTier::Disk,
        100_000,
    )
    .unwrap()];
    let mut planner = ElasticWeightPlanner::new(
        pages,
        ElasticWeightPolicy {
            ram_limit_bytes: 1_024,
            vram_limit_bytes: 1_024,
            max_active_parameters: 1_024,
            ternary_threshold_ppm: 500_000,
            residual4_threshold_ppm: 800_000,
            vram_threshold_ppm: 750_000,
        },
    )
    .unwrap();

    let group = BooleanPageGroup::new(PageId(0), 0, 128, 0).unwrap();
    let mut summary = BooleanPageCreditSummary::default();
    summary
        .observe(BooleanGroupObservation {
            group,
            baseline_loss_ppm: 500_000,
            perturbed_loss_ppm: 200_000,
        })
        .unwrap();
    summary
        .observe(BooleanGroupObservation {
            group: BooleanPageGroup::new(PageId(0), 128, 128, 0).unwrap(),
            baseline_loss_ppm: 500_000,
            perturbed_loss_ppm: 600_000,
        })
        .unwrap();

    assert_eq!(summary.importance_ppm(), 750_000);
    summary
        .record_elastic_importance(&mut planner, PageId(0))
        .unwrap();

    let page = planner.pages()[0];
    assert_eq!(page.importance_ppm, 750_000);
    assert_eq!(page.access_count, 0);
    assert_eq!(page.last_access_epoch, 0);
}

#[test]
fn target_active_scale_stays_compact_at_group_level() {
    let policy = HierarchicalBooleanPolicy::default();
    let groups =
        HierarchicalBooleanSearch::initial_groups(PageId(0), 62_500_000, policy).unwrap();

    assert_eq!(groups.len(), 60);
    assert_eq!(
        groups.iter().map(|group| group.len).sum::<u64>(),
        62_500_000
    );
}

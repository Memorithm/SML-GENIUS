use sml_core::{
    boolean_hierarchical::{BooleanCommitPolicy, HierarchicalBooleanPolicy},
    boolean_learning::BooleanLearningPolicy,
    boolean_overlapping::OverlappingProbePolicy,
    boolean_page::{BooleanPageError, BooleanPageRead, PackedBooleanPage},
    mor::MorDecision,
    mor_boolean_learning::{
        MorBooleanLearning, MorBooleanLearningError, MorBooleanLearningPolicy,
    },
    PageId,
};

fn decision(selected: &[u32]) -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: selected.iter().copied().map(PageId).collect(),
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn page_policy() -> BooleanLearningPolicy {
    BooleanLearningPolicy {
        hierarchical: HierarchicalBooleanPolicy {
            root_group_width: 8,
            leaf_width: 8,
            min_improvement_ppm: 1,
            max_next_groups: 32,
            max_depth: 8,
        },
        rescue_groups_per_round: 1,
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

fn objective(
    page_id: PageId,
    read: &dyn BooleanPageRead,
) -> Result<u32, BooleanPageError> {
    let mut loss = 500_000_i64;
    match page_id.0 {
        0 => {
            if read.bit(0)? {
                loss -= 100_000;
            }
            if read.bit(1)? {
                loss += 100_000;
            }
        }
        2 => {
            if read.bit(2)? {
                loss -= 80_000;
            }
            if read.bit(3)? {
                loss += 80_000;
            }
        }
        _ => {}
    }
    Ok(loss as u32)
}

#[test]
fn only_mor_selected_pages_are_eligible_for_boolean_learning() {
    let mut pages = vec![
        PackedBooleanPage::zeros(PageId(0), 8).unwrap(),
        PackedBooleanPage::zeros(PageId(1), 8).unwrap(),
        PackedBooleanPage::zeros(PageId(2), 8).unwrap(),
    ];

    let report = MorBooleanLearning::train_selected(
        &decision(&[0, 2]),
        &mut pages,
        MorBooleanLearningPolicy {
            page_learning: page_policy(),
            max_pages_per_step: 2,
            max_commits_per_step: 2,
        },
        objective,
        objective,
    )
    .unwrap();

    assert_eq!(report.selected_pages, vec![PageId(0), PageId(2)]);
    assert_eq!(report.trained_pages, vec![PageId(0), PageId(2)]);
    assert_eq!(report.committed_pages, 2);
    assert_eq!(report.committed_flips, 2);
    assert_eq!(pages[0].generation(), 1);
    assert_eq!(pages[2].generation(), 1);
    assert_eq!(pages[1].generation(), 0);
    assert!(!pages[1].bit(0).unwrap());
    assert!(pages[0].bit(0).unwrap());
    assert!(pages[2].bit(2).unwrap());
}

#[test]
fn commit_budget_stops_before_mutating_later_selected_page() {
    let mut pages = vec![
        PackedBooleanPage::zeros(PageId(0), 8).unwrap(),
        PackedBooleanPage::zeros(PageId(1), 8).unwrap(),
        PackedBooleanPage::zeros(PageId(2), 8).unwrap(),
    ];

    let report = MorBooleanLearning::train_selected(
        &decision(&[0, 2]),
        &mut pages,
        MorBooleanLearningPolicy {
            page_learning: page_policy(),
            max_pages_per_step: 2,
            max_commits_per_step: 1,
        },
        objective,
        objective,
    )
    .unwrap();

    assert_eq!(report.trained_pages, vec![PageId(0)]);
    assert_eq!(report.committed_pages, 1);
    assert_eq!(report.skipped_selected_pages, 1);
    assert_eq!(pages[0].generation(), 1);
    assert_eq!(pages[2].generation(), 0);
    assert!(!pages[2].bit(2).unwrap());
}

#[test]
fn missing_selected_page_fails_before_any_page_mutation() {
    let mut pages = vec![PackedBooleanPage::zeros(PageId(0), 8).unwrap()];

    let error = MorBooleanLearning::train_selected(
        &decision(&[0, 9]),
        &mut pages,
        MorBooleanLearningPolicy {
            page_learning: page_policy(),
            max_pages_per_step: 2,
            max_commits_per_step: 2,
        },
        objective,
        objective,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        MorBooleanLearningError::MissingSelectedPage(PageId(9))
    ));
    assert_eq!(pages[0].generation(), 0);
    assert!(!pages[0].bit(0).unwrap());
}

#[test]
fn page_budget_preserves_mor_order_and_skips_suffix() {
    let mut pages = vec![
        PackedBooleanPage::zeros(PageId(0), 8).unwrap(),
        PackedBooleanPage::zeros(PageId(2), 8).unwrap(),
    ];

    let report = MorBooleanLearning::train_selected(
        &decision(&[2, 0]),
        &mut pages,
        MorBooleanLearningPolicy {
            page_learning: page_policy(),
            max_pages_per_step: 1,
            max_commits_per_step: 1,
        },
        objective,
        objective,
    )
    .unwrap();

    assert_eq!(report.trained_pages, vec![PageId(2)]);
    assert_eq!(report.skipped_selected_pages, 1);
    assert_eq!(pages[1].generation(), 1);
    assert_eq!(pages[0].generation(), 0);
}

#[test]
fn duplicate_inventory_fails_closed_before_training() {
    let mut pages = vec![
        PackedBooleanPage::zeros(PageId(0), 8).unwrap(),
        PackedBooleanPage::zeros(PageId(0), 8).unwrap(),
    ];

    let error = MorBooleanLearning::train_selected(
        &decision(&[0]),
        &mut pages,
        MorBooleanLearningPolicy {
            page_learning: page_policy(),
            max_pages_per_step: 1,
            max_commits_per_step: 1,
        },
        objective,
        objective,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        MorBooleanLearningError::DuplicatePage(PageId(0))
    ));
    assert_eq!(pages[0].generation(), 0);
    assert_eq!(pages[1].generation(), 0);
}

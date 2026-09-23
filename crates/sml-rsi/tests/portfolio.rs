use sml_core::tasks::TaskKind;
use sml_rsi::{
    curriculum::{build_verified_curriculum, CurriculumPolicy, CurriculumProposal},
    portfolio::{
        run_curriculum_portfolio, CurriculumCandidate, CurriculumSource, PortfolioError,
        PortfolioPolicy,
    },
};

fn proposal(
    id: &str,
    family: TaskKind,
    start: u64,
    count: u32,
    difficulty: u16,
) -> CurriculumProposal {
    CurriculumProposal {
        proposal_id: id.into(),
        family,
        case_id_start: start,
        case_count: count,
        difficulty,
    }
}

fn curriculum(difficulty: u16, count: u32) -> sml_rsi::curriculum::CurriculumReport {
    build_verified_curriculum(
        &[
            proposal(
                "recall",
                TaskKind::DelayedBitRecall,
                1_000_000,
                count,
                difficulty,
            ),
            proposal(
                "overwrite",
                TaskKind::OverwriteBitRecall,
                1_100_000,
                count,
                difficulty,
            ),
            proposal(
                "copy",
                TaskKind::CopyAfterMarker,
                1_200_000,
                count,
                difficulty,
            ),
            proposal(
                "and",
                TaskKind::TwoFactConjunction,
                1_300_000,
                count,
                difficulty,
            ),
            proposal(
                "keyed",
                TaskKind::KeyedFactRecall,
                1_400_000,
                count,
                difficulty,
            ),
            proposal(
                "distractor",
                TaskKind::DistractorRejection,
                1_500_000,
                count,
                difficulty,
            ),
        ],
        CurriculumPolicy::default(),
    )
    .unwrap()
}

fn candidate(id: &str, source: CurriculumSource, difficulty: u16) -> CurriculumCandidate {
    CurriculumCandidate {
        candidate_id: id.into(),
        source,
        curriculum: curriculum(difficulty, 32),
    }
}

#[test]
fn equal_quality_portfolio_selects_lower_training_event_curriculum() {
    let fixed = candidate("fixed-control", CurriculumSource::FixedControl, 7);
    let proposed = candidate("model-proposed", CurriculumSource::ModelProposed, 3);

    let report = run_curriculum_portfolio(&[fixed, proposed], PortfolioPolicy::default()).unwrap();

    assert_eq!(report.entries.len(), 2);
    assert_eq!(report.selected_id, "model-proposed");
    assert_eq!(report.selected().source, CurriculumSource::ModelProposed);
    assert_eq!(
        report.entries[0].selection.exact_tasks,
        report.entries[1].selection.exact_tasks
    );
    assert!(report.entries[1].training_events < report.entries[0].training_events);
    assert_eq!(
        report.selected().selection.exact_tasks,
        report.selection_tasks
    );
}

#[test]
fn portfolio_uses_same_training_shape_for_fixed_and_proposed_candidates() {
    let report = run_curriculum_portfolio(
        &[
            candidate("fixed-control", CurriculumSource::FixedControl, 7),
            candidate("model-proposed", CurriculumSource::ModelProposed, 3),
        ],
        PortfolioPolicy::default(),
    )
    .unwrap();

    assert_eq!(
        report.entries[0].training_shape,
        report.entries[1].training_shape
    );
    assert_eq!(
        report.entries[0].training.training_evaluation.tasks,
        report.entries[1].training.training_evaluation.tasks
    );
}

#[test]
fn portfolio_requires_both_fixed_control_and_model_proposed_source_classes() {
    assert_eq!(
        run_curriculum_portfolio(
            &[
                candidate("fixed-a", CurriculumSource::FixedControl, 7),
                candidate("fixed-b", CurriculumSource::FixedControl, 3),
            ],
            PortfolioPolicy::default(),
        ),
        Err(PortfolioError::MissingSourceClass)
    );
}

#[test]
fn unmatched_training_shape_is_rejected_before_candidate_selection() {
    let fixed = candidate("fixed-control", CurriculumSource::FixedControl, 7);
    let proposed = CurriculumCandidate {
        candidate_id: "model-proposed".into(),
        source: CurriculumSource::ModelProposed,
        curriculum: curriculum(3, 31),
    };

    assert_eq!(
        run_curriculum_portfolio(&[fixed, proposed], PortfolioPolicy::default()),
        Err(PortfolioError::UnmatchedTrainingShape)
    );
}

#[test]
fn shared_selection_panel_must_be_disjoint_from_all_curriculum_tasks() {
    let policy = PortfolioPolicy {
        selection_start_case_id: 1_000_000,
        ..PortfolioPolicy::default()
    };
    assert!(matches!(
        run_curriculum_portfolio(
            &[
                candidate("fixed-control", CurriculumSource::FixedControl, 7),
                candidate("model-proposed", CurriculumSource::ModelProposed, 3),
            ],
            policy,
        ),
        Err(PortfolioError::SelectionOverlap(_))
    ));
}

#[test]
fn duplicate_candidate_ids_fail_closed() {
    assert_eq!(
        run_curriculum_portfolio(
            &[
                candidate("same", CurriculumSource::FixedControl, 7),
                candidate("same", CurriculumSource::ModelProposed, 3),
            ],
            PortfolioPolicy::default(),
        ),
        Err(PortfolioError::DuplicateCandidateId)
    );
}

#[test]
fn candidate_budget_is_enforced_before_training() {
    let policy = PortfolioPolicy {
        max_candidates: 2,
        ..PortfolioPolicy::default()
    };
    assert_eq!(
        run_curriculum_portfolio(
            &[
                candidate("fixed", CurriculumSource::FixedControl, 7),
                candidate("proposed-a", CurriculumSource::ModelProposed, 3),
                candidate("proposed-b", CurriculumSource::ModelProposed, 5),
            ],
            policy,
        ),
        Err(PortfolioError::CandidateBudget)
    );
}

#[test]
fn tampered_curriculum_fails_before_portfolio_training() {
    let fixed = candidate("fixed-control", CurriculumSource::FixedControl, 7);
    let mut proposed = candidate("model-proposed", CurriculumSource::ModelProposed, 3);
    proposed.curriculum.total_events = proposed.curriculum.total_events.saturating_add(1);

    assert!(matches!(
        run_curriculum_portfolio(&[fixed, proposed], PortfolioPolicy::default()),
        Err(PortfolioError::Curriculum(_))
    ));
}

#[test]
fn deterministic_candidate_id_breaks_complete_metric_and_cost_ties() {
    let report = run_curriculum_portfolio(
        &[
            candidate("fixed-z", CurriculumSource::FixedControl, 3),
            candidate("proposed-b", CurriculumSource::ModelProposed, 3),
            candidate("proposed-a", CurriculumSource::ModelProposed, 3),
        ],
        PortfolioPolicy::default(),
    )
    .unwrap();

    assert_eq!(report.selected_id, "fixed-z");
}

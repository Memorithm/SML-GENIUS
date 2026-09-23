use sml_core::tasks::TaskKind;
use sml_rsi::curriculum::{
    build_verified_curriculum, CurriculumError, CurriculumPartition, CurriculumPolicy,
    CurriculumProposal,
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

#[test]
fn all_native_task_families_are_recomputed_by_exact_oracle() {
    let base = 1_000_000;
    let proposals = [
        proposal("delay", TaskKind::DelayedBitRecall, base, 5, 7),
        proposal("overwrite", TaskKind::OverwriteBitRecall, base + 100, 5, 7),
        proposal("keyed", TaskKind::KeyedFactRecall, base + 200, 5, 7),
        proposal("copy", TaskKind::CopyAfterMarker, base + 300, 5, 7),
        proposal("and", TaskKind::TwoFactConjunction, base + 400, 5, 7),
        proposal(
            "distractor",
            TaskKind::DistractorRejection,
            base + 500,
            5,
            7,
        ),
    ];
    let report = build_verified_curriculum(&proposals, CurriculumPolicy::default()).unwrap();

    assert_eq!(report.proposal_count, 6);
    assert_eq!(report.verified_tasks, 30);
    assert_eq!(report.training_tasks + report.development_tasks, 30);
    assert!(report.total_events > 30);
    assert!(report
        .tasks
        .iter()
        .all(|item| item.task.oracle_matches().unwrap()));
}

#[test]
fn partition_is_host_deterministic_and_exposes_no_protected_variant() {
    let proposals = [proposal(
        "cases",
        TaskKind::DelayedBitRecall,
        1_000_000,
        64,
        3,
    )];
    let left = build_verified_curriculum(&proposals, CurriculumPolicy::default()).unwrap();
    let right = build_verified_curriculum(&proposals, CurriculumPolicy::default()).unwrap();

    let lp = left
        .tasks
        .iter()
        .map(|task| task.partition)
        .collect::<Vec<_>>();
    let rp = right
        .tasks
        .iter()
        .map(|task| task.partition)
        .collect::<Vec<_>>();
    assert_eq!(lp, rp);
    assert!(lp.contains(&CurriculumPartition::Training));
    assert!(lp.contains(&CurriculumPartition::Development));
}

#[test]
fn low_case_ids_reserved_for_existing_development_fixtures_are_rejected() {
    let proposals = [proposal(
        "collision-risk",
        TaskKind::DelayedBitRecall,
        999_999,
        1,
        1,
    )];
    assert_eq!(
        build_verified_curriculum(&proposals, CurriculumPolicy::default()),
        Err(CurriculumError::ReservedCaseRange)
    );
}

#[test]
fn duplicate_task_identity_across_proposals_fails_closed() {
    let proposals = [
        proposal("a", TaskKind::KeyedFactRecall, 1_000_100, 2, 2),
        proposal("b", TaskKind::KeyedFactRecall, 1_000_101, 2, 2),
    ];
    assert_eq!(
        build_verified_curriculum(&proposals, CurriculumPolicy::default()),
        Err(CurriculumError::DuplicateTask)
    );
}

#[test]
fn same_case_id_in_different_families_is_not_a_duplicate() {
    let proposals = [
        proposal("a", TaskKind::KeyedFactRecall, 1_000_100, 1, 2),
        proposal("b", TaskKind::TwoFactConjunction, 1_000_100, 1, 2),
    ];
    let report = build_verified_curriculum(&proposals, CurriculumPolicy::default()).unwrap();
    assert_eq!(report.verified_tasks, 2);
}

#[test]
fn duplicate_proposal_ids_fail_closed() {
    let proposals = [
        proposal("same", TaskKind::KeyedFactRecall, 1_000_100, 1, 2),
        proposal("same", TaskKind::TwoFactConjunction, 1_000_200, 1, 2),
    ];
    assert_eq!(
        build_verified_curriculum(&proposals, CurriculumPolicy::default()),
        Err(CurriculumError::DuplicateProposalId)
    );
}

#[test]
fn proposal_case_and_event_budgets_are_enforced() {
    let too_many = [proposal(
        "many",
        TaskKind::DelayedBitRecall,
        1_000_000,
        3,
        10,
    )];
    assert_eq!(
        build_verified_curriculum(
            &too_many,
            CurriculumPolicy {
                max_cases_per_proposal: 2,
                ..CurriculumPolicy::default()
            }
        ),
        Err(CurriculumError::CaseBudget)
    );

    assert_eq!(
        build_verified_curriculum(
            &too_many,
            CurriculumPolicy {
                max_total_events: 2,
                ..CurriculumPolicy::default()
            }
        ),
        Err(CurriculumError::EventBudget)
    );
}

#[test]
fn difficulty_is_bounded_before_task_allocation() {
    for difficulty in [0, 16_384] {
        let proposals = [proposal(
            "bad-difficulty",
            TaskKind::DistractorRejection,
            1_000_000,
            1,
            difficulty,
        )];
        assert_eq!(
            build_verified_curriculum(&proposals, CurriculumPolicy::default()),
            Err(CurriculumError::InvalidDifficulty)
        );
    }
}

#[test]
fn copy_payload_uses_declared_bounded_difficulty() {
    let proposals = [proposal(
        "copy",
        TaskKind::CopyAfterMarker,
        1_000_000,
        1,
        4096,
    )];
    let report = build_verified_curriculum(&proposals, CurriculumPolicy::default()).unwrap();
    assert_eq!(report.tasks[0].task.events.len(), 4099);
    assert!(report.tasks[0].task.oracle_matches().unwrap());
}

#[test]
fn empty_and_overlarge_proposal_sets_are_rejected() {
    assert_eq!(
        build_verified_curriculum(&[], CurriculumPolicy::default()),
        Err(CurriculumError::EmptyProposals)
    );

    let proposals = vec![
        proposal("a", TaskKind::DelayedBitRecall, 1_000_000, 1, 1),
        proposal("b", TaskKind::DelayedBitRecall, 1_000_100, 1, 1),
    ];
    assert_eq!(
        build_verified_curriculum(
            &proposals,
            CurriculumPolicy {
                max_proposals: 1,
                ..CurriculumPolicy::default()
            }
        ),
        Err(CurriculumError::ProposalBudget)
    );
}

use sml_core::{
    tasks::{TaskAnswer, TaskKind},
    train::{TruthTable1, TruthTable2},
};
use sml_rsi::{
    curriculum::{build_verified_curriculum, CurriculumPolicy, CurriculumProposal},
    multi_task_training::{
        train_multitask_candidate, MultiTaskTrainingError, MultiTaskTrainingPolicy, Operation,
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

fn full_curriculum() -> sml_rsi::curriculum::CurriculumReport {
    build_verified_curriculum(
        &[
            proposal("recall", TaskKind::DelayedBitRecall, 1_000_000, 64, 7),
            proposal("overwrite", TaskKind::OverwriteBitRecall, 1_100_000, 64, 7),
            proposal("copy", TaskKind::CopyAfterMarker, 1_200_000, 64, 8),
            proposal("and", TaskKind::TwoFactConjunction, 1_300_000, 64, 7),
            proposal("keyed", TaskKind::KeyedFactRecall, 1_400_000, 32, 7),
            proposal(
                "distractor",
                TaskKind::DistractorRejection,
                1_500_000,
                32,
                7,
            ),
        ],
        CurriculumPolicy::default(),
    )
    .unwrap()
}

#[test]
fn multitask_curriculum_learns_all_four_current_micro_operations() {
    let report =
        train_multitask_candidate(&full_curriculum(), MultiTaskTrainingPolicy::default()).unwrap();

    assert_eq!(report.conjunction_table, TruthTable2::AND);
    assert_eq!(report.overwrite_table, TruthTable2::RIGHT);
    assert_eq!(report.recall_table, TruthTable1::IDENTITY);
    assert_eq!(report.copy_table, TruthTable1::IDENTITY);
    assert_eq!(report.coverage.conjunction, 0b1111);
    assert_eq!(report.coverage.overwrite, 0b1111);
    assert_eq!(report.coverage.recall, 0b11);
    assert_eq!(report.coverage.copy, 0b11);
    assert_eq!(report.learned_boolean_bits, 12);
    assert_eq!(report.latent_training_scalars, 40);
}

#[test]
fn frozen_candidate_is_exact_on_training_and_development_curriculum_panels() {
    let curriculum = full_curriculum();
    let report =
        train_multitask_candidate(&curriculum, MultiTaskTrainingPolicy::default()).unwrap();

    assert_eq!(
        report.training_evaluation.exact_tasks,
        report.training_evaluation.tasks
    );
    assert_eq!(
        report.development_evaluation.exact_tasks,
        report.development_evaluation.tasks
    );
    assert_eq!(
        report.training_evaluation.tasks + report.development_evaluation.tasks,
        curriculum.verified_tasks
    );
}

#[test]
fn keyed_and_distractor_tasks_are_evaluated_without_being_claimed_as_learned_micro_ops() {
    let curriculum = full_curriculum();
    let training_keyed = curriculum
        .tasks
        .iter()
        .filter(|item| {
            item.partition == sml_rsi::curriculum::CurriculumPartition::Training
                && matches!(
                    item.task.kind,
                    TaskKind::KeyedFactRecall | TaskKind::DistractorRejection
                )
        })
        .count() as u64;

    let report =
        train_multitask_candidate(&curriculum, MultiTaskTrainingPolicy::default()).unwrap();

    let learned_task_instances = report.samples.conjunction_tasks
        + report.samples.overwrite_tasks
        + report.samples.recall_tasks
        + report.samples.copy_tasks;
    assert_eq!(
        report.training_evaluation.tasks - learned_task_instances,
        training_keyed
    );
}

#[test]
fn copy_learning_counts_bit_samples_separately_from_copy_task_instances() {
    let report =
        train_multitask_candidate(&full_curriculum(), MultiTaskTrainingPolicy::default()).unwrap();

    assert!(report.samples.copy_tasks >= 16);
    assert!(report.samples.copy_bit_samples > report.samples.copy_tasks);
    assert_eq!(
        report.copy_gate.sample_count,
        report.samples.copy_bit_samples
    );
}

#[test]
fn missing_training_family_fails_closed_before_candidate_freeze() {
    let curriculum = build_verified_curriculum(
        &[
            proposal("recall", TaskKind::DelayedBitRecall, 1_000_000, 64, 7),
            proposal("overwrite", TaskKind::OverwriteBitRecall, 1_100_000, 64, 7),
            proposal("copy", TaskKind::CopyAfterMarker, 1_200_000, 64, 8),
        ],
        CurriculumPolicy::default(),
    )
    .unwrap();

    assert_eq!(
        train_multitask_candidate(&curriculum, MultiTaskTrainingPolicy::default()),
        Err(MultiTaskTrainingError::InsufficientSamples {
            operation: Operation::Conjunction,
            actual: 0,
            minimum: 16,
        })
    );
}

#[test]
fn tampered_task_label_is_rejected_by_shared_curriculum_validation() {
    let mut curriculum = full_curriculum();
    curriculum.tasks[0].task.expected = match curriculum.tasks[0].task.expected {
        TaskAnswer::Bit(value) => TaskAnswer::Bit(!value),
        TaskAnswer::Bits(_) => unreachable!(),
    };

    assert!(matches!(
        train_multitask_candidate(&curriculum, MultiTaskTrainingPolicy::default()),
        Err(MultiTaskTrainingError::Curriculum(_))
    ));
}

#[test]
fn tampered_partition_counters_are_rejected_by_shared_curriculum_validation() {
    let mut curriculum = full_curriculum();
    curriculum.training_tasks = curriculum.training_tasks.saturating_add(1);

    assert!(matches!(
        train_multitask_candidate(&curriculum, MultiTaskTrainingPolicy::default()),
        Err(MultiTaskTrainingError::Curriculum(_))
    ));
}

#[test]
fn minimum_per_operation_is_enforced_as_a_task_count_not_copy_bit_count() {
    let curriculum = build_verified_curriculum(
        &[
            proposal("recall", TaskKind::DelayedBitRecall, 1_000_000, 8, 7),
            proposal("overwrite", TaskKind::OverwriteBitRecall, 1_100_000, 8, 7),
            proposal("copy", TaskKind::CopyAfterMarker, 1_200_000, 8, 64),
            proposal("and", TaskKind::TwoFactConjunction, 1_300_000, 8, 7),
        ],
        CurriculumPolicy::default(),
    )
    .unwrap();
    let policy = MultiTaskTrainingPolicy {
        min_task_instances_per_operation: 16,
        ..MultiTaskTrainingPolicy::default()
    };

    assert!(matches!(
        train_multitask_candidate(&curriculum, policy),
        Err(MultiTaskTrainingError::InsufficientSamples { .. })
    ));
}

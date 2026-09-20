use sml_core::{
    candidate::{SmlTaskCandidate, TaskCandidateConfig, TaskCandidateError},
    tasks::{
        delayed_bit_recall, development_suite, suite_from_case_id, TaskAnswer, TaskEvent,
        TaskInstance, TaskKind,
    },
    PageId,
};

#[test]
fn candidate_trains_nontrivial_event_router() {
    let (candidate, report) = SmlTaskCandidate::train(TaskCandidateConfig::default()).unwrap();

    assert_eq!(report.route.hard_correct, 16);
    assert_eq!(report.route.address_count, 16);
    assert!(report.route.final_loss < report.route.initial_loss);
    assert_eq!(report.route_accounting.entry_count, 16);
    assert_eq!(report.route_accounting.page_count, 10);
    assert_eq!(report.route_accounting.bits_per_route, 4);
    assert_eq!(report.route_accounting.learned_boolean_bits, 64);
    assert_eq!(report.route_accounting.latent_training_scalars, 64);

    let evidence = candidate.execute(&delayed_bit_recall(10_000, 0)).unwrap();
    let pages: Vec<PageId> = evidence
        .trace
        .iter()
        .map(|record| record.selected_page)
        .collect();
    assert_eq!(pages, vec![PageId(5), PageId(4)]);
    assert!(evidence.exact);
}

#[test]
fn holdout_task_panel_executes_exactly_with_full_event_traces() {
    let (candidate, _) = SmlTaskCandidate::train(TaskCandidateConfig::default()).unwrap();
    let holdout = suite_from_case_id(10_000, 64).unwrap();

    assert_eq!(holdout.len(), 320);
    for instance in &holdout {
        let evidence = candidate.execute(instance).unwrap();
        assert!(evidence.exact, "{:?} case {}", instance.kind, instance.case_id);
        assert_eq!(evidence.predicted, instance.expected);
        assert_eq!(evidence.trace.len(), instance.events.len());
        assert_eq!(evidence.counters.route_lookups, instance.events.len() as u64);
        assert_eq!(
            evidence.semantic_route_bits_touched,
            instance.events.len() as u64 * 4
        );
        assert_eq!(evidence.counters.pairwise_comparisons, 0);
        assert_eq!(evidence.counters.attention_score_ops, 0);
        assert_eq!(
            evidence
                .trace
                .last()
                .and_then(|record| record.emitted_answer.clone()),
            Some(instance.expected.clone())
        );
    }
}

#[test]
fn candidate_training_and_holdout_execution_are_deterministic() {
    let (left, left_report) = SmlTaskCandidate::train(TaskCandidateConfig::default()).unwrap();
    let (right, right_report) = SmlTaskCandidate::train(TaskCandidateConfig::default()).unwrap();
    assert_eq!(left, right);
    assert_eq!(left_report, right_report);

    let instance = &suite_from_case_id(20_000, 1).unwrap()[4];
    assert_eq!(left.execute(instance).unwrap(), right.execute(instance).unwrap());
}

#[test]
fn memory_collision_fails_closed_without_silent_eviction() {
    let config = TaskCandidateConfig {
        memory_capacity: 1,
        ..TaskCandidateConfig::default()
    };
    let (candidate, _) = SmlTaskCandidate::train(config).unwrap();
    let instance = TaskInstance {
        kind: TaskKind::KeyedFactRecall,
        case_id: 99_999,
        events: vec![
            TaskEvent::Fact {
                key: 1,
                value: true,
            },
            TaskEvent::Fact {
                key: 2,
                value: false,
            },
            TaskEvent::QueryKey(1),
        ],
        expected: TaskAnswer::Bit(true),
    };

    assert_eq!(
        candidate.execute(&instance).unwrap_err(),
        TaskCandidateError::MemoryCollision {
            key: 2,
            slot: 0,
            resident_key: 1,
        }
    );
}

#[test]
fn development_and_holdout_case_ranges_are_disjoint_and_reproducible() {
    let development = development_suite(8).unwrap();
    let holdout_a = suite_from_case_id(10_000, 8).unwrap();
    let holdout_b = suite_from_case_id(10_000, 8).unwrap();

    assert_eq!(holdout_a, holdout_b);
    assert!(development.iter().all(|item| item.case_id < 8));
    assert!(holdout_a.iter().all(|item| item.case_id >= 10_000));
    assert!(development.iter().all(|left| {
        holdout_a
            .iter()
            .all(|right| left.case_id != right.case_id)
    }));
}

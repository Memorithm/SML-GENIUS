use sml_core::{
    ablation::run_sml1_ablations,
    tasks::suite_from_case_id,
};

#[test]
fn controlled_ablations_separate_routing_and_memory_effects() {
    let holdout = suite_from_case_id(10_000, 64).unwrap();
    let report = run_sml1_ablations(&holdout, 1).unwrap();

    assert_eq!(report.learned_routing.tasks, 384);
    assert_eq!(report.learned_routing.successful_executions, 384);
    assert_eq!(report.learned_routing.exact_tasks, 384);
    assert_eq!(report.learned_routing.execution_failures, 0);
    assert_eq!(report.learned_routing.learned_route_bits, 64);

    assert_eq!(report.fixed_routing.successful_executions, 384);
    assert_eq!(report.fixed_routing.exact_tasks, 384);
    assert_eq!(report.fixed_routing.execution_failures, 0);
    assert_eq!(report.fixed_routing.learned_route_bits, 0);

    assert_eq!(report.no_associative_memory.execution_failures, 0);
    assert!(report.no_associative_memory.exact_tasks < report.learned_routing.exact_tasks);
    assert!(report.no_associative_memory.exact_tasks >= 192);
    assert_eq!(report.no_associative_memory.memory_reads, 0);
    assert_eq!(report.no_associative_memory.memory_writes, 0);

    assert!(report.constrained_memory.execution_failures > 0);
    assert!(report.constrained_memory.memory_collisions > 0);
    assert!(report.constrained_memory.exact_tasks < report.learned_routing.exact_tasks);
}

#[test]
fn fixed_route_ablation_changes_parameter_accounting_not_predictions() {
    let holdout = suite_from_case_id(20_000, 16).unwrap();
    let report = run_sml1_ablations(&holdout, 1).unwrap();

    assert_eq!(
        report.learned_routing.exact_tasks,
        report.fixed_routing.exact_tasks
    );
    assert_eq!(report.learned_routing.learned_route_bits, 64);
    assert_eq!(report.fixed_routing.learned_route_bits, 0);
}

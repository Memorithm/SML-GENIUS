use sml_core::{
    boolean_hierarchical::HierarchicalBooleanPolicy,
    boolean_overlapping::OverlappingProbePolicy,
    boolean_page::{BooleanPageError, BooleanPageRead, PackedBooleanPage},
    boolean_search_bench::{
        BooleanSearchBenchmark, BooleanSearchBenchmarkPolicy, BooleanSearchStrategy,
    },
    PageId,
};

fn policy() -> BooleanSearchBenchmarkPolicy {
    BooleanSearchBenchmarkPolicy {
        contiguous: HierarchicalBooleanPolicy {
            root_group_width: 256,
            leaf_width: 1,
            min_improvement_ppm: 1,
            max_next_groups: 1_024,
            max_depth: 16,
        },
        overlap_hierarchy: HierarchicalBooleanPolicy {
            root_group_width: 256,
            leaf_width: 256,
            min_improvement_ppm: 1,
            max_next_groups: 1_024,
            max_depth: 16,
        },
        overlapping: OverlappingProbePolicy {
            rounds: 4,
            max_group_width: 256,
            min_confidence_ppm: 250_000,
            min_net_score_ppm: 1,
            max_candidates: 1,
            seed: 1,
        },
        rescue_groups_per_round: 1,
        max_candidates: 1,
        max_hierarchy_rounds: 16,
    }
}

fn cancellation_objective(read: &dyn BooleanPageRead) -> Result<u32, BooleanPageError> {
    let mut loss = 500_000_i64;
    if read.bit(0)? {
        loss -= 100_000;
    }
    if read.bit(1)? {
        loss += 100_000;
    }
    Ok(loss as u32)
}

fn sparse_objective(read: &dyn BooleanPageRead) -> Result<u32, BooleanPageError> {
    let mut loss = 500_000_i64;
    if read.bit(42)? {
        loss -= 100_000;
    }
    Ok(loss as u32)
}

#[test]
fn overlapping_recovers_exact_cancellation_with_far_fewer_evaluations_than_coordinate() {
    let page = PackedBooleanPage::zeros(PageId(0), 256).unwrap();
    let report = BooleanSearchBenchmark::compare(&page, policy(), cancellation_objective).unwrap();

    assert_eq!(report.coordinate.strategy, BooleanSearchStrategy::Coordinate);
    assert_eq!(report.coordinate.objective_evaluations, 257);
    assert_eq!(
        report.coordinate.best_candidate().unwrap().parameter_index,
        0
    );

    assert!(report.contiguous.validated_candidates.is_empty());
    assert_eq!(report.contiguous.objective_evaluations, 2);

    assert_eq!(
        report.overlapping.best_candidate().unwrap().parameter_index,
        0
    );
    assert_eq!(
        report.overlapping.best_candidate().unwrap().candidate_loss_ppm,
        400_000
    );
    assert_eq!(report.overlapping.rescued_groups, 1);
    assert_eq!(report.overlapping.probes_evaluated, 64);
    assert_eq!(report.overlapping.objective_evaluations, 67);
    assert!(
        report.overlapping.objective_evaluations
            < report.coordinate.objective_evaluations
    );
}

#[test]
fn contiguous_hierarchy_is_cheapest_on_simple_sparse_signal() {
    let page = PackedBooleanPage::zeros(PageId(1), 256).unwrap();
    let report = BooleanSearchBenchmark::compare(&page, policy(), sparse_objective).unwrap();

    assert_eq!(
        report.coordinate.best_candidate().unwrap().parameter_index,
        42
    );
    assert_eq!(
        report.contiguous.best_candidate().unwrap().parameter_index,
        42
    );
    assert_eq!(
        report.overlapping.best_candidate().unwrap().parameter_index,
        42
    );

    assert_eq!(report.coordinate.objective_evaluations, 257);
    assert_eq!(report.contiguous.objective_evaluations, 19);
    assert_eq!(report.overlapping.objective_evaluations, 67);
    assert!(
        report.contiguous.objective_evaluations
            < report.overlapping.objective_evaluations
    );
    assert!(
        report.overlapping.objective_evaluations
            < report.coordinate.objective_evaluations
    );
}

#[test]
fn benchmark_is_deterministic() {
    let page = PackedBooleanPage::zeros(PageId(2), 256).unwrap();

    let first = BooleanSearchBenchmark::compare(&page, policy(), cancellation_objective).unwrap();
    let second = BooleanSearchBenchmark::compare(&page, policy(), cancellation_objective).unwrap();

    assert_eq!(first, second);
    assert_eq!(page.generation(), 0);
    assert!(!page.bit(0).unwrap());
}

#[test]
fn benchmark_does_not_mutate_hard_page() {
    let page = PackedBooleanPage::zeros(PageId(3), 256).unwrap();
    let _ = BooleanSearchBenchmark::compare(&page, policy(), sparse_objective).unwrap();

    assert_eq!(page.generation(), 0);
    for index in 0..256 {
        assert!(!page.bit(index).unwrap());
    }
}

use sml_core::{
    boolean_hierarchical::BooleanPageGroup,
    boolean_overlapping::{
        OverlappingProbeObservation, OverlappingProbePlan, OverlappingProbePolicy,
        OverlappingProbeError,
    },
    PageId,
};

fn policy() -> OverlappingProbePolicy {
    OverlappingProbePolicy {
        rounds: 4,
        max_group_width: 256,
        min_confidence_ppm: 250_000,
        min_net_score_ppm: 1,
        max_candidates: 16,
        seed: 1,
    }
}

fn additive_observations(
    plan: &OverlappingProbePlan,
    effects_ppm: &[i64],
    baseline_loss_ppm: u32,
) -> Vec<OverlappingProbeObservation> {
    assert_eq!(effects_ppm.len(), plan.group().len as usize);
    plan.probes()
        .iter()
        .copied()
        .map(|probe| {
            let mut improvement = 0_i64;
            for (local, effect) in effects_ppm.iter().copied().enumerate() {
                let parameter_index = plan.group().start + local as u64;
                if probe.contains_parameter(parameter_index).unwrap() {
                    improvement += effect;
                }
            }
            let perturbed = baseline_loss_ppm as i64 - improvement;
            assert!((0..=1_000_000).contains(&perturbed));
            OverlappingProbeObservation {
                probe,
                baseline_loss_ppm,
                perturbed_loss_ppm: perturbed as u32,
            }
        })
        .collect()
}

#[test]
fn default_leaf_plan_is_compact_and_deterministic() {
    let group = BooleanPageGroup::new(PageId(4), 1_000, 256, 7).unwrap();
    let first = OverlappingProbePlan::new(group, policy()).unwrap();
    let second = OverlappingProbePlan::new(group, policy()).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.dimensions(), 8);
    assert_eq!(first.probe_count(), 64);
    assert_eq!(first.probes().len(), 64);
}

#[test]
fn every_complementary_pair_partitions_the_leaf_exactly() {
    let group = BooleanPageGroup::new(PageId(0), 100, 17, 2).unwrap();
    let plan = OverlappingProbePlan::new(group, policy()).unwrap();

    for pair in plan.probes().chunks_exact(2) {
        assert_eq!(pair[0].round, pair[1].round);
        assert_eq!(pair[0].dimension, pair[1].dimension);
        assert_ne!(pair[0].side, pair[1].side);

        for parameter_index in group.start..group.end_exclusive() {
            let left = pair[0].contains_parameter(parameter_index).unwrap();
            let right = pair[1].contains_parameter(parameter_index).unwrap();
            assert_ne!(left, right);
        }
    }
}

#[test]
fn overlapping_contrasts_recover_signal_hidden_by_whole_group_cancellation() {
    let group = BooleanPageGroup::new(PageId(1), 0, 4, 0).unwrap();
    let mut tuned = policy();
    tuned.max_candidates = 1;
    let plan = OverlappingProbePlan::new(group, tuned).unwrap();

    // Whole-group intervention has zero net effect: +100k and -100k cancel.
    // Overlapping complementary probes should still rank parameter 0 first.
    let effects = [100_000_i64, -100_000, 0, 0];
    assert_eq!(effects.iter().sum::<i64>(), 0);

    let observations = additive_observations(&plan, &effects, 500_000);
    let report = plan.score(&observations).unwrap();

    assert_eq!(report.probe_count, 16);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].parameter_index, 0);
    assert!(report.candidates[0].net_score_ppm > 0);
    assert!(report.candidates[0].confidence_ppm > 0);

    assert!(report.cells[0].net_score_ppm > 0);
    assert!(report.cells[1].net_score_ppm < 0);
}

#[test]
fn score_is_invariant_to_observation_order() {
    let group = BooleanPageGroup::new(PageId(2), 50, 8, 1).unwrap();
    let plan = OverlappingProbePlan::new(group, policy()).unwrap();
    let effects = [50_000_i64, 0, -20_000, 0, 10_000, 0, 0, 0];

    let ordered = additive_observations(&plan, &effects, 500_000);
    let mut reversed = ordered.clone();
    reversed.reverse();

    assert_eq!(plan.score(&ordered).unwrap(), plan.score(&reversed).unwrap());
}

#[test]
fn incomplete_probe_evidence_fails_closed() {
    let group = BooleanPageGroup::new(PageId(3), 0, 16, 0).unwrap();
    let plan = OverlappingProbePlan::new(group, policy()).unwrap();
    let effects = vec![0_i64; 16];
    let mut observations = additive_observations(&plan, &effects, 500_000);
    observations.pop();

    assert_eq!(
        plan.score(&observations).unwrap_err(),
        OverlappingProbeError::ObservationCount {
            expected: plan.probe_count(),
            actual: plan.probe_count() - 1,
        }
    );
}

#[test]
fn mismatched_complementary_baselines_fail_closed() {
    let group = BooleanPageGroup::new(PageId(5), 0, 8, 0).unwrap();
    let plan = OverlappingProbePlan::new(group, policy()).unwrap();
    let effects = vec![0_i64; 8];
    let mut observations = additive_observations(&plan, &effects, 500_000);
    observations[1].baseline_loss_ppm = 499_999;

    assert!(matches!(
        plan.score(&observations).unwrap_err(),
        OverlappingProbeError::BaselineMismatch { .. }
    ));
}

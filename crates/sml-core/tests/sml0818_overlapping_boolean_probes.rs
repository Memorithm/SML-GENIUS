use sml_core::{
    boolean_hierarchical::BooleanPageGroup,
    boolean_overlapping::{
        OverlappingProbeError, OverlappingProbeObservation, OverlappingProbePlan,
        OverlappingProbePolicy,
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

    assert_eq!(
        plan.score(&ordered).unwrap(),
        plan.score(&reversed).unwrap()
    );
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

#[test]
fn policy_validation_rejection_matrix() {
    type Mutate = fn(&mut OverlappingProbePolicy);
    let cases: [(&str, Mutate, OverlappingProbeError); 6] = [
        (
            "zero rounds",
            |p| p.rounds = 0,
            OverlappingProbeError::InvalidRounds(0),
        ),
        (
            "too many rounds",
            |p| p.rounds = 65,
            OverlappingProbeError::InvalidRounds(65),
        ),
        (
            "max width zero",
            |p| p.max_group_width = 0,
            OverlappingProbeError::InvalidMaximumGroupWidth(0),
        ),
        (
            "max width one",
            |p| p.max_group_width = 1,
            OverlappingProbeError::InvalidMaximumGroupWidth(1),
        ),
        (
            "confidence above scale",
            |p| p.min_confidence_ppm = 1_000_001,
            OverlappingProbeError::PpmOutOfRange(1_000_001),
        ),
        (
            "zero candidate budget",
            |p| p.max_candidates = 0,
            OverlappingProbeError::ZeroCandidateBudget,
        ),
    ];
    for (name, mutate, expected) in cases {
        let mut p = policy();
        mutate(&mut p);
        assert_eq!(p.validate(), Err(expected), "{name}");
        let group = BooleanPageGroup::new(PageId(0), 0, 8, 0).unwrap();
        assert_eq!(
            OverlappingProbePlan::new(group, p),
            Err(expected),
            "{name} via plan"
        );
    }
    let mut edge = policy();
    edge.rounds = 64;
    edge.max_group_width = 2;
    edge.min_confidence_ppm = 1_000_000;
    edge.max_candidates = 1;
    assert_eq!(edge.validate(), Ok(edge));
}

#[test]
fn plan_group_width_rejections() {
    let single = BooleanPageGroup::new(PageId(0), 3, 1, 0).unwrap();
    assert_eq!(
        OverlappingProbePlan::new(single, policy()),
        Err(OverlappingProbeError::GroupTooSmall(single))
    );
    let wide = BooleanPageGroup::new(PageId(0), 0, 257, 0).unwrap();
    assert_eq!(
        OverlappingProbePlan::new(wide, policy()),
        Err(OverlappingProbeError::GroupTooWide {
            actual: 257,
            maximum: 256,
        })
    );
    let exact = BooleanPageGroup::new(PageId(0), 0, 256, 0).unwrap();
    assert!(OverlappingProbePlan::new(exact, policy()).is_ok());
    let pair = BooleanPageGroup::new(PageId(0), 0, 2, 0).unwrap();
    assert_eq!(
        OverlappingProbePlan::new(pair, policy())
            .unwrap()
            .dimensions(),
        1
    );
}

#[test]
fn contains_parameter_rejects_indexes_outside_group() {
    let group = BooleanPageGroup::new(PageId(0), 10, 4, 0).unwrap();
    let plan = OverlappingProbePlan::new(group, policy()).unwrap();
    let probe = plan.probes()[0];
    for index in [0, 9, 14, u64::MAX] {
        assert_eq!(
            probe.contains_parameter(index),
            Err(OverlappingProbeError::ParameterOutsideGroup {
                parameter_index: index,
                group,
            }),
            "index {index}"
        );
    }
    for index in 10..14 {
        assert!(probe.contains_parameter(index).is_ok());
    }
}

#[test]
fn score_observation_rejection_matrix() {
    let group = BooleanPageGroup::new(PageId(7), 0, 8, 0).unwrap();
    let plan = OverlappingProbePlan::new(group, policy()).unwrap();
    let valid = additive_observations(&plan, &[0; 8], 500_000);
    assert!(plan.score(&valid).is_ok());

    let mut bad_baseline = valid.clone();
    bad_baseline[0].baseline_loss_ppm = 1_000_001;
    assert_eq!(
        plan.score(&bad_baseline),
        Err(OverlappingProbeError::PpmOutOfRange(1_000_001))
    );

    let mut bad_perturbed = valid.clone();
    bad_perturbed[1].perturbed_loss_ppm = u32::MAX;
    assert_eq!(
        plan.score(&bad_perturbed),
        Err(OverlappingProbeError::PpmOutOfRange(u32::MAX))
    );

    let mut duplicate = valid.clone();
    duplicate[1] = duplicate[0];
    let first = duplicate[0].probe;
    assert_eq!(
        plan.score(&duplicate),
        Err(OverlappingProbeError::DuplicateObservation {
            round: first.round,
            dimension: first.dimension,
            side: first.side,
        })
    );

    let other_group = BooleanPageGroup::new(PageId(8), 0, 8, 0).unwrap();
    let other = OverlappingProbePlan::new(other_group, policy()).unwrap();
    let mut foreign_group = valid.clone();
    foreign_group[0].probe = other.probes()[0];
    assert_eq!(
        plan.score(&foreign_group),
        Err(OverlappingProbeError::ForeignProbe)
    );

    let mut reseeded = policy();
    reseeded.seed = 99;
    let reseeded_plan = OverlappingProbePlan::new(group, reseeded).unwrap();
    let mut foreign_seed = valid.clone();
    foreign_seed[0].probe = reseeded_plan.probes()[0];
    assert_eq!(
        plan.score(&foreign_seed),
        Err(OverlappingProbeError::ForeignProbe)
    );

    let mut more_rounds = policy();
    more_rounds.rounds = 5;
    let longer = OverlappingProbePlan::new(group, more_rounds).unwrap();
    let mut foreign_round = valid.clone();
    foreign_round[0].probe = *longer.probes().last().unwrap();
    assert_eq!(
        plan.score(&foreign_round),
        Err(OverlappingProbeError::ForeignProbe)
    );

    let mut extra = valid;
    extra.push(extra[0]);
    assert_eq!(
        plan.score(&extra),
        Err(OverlappingProbeError::ObservationCount {
            expected: plan.probe_count(),
            actual: plan.probe_count() + 1,
        })
    );
}

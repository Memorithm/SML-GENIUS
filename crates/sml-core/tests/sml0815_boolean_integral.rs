use sml_core::{
    boolean_integral::{
        BooleanIntegralAccumulator, BooleanIntegralDecision, BooleanIntegralError,
        BooleanIntegralPolicy, BooleanIntervention, PackedBooleanCreditLedger,
    },
    train::TruthTable2,
};

fn policy(min_observations: u32, min_confidence_ppm: u32) -> BooleanIntegralPolicy {
    BooleanIntegralPolicy {
        min_observations,
        min_confidence_ppm,
        max_mask_width: 16,
    }
}

fn sample_loss_ppm(table: TruthTable2, target: TruthTable2) -> u32 {
    let mut wrong = 0_u32;
    for input in 0..4_u8 {
        let left = input & 1 != 0;
        let right = input & 2 != 0;
        if table.evaluate(left, right) != target.evaluate(left, right) {
            wrong += 1;
        }
    }
    wrong * 250_000
}

#[test]
fn grouped_intervention_accumulates_normalized_shared_credit() {
    let mut integral = BooleanIntegralAccumulator::new(4, policy(1, 1)).unwrap();
    integral
        .observe(&BooleanIntervention {
            mask: vec![0, 2],
            baseline_loss_ppm: 800_000,
            perturbed_loss_ppm: 600_000,
        })
        .unwrap();

    let left = integral.cell(0).unwrap();
    let right = integral.cell(2).unwrap();

    assert_eq!(left.positive_mass, 100_000);
    assert_eq!(right.positive_mass, 100_000);
    assert_eq!(left.signed_confidence_ppm(), 1_000_000);
    assert_eq!(left.decision(policy(1, 1)), BooleanIntegralDecision::Flip);
    assert_eq!(integral.recommended_flips(), vec![0, 2]);
}

#[test]
fn contradictory_evidence_cancels_instead_of_forcing_a_flip() {
    let mut integral = BooleanIntegralAccumulator::new(2, policy(2, 250_000)).unwrap();

    integral
        .observe(&BooleanIntervention {
            mask: vec![1],
            baseline_loss_ppm: 700_000,
            perturbed_loss_ppm: 500_000,
        })
        .unwrap();
    integral
        .observe(&BooleanIntervention {
            mask: vec![1],
            baseline_loss_ppm: 500_000,
            perturbed_loss_ppm: 700_000,
        })
        .unwrap();

    let cell = integral.cell(1).unwrap();
    assert_eq!(cell.signed_confidence_ppm(), 0);
    assert_eq!(
        cell.decision(policy(2, 250_000)),
        BooleanIntegralDecision::Undecided
    );
    assert!(integral.recommended_flips().is_empty());
}

#[test]
fn boolean_integral_identifies_the_required_and_truth_table_flip() {
    let baseline = TruthTable2::FALSE;
    let target = TruthTable2::AND;
    let baseline_loss = sample_loss_ppm(baseline, target);
    assert_eq!(baseline_loss, 250_000);

    let mut integral = BooleanIntegralAccumulator::new(4, policy(1, 500_000)).unwrap();

    for bit in 0..4_u32 {
        let candidate = TruthTable2::new(baseline.code() ^ (1_u8 << bit)).unwrap();
        integral
            .observe(&BooleanIntervention {
                mask: vec![bit],
                baseline_loss_ppm: baseline_loss,
                perturbed_loss_ppm: sample_loss_ppm(candidate, target),
            })
            .unwrap();
    }

    assert_eq!(integral.recommended_flips(), vec![3]);
    let learned = TruthTable2::new(baseline.code() ^ (1_u8 << 3)).unwrap();
    assert_eq!(sample_loss_ppm(learned, target), 0);
}

#[test]
fn duplicate_mask_indexes_fail_closed() {
    let mut integral = BooleanIntegralAccumulator::new(4, policy(1, 0)).unwrap();
    let error = integral
        .observe(&BooleanIntervention {
            mask: vec![1, 1],
            baseline_loss_ppm: 100_000,
            perturbed_loss_ppm: 0,
        })
        .unwrap_err();

    assert_eq!(error, BooleanIntegralError::DuplicateMaskIndex(1));
    assert_eq!(integral.interventions(), 0);
}

#[test]
fn packed_credit_ledger_uses_four_bits_per_active_parameter() {
    let mut ledger = PackedBooleanCreditLedger::new(5).unwrap();
    assert_eq!(ledger.len(), 5);
    assert_eq!(PackedBooleanCreditLedger::storage_bytes_for_parameters(5), 3);
    assert_eq!(
        PackedBooleanCreditLedger::storage_bytes_for_parameters(62_500_000),
        31_250_000
    );

    for _ in 0..9 {
        ledger.vote(2, true).unwrap();
    }
    assert_eq!(ledger.score(2).unwrap(), 7);
    assert_eq!(ledger.candidate_flips(5).unwrap(), vec![2]);

    for _ in 0..20 {
        ledger.vote(2, false).unwrap();
    }
    assert_eq!(ledger.score(2).unwrap(), -7);
    assert!(ledger.candidate_flips(5).unwrap().is_empty());
}

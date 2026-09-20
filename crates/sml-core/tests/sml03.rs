use sml_core::train::{
    GateTrainingConfig, LatentGate2, TrainError, TruthTable2, TRUTH_TABLE_COUNT,
};

#[test]
fn hard_truth_table_codes_match_boolean_reference() {
    assert!(!TruthTable2::AND.evaluate(false, false));
    assert!(!TruthTable2::AND.evaluate(true, false));
    assert!(!TruthTable2::AND.evaluate(false, true));
    assert!(TruthTable2::AND.evaluate(true, true));

    assert!(!TruthTable2::XOR.evaluate(false, false));
    assert!(TruthTable2::XOR.evaluate(true, false));
    assert!(TruthTable2::XOR.evaluate(false, true));
    assert!(!TruthTable2::XOR.evaluate(true, true));
}

#[test]
fn latent_gate_learns_every_two_input_boolean_function() {
    let config = GateTrainingConfig::default();

    for code in 0..TRUTH_TABLE_COUNT as u8 {
        let target = TruthTable2::new(code).unwrap();
        let mut gate = LatentGate2::default();
        let report = gate.train_truth_table(target, config).unwrap();

        assert_eq!(report.final_table, target, "failed truth table {code}");
        assert_eq!(gate.hard_accuracy(target).unwrap(), 4);
        assert!(
            report.final_loss < 0.05,
            "loss={} code={code}",
            report.final_loss
        );
        assert_eq!(report.counters.epochs, config.epochs);
        assert_eq!(report.counters.samples, config.epochs as u64 * 4);
        assert_eq!(
            report.counters.latent_parameter_updates,
            config.epochs as u64 * TRUTH_TABLE_COUNT as u64
        );
    }
}

#[test]
fn hard_forward_remains_discrete_after_training() {
    let mut gate = LatentGate2::default();
    gate.train_truth_table(TruthTable2::XOR, GateTrainingConfig::default())
        .unwrap();

    for left in [false, true] {
        for right in [false, true] {
            assert_eq!(
                gate.hard_forward(left, right).unwrap(),
                left ^ right,
                "hard forward mismatch for ({left},{right})"
            );
        }
    }
}

#[test]
fn invalid_training_configuration_fails_closed() {
    let mut gate = LatentGate2::default();

    assert_eq!(
        gate.train_truth_table(
            TruthTable2::AND,
            GateTrainingConfig {
                epochs: 0,
                ..GateTrainingConfig::default()
            }
        ),
        Err(TrainError::ZeroEpochs)
    );
    assert_eq!(
        LatentGate2::from_logits([f64::NAN; TRUTH_TABLE_COUNT]),
        Err(TrainError::NonFiniteLatent)
    );
    assert_eq!(TruthTable2::new(16), Err(TrainError::InvalidTruthTable(16)));
}

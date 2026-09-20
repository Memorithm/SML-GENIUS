use sml_core::{
    circuit::{CircuitNodeSpec, CircuitSample, LatentBooleanCircuit, SignalRef},
    train::{GateTrainingConfig, TruthTable2},
};

fn xor_then_and_samples() -> Vec<CircuitSample> {
    let mut samples = Vec::with_capacity(8);
    for mask in 0_u8..8 {
        let x0 = mask & 1 != 0;
        let x1 = mask & 2 != 0;
        let x2 = mask & 4 != 0;
        samples.push(CircuitSample {
            inputs: vec![x0, x1, x2],
            target: (x0 ^ x1) & x2,
        });
    }
    samples
}

fn xor_then_and_circuit() -> LatentBooleanCircuit {
    LatentBooleanCircuit::new(
        3,
        vec![
            CircuitNodeSpec {
                left: SignalRef::Input(0),
                right: SignalRef::Input(1),
            },
            CircuitNodeSpec {
                left: SignalRef::Gate(0),
                right: SignalRef::Input(2),
            },
        ],
        SignalRef::Gate(1),
    )
    .unwrap()
}

#[test]
fn two_gate_circuit_learns_xor_then_and_exactly() {
    let samples = xor_then_and_samples();
    let mut circuit = xor_then_and_circuit();
    let config = GateTrainingConfig {
        epochs: 800,
        learning_rate: 0.2,
        epsilon: 1.0e-6,
    };
    let report = circuit.train(&samples, config).unwrap();

    assert_eq!(
        report.final_tables,
        vec![TruthTable2::XOR, TruthTable2::AND]
    );
    assert_eq!(report.hard_correct, 8);
    assert!(report.final_loss < report.initial_loss);
    assert_eq!(circuit.hard_accuracy(&samples).unwrap(), 8);
    assert_eq!(report.counters.epochs, 800);
    assert_eq!(report.counters.samples_seen, 6_400);
    assert_eq!(report.counters.soft_gate_evaluations, 12_800);
    assert_eq!(report.counters.latent_parameter_updates, 25_600);
}

#[test]
fn fixed_wiring_accounting_separates_learned_bits_from_metadata() {
    let circuit = xor_then_and_circuit();
    let accounting = circuit.accounting();

    assert_eq!(accounting.gate_count, 2);
    assert_eq!(accounting.learned_boolean_bits, 8);
    assert_eq!(accounting.latent_training_scalars, 32);
    assert_eq!(accounting.wiring_metadata_bits, 79);
    assert_eq!(accounting.parameter_budget().boolean, 8);
    assert_eq!(accounting.parameter_budget().storage_bits(), 8);
}

#[test]
fn circuit_training_is_deterministic() {
    let samples = xor_then_and_samples();
    let config = GateTrainingConfig {
        epochs: 800,
        learning_rate: 0.2,
        epsilon: 1.0e-6,
    };
    let mut left = xor_then_and_circuit();
    let mut right = xor_then_and_circuit();

    let left_report = left.train(&samples, config).unwrap();
    let right_report = right.train(&samples, config).unwrap();

    assert_eq!(left_report, right_report);
    assert_eq!(left.hard_tables().unwrap(), right.hard_tables().unwrap());
}

#[test]
fn circuit_constructor_rejects_cycles_and_forward_references() {
    let result = LatentBooleanCircuit::new(
        2,
        vec![CircuitNodeSpec {
            left: SignalRef::Gate(0),
            right: SignalRef::Input(0),
        }],
        SignalRef::Gate(0),
    );
    assert!(result.is_err());
}

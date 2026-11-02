//! Table-driven rejection matrix for fixed-wiring latent Boolean circuits.
//!
//! Covers every public error path of `circuit.rs`: `LatentBooleanCircuit::new`
//! (input/gate counts, input references, forward/cyclic gate references,
//! output references), `hard_forward`, `hard_accuracy`, and `train`
//! (config validation, sample validation, untrainable outputs), with
//! positive boundary controls and state-unchanged-on-rejection checks.
//!
//! Not reachable from the public API (documented, not tested):
//! - `CircuitError::Train(TrainError::InvalidTruthTable | InvalidUnaryTruthTable
//!   | EmptySamples)`: circuit code only builds tables from codes `0..16`
//!   and checks samples itself.
//! - `resolve_hard` / `resolve_soft` `InvalidInputRef` / `InvalidOutputGate`:
//!   the constructor proves every reference in-range before evaluation.
//! - `Train(NonFiniteLatent | InvalidSoftmax)`: deterministic initial logits
//!   are finite; a probe with `learning_rate` up to `f64::MAX` did not drive
//!   the logits non-finite.

use sml_core::{
    circuit::{CircuitError, CircuitNodeSpec, CircuitSample, LatentBooleanCircuit, SignalRef},
    train::{GateTrainingConfig, TrainError},
};

const fn node(left: SignalRef, right: SignalRef) -> CircuitNodeSpec {
    CircuitNodeSpec { left, right }
}

fn two_gate_specs() -> Vec<CircuitNodeSpec> {
    vec![
        node(SignalRef::Input(0), SignalRef::Input(1)),
        node(SignalRef::Gate(0), SignalRef::Input(2)),
    ]
}

fn circuit() -> LatentBooleanCircuit {
    LatentBooleanCircuit::new(3, two_gate_specs(), SignalRef::Gate(1)).unwrap()
}

fn samples() -> Vec<CircuitSample> {
    (0_u8..8)
        .map(|mask| {
            let (x0, x1, x2) = (mask & 1 != 0, mask & 2 != 0, mask & 4 != 0);
            CircuitSample {
                inputs: vec![x0, x1, x2],
                target: (x0 ^ x1) & x2,
            }
        })
        .collect()
}

fn config() -> GateTrainingConfig {
    GateTrainingConfig {
        epochs: 4,
        learning_rate: 0.2,
        epsilon: 1.0e-6,
    }
}

#[test]
fn positive_control_boundaries_are_accepted() {
    // Minimal circuit: one input, one gate reading it twice.
    let minimal = LatentBooleanCircuit::new(
        1,
        vec![node(SignalRef::Input(0), SignalRef::Input(0))],
        SignalRef::Gate(0),
    )
    .unwrap();
    assert_eq!(minimal.input_count(), 1);
    assert_eq!(minimal.gate_count(), 1);
    assert!(minimal.hard_forward(&[true]).is_ok());

    // Highest legal input reference, gate referencing the immediately previous gate,
    // output on the last gate and output directly on an input.
    let specs = vec![
        node(SignalRef::Input(2), SignalRef::Input(0)),
        node(SignalRef::Gate(0), SignalRef::Gate(0)),
    ];
    for output in [SignalRef::Gate(1), SignalRef::Gate(0), SignalRef::Input(2)] {
        let c = LatentBooleanCircuit::new(3, specs.clone(), output).unwrap();
        assert!(c.hard_forward(&[true, false, true]).is_ok());
        assert!(c.hard_accuracy(&samples()).is_ok());
    }

    // Exactly u16::MAX inputs and u16::MAX gates are accepted.
    let max = u16::MAX as usize;
    assert!(LatentBooleanCircuit::new(
        max,
        vec![node(SignalRef::Input(u16::MAX - 1), SignalRef::Input(0))],
        SignalRef::Gate(0),
    )
    .is_ok());
    let max_gates = vec![node(SignalRef::Input(0), SignalRef::Input(0)); max];
    let wide = LatentBooleanCircuit::new(1, max_gates, SignalRef::Gate(u16::MAX - 1)).unwrap();
    assert_eq!(wide.gate_count(), max);

    // Training config boundaries: epochs 1, epsilon just under 0.5.
    let mut c = circuit();
    let report = c
        .train(
            &samples(),
            GateTrainingConfig {
                epochs: 1,
                learning_rate: f64::MIN_POSITIVE,
                epsilon: 0.499_999,
            },
        )
        .unwrap();
    assert_eq!(report.counters.epochs, 1);
    assert_eq!(report.sample_count, 8);
}

#[test]
fn constructor_rejection_matrix() {
    let max = u16::MAX as usize;
    let cases: Vec<(&str, usize, Vec<CircuitNodeSpec>, SignalRef, CircuitError)> = vec![
        (
            "zero inputs",
            0,
            two_gate_specs(),
            SignalRef::Gate(1),
            CircuitError::ZeroInputs,
        ),
        (
            "zero inputs wins over zero gates",
            0,
            vec![],
            SignalRef::Gate(0),
            CircuitError::ZeroInputs,
        ),
        (
            "too many inputs",
            max + 1,
            two_gate_specs(),
            SignalRef::Gate(1),
            CircuitError::TooManyInputs(max + 1),
        ),
        (
            "too many inputs wins over zero gates",
            max + 1,
            vec![],
            SignalRef::Gate(0),
            CircuitError::TooManyInputs(max + 1),
        ),
        (
            "zero gates",
            3,
            vec![],
            SignalRef::Input(0),
            CircuitError::ZeroGates,
        ),
        (
            "too many gates",
            1,
            vec![node(SignalRef::Input(0), SignalRef::Input(0)); max + 1],
            SignalRef::Gate(0),
            CircuitError::TooManyGates(max + 1),
        ),
        (
            "left input out of range",
            3,
            vec![node(SignalRef::Input(3), SignalRef::Input(0))],
            SignalRef::Gate(0),
            CircuitError::InvalidInputRef(3),
        ),
        (
            "right input out of range",
            3,
            vec![node(SignalRef::Input(0), SignalRef::Input(u16::MAX))],
            SignalRef::Gate(0),
            CircuitError::InvalidInputRef(u16::MAX),
        ),
        (
            "self-loop left",
            3,
            vec![node(SignalRef::Gate(0), SignalRef::Input(0))],
            SignalRef::Gate(0),
            CircuitError::ForwardOrCyclicGateRef {
                referenced: 0,
                current: 0,
            },
        ),
        (
            "self-loop right on second gate",
            3,
            vec![
                node(SignalRef::Input(0), SignalRef::Input(1)),
                node(SignalRef::Gate(0), SignalRef::Gate(1)),
            ],
            SignalRef::Gate(1),
            CircuitError::ForwardOrCyclicGateRef {
                referenced: 1,
                current: 1,
            },
        ),
        (
            "forward reference",
            3,
            vec![
                node(SignalRef::Gate(1), SignalRef::Input(0)),
                node(SignalRef::Input(0), SignalRef::Input(1)),
            ],
            SignalRef::Gate(1),
            CircuitError::ForwardOrCyclicGateRef {
                referenced: 1,
                current: 0,
            },
        ),
        (
            "unknown gate id",
            3,
            two_gate_specs(),
            SignalRef::Gate(u16::MAX),
            CircuitError::InvalidOutputGate(u16::MAX),
        ),
        (
            "output gate one past end",
            3,
            two_gate_specs(),
            SignalRef::Gate(2),
            CircuitError::InvalidOutputGate(2),
        ),
        (
            "output input out of range",
            3,
            two_gate_specs(),
            SignalRef::Input(3),
            CircuitError::InvalidInputRef(3),
        ),
        (
            "wiring checked before output",
            3,
            vec![node(SignalRef::Input(9), SignalRef::Input(0))],
            SignalRef::Gate(7),
            CircuitError::InvalidInputRef(9),
        ),
    ];

    for (name, inputs, specs, output, expected) in cases {
        assert_eq!(
            LatentBooleanCircuit::new(inputs, specs, output),
            Err(expected),
            "case: {name}"
        );
    }
}

#[test]
fn hard_forward_rejects_wrong_input_width() {
    let c = circuit();
    for width in [0_usize, 1, 2, 4, 64] {
        assert_eq!(
            c.hard_forward(&vec![true; width]),
            Err(CircuitError::InputCount {
                expected: 3,
                actual: width
            }),
            "width {width}"
        );
    }
    assert!(c.hard_forward(&[true, true, true]).is_ok());
}

#[test]
fn hard_accuracy_rejection_matrix() {
    let c = circuit();
    assert_eq!(c.hard_accuracy(&[]), Err(CircuitError::EmptySamples));

    // A single forged sample anywhere in the panel rejects the whole panel.
    for position in [0_usize, 3, 7] {
        let mut panel = samples();
        panel[position].inputs.push(false);
        assert_eq!(
            c.hard_accuracy(&panel),
            Err(CircuitError::InputCount {
                expected: 3,
                actual: 4
            }),
            "position {position}"
        );
    }
    let mut short = samples();
    short[5].inputs.clear();
    assert_eq!(
        c.hard_accuracy(&short),
        Err(CircuitError::InputCount {
            expected: 3,
            actual: 0
        })
    );
    assert!(c.hard_accuracy(&samples()).unwrap() <= 8);
}

#[test]
fn train_rejection_matrix_leaves_state_unchanged() {
    let good = config();
    let bad_configs = [
        (
            "zero epochs",
            GateTrainingConfig { epochs: 0, ..good },
            TrainError::ZeroEpochs,
        ),
        (
            "zero lr",
            GateTrainingConfig {
                learning_rate: 0.0,
                ..good
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "negative lr",
            GateTrainingConfig {
                learning_rate: -0.1,
                ..good
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "nan lr",
            GateTrainingConfig {
                learning_rate: f64::NAN,
                ..good
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "inf lr",
            GateTrainingConfig {
                learning_rate: f64::INFINITY,
                ..good
            },
            TrainError::InvalidLearningRate,
        ),
        (
            "zero epsilon",
            GateTrainingConfig {
                epsilon: 0.0,
                ..good
            },
            TrainError::InvalidEpsilon,
        ),
        (
            "negative epsilon",
            GateTrainingConfig {
                epsilon: -1.0e-6,
                ..good
            },
            TrainError::InvalidEpsilon,
        ),
        (
            "epsilon one half",
            GateTrainingConfig {
                epsilon: 0.5,
                ..good
            },
            TrainError::InvalidEpsilon,
        ),
        (
            "nan epsilon",
            GateTrainingConfig {
                epsilon: f64::NAN,
                ..good
            },
            TrainError::InvalidEpsilon,
        ),
    ];

    for (name, cfg, expected) in bad_configs {
        let mut c = circuit();
        let before = c.clone();
        assert_eq!(
            c.train(&samples(), cfg),
            Err(CircuitError::Train(expected)),
            "case: {name}"
        );
        assert_eq!(c, before, "state changed on rejection: {name}");
    }

    // Config is validated before samples.
    let mut c = circuit();
    let before = c.clone();
    assert_eq!(
        c.train(&[], GateTrainingConfig { epochs: 0, ..good }),
        Err(CircuitError::Train(TrainError::ZeroEpochs))
    );
    assert_eq!(c, before);

    // Sample rejections.
    assert_eq!(c.train(&[], good), Err(CircuitError::EmptySamples));
    assert_eq!(c, before);
    let mut forged = samples();
    forged[2].inputs.pop();
    assert_eq!(
        c.train(&forged, good),
        Err(CircuitError::InputCount {
            expected: 3,
            actual: 2
        })
    );
    assert_eq!(c, before);
}

#[test]
fn train_rejects_input_output_without_mutation() {
    let mut c = LatentBooleanCircuit::new(3, two_gate_specs(), SignalRef::Input(1)).unwrap();
    let before = c.clone();
    assert_eq!(
        c.train(&samples(), config()),
        Err(CircuitError::UntrainableOutput)
    );
    assert_eq!(c, before);

    // Samples are validated before the output kind.
    assert_eq!(c.train(&[], config()), Err(CircuitError::EmptySamples));
    assert_eq!(c, before);

    // Hard evaluation is still allowed for an input-routed output.
    assert_eq!(c.hard_forward(&[false, true, false]), Ok(true));
}

#[test]
fn accounting_is_stable_across_rejections() {
    let mut c = circuit();
    let accounting = c.accounting();
    let _ = c.train(&[], config());
    let _ = c.hard_forward(&[true]);
    let _ = c.hard_accuracy(&[]);
    assert_eq!(c.accounting(), accounting);
    assert_eq!(
        accounting.parameter_budget().boolean,
        accounting.learned_boolean_bits
    );
    assert_eq!(accounting.learned_boolean_bits, 4 * accounting.gate_count);
}

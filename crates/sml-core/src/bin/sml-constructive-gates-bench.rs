//! A6-C1 preregistered construction experiment: two train-fitted recurrent
//! residual ANF gates in one frozen-history network, on fully disjoint random
//! train/validation/test sequences. No corpus, GPU, or predictor tuning here.
//!
//! Running: sml-constructive-gates-bench [seeds:1..=10]
//!
//! Panel 1 (clean): the known residual sources implement XOR3 and MUX3.
//! Panel 2 (noise): the same functions with independent 1/16 label noise.
//! Baseline = current input channel 0. Constructive targets are the XOR
//! residual of that baseline. Causal gate inputs are the current input plus
//! PREVIOUS recurrent state. An exact ANF two-input XOR output reads the newly
//! produced state and the baseline input at each time step.
//!
//! Controls search all source subsets with fan-in <=1 and <=2. The proposed
//! constructive learner searches <=3 simultaneously (needed for XOR synergy).
//! All source/ANF fits use training data only; each joint gate is committed
//! only when validation errors strictly decrease. All choices for every seed
//! and panel are frozen BEFORE generating/scoring their independent test split.
//! Test errors here measure synthetic local gate-task fidelity, NOT accuracy of
//! the SML language model.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_constructive::{
        fit_residual_gate, residual_output_errors, validate_and_commit, ConstructedGate,
        ConstructionDecision,
    },
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const INPUT_BITS: usize = 4;
const FREE_GATES: usize = 2;
const LAGS: usize = 2;
const STATE_BITS: usize = FREE_GATES + INPUT_BITS * LAGS;
const TRAIN_STEPS: usize = 96;
const VALIDATION_STEPS: usize = 48;
const TEST_STEPS: usize = 48;
const BASELINE_INPUT: usize = 0;
const SOURCE_COUNT: usize = INPUT_BITS * (LAGS + 1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Clean,
    Noisy,
}

impl Panel {
    fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Noisy => "noise_1_in_16",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Clean => 0,
            Self::Noisy => 1,
        }
    }
}

fn base_network() -> LocalAnfNetwork {
    let width = STATE_BITS + INPUT_BITS;
    let mut gates = Vec::with_capacity(STATE_BITS);
    // Two target-specific free gates initially have ZERO output.
    for _ in 0..FREE_GATES {
        gates.push(LocalAnfGate::zero(vec![STATE_BITS], width).unwrap());
    }
    // Frozen exact 1- and 2-step delay lines. No task-target supervision.
    for lag in 0..LAGS {
        for p in 0..INPUT_BITS {
            let source = if lag == 0 {
                STATE_BITS + p
            } else {
                FREE_GATES + (lag - 1) * INPUT_BITS + p
            };
            let mut gate = LocalAnfGate::zero(vec![source], width).unwrap();
            gate.flip(1);
            gates.push(gate);
        }
    }
    let readout = LocalAnfGate::zero(vec![STATE_BITS], width).unwrap();
    LocalAnfNetwork::with_input_bits(STATE_BITS, INPUT_BITS, gates, readout).unwrap()
}

fn source_candidates() -> Vec<usize> {
    let mut sources: Vec<usize> = (FREE_GATES..STATE_BITS).collect();
    sources.extend(STATE_BITS..STATE_BITS + INPUT_BITS);
    assert_eq!(sources.len(), SOURCE_COUNT);
    sources
}

/// Synthetic fixture with independent streams on 64 bit-sliced lanes.
/// Each target is x0 XOR residual, so x0 is a frozen baseline while the
/// learned gate must construct one residual from three unknown source bits:
///   target0 residual = XOR(x3(t), x1(t-1), x2(t-2))
///   target1 residual = MUX(x3(t), x1(t-1), x2(t-2))
/// Input bits for negative-time indices are zero, as is recurrent state.
fn fixture(seed: u64, steps: usize, panel: Panel) -> LaneData {
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut targets = vec![0u64; steps * FREE_GATES];
    for t in 0..steps {
        let baseline = inputs[t * INPUT_BITS + BASELINE_INPUT];
        let selector = inputs[t * INPUT_BITS + 3];
        let previous_one = if t > 0 {
            inputs[(t - 1) * INPUT_BITS + 1]
        } else {
            0
        };
        let previous_two = if t > 1 {
            inputs[(t - 2) * INPUT_BITS + 2]
        } else {
            0
        };
        let xor_three = selector ^ previous_one ^ previous_two;
        let mux_three = (selector & previous_one) | (!selector & previous_two);
        let flip0 = if panel == Panel::Noisy {
            // Each bit is corrupted independently with probability 1/16.
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        let flip1 = if panel == Panel::Noisy {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        targets[t * FREE_GATES] = baseline ^ xor_three ^ flip0;
        targets[t * FREE_GATES + 1] = baseline ^ mux_three ^ flip1;
    }
    LaneData {
        input_bits: INPUT_BITS,
        target_bits: FREE_GATES,
        blocks: vec![LaneBlock {
            steps,
            inputs,
            targets,
            valid: u64::MAX,
        }],
    }
}

fn panel_seed(seed: u64, panel: Panel, stage: u64) -> u64 {
    (seed + 1)
        .wrapping_mul(0xd134_2543_de82_ef95)
        .wrapping_add((panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
        ^ stage
}

struct GateRun {
    target: usize,
    controls: Vec<ConstructedGate>,
    decision: ConstructionDecision,
    fit_seconds: [f64; 3],
}

struct Frozen {
    seed: u64,
    panel: Panel,
    network: LocalAnfNetwork,
    gates: Vec<GateRun>,
}

fn score_override(
    original: &LocalAnfNetwork,
    data: &LaneData,
    target: usize,
    gate: &LocalAnfGate,
) -> Result<u64, Box<dyn Error>> {
    let mut network = original.clone();
    network.gates_mut()[target] = gate.clone();
    Ok(residual_output_errors(&network, data, target, target, BASELINE_INPUT)?.0)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-constructive-gates-bench [seeds:1..=10]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |value| value.parse())?;
    if !(1..=10).contains(&seeds) {
        return Err("seeds must be between 1 and 10".into());
    }
    let candidate_positions = source_candidates();
    println!(
        "A6_CONFIG,seeds={seeds},train_tokens={},validation_tokens={},test_tokens={},source_candidates={},free_gates=2,max_joint_fan_in=3,history_lags=2",
        TRAIN_STEPS * 64,
        VALIDATION_STEPS * 64,
        TEST_STEPS * 64,
        candidate_positions.len()
    );
    println!(
        "A6_PROTOCOL,train_only_mobius=true,validation_only_commit=true,test_after_all_frozen=true,exact_anfnative_xor_output=true,independent_stream_seeds=true"
    );
    println!("A6_PANEL_TYPES,clean,noise_1_in_16");
    println!("A6_CONTROLS,baseline,fitted_fanin1,fitted_fanin2,constructive_fanin3");

    let mut frozen = Vec::with_capacity(seeds * 2);
    for panel in [Panel::Clean, Panel::Noisy] {
        for seed in 0..seeds as u64 {
            let train = fixture(panel_seed(seed, panel, 0xa6a6_1001), TRAIN_STEPS, panel);
            let validation = fixture(
                panel_seed(seed, panel, 0xa6a6_2002),
                VALIDATION_STEPS,
                panel,
            );
            let mut network = base_network();
            let mut gate_runs = Vec::new();
            for target in 0..FREE_GATES {
                let mut controls = Vec::new();
                let mut validation_errors = [0u64; 3];
                let mut fit_seconds = [0.0f64; 3];
                for cap in 1..=3 {
                    let clock = Instant::now();
                    let fit = fit_residual_gate(
                        &network,
                        &train,
                        target,
                        target,
                        BASELINE_INPUT,
                        &candidate_positions,
                        cap,
                    )?;
                    fit_seconds[cap - 1] = clock.elapsed().as_secs_f64();
                    validation_errors[cap - 1] =
                        score_override(&network, &validation, target, &fit.gate)?;
                    controls.push(fit);
                }
                let decision = validate_and_commit(
                    &mut network,
                    &validation,
                    target,
                    target,
                    BASELINE_INPUT,
                    &controls[2],
                )?;
                if decision.candidate_validation_errors != validation_errors[2] {
                    return Err("candidate validation oracle mismatch".into());
                }
                println!(
                    "A6_GATE_FROZEN,panel={},seed={seed},target={target},accepted={},selected_sources={:?},anf_terms={},train_errors={},val_baseline_errors={},val_fanin1_errors={},val_fanin2_errors={},val_fanin3_errors={},subsets_explored={}",
                    panel.name(),
                    decision.committed,
                    controls[2].inputs,
                    controls[2].gate.nonzero_monomials(),
                    controls[2].train_errors,
                    decision.baseline_validation_errors,
                    validation_errors[0],
                    validation_errors[1],
                    validation_errors[2],
                    controls[2].evaluated_subsets,
                );
                gate_runs.push(GateRun {
                    target,
                    controls,
                    decision,
                    fit_seconds,
                });
            }
            frozen.push(Frozen {
                seed,
                panel,
                network,
                gates: gate_runs,
            });
        }
    }
    // Fitted models, source subsets and acceptance decisions are now immutable.
    println!("A6_ALL_FROZEN,true");
    println!("A6_CASE_HEADER,panel,seed,target,baseline_errors,fanin1_errors,fanin2_errors,constructed_errors,samples,accepted,train_errors,nonzero_anf_terms,fit_fanin3_sec");
    let mut totals = [[[0u64; 4]; FREE_GATES]; 2];
    let mut accepted = [[0usize; FREE_GATES]; 2];
    for run in &frozen {
        let test = fixture(
            panel_seed(run.seed, run.panel, 0xa6a6_3003),
            TEST_STEPS,
            run.panel,
        );
        let baseline = base_network();
        for gate in &run.gates {
            let target = gate.target;
            let (base_err, samples) =
                residual_output_errors(&baseline, &test, target, target, BASELINE_INPUT)?;
            let single = score_override(&baseline, &test, target, &gate.controls[0].gate)?;
            let pair = score_override(&baseline, &test, target, &gate.controls[1].gate)?;
            let (constructed, compared_samples) =
                residual_output_errors(&run.network, &test, target, target, BASELINE_INPUT)?;
            if samples != compared_samples {
                return Err("test sample mismatch".into());
            }
            let values = [base_err, single, pair, constructed];
            for (sum, &errors) in totals[run.panel.index()][target].iter_mut().zip(&values) {
                *sum += errors;
            }
            accepted[run.panel.index()][target] += usize::from(gate.decision.committed);
            println!(
                "A6_CASE_RESULT,{},{},{},{},{},{},{},{},{},{},{},{:.5}",
                run.panel.name(),
                run.seed,
                target,
                base_err,
                single,
                pair,
                constructed,
                samples,
                gate.decision.committed,
                gate.controls[2].train_errors,
                gate.controls[2].gate.nonzero_monomials(),
                gate.fit_seconds[2]
            );
        }
    }
    for panel in [Panel::Clean, Panel::Noisy] {
        for target in 0..FREE_GATES {
            let sum = totals[panel.index()][target];
            let denominator = (seeds * TEST_STEPS * 64) as f64;
            println!(
                "A6_SUMMARY,panel={},target={},seeds={seeds},baseline_test_error_rate={:.8},fanin1_test_error_rate={:.8},fanin2_test_error_rate={:.8},constructive_test_error_rate={:.8},accepted={}",
                panel.name(),
                target,
                sum[0] as f64 / denominator,
                sum[1] as f64 / denominator,
                sum[2] as f64 / denominator,
                sum[3] as f64 / denominator,
                accepted[panel.index()][target],
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_has_known_three_source_parity_and_mux_residuals() {
        let data = fixture(56, 20, Panel::Clean);
        let block = &data.blocks[0];
        for t in 0..block.steps {
            let p = block.inputs[t * 4 + 3];
            let a = if t > 0 { block.inputs[(t - 1) * 4 + 1] } else { 0 };
            let b = if t > 1 { block.inputs[(t - 2) * 4 + 2] } else { 0 };
            assert_eq!(block.targets[t * 2] ^ block.inputs[t * 4], p ^ a ^ b);
            assert_eq!(
                block.targets[t * 2 + 1] ^ block.inputs[t * 4],
                (p & a) | (!p & b)
            );
        }
    }

    #[test]
    fn two_constructed_recurrent_gates_resolve_xor_and_mux_on_unseen_sequences() {
        let mut network = base_network();
        let train = fixture(100, 48, Panel::Clean);
        let validation = fixture(200, 24, Panel::Clean);
        let test = fixture(300, 24, Panel::Clean);
        for target in 0..FREE_GATES {
            let fit = fit_residual_gate(
                &network,
                &train,
                target,
                target,
                BASELINE_INPUT,
                &source_candidates(),
                3,
            )
            .unwrap();
            assert_eq!(fit.train_errors, 0);
            assert_eq!(fit.inputs, vec![FREE_GATES + 1, FREE_GATES + INPUT_BITS + 2, STATE_BITS + 3]);
            assert_eq!(fit.evaluated_subsets, 12 + 66 + 220);
            let decision = validate_and_commit(
                &mut network,
                &validation,
                target,
                target,
                BASELINE_INPUT,
                &fit,
            )
            .unwrap();
            assert!(decision.committed);
            assert_eq!(decision.candidate_validation_errors, 0);
        }
        for target in 0..FREE_GATES {
            let (errors, samples) =
                residual_output_errors(&network, &test, target, target, BASELINE_INPUT).unwrap();
            assert_eq!(errors, 0);
            assert_eq!(samples, 24 * 64);
        }
    }

    #[test]
    fn independent_noisy_data_has_nonzero_irreducible_label_errors() {
        let noisy = fixture(1024, 100, Panel::Noisy);
        let clean = fixture(1024, 100, Panel::Clean);
        let differences = noisy.blocks[0]
            .targets
            .iter()
            .zip(&clean.blocks[0].targets)
            .map(|(&a, &b)| (a ^ b).count_ones() as u64)
            .sum::<u64>();
        assert!(differences > 0);
        assert!(differences < 100 * 2 * 64 / 5);
    }
}

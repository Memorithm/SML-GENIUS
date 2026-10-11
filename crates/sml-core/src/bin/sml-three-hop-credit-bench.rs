//! A6-C3: end-task-only three-gate credit across TWO recurrent temporal hops.
//!
//! Usage: sml-three-hop-credit-bench [seeds:1..=10].
//! Fixed synthetic task:
//! y_t = x0_t XOR x4_t XOR (x3_(t-1) AND (x1_(t-2) XOR x2_(t-2))).
//! All negative-time inputs and all initial recurrent states are zero.
//! Gate targets are never exposed: only final y_t labels are available.
//!
//! The learner must identify two raw inputs and a Boolean stage-0 function,
//! a stage-1 downstream function/source, and a stage-2 function/source.
//! Strict single-gate coordinate descent and capped-budget randomized-restart
//! coordinate descent are negative controls. Removing each gate from the
//! proposed circuit tests its individual causal necessity.
//!
//! Every training/validation decision in every seed/panel is frozen BEFORE
//! independent test data is generated. Errors are local Boolean task errors,
//! NOT language-model next-token accuracy or bits-per-byte.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_constructive::residual_output_errors,
    anf_credit_multihop::{fit_three_gate_credit, MultiHopReport, MultiHopSpec},
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const TRAIN_STEPS: usize = 96;
const VALIDATION_STEPS: usize = 48;
const TEST_STEPS: usize = 48;
const LANES: usize = 64;
const INPUT_BITS: usize = 5;
const STATES: usize = 3;
const OUTPUT_COLUMNS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Clean,
    LabelNoise,
}
impl Panel {
    fn index(self) -> usize {
        match self {
            Self::Clean => 0,
            Self::LabelNoise => 1,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::LabelNoise => "noise_1_in_16",
        }
    }
}

fn baseline() -> LocalAnfNetwork {
    let width = STATES + INPUT_BITS;
    let gates = vec![
        LocalAnfGate::zero(vec![STATES + 1, STATES + 2], width).unwrap(),
        LocalAnfGate::zero(vec![0, STATES + 1], width).unwrap(),
        LocalAnfGate::zero(vec![1, STATES + 1], width).unwrap(),
    ];
    let readout = LocalAnfGate::zero(vec![STATES], width).unwrap();
    LocalAnfNetwork::with_input_bits(STATES, INPUT_BITS, gates, readout).unwrap()
}

fn credit_spec() -> MultiHopSpec {
    MultiHopSpec {
        raw_candidates: vec![1, 2, 3, 4],
        baseline_input: 0,
        target_bit: 0,
    }
}

fn derive_seed(seed: usize, panel: Panel, stage: u64) -> u64 {
    (seed as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

fn dataset(seed: u64, steps: usize, panel: Panel) -> LaneData {
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut targets = vec![0u64; steps];
    for t in 0..steps {
        let delayed_feature = if t >= 2 {
            let xor = inputs[(t - 2) * INPUT_BITS + 1] ^ inputs[(t - 2) * INPUT_BITS + 2];
            xor & inputs[(t - 1) * INPUT_BITS + 3]
        } else {
            0
        };
        let noise = if panel == Panel::LabelNoise {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        targets[t] = inputs[t * INPUT_BITS] ^ inputs[t * INPUT_BITS + 4] ^ delayed_feature ^ noise;
    }
    LaneData {
        input_bits: INPUT_BITS,
        target_bits: 1,
        blocks: vec![LaneBlock {
            steps,
            inputs,
            targets,
            valid: u64::MAX,
        }],
    }
}

fn errors(model: &LocalAnfNetwork, data: &LaneData) -> Result<u64, Box<dyn Error>> {
    Ok(residual_output_errors(model, data, 2, 0, 0)?.0)
}

struct Frozen {
    seed: usize,
    panel: Panel,
    report: MultiHopReport,
    fit_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-three-hop-credit-bench [seeds:1..=10]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |value| value.parse())?;
    if !(1..=10).contains(&seeds) {
        return Err("seeds must be 1..=10".into());
    }
    println!(
        "A6C3_CONFIG,seeds={seeds},train_tokens={},validation_tokens={},test_tokens={},states=3,input_bits=5,temporal_hops=2,intermediate_target_labels=false,output_labels_only=true",
        TRAIN_STEPS * LANES,
        VALIDATION_STEPS * LANES,
        TEST_STEPS * LANES
    );
    println!(
        "A6C3_PROTOCOL,train_only_selection=true,validation_only_model_commit=true,test_generated_after_all_frozen=true,raw_candidate_pool=4,two_input_gate_truth_tables=16"
    );
    println!(
        "A6C3_CONTROLS,all_zero,strict_single_gate_coordinate,random_restart_coordinate_with_joint_budget_cap,fully_joint_triple,upstream_ablation,middle_ablation,downstream_ablation"
    );

    let mut frozen = Vec::with_capacity(seeds * 2);
    for panel in [Panel::Clean, Panel::LabelNoise] {
        for seed in 0..seeds {
            let train = dataset(derive_seed(seed, panel, 0xa6c3_1001), TRAIN_STEPS, panel);
            let validation = dataset(
                derive_seed(seed, panel, 0xa6c3_2002),
                VALIDATION_STEPS,
                panel,
            );
            let clock = Instant::now();
            let report = fit_three_gate_credit(
                &baseline(),
                &train,
                &validation,
                &credit_spec(),
                derive_seed(seed, panel, 0xa6c3_0001),
            )?;
            let fit_seconds = clock.elapsed().as_secs_f64();
            if report.train_samples != (TRAIN_STEPS * LANES) as u64
                || report.validation_samples != (VALIDATION_STEPS * LANES) as u64
                || report.joint_evaluations != 6 * 16 * 4 * 16 * 4 * 16
                || report.restart_evaluations > report.joint_evaluations
            {
                return Err("A6-C3 preregistered protocol mismatch".into());
            }
            println!(
                "A6C3_FROZEN,panel={},seed={},accepted_joint={},accepted_coordinate={},accepted_restarted={},train_baseline_errors={},train_joint_errors={},train_coordinate_errors={},train_restarted_errors={},val_baseline_errors={},val_joint_errors={},val_coordinate_errors={},val_restarted_errors={},stage0_sources={:?},stage0_terms={},stage1_sources={:?},stage1_terms={},stage2_sources={:?},stage2_terms={},joint_evals={},coordinate_evals={},restarted_evals={},restart_attempts={},train_cache_bytes={},fit_seconds={:.6}",
                panel.name(),
                seed,
                report.joint_accepted,
                report.coordinate_accepted,
                report.restarted_accepted,
                report.baseline_train_errors,
                report.joint_train_errors,
                report.coordinate_train_errors,
                report.restarted_train_errors,
                report.baseline_validation_errors,
                report.joint_validation_errors,
                report.coordinate_validation_errors,
                report.restarted_validation_errors,
                report.proposed.gates()[0].inputs(),
                report.proposed.gates()[0].nonzero_monomials(),
                report.proposed.gates()[1].inputs(),
                report.proposed.gates()[1].nonzero_monomials(),
                report.proposed.gates()[2].inputs(),
                report.proposed.gates()[2].nonzero_monomials(),
                report.joint_evaluations,
                report.coordinate_evaluations,
                report.restart_evaluations,
                report.restart_attempts,
                report.cached_word_bytes,
                fit_seconds
            );
            frozen.push(Frozen {
                seed,
                panel,
                report,
                fit_seconds,
            });
        }
    }

    // Strict leakage barrier: no independent test sample exists until NOW.
    println!("A6C3_ALL_FROZEN,true");
    println!(
        "A6C3_TEST_HEADER,panel,seed,samples,baseline_errors,coordinate_errors,restarted_errors,joint_errors,ablate0_errors,ablate1_errors,ablate2_errors,fit_seconds"
    );
    let mut totals = [[0u64; OUTPUT_COLUMNS]; 2];
    let mut no_error = [0usize; 2];
    for run in &frozen {
        let test = dataset(
            derive_seed(run.seed, run.panel, 0xa6c3_3003),
            TEST_STEPS,
            run.panel,
        );
        let reference = baseline();
        let base_error = errors(&reference, &test)?;
        let coordinate_error = errors(&run.report.coordinate, &test)?;
        let restarted_error = errors(&run.report.restarted, &test)?;
        let joint_error = errors(&run.report.committed, &test)?;

        let mut ablated = [0u64; 3];
        for (gate, errors_out) in ablated.iter_mut().enumerate() {
            let mut damaged = run.report.committed.clone();
            damaged.gates_mut()[gate] = reference.gates()[gate].clone();
            *errors_out = errors(&damaged, &test)?;
        }
        let row = [
            base_error,
            coordinate_error,
            restarted_error,
            joint_error,
            ablated[0],
            ablated[1],
            ablated[2],
        ];
        for (total, value) in totals[run.panel.index()].iter_mut().zip(row) {
            *total += value;
        }
        no_error[run.panel.index()] += usize::from(joint_error == 0);
        println!(
            "A6C3_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{:.6}",
            run.panel.name(),
            run.seed,
            TEST_STEPS * LANES,
            base_error,
            coordinate_error,
            restarted_error,
            joint_error,
            ablated[0],
            ablated[1],
            ablated[2],
            run.fit_seconds
        );
    }
    for panel in [Panel::Clean, Panel::LabelNoise] {
        let totals = totals[panel.index()];
        let n = (seeds * TEST_STEPS * LANES) as f64;
        println!(
            "A6C3_SUMMARY,panel={},seeds={},baseline_error_rate={:.8},coordinate_error_rate={:.8},restart_error_rate={:.8},joint_error_rate={:.8},ablate0_error_rate={:.8},ablate1_error_rate={:.8},ablate2_error_rate={:.8},joint_zero_test_seeds={}",
            panel.name(),
            seeds,
            totals[0] as f64 / n,
            totals[1] as f64 / n,
            totals[2] as f64 / n,
            totals[3] as f64 / n,
            totals[4] as f64 / n,
            totals[5] as f64 / n,
            totals[6] as f64 / n,
            no_error[panel.index()]
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_target_requires_two_causal_delays() {
        let data = dataset(1101, 16, Panel::Clean);
        let block = &data.blocks[0];
        for t in 0..block.steps {
            let delayed = if t >= 2 {
                (block.inputs[(t - 2) * INPUT_BITS + 1] ^ block.inputs[(t - 2) * INPUT_BITS + 2])
                    & block.inputs[(t - 1) * INPUT_BITS + 3]
            } else {
                0
            };
            let expected =
                block.inputs[t * INPUT_BITS] ^ block.inputs[t * INPUT_BITS + 4] ^ delayed;
            assert_eq!(block.targets[t], expected);
        }
    }

    #[test]
    fn noisy_panel_differs_from_clean_with_known_corruption_rate() {
        let clean = dataset(101, 96, Panel::Clean);
        let noisy = dataset(101, 96, Panel::LabelNoise);
        assert_eq!(clean.blocks[0].inputs, noisy.blocks[0].inputs);
        let total = (0..96)
            .map(|t| {
                (clean.blocks[0].targets[t] ^ noisy.blocks[0].targets[t]).count_ones() as usize
            })
            .sum::<usize>();
        assert!(total > 200);
        assert!(total < 600);
    }
}

//! A6-C2 paired end-task credit assignment across two recurrent ANF gates.
//! Usage: sml-coupled-credit-bench [seeds:1..=10].
//!
//! All decisions over five disjoint seeds and two panels are frozen before
//! generating/scoring ANY independent test data. Measures *synthetic Boolean
//! task errors*, never next-byte accuracy or language-model perplexity.
//!
//! Target: y_t = x0_t XOR [x1_t AND (x2_(t-1) XOR x3_(t-1))].
//! The parenthesized intermediate target is NEVER supplied to the trainer.
//! Gate 0 must encode the past parity, gate 1 combine it with current x1.
//! Both outputs are evaluated using the shared recurrent LocalAnfNetwork.
//! Negative controls: zero model, single-gate greedy coordinate descent,
//! learned-chain upstream/downstream ablations.

use sml_core::{
    anf_constructive::residual_output_errors,
    anf_credit::{fit_coupled_credit, CoupledCreditReport, CoupledCreditSpec},
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};
use std::{error::Error, time::Instant};

const TRAIN_STEPS: usize = 96;
const VAL_STEPS: usize = 48;
const TEST_STEPS: usize = 48;
const LANES: usize = 64;
const INPUT_BITS: usize = 4;

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
    let n = 2;
    let width = n + INPUT_BITS;
    let gates = vec![
        LocalAnfGate::zero(vec![n], width).unwrap(),
        LocalAnfGate::zero(vec![0, n + 1], width).unwrap(),
    ];
    let readout = LocalAnfGate::zero(vec![n], width).unwrap();
    LocalAnfNetwork::with_input_bits(n, INPUT_BITS, gates, readout).unwrap()
}

fn spec() -> CoupledCreditSpec {
    CoupledCreditSpec {
        intermediate_gate: 0,
        downstream_gate: 1,
        raw_inputs: vec![0, 1, 2, 3],
        target_bit: 0,
        baseline_input: 0,
    }
}

fn derive_seed(seed: usize, panel: Panel, stage: u64) -> u64 {
    (seed as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

/// Independent 64-lane synthetic sequences; all negative-time bits are zero.
fn dataset(seed: u64, steps: usize, panel: Panel) -> LaneData {
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut targets = vec![0u64; steps];
    for t in 0..steps {
        let x0 = inputs[t * INPUT_BITS];
        let x1 = inputs[t * INPUT_BITS + 1];
        let past = if t > 0 {
            inputs[(t - 1) * INPUT_BITS + 2] ^ inputs[(t - 1) * INPUT_BITS + 3]
        } else {
            0
        };
        let noise = if panel == Panel::Noisy {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        targets[t] = x0 ^ (x1 & past) ^ noise;
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
    Ok(residual_output_errors(model, data, 1, 0, 0)?.0)
}

struct Frozen {
    panel: Panel,
    seed: usize,
    learned: CoupledCreditReport,
    seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-coupled-credit-bench [seeds:1..=10]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |value| value.parse())?;
    if !(1..=10).contains(&seeds) {
        return Err("seeds must be 1..=10".into());
    }
    println!(
        "A6C2_CONFIG,seeds={seeds},train_tokens={},validation_tokens={},test_tokens={},train_only_end_task=true,validation_only_commit=true,test_after_all_frozen=true,intermediate_labels=false,recurrent_gate_count=2",
        TRAIN_STEPS * LANES, VAL_STEPS * LANES, TEST_STEPS * LANES
    );
    println!(
        "A6C2_GRAMMAR,upstream=two_current_raw_inputs,downstream=previous_upstream_state_plus_one_current_input,raw_choices=4,truth_tables_per_two_input_gate=16,paired_candidates=6144"
    );
    println!(
        "A6C2_CONTROLS,zero_gate_baseline,single_gate_coordinate_descent,paired_search,upstream_ablation,downstream_ablation"
    );

    let mut frozen = Vec::with_capacity(seeds * 2);
    for panel in [Panel::Clean, Panel::Noisy] {
        for seed in 0..seeds {
            let training = dataset(derive_seed(seed, panel, 0xa6c2_1001), TRAIN_STEPS, panel);
            let validation = dataset(derive_seed(seed, panel, 0xa6c2_2002), VAL_STEPS, panel);
            let clock = Instant::now();
            let learned = fit_coupled_credit(&base_network(), &training, &validation, &spec())?;
            let seconds = clock.elapsed().as_secs_f64();
            if learned.samples_train != (TRAIN_STEPS * LANES) as u64
                || learned.samples_validation != (VAL_STEPS * LANES) as u64
                || learned.searched_pairs != 6 * 16 * 4 * 16
            {
                return Err("A6-C2 declared training protocol mismatch".into());
            }
            println!(
                "A6C2_FROZEN,panel={},seed={},joint_accepted={},coordinate_accepted={},train_baseline_errors={},train_coordinate_errors={},train_joint_errors={},val_baseline_errors={},val_coordinate_proposed_errors={},val_joint_proposed_errors={},gate0_inputs={:?},gate0_anf_terms={},gate1_inputs={:?},gate1_anf_terms={},paired_candidates={},coordinate_proposals={},fit_seconds={:.5}",
                panel.name(), seed, learned.committed_joint, learned.committed_coordinate,
                learned.baseline_train_errors, learned.coordinate_train_errors,
                learned.joint_train_errors, learned.baseline_validation_errors,
                learned.coordinate_validation_errors, learned.joint_validation_errors,
                learned.proposed.gates()[0].inputs(),
                learned.proposed.gates()[0].nonzero_monomials(),
                learned.proposed.gates()[1].inputs(),
                learned.proposed.gates()[1].nonzero_monomials(),
                learned.searched_pairs, learned.coordinate_proposals, seconds
            );
            frozen.push(Frozen {
                panel,
                seed,
                learned,
                seconds,
            });
        }
    }

    // Test streams are not even GENERATED until the search is fully frozen.
    println!("A6C2_ALL_FROZEN,true");
    println!(
        "A6C2_TEST_HEADER,panel,seed,samples,baseline_errors,coordinate_errors,paired_errors,upstream_ablation_errors,downstream_ablation_errors,paired_committed,coordinate_committed,fit_seconds"
    );
    let mut totals = [[0u64; 5]; 2];
    let mut clean_zero_error_seeds = [0usize; 2];
    for run in &frozen {
        let test = dataset(
            derive_seed(run.seed, run.panel, 0xa6c2_3003),
            TEST_STEPS,
            run.panel,
        );
        let zero = base_network();
        let baseline_errors = errors(&zero, &test)?;
        let coordinate_errors = errors(&run.learned.coordinate, &test)?;
        let joint_errors = errors(&run.learned.committed, &test)?;
        let mut upstream_off = run.learned.committed.clone();
        upstream_off.gates_mut()[0] = zero.gates()[0].clone();
        let mut downstream_off = run.learned.committed.clone();
        downstream_off.gates_mut()[1] = zero.gates()[1].clone();
        let upstream_ablation = errors(&upstream_off, &test)?;
        let downstream_ablation = errors(&downstream_off, &test)?;
        let values = [
            baseline_errors,
            coordinate_errors,
            joint_errors,
            upstream_ablation,
            downstream_ablation,
        ];
        for (total, &value) in totals[run.panel.index()].iter_mut().zip(&values) {
            *total += value;
        }
        clean_zero_error_seeds[run.panel.index()] += usize::from(joint_errors == 0);
        println!(
            "A6C2_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{:.5}",
            run.panel.name(),
            run.seed,
            TEST_STEPS * LANES,
            baseline_errors,
            coordinate_errors,
            joint_errors,
            upstream_ablation,
            downstream_ablation,
            run.learned.committed_joint,
            run.learned.committed_coordinate,
            run.seconds
        );
    }
    for panel in [Panel::Clean, Panel::Noisy] {
        let sum = totals[panel.index()];
        let n = (seeds * TEST_STEPS * LANES) as f64;
        println!(
            "A6C2_SUMMARY,panel={},seeds={},baseline_error_rate={:.8},coordinate_error_rate={:.8},paired_error_rate={:.8},upstream_ablation_error_rate={:.8},downstream_ablation_error_rate={:.8},zero_error_seeds={}",
            panel.name(), seeds, sum[0] as f64 / n, sum[1] as f64 / n,
            sum[2] as f64 / n, sum[3] as f64 / n, sum[4] as f64 / n,
            clean_zero_error_seeds[panel.index()]
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_requires_delayed_hidden_boolean_interaction() {
        let data = dataset(19, 20, Panel::Clean);
        let block = &data.blocks[0];
        for t in 0..block.steps {
            let past = if t > 0 {
                block.inputs[(t - 1) * INPUT_BITS + 2] ^ block.inputs[(t - 1) * INPUT_BITS + 3]
            } else {
                0
            };
            assert_eq!(
                block.targets[t],
                block.inputs[t * INPUT_BITS] ^ (block.inputs[t * INPUT_BITS + 1] & past)
            );
        }
    }

    #[test]
    fn coupled_two_gate_model_generalizes_without_intermediate_supervision() {
        let baseline = base_network();
        let old = baseline.clone();
        let train = dataset(5, 48, Panel::Clean);
        let validation = dataset(6, 24, Panel::Clean);
        let report = fit_coupled_credit(&baseline, &train, &validation, &spec()).unwrap();
        let replay = fit_coupled_credit(&baseline, &train, &validation, &spec()).unwrap();
        assert_eq!(report, replay);
        assert_eq!(baseline, old);
        assert_eq!(report.joint_train_errors, 0);
        assert_eq!(report.joint_validation_errors, 0);
        assert!(report.committed_joint);
        let test = dataset(7, 24, Panel::Clean);
        assert_eq!(errors(&report.committed, &test).unwrap(), 0);
        let mut ablated = report.committed.clone();
        ablated.gates_mut()[0] = baseline.gates()[0].clone();
        assert!(errors(&ablated, &test).unwrap() > 0);
        ablated = report.committed.clone();
        ablated.gates_mut()[1] = baseline.gates()[1].clone();
        assert!(errors(&ablated, &test).unwrap() > 0);
    }
}

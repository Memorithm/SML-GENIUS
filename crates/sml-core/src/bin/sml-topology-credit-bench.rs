//! A6-C4: experiment on learning recurrent Boolean graph topology from ONLY
//! the final observable target, rather than a predeclared A6-C3 gate chain.
//!
//! Usage: sml-topology-credit-bench [seeds:1..=5].
//!
//! Predeclared tasks expose four topological families:
//!  DIRECT:   y=x0 XOR AND(x1(t), x2(t))                  (0 temporal hops)
//!  SKIP:     y=x0 XOR [ XOR(x1(t-1),x2(t-1)) AND x3(t) ] (1 hop via stage 0)
//!  CHAIN:    y=x0 XOR x4(t) XOR [ x3(t-1) AND
//!                               XOR(x1(t-2),x2(t-2)) ]  (2 hops)
//!  PARALLEL: y=x0 XOR [AND(x1(t-1),x2(t-1)) XOR
//!                      AND(x3(t-1),x4(t-1))]            (two 1-hop branches)
//!
//! The task equations exist ONLY in the data generator, never the trainer.
//! The learner receives the input bitplanes and observable final y; it must
//! choose upstream gate inputs, their truth tables, skip/serial/parallel links,
//! and the final gate truth table on TRAIN. Validation only accepts/rejects a
//! whole model. All 40 train/validation configurations are frozen before any
//! held-out test data is GENERATED or scored.
//!
//! Controls are built with exactly the same datasets and selection conditions:
//! (a) three zero ANF state gates, (b) no recurrent edge/direct-only readout,
//! (c) A6-C3's fixed 2-hop chain grammar (trained independently).
//!
//! These metrics are local Boolean task errors, NOT language-model accuracy.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_constructive::residual_output_errors,
    anf_credit_topology::{
        fit_adaptive_topology, DiscoveredTopology, TopologyReport, TopologySpec,
    },
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const INPUT_BITS: usize = 5;
const STATES: usize = 3;
const TRAIN_STEPS: usize = 48;
const VALIDATION_STEPS: usize = 24;
const TEST_STEPS: usize = 24;
const LANES: usize = 64;
const METRICS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Direct,
    Skip,
    Chain,
    Parallel,
}
impl Task {
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Skip => "skip",
            Self::Chain => "chain",
            Self::Parallel => "parallel",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Skip => 1,
            Self::Chain => 2,
            Self::Parallel => 3,
        }
    }
    fn expected_depth(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Skip | Self::Parallel => 1,
            Self::Chain => 2,
        }
    }
    fn kind_recovered(self, topology: DiscoveredTopology) -> bool {
        match self {
            Self::Direct => topology == DiscoveredTopology::Direct,
            Self::Skip => matches!(
                topology,
                DiscoveredTopology::Skip | DiscoveredTopology::ViaMiddle
            ),
            Self::Chain => topology == DiscoveredTopology::Chain,
            Self::Parallel => topology == DiscoveredTopology::Parallel,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Clean,
    LabelNoise,
}
impl Panel {
    fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::LabelNoise => "noise_1_in_16",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Clean => 0,
            Self::LabelNoise => 1,
        }
    }
}

fn zero_network() -> LocalAnfNetwork {
    let n = STATES;
    let width = n + INPUT_BITS;
    let gates = vec![
        LocalAnfGate::zero(vec![n + 1, n + 2], width).unwrap(),
        LocalAnfGate::zero(vec![0, n + 1], width).unwrap(),
        LocalAnfGate::zero(vec![1, n + 1], width).unwrap(),
    ];
    let readout = LocalAnfGate::zero(vec![n], width).unwrap();
    LocalAnfNetwork::with_input_bits(n, INPUT_BITS, gates, readout).unwrap()
}

fn topology_spec() -> TopologySpec {
    TopologySpec {
        raw_candidates: vec![1, 2, 3, 4],
        baseline_input: 0,
        target_bit: 0,
    }
}

fn derive_seed(seed: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (seed as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

fn fixture(seed: u64, steps: usize, task: Task, panel: Panel) -> LaneData {
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut targets = vec![0u64; steps];
    let x = |t: usize, channel: usize| inputs[t * INPUT_BITS + channel];
    for (t, target) in targets.iter_mut().enumerate() {
        let residual = match task {
            Task::Direct => x(t, 1) & x(t, 2),
            Task::Skip => {
                if t == 0 {
                    0
                } else {
                    (x(t - 1, 1) ^ x(t - 1, 2)) & x(t, 3)
                }
            }
            Task::Chain => {
                x(t, 4)
                    ^ if t < 2 {
                        0
                    } else {
                        (x(t - 2, 1) ^ x(t - 2, 2)) & x(t - 1, 3)
                    }
            }
            Task::Parallel => {
                if t == 0 {
                    0
                } else {
                    (x(t - 1, 1) & x(t - 1, 2)) ^ (x(t - 1, 3) & x(t - 1, 4))
                }
            }
        };
        let label_noise = if panel == Panel::LabelNoise {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        *target = x(t, 0) ^ residual ^ label_noise;
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

fn count_errors(model: &LocalAnfNetwork, data: &LaneData) -> Result<u64, Box<dyn Error>> {
    Ok(residual_output_errors(model, data, 2, 0, 0)?.0)
}

struct Frozen {
    task: Task,
    panel: Panel,
    seed: usize,
    report: TopologyReport,
    fit_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-topology-credit-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |v| v.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seeds must be in 1..=5".into());
    }
    let tasks = [Task::Direct, Task::Skip, Task::Chain, Task::Parallel];
    let panels = [Panel::Clean, Panel::LabelNoise];
    println!(
        "A6C4_CONFIG,seeds={seeds},tasks=direct;skip;chain;parallel,panels=clean;noise_1_in_16,train_tokens={},validation_tokens={},test_tokens={},state_slots=3,raw_candidates=4,gates_fan_in=2",
        TRAIN_STEPS * LANES,
        VALIDATION_STEPS * LANES,
        TEST_STEPS * LANES
    );
    println!(
        "A6C4_PROTOCOL,intermediate_labels=false,train_only_source_and_truth_selection=true,validation_only_commit=true,all_models_frozen_before_test_generation=true,initial_topology_unknown=true"
    );
    println!(
        "A6C4_CONTROLS,three_zero_gates,train_fitted_direct_only,train_fitted_fixed_chain,train_fitted_adaptive_dag"
    );

    let mut frozen = Vec::with_capacity(seeds * tasks.len() * panels.len());
    for task in tasks {
        for panel in panels {
            for seed in 0..seeds {
                let train = fixture(
                    derive_seed(seed, task, panel, 0xa6c4_1001),
                    TRAIN_STEPS,
                    task,
                    panel,
                );
                let validation = fixture(
                    derive_seed(seed, task, panel, 0xa6c4_2002),
                    VALIDATION_STEPS,
                    task,
                    panel,
                );
                let clock = Instant::now();
                let report =
                    fit_adaptive_topology(&zero_network(), &train, &validation, &topology_spec())?;
                let fit_seconds = clock.elapsed().as_secs_f64();
                if report.train_evaluations != 77_190
                    || report.chain_evaluations != 24_576
                    || report.train_samples != (TRAIN_STEPS * LANES) as u64
                    || report.validation_samples != (VALIDATION_STEPS * LANES) as u64
                {
                    return Err("A6-C4 registered train/validation protocol mismatch".into());
                }
                println!(
                    "A6C4_FROZEN,task={},panel={},seed={},accepted_adaptive={},accepted_direct={},accepted_fixed_chain={},topology={},depth={},expected_depth={},depth_match={},shape_match={},live_gates={},anf_monomials={},train_baseline_errors={},train_adaptive_errors={},train_direct_errors={},train_fixed_chain_errors={},val_baseline_errors={},val_adaptive_errors={},val_direct_errors={},val_fixed_chain_errors={},train_fits={},fixed_chain_fits={},series_payload_bytes={},gate0_inputs={:?},gate1_inputs={:?},gate2_inputs={:?},fit_seconds={:.6}",
                    task.name(), panel.name(), seed,
                    report.accepted, report.direct_accepted, report.chain_accepted,
                    report.topology.name(), report.depth, task.expected_depth(),
                    report.depth == task.expected_depth(),
                    task.kind_recovered(report.topology),
                    report.live_state_gates, report.learned_monomials,
                    report.baseline_train_errors, report.train_errors,
                    report.direct_train_errors, report.chain_train_errors,
                    report.baseline_validation_errors, report.validation_errors,
                    report.direct_validation_errors, report.chain_validation_errors,
                    report.train_evaluations, report.chain_evaluations,
                    report.peak_series_payload_bytes,
                    report.proposed.gates()[0].inputs(),
                    report.proposed.gates()[1].inputs(),
                    report.proposed.gates()[2].inputs(), fit_seconds
                );
                frozen.push(Frozen {
                    task,
                    panel,
                    seed,
                    report,
                    fit_seconds,
                });
            }
        }
    }

    // Only now may the holdout streams be produced and evaluated.
    println!("A6C4_ALL_FROZEN,true");
    println!(
        "A6C4_TEST_HEADER,task,panel,seed,samples,zero_errors,direct_errors,fixed_chain_errors,adaptive_errors,ablate0_errors,ablate1_errors,ablate2_errors,topology,depth,fit_seconds"
    );
    let mut totals = [[[0u64; METRICS]; 2]; 4];
    let mut depth_hits = [[0usize; 2]; 4];
    let mut shape_hits = [[0usize; 2]; 4];
    let mut adaptive_accepts = [[0usize; 2]; 4];
    let mut direct_accepts = [[0usize; 2]; 4];
    let mut chain_accepts = [[0usize; 2]; 4];
    let mut times = [[0.0f64; 2]; 4];
    for run in &frozen {
        let test = fixture(
            derive_seed(run.seed, run.task, run.panel, 0xa6c4_3003),
            TEST_STEPS,
            run.task,
            run.panel,
        );
        let reference = zero_network();
        let zero_errors = count_errors(&reference, &test)?;
        let direct_errors = count_errors(&run.report.direct_control, &test)?;
        let chain_errors = count_errors(&run.report.chain_control, &test)?;
        let adaptive_errors = count_errors(&run.report.selected, &test)?;
        let mut ablated = [0u64; STATES];
        for (i, e) in ablated.iter_mut().enumerate() {
            let mut altered = run.report.selected.clone();
            altered.gates_mut()[i] = reference.gates()[i].clone();
            *e = count_errors(&altered, &test)?;
        }
        let errors = [
            zero_errors,
            direct_errors,
            chain_errors,
            adaptive_errors,
            ablated[0],
            ablated[1],
            ablated[2],
        ];
        let t = run.task.index();
        let p = run.panel.index();
        for (total, value) in totals[t][p].iter_mut().zip(errors) {
            *total += value;
        }
        depth_hits[t][p] += usize::from(run.report.depth == run.task.expected_depth());
        shape_hits[t][p] += usize::from(run.task.kind_recovered(run.report.topology));
        adaptive_accepts[t][p] += usize::from(run.report.accepted);
        direct_accepts[t][p] += usize::from(run.report.direct_accepted);
        chain_accepts[t][p] += usize::from(run.report.chain_accepted);
        times[t][p] += run.fit_seconds;
        println!(
            "A6C4_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6}",
            run.task.name(),
            run.panel.name(),
            run.seed,
            TEST_STEPS * LANES,
            zero_errors,
            direct_errors,
            chain_errors,
            adaptive_errors,
            ablated[0],
            ablated[1],
            ablated[2],
            run.report.topology.name(),
            run.report.depth,
            run.fit_seconds
        );
    }
    for task in tasks {
        for panel in panels {
            let t = task.index();
            let p = panel.index();
            let sum = totals[t][p];
            let tokens = (seeds * TEST_STEPS * LANES) as f64;
            println!(
                "A6C4_SUMMARY,task={},panel={},seeds={},zero_error_rate={:.8},direct_error_rate={:.8},fixed_chain_error_rate={:.8},adaptive_error_rate={:.8},ablate0_error_rate={:.8},ablate1_error_rate={:.8},ablate2_error_rate={:.8},depth_matches={},topology_matches={},adaptive_accepts={},direct_accepts={},chain_accepts={},mean_fit_seconds={:.6}",
                task.name(), panel.name(), seeds,
                sum[0] as f64 / tokens, sum[1] as f64 / tokens,
                sum[2] as f64 / tokens, sum[3] as f64 / tokens,
                sum[4] as f64 / tokens, sum[5] as f64 / tokens,
                sum[6] as f64 / tokens,
                depth_hits[t][p], shape_hits[t][p],
                adaptive_accepts[t][p], direct_accepts[t][p], chain_accepts[t][p],
                times[t][p] / seeds as f64
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_synthetic_task_observes_its_expected_causal_residual() {
        for task in [Task::Direct, Task::Skip, Task::Chain, Task::Parallel] {
            let data = fixture(55, 15, task, Panel::Clean);
            let block = &data.blocks[0];
            let x = |t: usize, p: usize| block.inputs[t * INPUT_BITS + p];
            for t in 0..block.steps {
                let label = block.targets[t] ^ x(t, 0);
                let expected = match task {
                    Task::Direct => x(t, 1) & x(t, 2),
                    Task::Skip => {
                        if t == 0 {
                            0
                        } else {
                            (x(t - 1, 1) ^ x(t - 1, 2)) & x(t, 3)
                        }
                    }
                    Task::Chain => {
                        x(t, 4)
                            ^ if t < 2 {
                                0
                            } else {
                                (x(t - 2, 1) ^ x(t - 2, 2)) & x(t - 1, 3)
                            }
                    }
                    Task::Parallel => {
                        if t == 0 {
                            0
                        } else {
                            (x(t - 1, 1) & x(t - 1, 2)) ^ (x(t - 1, 3) & x(t - 1, 4))
                        }
                    }
                };
                assert_eq!(label, expected);
            }
        }
    }

    #[test]
    fn panel_noise_is_independent_of_inputs() {
        let a = fixture(99, 64, Task::Parallel, Panel::Clean);
        let b = fixture(99, 64, Task::Parallel, Panel::LabelNoise);
        assert_eq!(a.blocks[0].inputs, b.blocks[0].inputs);
        let flips = a.blocks[0]
            .targets
            .iter()
            .zip(&b.blocks[0].targets)
            .map(|(a, b)| (a ^ b).count_ones() as usize)
            .sum::<usize>();
        assert!((150..400).contains(&flips));
    }
}

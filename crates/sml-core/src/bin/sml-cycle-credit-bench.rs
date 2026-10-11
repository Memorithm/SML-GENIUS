//! A6-C5: discover self- and mutual-feedback in native recurrent ANF
//! circuits from final observed labels, with out-of-length evaluation.
//!
//! Usage: sml-cycle-credit-bench [seeds:1..=5].
//!
//! Causal models have TWO binary state words, initialized to zero at stream
//! start, updated synchronously on the current five input channels.
//! Baseline output x0 XOR s1(t+1) is frozen. No intermediate gate labels.
//!
//! Predeclared task families:
//!   direct         = x0(t) XOR [x1(t) AND x2(t)]       -- no recurrence
//!   lag1           = x0(t) XOR x1(t-1)                 -- feedforward delay
//!   running_parity = x0(t) XOR XOR_{i=0..t} x1(i)      -- self-feedback
//!   stride2_parity = x0(t) XOR XOR_{j>=0} x1(t-2j)     -- mutual cycle
//!   stride3_NEG    = x0(t) XOR XOR_{j>=0} x1(t-3j)     -- needs >=3 bits
//!
//! Five train/validation seeds per family and each clean/noisy panel. Every
//! train/validation selection is FROZEN BEFORE independent test sequences are
//! created (512 and 4096 steps, each 64 parallel i.i.d. sequences).
//!
//! Do NOT compare these synthetic error rates with language-model accuracy.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_constructive::residual_output_errors,
    anf_credit_cycles::{
        fit_recurrent_cycles, CycleReport, CycleSpec, FeedbackClass, A6C5_PAIRS,
    },
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const INPUT_BITS: usize = 5;
const STATE_BITS: usize = 2;
const LANES: usize = 64;
const TRAIN_STEPS: usize = 64;
const VALIDATION_STEPS: usize = 64;
const TEST_STEPS: [usize; 2] = [512, 4096];
const MODELS: usize = 7;

#[derive(Clone, Copy, Debug)]
enum Task {
    Direct,
    Lag1,
    RunningParity,
    Stride2Parity,
    Stride3Negative,
}
impl Task {
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Lag1 => "lag1",
            Self::RunningParity => "running_parity",
            Self::Stride2Parity => "stride2_parity",
            Self::Stride3Negative => "stride3_unrepresentable",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Lag1 => 1,
            Self::RunningParity => 2,
            Self::Stride2Parity => 3,
            Self::Stride3Negative => 4,
        }
    }
    fn expected_class(self) -> Option<FeedbackClass> {
        match self {
            Self::Direct => Some(FeedbackClass::Direct),
            Self::Lag1 => Some(FeedbackClass::Acyclic),
            Self::RunningParity => Some(FeedbackClass::SelfFeedback),
            Self::Stride2Parity => Some(FeedbackClass::MutualFeedback),
            Self::Stride3Negative => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
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
    let gates = vec![
        LocalAnfGate::zero(vec![STATE_BITS + 1, STATE_BITS + 2], width).unwrap(),
        LocalAnfGate::zero(vec![STATE_BITS + 1, STATE_BITS + 2], width).unwrap(),
    ];
    let readout = LocalAnfGate::zero(vec![STATE_BITS], width).unwrap();
    LocalAnfNetwork::with_input_bits(STATE_BITS, INPUT_BITS, gates, readout).unwrap()
}
fn spec() -> CycleSpec {
    CycleSpec {
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
fn dataset(seed: u64, steps: usize, task: Task, panel: Panel) -> LaneData {
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs { *word = rng.next_u64(); }
    let mut targets = vec![0u64; steps];
    let mut p1 = 0u64;
    let mut p2 = [0u64; 2];
    let mut p3 = [0u64; 3];
    for (t, label) in targets.iter_mut().enumerate() {
        let x1 = inputs[t * INPUT_BITS + 1];
        p1 ^= x1;
        p2[t % 2] ^= x1;
        p3[t % 3] ^= x1;
        let residual = match task {
            Task::Direct => x1 & inputs[t * INPUT_BITS + 2],
            Task::Lag1 => if t == 0 { 0 } else { inputs[(t - 1) * INPUT_BITS + 1] },
            Task::RunningParity => p1,
            Task::Stride2Parity => p2[t % 2],
            Task::Stride3Negative => p3[t % 3],
        };
        let noise = if matches!(panel, Panel::Noisy) {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else { 0 };
        *label = inputs[t * INPUT_BITS] ^ residual ^ noise;
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

/// Remove every effective recurrent input from both fitted state gates by
/// clamping previous-state source inputs to zero in their ANF truth tables.
/// This preserves current raw edges and the fixed output/readout contract.
fn no_recurrent_edges(model: &LocalAnfNetwork) -> LocalAnfNetwork {
    let mut changed = model.clone();
    for i in 0..STATE_BITS {
        let gate = &model.gates()[i];
        let inputs = gate.inputs().to_vec();
        let reference = gate.truth_table();
        let table: Vec<bool> = (0..reference.len()).map(|address| {
            let mut reset = address;
            for (bit, &source) in inputs.iter().enumerate() {
                if source < STATE_BITS {
                    reset &= !(1 << bit);
                }
            }
            reference[reset]
        }).collect();
        changed.gates_mut()[i] =
            LocalAnfGate::from_truth_table(inputs, &table, model.source_width()).unwrap();
    }
    changed
}

/// Remove the upstream state gate while retaining the downstream gate and
/// all its inputs. For direct/self-feedback models this should be neutral.
fn disable_upstream(model: &LocalAnfNetwork) -> LocalAnfNetwork {
    let mut changed = model.clone();
    let upstream = &model.gates()[0];
    let inputs = upstream.inputs().to_vec();
    let zeros = vec![false; upstream.monomial_count()];
    changed.gates_mut()[0] =
        LocalAnfGate::from_truth_table(inputs, &zeros, model.source_width()).unwrap();
    changed
}

struct Frozen {
    seed: usize,
    task: Task,
    panel: Panel,
    fit_seconds: f64,
    result: CycleReport,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-cycle-credit-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |value| value.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seed count must be in 1..=5".into());
    }
    let tasks = [
        Task::Direct,
        Task::Lag1,
        Task::RunningParity,
        Task::Stride2Parity,
        Task::Stride3Negative,
    ];
    let panels = [Panel::Clean, Panel::Noisy];
    println!(
        "A6C5_CONFIG,seeds={seeds},task_types=5,panels=2,train_steps={TRAIN_STEPS},validation_steps={VALIDATION_STEPS},holdout_steps=512;4096,lanes={LANES},state_bits={STATE_BITS},fan_in=2"
    );
    println!(
        "A6C5_PROTOCOL,intermediate_labels=false,terminal_labels_only=true,train_only_function_and_wiring_selection=true,validation_only_commit=true,all_models_frozen_before_test_generation=true,negative_unrepresentable_strided_period_3=true"
    );
    println!(
        "A6C5_CONTROLS,zero,raw_only,acyclic_DAG,strict_coordinate,coordinate_restarts_joint_budget_cap,full_cycle_grammar,remove_recurrent_edges,remove_upstream_gate"
    );

    let mut frozen = Vec::with_capacity(seeds * tasks.len() * panels.len());
    for task in tasks {
        for panel in panels {
            for seed in 0..seeds {
                let train = dataset(
                    derive_seed(seed, task, panel, 0xa6c5_1001),
                    TRAIN_STEPS, task, panel,
                );
                let validation = dataset(
                    derive_seed(seed, task, panel, 0xa6c5_2002),
                    VALIDATION_STEPS, task, panel,
                );
                let clock = Instant::now();
                let result = fit_recurrent_cycles(
                    &base_network(),
                    &train,
                    &validation,
                    &spec(),
                    derive_seed(seed, task, panel, 0xa6c5_3003),
                )?;
                let fit_seconds = clock.elapsed().as_secs_f64();
                if result.joint_evaluations != A6C5_PAIRS
                    || result.direct_evaluations != 96
                    || result.acyclic_evaluations != 15_360
                    || result.restarted_evaluations > A6C5_PAIRS
                    || result.train_samples != (TRAIN_STEPS * LANES) as u64
                    || result.validation_samples != (VALIDATION_STEPS * LANES) as u64
                {
                    return Err("A6-C5 fixed protocol mismatch".into());
                }
                let class_match = task.expected_class() == Some(result.proposed_class);
                println!(
                    "A6C5_FROZEN,task={},panel={},seed={},accepted_joint={},accepted_direct={},accepted_acyclic={},accepted_coordinate={},accepted_restart={},topology={},expected_class={},class_match={},active_state_gates={},active_state_edges={},monomials={},train_joint_errors={},train_acyclic_errors={},train_direct_errors={},train_coordinate_errors={},train_restarted_errors={},val_joint_errors={},val_acyclic_errors={},val_direct_errors={},joint_evaluations={},acyclic_evaluations={},direct_evaluations={},coordinate_evaluations={},restart_evaluations={},restart_attempts={},gate0_inputs={:?},gate1_inputs={:?},fit_seconds={:.6}",
                    task.name(), panel.name(), seed,
                    result.accepted_joint, result.accepted_direct,
                    result.accepted_acyclic, result.accepted_coordinate,
                    result.accepted_restarted, result.proposed_class.name(),
                    task.expected_class().map_or("unsupported", FeedbackClass::name),
                    class_match,
                    result.active_gate_count, result.active_feedback_edges, result.monomials,
                    result.joint_train_errors, result.acyclic_train_errors,
                    result.direct_train_errors, result.coordinate_train_errors,
                    result.restarted_train_errors, result.joint_validation_errors,
                    result.acyclic_validation_errors, result.direct_validation_errors,
                    result.joint_evaluations, result.acyclic_evaluations,
                    result.direct_evaluations, result.coordinate_evaluations,
                    result.restarted_evaluations, result.restart_attempts,
                    result.proposed.gates()[0].inputs(),
                    result.proposed.gates()[1].inputs(), fit_seconds
                );
                frozen.push(Frozen { seed, task, panel, fit_seconds, result });
            }
        }
    }
    // Separate independent 512- and 4096-step sequences do not exist until
    // all 50 training and validation decisions have been made and frozen.
    println!("A6C5_ALL_FROZEN,true");
    println!("A6C5_TEST_HEADER,task,panel,seed,steps,samples,baseline_errors,direct_errors,acyclic_errors,coordinate_errors,restart_errors,joint_errors,feedback_clamped_errors,gate0_disabled_errors,fit_seconds");

    let mut totals = [[[[0u64; MODELS]; 2]; 2]; 5];
    let mut class_hits = [[0usize; 2]; 5];
    let mut joint_accepts = [[0usize; 2]; 5];
    let mut restart_wins = [[0usize; 2]; 5];
    let mut fit_time = [[0.0f64; 2]; 5];
    for entry in &frozen {
        let t = entry.task.index();
        let p = entry.panel.index();
        class_hits[t][p] += usize::from(entry.task.expected_class() == Some(entry.result.proposed_class));
        joint_accepts[t][p] += usize::from(entry.result.accepted_joint);
        restart_wins[t][p] += usize::from(entry.result.restarted_train_errors == entry.result.joint_train_errors);
        fit_time[t][p] += entry.fit_seconds;
        for (horizon, steps) in TEST_STEPS.iter().copied().enumerate() {
            let test = dataset(
                derive_seed(entry.seed, entry.task, entry.panel, 0xa6c5_4004 + steps as u64),
                steps, entry.task, entry.panel,
            );
            let zero = base_network();
            let error_values = [
                errors(&zero, &test)?,
                errors(&entry.result.direct, &test)?,
                errors(&entry.result.acyclic, &test)?,
                errors(&entry.result.coordinate, &test)?,
                errors(&entry.result.restarted, &test)?,
                errors(&entry.result.committed, &test)?,
                errors(&no_recurrent_edges(&entry.result.committed), &test)?,
            ];
            let upstream_disabled = errors(&disable_upstream(&entry.result.committed), &test)?;
            for (total, &value) in totals[t][p][horizon].iter_mut().zip(&error_values) {
                *total += value;
            }
            println!(
                "A6C5_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6}",
                entry.task.name(), entry.panel.name(), entry.seed, steps,
                steps * LANES, error_values[0], error_values[1], error_values[2],
                error_values[3], error_values[4], error_values[5],
                error_values[6], upstream_disabled, entry.fit_seconds
            );
        }
    }
    for task in tasks {
        for panel in panels {
            let t = task.index();
            let p = panel.index();
            for (h, steps) in TEST_STEPS.iter().copied().enumerate() {
                let results = totals[t][p][h];
                let n = (seeds * steps * LANES) as f64;
                println!(
                    "A6C5_SUMMARY,task={},panel={},steps={},seeds={},baseline_error_rate={:.8},direct_error_rate={:.8},acyclic_error_rate={:.8},coordinate_error_rate={:.8},restart_error_rate={:.8},joint_error_rate={:.8},feedback_clamped_error_rate={:.8},class_hits={},joint_accepts={},restart_equal_train={},mean_fit_seconds={:.6}",
                    task.name(), panel.name(), steps, seeds,
                    results[0] as f64 / n, results[1] as f64 / n,
                    results[2] as f64 / n, results[3] as f64 / n,
                    results[4] as f64 / n, results[5] as f64 / n,
                    results[6] as f64 / n,
                    class_hits[t][p], joint_accepts[t][p],
                    restart_wins[t][p], fit_time[t][p] / seeds as f64
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_generators_preserve_clean_and_independent_noisy_inputs() {
        for task in [
            Task::Direct, Task::Lag1, Task::RunningParity,
            Task::Stride2Parity, Task::Stride3Negative,
        ] {
            let clean = dataset(19, 64, task, Panel::Clean);
            let noisy = dataset(19, 64, task, Panel::Noisy);
            assert_eq!(clean.blocks[0].inputs, noisy.blocks[0].inputs);
            assert_ne!(clean.blocks[0].targets, noisy.blocks[0].targets);
        }
    }

    #[test]
    fn three_stride_periods_are_distinct_on_independent_stream() {
        let a = dataset(29, 64, Task::RunningParity, Panel::Clean);
        let b = dataset(29, 64, Task::Stride2Parity, Panel::Clean);
        let c = dataset(29, 64, Task::Stride3Negative, Panel::Clean);
        assert_ne!(a.blocks[0].targets, b.blocks[0].targets);
        assert_ne!(b.blocks[0].targets, c.blocks[0].targets);
    }

    #[test]
    fn zeroing_feedback_is_a_real_native_anf_ablation() {
        let model = base_network();
        let task = Task::RunningParity;
        let result = fit_recurrent_cycles(
            &model,
            &dataset(31, 32, task, Panel::Clean),
            &dataset(32, 32, task, Panel::Clean),
            &spec(),
            333,
        ).unwrap();
        let test = dataset(33, 256, task, Panel::Clean);
        assert_eq!(errors(&result.committed, &test).unwrap(), 0);
        assert!(errors(&no_recurrent_edges(&result.committed), &test).unwrap() > 0);
    }
}

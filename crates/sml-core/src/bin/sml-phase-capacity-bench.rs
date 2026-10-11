//! A6-C5b: capacity ablation of terminal-only ANF recurrent phase memory.
//!
//! Usage: sml-phase-capacity-bench [seeds:1..=5].
//!
//! Predeclared tasks (the learner NEVER sees this task identity):
//!  direct:   y(t) = x0(t) XOR x1(t)                (no memory)
//!  stride-D: y(t) = x0(t) XOR XOR_{j>=0} x1(t-j*D)
//!  D = 1,2,3,4 representable by D state bits;
//!  D = 5 is NOT representable exactly with <=4 state bits.
//!
//! All gates are trained from terminal y labels, including the root two-input
//! Boolean function, feedback connection, input bit and EACH delay-gate unary
//! truth table. The delay ring itself is a FIXED scaffold. Models trained at
//! state widths 1/2/3/4 are compared against the independent A6-C5
//! unrestricted two-state, two-input-gate search as a stronger 2-bit control.
//!
//! 6 tasks * 2 clean/noise panels * 5 seeds = 60 train+validation decisions.
//! All 60 decisions are frozen BEFORE test data is even generated.
//! Independent test streams at 512 and 4096 steps (64 parallel lanes).
//!
//! Output statistics are *synthetic Boolean bit-error rates*, never language
//! quality or model perplexity. There is no NVIDIA-specific software.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_constructive::residual_output_errors,
    anf_credit_cycles::{fit_recurrent_cycles, CycleReport, CycleSpec},
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
    anf_phase_capacity::{fit_phase_memory, native_errors, PhaseReport, PhaseSpec},
    toy_tasks::SplitMix64,
};

const INPUT_BITS: usize = 5;
const LANES: usize = 64;
const TRAIN_STEPS: usize = 64;
const VALIDATION_STEPS: usize = 64;
const TEST_STEPS: [usize; 2] = [512, 4096];
const TASKS: usize = 6;
const PANELS: usize = 2;
const HORIZONS: usize = 2;
const MODELS: usize = 7;

#[derive(Debug, Clone, Copy)]
enum Task {
    Direct,
    Stride1,
    Stride2,
    Stride3,
    Stride4,
    Stride5Control,
}
impl Task {
    fn name(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Stride1 => "stride1",
            Self::Stride2 => "stride2",
            Self::Stride3 => "stride3",
            Self::Stride4 => "stride4",
            Self::Stride5Control => "stride5_negative",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Direct => 0,
            Self::Stride1 => 1,
            Self::Stride2 => 2,
            Self::Stride3 => 3,
            Self::Stride4 => 4,
            Self::Stride5Control => 5,
        }
    }
    fn phase(self) -> usize {
        self.index()
    }
    fn exact_width(self) -> Option<usize> {
        if self.index() == 5 {
            None
        } else {
            Some(self.index().max(1))
        }
    }
    fn expected_period(self) -> usize {
        self.phase()
    }
}
const ALL_TASKS: [Task; TASKS] = [
    Task::Direct,
    Task::Stride1,
    Task::Stride2,
    Task::Stride3,
    Task::Stride4,
    Task::Stride5Control,
];

#[derive(Debug, Clone, Copy)]
enum Panel {
    Clean,
    Noise,
}
impl Panel {
    fn name(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Noise => "noise_1_in_16",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Clean => 0,
            Self::Noise => 1,
        }
    }
}
const ALL_PANELS: [Panel; PANELS] = [Panel::Clean, Panel::Noise];

fn spec() -> PhaseSpec {
    PhaseSpec {
        raw_candidates: vec![1, 2, 3, 4],
        baseline_input: 0,
        target_bit: 0,
    }
}
fn cycle_spec() -> CycleSpec {
    CycleSpec {
        raw_candidates: vec![1, 2, 3, 4],
        baseline_input: 0,
        target_bit: 0,
    }
}
fn original_two_state_initial() -> LocalAnfNetwork {
    let n = 2;
    let width = n + INPUT_BITS;
    let gates = vec![
        LocalAnfGate::zero(vec![n + 1, n + 2], width).unwrap(),
        LocalAnfGate::zero(vec![n + 1, n + 2], width).unwrap(),
    ];
    let readout = LocalAnfGate::zero(vec![n], width).unwrap();
    LocalAnfNetwork::with_input_bits(n, INPUT_BITS, gates, readout).unwrap()
}
fn seed(seed: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (seed as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

/// Bit-sliced independent synthetic streams. Initial phase-memory state is
/// exactly zero for every lane at the start of each new sequence.
fn dataset(seed: u64, steps: usize, task: Task, panel: Panel) -> LaneData {
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut phases = [0u64; 6];
    let mut targets = vec![0u64; steps];
    for (t, target) in targets.iter_mut().enumerate() {
        let signal = inputs[t * INPUT_BITS + 1];
        let residual = if task.phase() == 0 {
            signal
        } else {
            phases[t % task.phase()] ^= signal;
            phases[t % task.phase()]
        };
        let noise = if matches!(panel, Panel::Noise) {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        *target = inputs[t * INPUT_BITS] ^ residual ^ noise;
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

fn baseline_errors(data: &LaneData) -> u64 {
    data.blocks
        .iter()
        .map(|block| {
            (0..block.steps)
                .map(|t| {
                    let residual = block.targets[t] ^ block.inputs[t * INPUT_BITS];
                    u64::from((residual & block.valid).count_ones())
                })
                .sum::<u64>()
        })
        .sum()
}
fn old_two_bit_errors(model: &LocalAnfNetwork, data: &LaneData) -> Result<u64, Box<dyn Error>> {
    Ok(residual_output_errors(model, data, 1, 0, 0)?.0)
}

struct Frozen {
    seed: usize,
    task: Task,
    panel: Panel,
    phase: PhaseReport,
    old_two_bit: CycleReport,
    phase_fit_seconds: f64,
    old_fit_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-phase-capacity-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |value| value.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seeds must be in 1..=5".into());
    }
    println!(
        "A6C5B_CONFIG,seeds={seeds},tasks=direct;stride1;stride2;stride3;stride4;stride5_negative,panels=clean;noise_1_in_16,train_steps={TRAIN_STEPS},validation_steps={VALIDATION_STEPS},test_steps=512;4096,lanes={LANES},state_widths=1;2;3;4"
    );
    println!(
        "A6C5B_PROTOCOL,terminal_labels_only=true,train_only_gate_and_feedback_selection=true,validation_only_width_selection=true,test_generated_after_all_frozen=true,old_A6C5_two_bit_full_grammar_control=true"
    );
    println!(
        "A6C5B_BUDGET,ring_candidates_width1=64,width2=512,width3=3072,width4=16384,total_ring=20032,original_A6C5_two_bit_joint=57600"
    );

    let mut frozen = Vec::with_capacity(TASKS * PANELS * seeds);
    for task in ALL_TASKS {
        for panel in ALL_PANELS {
            for index in 0..seeds {
                let train = dataset(
                    seed(index, task, panel, 0xa6c5_b101),
                    TRAIN_STEPS,
                    task,
                    panel,
                );
                let validation = dataset(
                    seed(index, task, panel, 0xa6c5_b202),
                    VALIDATION_STEPS,
                    task,
                    panel,
                );
                let clock = Instant::now();
                let phase = fit_phase_memory(&train, &validation, &spec(), 4)?;
                let phase_fit_seconds = clock.elapsed().as_secs_f64();

                let clock = Instant::now();
                let old_two_bit = fit_recurrent_cycles(
                    &original_two_state_initial(),
                    &train,
                    &validation,
                    &cycle_spec(),
                    seed(index, task, panel, 0xa6c5_b303),
                )?;
                let old_fit_seconds = clock.elapsed().as_secs_f64();

                if phase.objective_evaluations != 20_032
                    || phase.train_samples != (TRAIN_STEPS * LANES) as u64
                    || phase.validation_samples != (VALIDATION_STEPS * LANES) as u64
                    || old_two_bit.joint_evaluations != 57_600
                {
                    return Err("A6-C5b preregistered protocol mismatch".into());
                }
                let width_train: Vec<String> = phase
                    .widths
                    .iter()
                    .map(|v| v.train_errors.to_string())
                    .collect();
                let width_val: Vec<String> = phase
                    .widths
                    .iter()
                    .map(|v| v.validation_errors.to_string())
                    .collect();
                println!(
                    "A6C5B_FROZEN,task={},panel={},seed={},selected_width={},selected_period={},expected_period={},width_match={},accepted={},selected_feedback={},selected_raw={},selected_root_truth={},selected_delay_masks={:?},selected_anf_terms={},train_errors_per_width={},val_errors_per_width={},train_selected_errors={},val_selected_errors={},old_two_bit_train_errors={},old_two_bit_val_errors={},ring_evaluations={},old_joint_evaluations={},phase_fit_seconds={:.6},old_fit_seconds={:.6}",
                    task.name(), panel.name(), index,
                    phase.selected_width,
                    phase.selected_effective_period,
                    task.expected_period(),
                    task.exact_width() == Some(phase.selected_width)
                        && phase.selected_effective_period == task.expected_period(),
                    phase.accepted, phase.selected_rule.feedback,
                    phase.selected_rule.raw, phase.selected_rule.truth,
                    phase.selected_rule.delays,
                    phase.widths[phase.selected_width.max(1) - 1].anf_nonzero_monomials,
                    width_train.join(";"), width_val.join(";"),
                    phase.selected_train_errors, phase.selected_validation_errors,
                    old_two_bit.joint_train_errors,
                    old_two_bit.joint_validation_errors,
                    phase.objective_evaluations, old_two_bit.joint_evaluations,
                    phase_fit_seconds, old_fit_seconds
                );
                frozen.push(Frozen {
                    seed: index,
                    task,
                    panel,
                    phase,
                    old_two_bit,
                    phase_fit_seconds,
                    old_fit_seconds,
                });
            }
        }
    }
    // Leakage barrier: no test data has been generated until ALL train and
    // validation decisions across the 60 cases are frozen.
    println!("A6C5B_ALL_FROZEN,true");
    println!(
        "A6C5B_TEST_HEADER,task,panel,seed,steps,tokens,baseline_errors,original2_errors,width1_errors,width2_errors,width3_errors,width4_errors,adaptive_errors,selected_width,selected_period,phase_fit_seconds,old_fit_seconds"
    );

    let mut totals = [[[[0u64; MODELS]; HORIZONS]; PANELS]; TASKS];
    let mut width_matches = [[0usize; PANELS]; TASKS];
    let mut acceptances = [[0usize; PANELS]; TASKS];
    let mut fit_secs = [[0f64; PANELS]; TASKS];
    let mut old_secs = [[0f64; PANELS]; TASKS];

    for case in &frozen {
        let task_index = case.task.index();
        let panel_index = case.panel.index();
        width_matches[task_index][panel_index] += usize::from(
            case.task.exact_width() == Some(case.phase.selected_width)
                && case.phase.selected_effective_period == case.task.expected_period(),
        );
        acceptances[task_index][panel_index] += usize::from(case.phase.accepted);
        fit_secs[task_index][panel_index] += case.phase_fit_seconds;
        old_secs[task_index][panel_index] += case.old_fit_seconds;
        for (h, length) in TEST_STEPS.iter().copied().enumerate() {
            let data = dataset(
                seed(
                    case.seed,
                    case.task,
                    case.panel,
                    0xa6c5_b404 + length as u64,
                ),
                length,
                case.task,
                case.panel,
            );
            let values = [
                baseline_errors(&data),
                old_two_bit_errors(&case.old_two_bit.committed, &data)?,
                native_errors(&case.phase.widths[0].network, &data, &spec())?,
                native_errors(&case.phase.widths[1].network, &data, &spec())?,
                native_errors(&case.phase.widths[2].network, &data, &spec())?,
                native_errors(&case.phase.widths[3].network, &data, &spec())?,
                native_errors(&case.phase.selected, &data, &spec())?,
            ];
            for (sum, value) in totals[task_index][panel_index][h].iter_mut().zip(values) {
                *sum += value;
            }
            println!(
                "A6C5B_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6}",
                case.task.name(),
                case.panel.name(),
                case.seed,
                length,
                length * LANES,
                values[0],
                values[1],
                values[2],
                values[3],
                values[4],
                values[5],
                values[6],
                case.phase.selected_width,
                case.phase.selected_effective_period,
                case.phase_fit_seconds,
                case.old_fit_seconds
            );
        }
    }

    for task in ALL_TASKS {
        for panel in ALL_PANELS {
            for (h, length) in TEST_STEPS.iter().copied().enumerate() {
                let t = task.index();
                let p = panel.index();
                let errors = totals[t][p][h];
                let denominator = (seeds * length * LANES) as f64;
                println!(
                    "A6C5B_SUMMARY,task={},panel={},steps={},seeds={},baseline_error_rate={:.8},old_two_bit_error_rate={:.8},width1_error_rate={:.8},width2_error_rate={:.8},width3_error_rate={:.8},width4_error_rate={:.8},adaptive_error_rate={:.8},exact_width_and_period_matches={},adaptive_accepts={},mean_ring_fit_seconds={:.6},mean_old2_fit_seconds={:.6}",
                    task.name(), panel.name(), length, seeds,
                    errors[0] as f64 / denominator,
                    errors[1] as f64 / denominator,
                    errors[2] as f64 / denominator,
                    errors[3] as f64 / denominator,
                    errors[4] as f64 / denominator,
                    errors[5] as f64 / denominator,
                    errors[6] as f64 / denominator,
                    width_matches[t][p],
                    acceptances[t][p],
                    fit_secs[t][p] / seeds as f64,
                    old_secs[t][p] / seeds as f64
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
    fn stride_generator_has_the_correct_terminal_target_on_each_step() {
        for task in ALL_TASKS {
            let data = dataset(27, 24, task, Panel::Clean);
            let block = &data.blocks[0];
            let mut parity = [0u64; 6];
            for t in 0..block.steps {
                let source = block.inputs[t * INPUT_BITS + 1];
                let residual = if task.phase() == 0 {
                    source
                } else {
                    parity[t % task.phase()] ^= source;
                    parity[t % task.phase()]
                };
                assert_eq!(block.targets[t] ^ block.inputs[t * INPUT_BITS], residual);
            }
        }
    }

    #[test]
    fn clean_and_noisy_panels_have_identical_raw_inputs() {
        for task in ALL_TASKS {
            let clean = dataset(41, 64, task, Panel::Clean);
            let noisy = dataset(41, 64, task, Panel::Noise);
            assert_eq!(clean.blocks[0].inputs, noisy.blocks[0].inputs);
            assert_ne!(clean.blocks[0].targets, noisy.blocks[0].targets);
        }
    }

    #[test]
    fn old_full_two_state_cycle_cannot_learn_exact_stride_three() {
        let task = Task::Stride3;
        let train = dataset(7, 64, task, Panel::Clean);
        let validation = dataset(8, 64, task, Panel::Clean);
        let old = fit_recurrent_cycles(
            &original_two_state_initial(),
            &train,
            &validation,
            &cycle_spec(),
            0xabc,
        )
        .unwrap();
        assert!(old.joint_train_errors > 0);
    }
}

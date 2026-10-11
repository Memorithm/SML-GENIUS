//! A6-C7: learn ONE hidden Boolean gate in a 128/256-state-bit network.
//!
//! Seven preregistered tasks, with only final-output supervision:
//! - identity baseline at width 128,
//! - XOR injection 128/j45/raw2,
//! - AND injection 128/j92/raw3,
//! - XOR injection 256/j1/raw2 (maximal hidden-to-output delay),
//! - XOR injection 256/j127/raw2,
//! - AND injection 256/j192/raw3,
//! - negative: TWO distinct XOR injections 128/j31/raw2 and j88/raw3,
//!   intentionally unrepresentable in the learner's 1-mutation grammar.
//!
//! Five independent seeds per task/panel; clean and Bernoulli(1/16) label
//! noise. Training/validation each 512 steps x64 lanes; after ALL 70
//! models are FROZEN, independent test streams of 4096 and 16384 steps.
//! Only labels y(t), not hidden state labels, are given to the learner.
//!
//! The teacher generator executes an actual native LocalAnfNetwork and
//! does not share its gate rules with the learner's input API.
//! This is not NLP prediction quality or a free 256-gate learning result.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_intermediate_credit::{
        fit_intermediate_gate, native_errors, native_model, score_rule,
        IntermediateReport, IntermediateRule,
        A6C7_INPUT_BITS, A6C7_MAX_WIDTH,
    },
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const TRAIN_STEPS: usize = 512;
const VALIDATION_STEPS: usize = 512;
const HORIZONS: [usize; 2] = [4096, 16384];
const LANES: usize = 64;
const TASKS: usize = 7;
const PANELS: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Identity128,
    Xor128Mid,
    And128Late,
    Xor256Deep,
    Xor256Mid,
    And256Late,
    DoubleXor128Negative,
}

impl Task {
    const ALL: [Self; TASKS] = [
        Self::Identity128, Self::Xor128Mid, Self::And128Late,
        Self::Xor256Deep, Self::Xor256Mid, Self::And256Late,
        Self::DoubleXor128Negative,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Identity128 => "identity128",
            Self::Xor128Mid => "xor128_j45",
            Self::And128Late => "and128_j92",
            Self::Xor256Deep => "xor256_j1",
            Self::Xor256Mid => "xor256_j127",
            Self::And256Late => "and256_j192",
            Self::DoubleXor128Negative => "negative_two_xor128",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Identity128 => 0,
            Self::Xor128Mid => 1,
            Self::And128Late => 2,
            Self::Xor256Deep => 3,
            Self::Xor256Mid => 4,
            Self::And256Late => 5,
            Self::DoubleXor128Negative => 6,
        }
    }
    fn width(self) -> usize {
        match self {
            Self::Xor256Deep | Self::Xor256Mid | Self::And256Late => 256,
            _ => 128,
        }
    }
    /// These are used ONLY in the independent teacher/data generator.
    fn teacher_first(self) -> Option<IntermediateRule> {
        match self {
            Self::Identity128 => None,
            Self::Xor128Mid => Some(IntermediateRule { gate: 45, raw: 2, truth: 6 }),
            Self::And128Late => Some(IntermediateRule { gate: 92, raw: 3, truth: 8 }),
            Self::Xor256Deep => Some(IntermediateRule { gate: 1, raw: 2, truth: 6 }),
            Self::Xor256Mid => Some(IntermediateRule { gate: 127, raw: 2, truth: 6 }),
            Self::And256Late => Some(IntermediateRule { gate: 192, raw: 3, truth: 8 }),
            Self::DoubleXor128Negative => Some(IntermediateRule { gate: 31, raw: 2, truth: 6 }),
        }
    }
    fn teacher_second(self) -> Option<IntermediateRule> {
        match self {
            Self::DoubleXor128Negative => Some(IntermediateRule { gate: 88, raw: 3, truth: 6 }),
            _ => None,
        }
    }
    fn is_representable(self) -> bool {
        self != Self::DoubleXor128Negative
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel { Clean, LabelNoise }
impl Panel {
    const ALL: [Self; PANELS] = [Self::Clean, Self::LabelNoise];
    fn name(self) -> &'static str {
        match self { Self::Clean => "clean", Self::LabelNoise => "noise_1_in_16" }
    }
    fn index(self) -> usize {
        match self { Self::Clean => 0, Self::LabelNoise => 1 }
    }
}

fn derive_seed(seed: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (seed as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

fn teacher_network(task: Task) -> LocalAnfNetwork {
    let width = task.width();
    let mut network = native_model(width, task.teacher_first()).unwrap();
    if let Some(extra) = task.teacher_second() {
        let truth: [bool; 4] = std::array::from_fn(|i| extra.truth & (1 << i) != 0);
        network.gates_mut()[extra.gate] =
            LocalAnfGate::from_truth_table(
                vec![extra.gate - 1, width + extra.raw],
                &truth,
                width + A6C7_INPUT_BITS,
            ).unwrap();
    }
    network
}

fn dataset(seed: u64, steps: usize, task: Task, panel: Panel) -> LaneData {
    let teacher = teacher_network(task);
    let width = task.width();
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * A6C7_INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut targets = vec![0u64; steps];
    let mut state = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; width];
    let mut products = vec![0u64; teacher.scratch_len()];
    for (t, target) in targets.iter_mut().enumerate() {
        let begin = t * A6C7_INPUT_BITS;
        state[width..].copy_from_slice(&inputs[begin..begin + A6C7_INPUT_BITS]);
        teacher.step_lanes(&mut state, &mut next, &mut products);
        let exact = teacher.readout().evaluate_lanes(&state, &mut products);
        let noise = if panel == Panel::LabelNoise {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else { 0 };
        *target = exact ^ noise;
    }
    LaneData {
        input_bits: A6C7_INPUT_BITS,
        target_bits: 1,
        blocks: vec![LaneBlock {
            steps,
            inputs,
            targets,
            valid: u64::MAX,
        }],
    }
}

struct Frozen {
    task: Task,
    panel: Panel,
    seed: usize,
    report: IntermediateReport,
    fit_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-intermediate-credit-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |raw| raw.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seed count must be 1..=5".into());
    }

    println!(
        "A6C7_CONFIG,seeds={seeds},tasks=identity128;xor128_j45;and128_j92;xor256_j1;xor256_j127;and256_j192;negative_two_xor128,panels=clean;noise_1_in_16,train_steps={TRAIN_STEPS},validation_steps={VALIDATION_STEPS},test_steps=4096;16384,lanes={LANES}"
    );
    println!(
        "A6C7_PROTOCOL,teacher_native_anf=true,terminal_labels_only=true,root_and_delay_backbone_fixed=true,one_hidden_gate_function_and_source_and_position_learned=true,train_only_gate_search=true,validation_only_commit=true,all_models_frozen_before_test_generation=true"
    );
    println!(
        "A6C7_BUDGET,width128_candidate_scores={},width256_candidate_scores={},ring_128_payload_bytes={},ring_256_payload_bytes={}",
        (128 - 1) * 4 * 16, (256 - 1) * 4 * 16, 128 * 8, 256 * 8
    );

    let mut frozen = Vec::with_capacity(TASKS * PANELS * seeds);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for seed in 0..seeds {
                let train = dataset(
                    derive_seed(seed, task, panel, 0xa6c7_1001),
                    TRAIN_STEPS, task, panel,
                );
                let val = dataset(
                    derive_seed(seed, task, panel, 0xa6c7_2002),
                    VALIDATION_STEPS, task, panel,
                );
                let start = Instant::now();
                let report = fit_intermediate_gate(&train, &val, task.width())?;
                let fit_seconds = start.elapsed().as_secs_f64();
                if report.candidate_evaluations != (task.width() - 1) * 4 * 16
                    || report.width != task.width()
                    || report.ring_payload_bytes != task.width() * 8
                    || report.train_samples != (TRAIN_STEPS * LANES) as u64
                    || report.validation_samples != (VALIDATION_STEPS * LANES) as u64
                {
                    return Err("A6-C7 protocol violated".into());
                }
                let match_rule = if task.is_representable() {
                    task.teacher_first() == report.proposed_rule
                } else {
                    false
                };
                let r = report.proposed_rule;
                println!(
                    "A6C7_FROZEN,task={},panel={},seed={},width={},representable={},accepted={},rule_match={},gate={},raw={},truth={},baseline_train_errors={},baseline_val_errors={},candidate_train_errors={},candidate_val_errors={},hypotheses={},ring_payload_bytes={},native_gate_count={},native_anf_coefficient_bits={},fit_seconds={:.6}",
                    task.name(), panel.name(), seed, task.width(),
                    task.is_representable(), report.accepted, match_rule,
                    r.map_or(0, |x| x.gate), r.map_or(0, |x| x.raw),
                    r.map_or(10, |x| x.truth),
                    report.baseline_train_errors, report.baseline_validation_errors,
                    report.proposed_train_errors, report.proposed_validation_errors,
                    report.candidate_evaluations, report.ring_payload_bytes,
                    report.committed.state_bits(), report.committed.learned_bits(),
                    fit_seconds
                );
                frozen.push(Frozen {
                    task, panel, seed, report, fit_seconds,
                });
            }
        }
    }

    println!("A6C7_ALL_FROZEN,true");
    println!(
        "A6C7_TEST_HEADER,task,panel,seed,width,steps,samples,baseline_errors,proposed_errors,committed_errors,ablated_identity_errors,committed_bitlane_per_second,fit_seconds"
    );

    let mut error_totals = [[[[0u64; 3]; 2]; PANELS]; TASKS];
    let mut hits = [[0usize; PANELS]; TASKS];
    let mut accepted = [[0usize; PANELS]; TASKS];
    let mut training_times = [[0f64; PANELS]; TASKS];
    let mut native_test_seconds = [[[0f64; 2]; PANELS]; TASKS];

    for entry in &frozen {
        let t = entry.task.index();
        let p = entry.panel.index();
        if entry.task.is_representable()
            && entry.report.proposed_rule == entry.task.teacher_first()
        {
            hits[t][p] += 1;
        }
        accepted[t][p] += usize::from(entry.report.accepted);
        training_times[t][p] += entry.fit_seconds;
        for (h, steps) in HORIZONS.iter().copied().enumerate() {
            let test = dataset(
                derive_seed(entry.seed, entry.task, entry.panel, 0xa6c7_3003 + steps as u64),
                steps, entry.task, entry.panel,
            );
            let baseline = native_errors(&entry.report.reference, &test)?;
            let proposed = native_errors(&entry.report.proposed, &test)?;
            let start = Instant::now();
            let committed = native_errors(&entry.report.committed, &test)?;
            let elapsed = start.elapsed().as_secs_f64().max(f64::EPSILON);
            // Independent bit-exact specialized ring oracle on the held-out
            // data; no fitting/selection is allowed at this stage.
            let mut scratch = vec![0u64; entry.task.width().min(A6C7_MAX_WIDTH)];
            let expected = score_rule(
                entry.task.width(),
                if entry.report.accepted { entry.report.proposed_rule } else { None },
                &test, &mut scratch,
            )?;
            if expected != committed {
                return Err("native ANF vs split-ring test loss mismatch".into());
            }
            let counts = [baseline, proposed, committed];
            for (total, count) in error_totals[t][p][h].iter_mut().zip(counts) {
                *total += count;
            }
            native_test_seconds[t][p][h] += elapsed;
            let throughput = (steps * LANES) as f64 / elapsed;
            println!(
                "A6C7_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{:.2},{:.6}",
                entry.task.name(), entry.panel.name(), entry.seed,
                entry.task.width(), steps, steps * LANES,
                baseline, proposed, committed, baseline,
                throughput, entry.fit_seconds,
            );
        }
    }

    for task in Task::ALL {
        for panel in Panel::ALL {
            let t = task.index();
            let p = panel.index();
            for (h, steps) in HORIZONS.iter().copied().enumerate() {
                let total = error_totals[t][p][h];
                let n = (seeds * steps * LANES) as f64;
                println!(
                    "A6C7_SUMMARY,task={},panel={},width={},steps={},seeds={},baseline_error_rate={:.8},proposed_error_rate={:.8},committed_error_rate={:.8},rule_matches={},accepted={},mean_fit_seconds={:.6},native_bitlane_per_second={:.2}",
                    task.name(), panel.name(), task.width(), steps, seeds,
                    total[0] as f64 / n, total[1] as f64 / n,
                    total[2] as f64 / n, hits[t][p], accepted[t][p],
                    training_times[t][p] / seeds as f64,
                    n / native_test_seconds[t][p][h],
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
    fn teacher_and_learner_native_network_agree_on_a_known_single_gate() {
        for task in [
            Task::Xor128Mid, Task::And128Late, Task::Xor256Deep,
            Task::Xor256Mid, Task::And256Late,
        ] {
            let teacher = teacher_network(task);
            let expected = native_model(task.width(), task.teacher_first()).unwrap();
            assert_eq!(teacher, expected);
            let test = dataset(91, 340, task, Panel::Clean);
            assert_eq!(native_errors(&expected, &test).unwrap(), 0);
        }
    }

    #[test]
    fn both_noise_panels_preserve_raw_input_stream() {
        for task in Task::ALL {
            let a = dataset(33, 512, task, Panel::Clean);
            let b = dataset(33, 512, task, Panel::LabelNoise);
            assert_eq!(a.blocks[0].inputs, b.blocks[0].inputs);
            let flips = a.blocks[0].targets.iter().zip(&b.blocks[0].targets)
                .map(|(&x, &y)| (x ^ y).count_ones() as u64).sum::<u64>();
            assert!(flips > 1_000 && flips < 3_300);
        }
    }

    #[test]
    fn two_injections_cannot_be_recovered_by_one_hidden_gate() {
        let train = dataset(111, 512, Task::DoubleXor128Negative, Panel::Clean);
        let val = dataset(222, 512, Task::DoubleXor128Negative, Panel::Clean);
        let fitted = fit_intermediate_gate(&train, &val, 128).unwrap();
        assert!(fitted.proposed_train_errors > 0);
        let independent = dataset(333, 1024, Task::DoubleXor128Negative, Panel::Clean);
        assert!(native_errors(&fitted.committed, &independent).unwrap() > 0);
    }
}

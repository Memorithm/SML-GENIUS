//! A6-C8: paired zero-marginal ANF hidden-gate credit at width 128/256.
//!
//! Data generator hides native ANF gate functions; learner receives only
//! 5 raw input bitplanes and final label bitplanes. One and two affine
//! XOR/XNOR internal gates are learned by train-only temporal innovation.
//! Controls: identity, previous A6-C7 full-truth single gate, new affine
//! one-gate, and new affine two-gate. Validation selects model count 0/1/2.
//!
//! Registered tasks (9) x clean/noise panels (2) x seeds (5), 512-step
//! independent train and validation streams. All 90 decisions are FROZEN
//! before independent 4096/16384-step tests are generated.
//! Two NEGATIVE tasks: three independent XOR gates, and one XOR + one
//! nonlinear AND gate; the new paired learner cannot exactly represent
//! either, and MUST report the resulting error without manual repair.
//!
//! This is a synthetic Boolean task experiment, NOT SML language quality.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_dual_hidden_credit::{
        fit_dual_hidden_credit, make_multi_gate_native, native_errors, DualCreditReport,
        A6C8_BEAM_WIDTH, A6C8_INPUT_BITS, A6C8_PROBE_STEPS,
    },
    anf_intermediate_credit::{fit_intermediate_gate, IntermediateReport, IntermediateRule},
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const LANES: usize = 64;
const TRAIN_STEPS: usize = 512;
const VALIDATION_STEPS: usize = 512;
const HORIZONS: [usize; 2] = [4096, 16384];
const TASK_COUNT: usize = 9;
const PANEL_COUNT: usize = 2;
const METRIC_COUNT: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Identity128,
    SingleXor128,
    DualXor128,
    DualXor256Deep,
    DualXor256Middle,
    DualXorXnor128,
    DualXorXnor256,
    TripleXor128Negative,
    MixedXorAnd128Negative,
}
impl Task {
    const ALL: [Self; TASK_COUNT] = [
        Self::Identity128,
        Self::SingleXor128,
        Self::DualXor128,
        Self::DualXor256Deep,
        Self::DualXor256Middle,
        Self::DualXorXnor128,
        Self::DualXorXnor256,
        Self::TripleXor128Negative,
        Self::MixedXorAnd128Negative,
    ];
    fn index(self) -> usize {
        match self {
            Self::Identity128 => 0,
            Self::SingleXor128 => 1,
            Self::DualXor128 => 2,
            Self::DualXor256Deep => 3,
            Self::DualXor256Middle => 4,
            Self::DualXorXnor128 => 5,
            Self::DualXorXnor256 => 6,
            Self::TripleXor128Negative => 7,
            Self::MixedXorAnd128Negative => 8,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Identity128 => "identity128",
            Self::SingleXor128 => "single_xor128",
            Self::DualXor128 => "dual_xor128",
            Self::DualXor256Deep => "dual_xor256_deep",
            Self::DualXor256Middle => "dual_xor256_middle",
            Self::DualXorXnor128 => "xor_xnor128",
            Self::DualXorXnor256 => "xor_xnor256",
            Self::TripleXor128Negative => "triple_xor128_negative",
            Self::MixedXorAnd128Negative => "xor_and128_negative",
        }
    }
    fn width(self) -> usize {
        match self {
            Self::DualXor256Deep | Self::DualXor256Middle | Self::DualXorXnor256 => 256,
            _ => 128,
        }
    }
    fn teacher_gates(self) -> Vec<IntermediateRule> {
        let rule = |gate, raw, truth| IntermediateRule { gate, raw, truth };
        match self {
            Self::Identity128 => Vec::new(),
            Self::SingleXor128 => vec![rule(45, 2, 6)],
            Self::DualXor128 => vec![rule(31, 2, 6), rule(88, 3, 6)],
            Self::DualXor256Deep => vec![rule(1, 2, 6), rule(191, 3, 6)],
            Self::DualXor256Middle => vec![rule(57, 2, 6), rule(127, 3, 6)],
            Self::DualXorXnor128 => vec![rule(37, 2, 6), rule(93, 3, 9)],
            Self::DualXorXnor256 => vec![rule(1, 2, 6), rule(190, 3, 9)],
            Self::TripleXor128Negative => vec![rule(19, 2, 6), rule(67, 3, 6), rule(108, 4, 6)],
            Self::MixedXorAnd128Negative => vec![rule(31, 2, 6), rule(88, 3, 8)],
        }
    }
    fn positive(self) -> bool {
        !matches!(
            self,
            Self::TripleXor128Negative | Self::MixedXorAnd128Negative
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Clean,
    LabelNoise,
}
impl Panel {
    const ALL: [Self; PANEL_COUNT] = [Self::Clean, Self::LabelNoise];
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

fn derive_seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

/// Causal native teacher. Gate labels/functions are NOT part of the learner's
/// input: only the bit-sliced raw inputs and final labels are returned.
fn dataset(seed: u64, task: Task, panel: Panel, steps: usize) -> LaneData {
    let width = task.width();
    let teacher = make_multi_gate_native(width, &task.teacher_gates()).unwrap();
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * A6C8_INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut state = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; width];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    for (t, label) in targets.iter_mut().enumerate() {
        let from = t * A6C8_INPUT_BITS;
        state[width..].copy_from_slice(&inputs[from..from + A6C8_INPUT_BITS]);
        teacher.step_lanes(&mut state, &mut next, &mut scratch);
        let exact = teacher.readout().evaluate_lanes(&state, &mut scratch);
        let noise = if panel == Panel::LabelNoise {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        *label = exact ^ noise;
    }
    LaneData {
        input_bits: A6C8_INPUT_BITS,
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
    new: DualCreditReport,
    old: IntermediateReport,
    new_fit_seconds: f64,
    old_fit_seconds: f64,
}
fn expected_two_rules(task: Task) -> Option<[IntermediateRule; 2]> {
    let rules = task.teacher_gates();
    if task.positive() && rules.len() == 2 {
        Some([rules[0], rules[1]])
    } else {
        None
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-dual-hidden-credit-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |v| v.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seed count must be between 1 and 5".into());
    }

    println!(
        "A6C8_CONFIG,seeds={seeds},tasks=identity128;single_xor128;dual_xor128;dual_xor256_deep;dual_xor256_middle;xor_xnor128;xor_xnor256;triple_xor128_negative;xor_and128_negative,panels=clean;noise_1_in_16,train_steps={TRAIN_STEPS},validation_steps={VALIDATION_STEPS},test_steps=4096;16384,lanes={LANES}"
    );
    println!(
        "A6C8_PROTOCOL,terminal_labels_only=true,fixed_ring_backbone=true,affine_xor_xnor_gate_class=true,full_pair_probe_32_train_steps=true,top64_pair_beam=true,full_train_rescore=true,validation_only_model_count=true,old_A6C7_control=true,all_models_frozen_before_test_generation=true"
    );
    println!(
        "A6C8_BUDGET,width128_candidates=1016,width128_distinct_pairs=512064,width256_candidates=2040,width256_distinct_pairs=2072640,probe_steps={A6C8_PROBE_STEPS},beam={A6C8_BEAM_WIDTH}"
    );

    let mut frozen = Vec::with_capacity(TASK_COUNT * PANEL_COUNT * seeds);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..seeds {
                let training = dataset(
                    derive_seed(index, task, panel, 0xa6c8_1001),
                    task,
                    panel,
                    TRAIN_STEPS,
                );
                let validation = dataset(
                    derive_seed(index, task, panel, 0xa6c8_2002),
                    task,
                    panel,
                    VALIDATION_STEPS,
                );
                let start = Instant::now();
                let new = fit_dual_hidden_credit(&training, &validation, task.width())?;
                let new_fit_seconds = start.elapsed().as_secs_f64();
                let start = Instant::now();
                let old = fit_intermediate_gate(&training, &validation, task.width())?;
                let old_fit_seconds = start.elapsed().as_secs_f64();

                let expected_pairs = if task.width() == 128 {
                    512_064
                } else {
                    2_072_640
                };
                let expected_singles = if task.width() == 128 { 1016 } else { 2040 };
                if new.two_gate_pairs_probed != expected_pairs
                    || new.one_gate_candidates != expected_singles
                    || new.pairs_full_train_rescored != A6C8_BEAM_WIDTH
                    || new.train_samples != (TRAIN_STEPS * LANES) as u64
                    || new.validation_samples != (VALIDATION_STEPS * LANES) as u64
                {
                    return Err("A6-C8 registered training-count violation".into());
                }
                let correct_count = if task.positive() {
                    new.selected_count == task.teacher_gates().len()
                } else {
                    false
                };
                let exact_two =
                    expected_two_rules(task).is_some_and(|rules| new.selected_rules == rules);
                let exact_one =
                    task == Task::SingleXor128 && new.selected_rules == task.teacher_gates();
                println!(
                    "A6C8_FROZEN,task={},panel={},seed={},width={},representable={},selected_count={},selected_rules={:?},correct_count={},exact_two={},exact_one={},baseline_train_errors={},baseline_val_errors={},single_train_errors={},single_val_errors={},joint_train_errors={},joint_val_errors={},selected_train_errors={},selected_val_errors={},old_a6c7_train_errors={},old_a6c7_val_errors={},pair_probe_evaluations={},beam_rescores={},one_gate_candidates={},probe_payload_bytes={},fit_seconds={:.6},old_fit_seconds={:.6}",
                    task.name(), panel.name(), index, task.width(),
                    task.positive(), new.selected_count, new.selected_rules,
                    correct_count, exact_two, exact_one,
                    new.baseline_train_errors, new.baseline_validation_errors,
                    new.single_train_errors, new.single_validation_errors,
                    new.joint_train_errors, new.joint_validation_errors,
                    new.selected_train_errors, new.selected_validation_errors,
                    if old.accepted { old.proposed_train_errors } else { old.baseline_train_errors },
                    if old.accepted { old.proposed_validation_errors } else { old.baseline_validation_errors },
                    new.two_gate_pairs_probed, new.pairs_full_train_rescored,
                    new.one_gate_candidates, new.feature_probe_payload_bytes,
                    new_fit_seconds, old_fit_seconds
                );
                frozen.push(Frozen {
                    task,
                    panel,
                    seed: index,
                    new,
                    old,
                    new_fit_seconds,
                    old_fit_seconds,
                });
            }
        }
    }

    println!("A6C8_ALL_FROZEN,true");
    println!(
        "A6C8_TEST_HEADER,task,panel,seed,width,steps,samples,reference_errors,old_A6C7_errors,new_single_errors,new_joint_errors,new_selected_errors,remove_gate0_errors,remove_gate1_errors,selected_count,new_fit_seconds,old_fit_seconds"
    );

    let mut totals = [[[[0u64; METRIC_COUNT]; 2]; PANEL_COUNT]; TASK_COUNT];
    let mut selected_two = [[0usize; PANEL_COUNT]; TASK_COUNT];
    let mut pair_exact = [[0usize; PANEL_COUNT]; TASK_COUNT];
    let mut chosen_exact = [[0usize; PANEL_COUNT]; TASK_COUNT];
    let mut sum_new = [[0.0f64; PANEL_COUNT]; TASK_COUNT];
    let mut sum_old = [[0.0f64; PANEL_COUNT]; TASK_COUNT];

    for case in &frozen {
        let t = case.task.index();
        let p = case.panel.index();
        selected_two[t][p] += usize::from(case.new.selected_count == 2);
        pair_exact[t][p] += usize::from(
            expected_two_rules(case.task).is_some_and(|rules| case.new.selected_rules == rules),
        );
        chosen_exact[t][p] += usize::from(
            case.task.positive() && case.new.selected_rules == case.task.teacher_gates(),
        );
        sum_new[t][p] += case.new_fit_seconds;
        sum_old[t][p] += case.old_fit_seconds;
        for (h, steps) in HORIZONS.iter().copied().enumerate() {
            let test = dataset(
                derive_seed(case.seed, case.task, case.panel, 0xa6c8_3003 + steps as u64),
                case.task,
                case.panel,
                steps,
            );
            let reference = native_errors(&case.new.reference, &test)?;
            let old = native_errors(&case.old.committed, &test)?;
            let single = native_errors(&case.new.single_proposed, &test)?;
            let joint = native_errors(&case.new.joint_proposed, &test)?;
            let selected = native_errors(&case.new.selected, &test)?;

            let mut ablations = [selected; 2];
            for (i, count) in ablations.iter_mut().enumerate() {
                if case.new.selected_rules.len() > i {
                    let remaining = case
                        .new
                        .selected_rules
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| *idx != i)
                        .map(|(_, rule)| *rule)
                        .collect::<Vec<_>>();
                    let ablated = make_multi_gate_native(case.task.width(), &remaining)?;
                    *count = native_errors(&ablated, &test)?;
                }
            }
            let rates = [
                reference,
                old,
                single,
                joint,
                selected,
                ablations[0],
                ablations[1],
            ];
            for (total, value) in totals[t][p][h].iter_mut().zip(rates) {
                *total += value;
            }
            println!(
                "A6C8_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6}",
                case.task.name(),
                case.panel.name(),
                case.seed,
                case.task.width(),
                steps,
                steps * LANES,
                reference,
                old,
                single,
                joint,
                selected,
                ablations[0],
                ablations[1],
                case.new.selected_count,
                case.new_fit_seconds,
                case.old_fit_seconds
            );
        }
    }
    for task in Task::ALL {
        for panel in Panel::ALL {
            let t = task.index();
            let p = panel.index();
            for (h, steps) in HORIZONS.iter().copied().enumerate() {
                let value = totals[t][p][h];
                let n = (seeds * steps * LANES) as f64;
                println!(
                    "A6C8_SUMMARY,task={},panel={},width={},steps={},seeds={},baseline_error_rate={:.8},old_one_gate_error_rate={:.8},new_single_error_rate={:.8},new_joint_error_rate={:.8},selected_error_rate={:.8},ablate_first_error_rate={:.8},ablate_second_error_rate={:.8},selected_two={},pair_exact={},architecture_exact={},mean_joint_fit_seconds={:.6},mean_old_fit_seconds={:.6}",
                    task.name(), panel.name(), task.width(), steps, seeds,
                    value[0] as f64 / n, value[1] as f64 / n,
                    value[2] as f64 / n, value[3] as f64 / n,
                    value[4] as f64 / n, value[5] as f64 / n,
                    value[6] as f64 / n, selected_two[t][p],
                    pair_exact[t][p], chosen_exact[t][p],
                    sum_new[t][p] / seeds as f64,
                    sum_old[t][p] / seeds as f64
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
    fn target_teacher_and_hidden_rules_are_causally_consistent() {
        for task in Task::ALL {
            let inputs = dataset(50, task, Panel::Clean, 512);
            let teacher = make_multi_gate_native(task.width(), &task.teacher_gates()).unwrap();
            assert_eq!(native_errors(&teacher, &inputs).unwrap(), 0);
        }
    }

    #[test]
    fn clean_and_noisy_panels_have_identical_inputs() {
        for task in Task::ALL {
            let clean = dataset(51, task, Panel::Clean, 512);
            let noise = dataset(51, task, Panel::LabelNoise, 512);
            assert_eq!(clean.blocks[0].inputs, noise.blocks[0].inputs);
            assert_ne!(clean.blocks[0].targets, noise.blocks[0].targets);
        }
    }

    #[test]
    fn previously_failed_two_xor_128_is_solved_by_paired_search() {
        let task = Task::DualXor128;
        let train = dataset(52, task, Panel::Clean, 512);
        let val = dataset(53, task, Panel::Clean, 512);
        let trained = fit_dual_hidden_credit(&train, &val, 128).unwrap();
        assert_eq!(trained.selected_count, 2);
        assert_eq!(trained.selected_rules, task.teacher_gates());
        assert_eq!(trained.selected_train_errors, 0);
        assert_eq!(trained.selected_validation_errors, 0);
        let test = dataset(54, task, Panel::Clean, 1024);
        assert_eq!(native_errors(&trained.selected, &test).unwrap(), 0);
    }

    #[test]
    fn identity_retains_all_original_delay_gates() {
        let task = Task::Identity128;
        let train = dataset(55, task, Panel::Clean, 512);
        let val = dataset(56, task, Panel::Clean, 512);
        let result = fit_dual_hidden_credit(&train, &val, 128).unwrap();
        assert_eq!(result.selected_count, 0);
        assert_eq!(result.selected, result.reference);
    }
}

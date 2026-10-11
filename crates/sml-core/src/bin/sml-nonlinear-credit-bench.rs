//! A6-C9: end-task-only nonlinear two-gate credit in N=128/256 ANF state.
//!
//! Run: sml-nonlinear-credit-bench [seeds:1..=5].
//! 10 preregistered tasks, 2 clean/noisy panels, 5 independent seeds.
//! TRAIN and VALIDATION: 512 steps x64 lanes each, independently seeded.
//! All 100 models FROZEN BEFORE any heldout test generated.
//! TEST: independently seeded 4096 and 16384 steps x64 lanes (200 rows).
//! These are synthetic Boolean task errors, NOT language quality.
//!
//! Comparison: identity baseline, native A6-C7 single full-truth learner,
//! A6-C8 two-affine-gate learner, new conditional general two-gate learner,
//! validation-selected network, ablation of each of its two hidden gates.
//!
//! Teacher functions appear ONLY in this isolated fixture generator. They
//! are never supplied to the candidate search or validation APIs.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors},
    anf_intermediate_credit::IntermediateRule,
    anf_local_incremental::{LaneBlock, LaneData},
    anf_nonlinear_joint_credit::{
        fit_nonlinear_credit, second_candidate_count, CreditOrigin, NonlinearReport,
        A6C9_INPUT_BITS,
    },
    toy_tasks::SplitMix64,
};

const TRAIN_STEPS: usize = 512;
const VALIDATION_STEPS: usize = 512;
const HORIZONS: [usize; 2] = [4096, 16384];
const LANES: usize = 64;
const NUM_TASKS: usize = 10;
const NUM_PANELS: usize = 2;
const ERROR_METRICS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    Identity128,
    SingleAnd128,
    XorAnd128,
    AndXor128,
    AndAnd128,
    XorOr128,
    XorAnd256,
    AndAnd256,
    XorXor256,
    NegativeThree128,
}
impl Task {
    const ALL: [Self; NUM_TASKS] = [
        Self::Identity128,
        Self::SingleAnd128,
        Self::XorAnd128,
        Self::AndXor128,
        Self::AndAnd128,
        Self::XorOr128,
        Self::XorAnd256,
        Self::AndAnd256,
        Self::XorXor256,
        Self::NegativeThree128,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Identity128 => "identity128",
            Self::SingleAnd128 => "single_and128",
            Self::XorAnd128 => "xor_and128",
            Self::AndXor128 => "and_xor128",
            Self::AndAnd128 => "and_and128",
            Self::XorOr128 => "xor_or128",
            Self::XorAnd256 => "xor_and256",
            Self::AndAnd256 => "and_and256",
            Self::XorXor256 => "xor_xor256",
            Self::NegativeThree128 => "negative_three128",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::Identity128 => 0,
            Self::SingleAnd128 => 1,
            Self::XorAnd128 => 2,
            Self::AndXor128 => 3,
            Self::AndAnd128 => 4,
            Self::XorOr128 => 5,
            Self::XorAnd256 => 6,
            Self::AndAnd256 => 7,
            Self::XorXor256 => 8,
            Self::NegativeThree128 => 9,
        }
    }
    fn width(self) -> usize {
        match self {
            Self::XorAnd256 | Self::AndAnd256 | Self::XorXor256 => 256,
            _ => 128,
        }
    }
    fn teacher(self) -> Vec<IntermediateRule> {
        let gate = |position, raw, truth| IntermediateRule {
            gate: position,
            raw,
            truth,
        };
        match self {
            Self::Identity128 => vec![],
            Self::SingleAnd128 => vec![gate(92, 3, 8)],
            Self::XorAnd128 => vec![gate(31, 2, 6), gate(88, 3, 8)],
            Self::AndXor128 => vec![gate(33, 2, 8), gate(94, 3, 6)],
            Self::AndAnd128 => vec![gate(41, 2, 8), gate(87, 3, 8)],
            Self::XorOr128 => vec![gate(29, 2, 6), gate(105, 3, 14)],
            Self::XorAnd256 => vec![gate(1, 2, 6), gate(190, 3, 8)],
            Self::AndAnd256 => vec![gate(51, 2, 8), gate(181, 3, 8)],
            Self::XorXor256 => vec![gate(1, 2, 6), gate(191, 3, 6)],
            Self::NegativeThree128 => vec![gate(19, 2, 6), gate(67, 3, 8), gate(108, 4, 6)],
        }
    }
    fn representable(self) -> bool {
        self != Self::NegativeThree128
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    Clean,
    LabelNoise,
}
impl Panel {
    const ALL: [Self; NUM_PANELS] = [Self::Clean, Self::LabelNoise];
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
fn seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

fn dataset(seed: u64, task: Task, panel: Panel, steps: usize) -> LaneData {
    let width = task.width();
    let teacher = make_multi_gate_native(width, &task.teacher()).unwrap();
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * A6C9_INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut source = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; width];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    for (t, target) in targets.iter_mut().enumerate() {
        let begin = t * A6C9_INPUT_BITS;
        source[width..].copy_from_slice(&inputs[begin..begin + A6C9_INPUT_BITS]);
        teacher.step_lanes(&mut source, &mut next, &mut scratch);
        let exact = teacher.readout().evaluate_lanes(&source, &mut scratch);
        let noise = if panel == Panel::LabelNoise {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        *target = exact ^ noise;
    }
    LaneData {
        input_bits: A6C9_INPUT_BITS,
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
    seed: usize,
    task: Task,
    panel: Panel,
    report: NonlinearReport,
    fit_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-nonlinear-credit-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |s| s.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seed count must be 1..=5".into());
    }

    println!(
        "A6C9_CONFIG,seeds={seeds},tasks=identity128;single_and128;xor_and128;and_xor128;and_and128;xor_or128;xor_and256;and_and256;xor_xor256;negative_three128,panels=clean;noise_1_in_16,train_steps={TRAIN_STEPS},validation_steps={VALIDATION_STEPS},test_steps=4096;16384,lanes={LANES}"
    );
    println!(
        "A6C9_PROTOCOL,terminal_labels_only=true,full_16_truth_table_search=true,exact_3_segment_pair_oracle=true,conditional_anchor_A6C7=true,affine_pair_initializer_A6C8=true,one_train_coordinate_pass=true,validation_only_model_selection=true,all_models_frozen_before_test_generation=true"
    );
    println!(
        "A6C9_BUDGET,nonlinear_second_candidates_N128={},nonlinear_second_candidates_N256={},coordinate_axes=2,affine_pair_prior=true,strict_train_improvement_only=true",
        second_candidate_count(128).unwrap(),
        second_candidate_count(256).unwrap()
    );

    let mut frozen = Vec::with_capacity(NUM_TASKS * NUM_PANELS * seeds);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..seeds {
                let train = dataset(
                    seed(index, task, panel, 0xa6c9_1001),
                    task,
                    panel,
                    TRAIN_STEPS,
                );
                let validation = dataset(
                    seed(index, task, panel, 0xa6c9_2002),
                    task,
                    panel,
                    VALIDATION_STEPS,
                );
                let start = Instant::now();
                let report = fit_nonlinear_credit(&train, &validation, task.width())?;
                let fit_seconds = start.elapsed().as_secs_f64();
                let axes = if report.old_one.proposed_rule.is_some() { 8 } else { 4 };
                if report.train_samples != (TRAIN_STEPS * LANES) as u64
                    || report.validation_samples != (VALIDATION_STEPS * LANES) as u64
                    || report.width != task.width()
                    || report.coordinate_evaluations
                        != axes * second_candidate_count(task.width()).unwrap()
                    || report.ring_payload_bytes != task.width() * 8
                {
                    return Err("A6-C9 frozen protocol/count disagreement".into());
                }
                let teacher = task.teacher();
                let exact = task.representable() && report.selected_rules == teacher;
                let train_zero = report.selected_train_errors == 0;
                println!(
                    "A6C9_FROZEN,task={},panel={},seed={},width={},representable={},selected_origin={},selected_count={},selected_rules={:?},exact_rule_match={},train_zero_if_clean={},baseline_train_errors={},baseline_val_errors={},old_one_train_errors={},old_one_val_errors={},old_affine_train_errors={},old_affine_val_errors={},conditional_train_errors={:?},conditional_val_errors={:?},selected_train_errors={},selected_val_errors={},conditional_evaluations={},coordinate_evaluations={},coordinate_commits={},ring_payload_bytes={},fit_seconds={:.6}",
                    task.name(), panel.name(), index, task.width(),
                    task.representable(), report.selected_origin.name(),
                    report.selected_count, report.selected_rules, exact,
                    train_zero, report.baseline_train_errors,
                    report.baseline_validation_errors,
                    report.old_one.proposed_train_errors,
                    report.old_one.proposed_validation_errors,
                    report.old_affine.joint_train_errors,
                    report.old_affine.joint_validation_errors,
                    report.conditional_train_errors, report.conditional_validation_errors,
                    report.selected_train_errors, report.selected_validation_errors,
                    report.conditional_evaluations, report.coordinate_evaluations,
                    report.conditional_accepted_updates, report.ring_payload_bytes,
                    fit_seconds
                );
                frozen.push(Frozen {
                    seed: index,
                    task,
                    panel,
                    report,
                    fit_seconds,
                });
            }
        }
    }
    // Strict protocol: no independent test sample was generated before now.
    println!("A6C9_ALL_FROZEN,true");
    println!(
        "A6C9_TEST_HEADER,task,panel,seed,width,steps,samples,baseline_errors,old_one_errors,old_affine_errors,conditional_errors,selected_errors,ablate0_errors,ablate1_errors,selected_origin,selected_count,native_bitlane_per_second,fit_seconds"
    );

    let mut totals = [[[[0u64; ERROR_METRICS]; 2]; NUM_PANELS]; NUM_TASKS];
    let mut exact_matches = [[0usize; NUM_PANELS]; NUM_TASKS];
    let mut selected_counts = [[[0usize; 3]; NUM_PANELS]; NUM_TASKS];
    let mut origins = [[[0usize; 4]; NUM_PANELS]; NUM_TASKS];
    let mut fit_totals = [[0.0f64; NUM_PANELS]; NUM_TASKS];
    let mut native_seconds = [[[0.0f64; 2]; NUM_PANELS]; NUM_TASKS];
    for item in &frozen {
        let t = item.task.index();
        let p = item.panel.index();
        if item.task.representable() && item.report.selected_rules == item.task.teacher() {
            exact_matches[t][p] += 1;
        }
        selected_counts[t][p][item.report.selected_count] += 1;
        let origin_index = match item.report.selected_origin {
            CreditOrigin::Identity => 0,
            CreditOrigin::OneGate => 1,
            CreditOrigin::AffinePair => 2,
            CreditOrigin::NonlinearPair => 3,
        };
        origins[t][p][origin_index] += 1;
        fit_totals[t][p] += item.fit_seconds;
        for (h, steps) in HORIZONS.iter().copied().enumerate() {
            let test = dataset(
                seed(item.seed, item.task, item.panel, 0xa6c9_3003 + steps as u64),
                item.task,
                item.panel,
                steps,
            );
            let baseline = native_errors(&item.report.old_one.reference, &test)?;
            let old_one = native_errors(&item.report.old_one.committed, &test)?;
            let old_affine = native_errors(&item.report.old_affine.selected, &test)?;
            let conditional = if let Some(ref model) = item.report.conditional_proposed {
                native_errors(model, &test)?
            } else {
                baseline
            };
            let start = Instant::now();
            let chosen = native_errors(&item.report.selected, &test)?;
            let secs = start.elapsed().as_secs_f64().max(f64::EPSILON);
            native_seconds[t][p][h] += secs;
            let mut ablate = [chosen; 2];
            for (axis, count) in ablate.iter_mut().enumerate() {
                if item.report.selected_rules.len() > axis {
                    let rest = item
                        .report
                        .selected_rules
                        .iter()
                        .enumerate()
                        .filter(|(idx, _)| *idx != axis)
                        .map(|(_, rule)| *rule)
                        .collect::<Vec<_>>();
                    let model = make_multi_gate_native(item.task.width(), &rest)?;
                    *count = native_errors(&model, &test)?;
                }
            }
            let data = [
                baseline,
                old_one,
                old_affine,
                conditional,
                chosen,
                ablate[0],
                ablate[1],
            ];
            for (total, val) in totals[t][p][h].iter_mut().zip(data) {
                *total += val;
            }
            println!(
                "A6C9_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.2},{:.6}",
                item.task.name(),
                item.panel.name(),
                item.seed,
                item.task.width(),
                steps,
                steps * LANES,
                baseline,
                old_one,
                old_affine,
                conditional,
                chosen,
                ablate[0],
                ablate[1],
                item.report.selected_origin.name(),
                item.report.selected_count,
                (steps * LANES) as f64 / secs,
                item.fit_seconds
            );
        }
    }

    for task in Task::ALL {
        for panel in Panel::ALL {
            let t = task.index();
            let p = panel.index();
            for (h, steps) in HORIZONS.iter().copied().enumerate() {
                let sum = totals[t][p][h];
                let n = (seeds * steps * LANES) as f64;
                let rate = |v: u64| v as f64 / n;
                println!(
                    "A6C9_SUMMARY,task={},panel={},width={},steps={},seeds={},baseline_error_rate={:.8},old_one_error_rate={:.8},old_affine_error_rate={:.8},conditional_error_rate={:.8},selected_error_rate={:.8},ablate0_error_rate={:.8},ablate1_error_rate={:.8},exact_matches={},selected_zero={},selected_one={},selected_two={},origin_zero={},origin_one={},origin_affine_two={},origin_nonlinear_two={},mean_fit_seconds={:.6},native_bitlane_per_second={:.2}",
                    task.name(),panel.name(),task.width(),steps,seeds,
                    rate(sum[0]),rate(sum[1]),rate(sum[2]),rate(sum[3]),
                    rate(sum[4]),rate(sum[5]),rate(sum[6]),exact_matches[t][p],
                    selected_counts[t][p][0],selected_counts[t][p][1],
                    selected_counts[t][p][2],
                    origins[t][p][0],origins[t][p][1],
                    origins[t][p][2],origins[t][p][3],
                    fit_totals[t][p]/seeds as f64,
                    n/native_seconds[t][p][h],
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
    fn native_teacher_outputs_are_self_consistent_for_every_task() {
        for task in Task::ALL {
            let input = dataset(123, task, Panel::Clean, 300);
            let true_model = make_multi_gate_native(task.width(), &task.teacher()).unwrap();
            assert_eq!(native_errors(&true_model, &input).unwrap(), 0);
        }
    }

    #[test]
    fn label_noise_does_not_modify_raw_inputs() {
        for task in Task::ALL {
            let clean = dataset(234, task, Panel::Clean, 512);
            let noisy = dataset(234, task, Panel::LabelNoise, 512);
            assert_eq!(clean.blocks[0].inputs, noisy.blocks[0].inputs);
            assert_ne!(clean.blocks[0].targets, noisy.blocks[0].targets);
        }
    }

    #[test]
    fn affine_prior_is_retained_when_single_gate_has_zero_marginal_credit() {
        let task = Task::XorXor256;
        let train = dataset(345, task, Panel::Clean, 512);
        let val = dataset(456, task, Panel::Clean, 512);
        let fit = fit_nonlinear_credit(&train, &val, 256).unwrap();
        assert_eq!(fit.selected_count, 2);
        assert_eq!(fit.selected_train_errors, 0);
        assert_eq!(fit.selected_validation_errors, 0);
        let test = dataset(567, task, Panel::Clean, 1024);
        assert_eq!(native_errors(&fit.selected, &test).unwrap(), 0);
    }
}

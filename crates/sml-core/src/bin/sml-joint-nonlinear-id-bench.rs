//! A6-C10 — joint nonlinear ANF system identification in N=128/256 state.
//!
//! Observed TRAIN output is used as past autoregressive input to rank
//! joint 2-gate structures; NO hidden gate labels or future labels.
//! Native free-running ANF network must match score on TRAIN/VAL/TEST.
//!
//! 10 preregistered tasks x 2 clean/label-noise panels x 5 seeds.
//! All 100 train and validation model decisions frozen BEFORE the first
//! independent 4096/16384 step TEST stream is ever generated.
//!
//! Controls use same training/validation/test data: baseline, A6-C7,
//! A6-C8, A6-C9, two-gate A6-C10 proposal, validation-selected model,
//! individual gate ablations and 3-gate non-representable negative.
//! All values are synthetic Boolean bit-lane errors, NOT NLP metrics.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors},
    anf_intermediate_credit::IntermediateRule,
    anf_joint_nonlinear_system_id::{
        fit_joint_system_id, structure_count, JointIdReport, JointOrigin,
        A6C10_PROBE_STEPS, A6C10_STRUCTURAL_BEAM, A6C10_FACTORS_PER_STRUCTURE,
    },
    anf_local_incremental::{LaneBlock, LaneData},
    toy_tasks::SplitMix64,
};

const INPUT_BITS: usize = 5;
const TRAIN_STEPS: usize = 512;
const VALIDATION_STEPS: usize = 512;
const HORIZONS: [usize; 2] = [4096, 16384];
const LANES: usize = 64;
const NUM_TASKS: usize = 10;
const NUM_PANELS: usize = 2;
const ERRORS: usize = 8;

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
    fn teacher_gates(self) -> Vec<IntermediateRule> {
        let rule = |gate, raw, truth| IntermediateRule { gate, raw, truth };
        match self {
            Self::Identity128 => vec![],
            Self::SingleAnd128 => vec![rule(92, 3, 8)],
            Self::XorAnd128 => vec![rule(31, 2, 6), rule(88, 3, 8)],
            Self::AndXor128 => vec![rule(33, 2, 8), rule(94, 3, 6)],
            Self::AndAnd128 => vec![rule(41, 2, 8), rule(87, 3, 8)],
            Self::XorOr128 => vec![rule(29, 2, 6), rule(105, 3, 14)],
            Self::XorAnd256 => vec![rule(1, 2, 6), rule(190, 3, 8)],
            Self::AndAnd256 => vec![rule(51, 2, 8), rule(181, 3, 8)],
            Self::XorXor256 => vec![rule(1, 2, 6), rule(191, 3, 6)],
            Self::NegativeThree128 => vec![
                rule(19, 2, 6),
                rule(67, 3, 8),
                rule(108, 4, 6),
            ],
        }
    }
    fn representable(self) -> bool { self != Self::NegativeThree128 }
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

fn derive_seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

fn dataset(seed: u64, task: Task, panel: Panel, steps: usize) -> LaneData {
    let width = task.width();
    let teacher = make_multi_gate_native(width, &task.teacher_gates()).unwrap();
    let mut rng = SplitMix64::new(seed);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    let mut source = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; width];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    for (t, label) in targets.iter_mut().enumerate() {
        let offset = t * INPUT_BITS;
        source[width..].copy_from_slice(&inputs[offset..offset+INPUT_BITS]);
        teacher.step_lanes(&mut source, &mut next, &mut scratch);
        let exact = teacher.readout().evaluate_lanes(&source, &mut scratch);
        let noise = if panel == Panel::LabelNoise {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        } else {
            0
        };
        *label = exact ^ noise;
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

struct Frozen {
    seed: usize,
    task: Task,
    panel: Panel,
    report: JointIdReport,
    fit_seconds: f64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-joint-nonlinear-id-bench [seeds:1..=5]".into());
    }
    let seeds: usize = args.get(1).map_or(Ok(5), |arg| arg.parse())?;
    if !(1..=5).contains(&seeds) {
        return Err("seeds must be 1..=5".into());
    }

    println!(
        "A6C10_CONFIG,seeds={seeds},tasks=identity128;single_and128;xor_and128;and_xor128;and_and128;xor_or128;xor_and256;and_and256;xor_xor256;negative_three128,panels=clean;noise_1_in_16,train_steps={TRAIN_STEPS},val_steps={VALIDATION_STEPS},test_steps=4096;16384,lanes={LANES}"
    );
    println!(
        "A6C10_PROTOCOL,terminal_labels_only=true,teacher_forced_past_output_TRAIN_ONLY=true,autoregressive_target_no_hidden_supervision=true,probe_64_late_train_steps=true,beam_64_structures=true,full_train_8bin_truth_factorization=true,top4_factors_per_structure=true,autonomous_native_train_score=true,validation_only_model_selection=true,all_frozen_before_any_test_generation=true"
    );
    println!(
        "A6C10_BUDGET,structures_N128={},structures_N256={},beam={},factorizations_per_structure={},native_train_rescores={},probe_steps={}",
        structure_count(128).unwrap(),
        structure_count(256).unwrap(),
        A6C10_STRUCTURAL_BEAM,
        A6C10_FACTORS_PER_STRUCTURE,
        A6C10_STRUCTURAL_BEAM * A6C10_FACTORS_PER_STRUCTURE,
        A6C10_PROBE_STEPS
    );

    let mut frozen = Vec::with_capacity(NUM_TASKS * NUM_PANELS * seeds);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..seeds {
                let train = dataset(
                    derive_seed(index, task, panel, 0xa6c1_0101),
                    task, panel, TRAIN_STEPS,
                );
                let val = dataset(
                    derive_seed(index, task, panel, 0xa6c1_0202),
                    task, panel, VALIDATION_STEPS,
                );
                let start = Instant::now();
                let report = fit_joint_system_id(&train, &val, task.width())?;
                let fit_seconds = start.elapsed().as_secs_f64();
                if report.probed_structures != structure_count(task.width()).unwrap()
                    || report.beam_count != A6C10_STRUCTURAL_BEAM
                    || report.native_train_rescores != A6C10_STRUCTURAL_BEAM * A6C10_FACTORS_PER_STRUCTURE
                    || report.train_samples != (TRAIN_STEPS * LANES) as u64
                    || report.validation_samples != (VALIDATION_STEPS * LANES) as u64
                    || report.ring_payload_bytes != task.width() * 8
                {
                    return Err("A6-C10 preregistered train protocol mismatch".into());
                }
                let rules = task.teacher_gates();
                let exact = task.representable() && report.selected_rules == rules;
                let proposed_exact = rules.len() == 2 && report.proposed_rules.as_slice() == rules;
                println!(
                    "A6C10_FROZEN,task={},panel={},seed={},width={},representable={},origin={},selected_gate_count={},selected_rules={:?},teacher_exact={},proposed_exact={},joint_rules={:?},joint_full_train_errors={},joint_teacher_forced_train_errors={},joint_validation_errors={},selected_train_errors={},selected_validation_errors={},old_a6c9_train_errors={},old_a6c9_validation_errors={},probed_structures={},beam_size={},factorized_truth_masks={},native_rescores={},ring_payload_bytes={},beam_heap_payload_bytes={},fit_seconds={:.6}",
                    task.name(), panel.name(), index, task.width(),
                    task.representable(), report.origin.name(),
                    report.selected_rules.len(), report.selected_rules,
                    exact, proposed_exact, report.proposed_rules,
                    report.train_errors, report.teacher_forced_errors,
                    report.validation_errors, report.selected_train_errors,
                    report.selected_validation_errors,
                    report.previous.selected_train_errors,
                    report.previous.selected_validation_errors,
                    report.probed_structures, report.beam_count,
                    report.factorized_truth_masks,
                    report.native_train_rescores, report.ring_payload_bytes,
                    report.beam_heap_payload_bytes, fit_seconds,
                );
                frozen.push(Frozen { seed: index, task, panel, report, fit_seconds });
            }
        }
    }

    // Strict pre-registered leakage guard: the following independent test
    // sequences do not exist until all 100 TRAIN+VAL decisions are frozen.
    println!("A6C10_ALL_FROZEN,true");
    println!(
        "A6C10_TEST_HEADER,task,panel,seed,width,steps,samples,identity_errors,a6c7_errors,a6c8_errors,a6c9_errors,a6c10_proposal_errors,selected_errors,ablate0_errors,ablate1_errors,origin,selected_gates,selected_native_bitlane_per_second,fit_seconds"
    );

    let mut totals = [[[[0u64; ERRORS]; 2]; NUM_PANELS]; NUM_TASKS];
    let mut exacts = [[0usize; NUM_PANELS]; NUM_TASKS];
    let mut proposal_exacts = [[0usize; NUM_PANELS]; NUM_TASKS];
    let mut new_wins = [[0usize; NUM_PANELS]; NUM_TASKS];
    let mut selected_two = [[0usize; NUM_PANELS]; NUM_TASKS];
    let mut fit_totals = [[0.0f64; NUM_PANELS]; NUM_TASKS];
    let mut native_seconds = [[[0.0f64; 2]; NUM_PANELS]; NUM_TASKS];

    for case in &frozen {
        let ti = case.task.index();
        let pi = case.panel.index();
        let rules = case.task.teacher_gates();
        exacts[ti][pi] += usize::from(
            case.task.representable() && case.report.selected_rules == rules
        );
        proposal_exacts[ti][pi] += usize::from(
            rules.len() == 2 && case.report.proposed_rules.as_slice() == rules
        );
        new_wins[ti][pi] += usize::from(case.report.origin == JointOrigin::JointA6C10);
        selected_two[ti][pi] += usize::from(case.report.selected_rules.len() == 2);
        fit_totals[ti][pi] += case.fit_seconds;

        for (h, steps) in HORIZONS.iter().copied().enumerate() {
            let test = dataset(
                derive_seed(case.seed, case.task, case.panel, 0xa6c1_0303 + steps as u64),
                case.task, case.panel, steps,
            );
            let prior = &case.report.previous;
            let baseline = native_errors(&prior.old_one.reference, &test)?;
            let old_one = native_errors(&prior.old_one.committed, &test)?;
            let old_affine = native_errors(&prior.old_affine.selected, &test)?;
            let old = native_errors(&prior.selected, &test)?;
            let joint = native_errors(&case.report.proposed, &test)?;
            let start = Instant::now();
            let chosen = native_errors(&case.report.selected, &test)?;
            let secs = start.elapsed().as_secs_f64().max(f64::EPSILON);
            native_seconds[ti][pi][h] += secs;
            let mut ablated = [chosen; 2];
            for (gate_index, errors_out) in ablated.iter_mut().enumerate() {
                if case.report.selected_rules.len() > gate_index {
                    let remaining = case.report.selected_rules.iter()
                        .enumerate()
                        .filter(|(i,_)| *i != gate_index)
                        .map(|(_,rule)| *rule)
                        .collect::<Vec<_>>();
                    let model = make_multi_gate_native(case.task.width(), &remaining)?;
                    *errors_out = native_errors(&model, &test)?;
                }
            }
            let measures = [
                baseline, old_one, old_affine, old, joint,
                chosen, ablated[0], ablated[1],
            ];
            for (sum, value) in totals[ti][pi][h].iter_mut().zip(measures) {
                *sum += value;
            }
            println!(
                "A6C10_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.2},{:.6}",
                case.task.name(), case.panel.name(), case.seed,
                case.task.width(), steps, steps * LANES,
                baseline, old_one, old_affine, old, joint, chosen,
                ablated[0], ablated[1], case.report.origin.name(),
                case.report.selected_rules.len(), (steps * LANES) as f64 / secs,
                case.fit_seconds
            );
        }
    }

    for task in Task::ALL {
        for panel in Panel::ALL {
            let ti = task.index();
            let pi = panel.index();
            for (h, steps) in HORIZONS.iter().copied().enumerate() {
                let sum = totals[ti][pi][h];
                let n = (seeds * steps * LANES) as f64;
                let pct = |errors: u64| errors as f64 / n;
                println!(
                    "A6C10_SUMMARY,task={},panel={},steps={},width={},seeds={},baseline_error_rate={:.8},old_a6c7_error_rate={:.8},old_a6c8_error_rate={:.8},old_a6c9_error_rate={:.8},new_joint_error_rate={:.8},selected_error_rate={:.8},ablate_first_error_rate={:.8},ablate_second_error_rate={:.8},exact_selected_rules={},exact_proposed_pair={},new_joint_wins={},selected_two={},mean_fit_seconds={:.6},native_bitlane_per_second={:.2}",
                    task.name(), panel.name(), steps, task.width(), seeds,
                    pct(sum[0]), pct(sum[1]), pct(sum[2]), pct(sum[3]),
                    pct(sum[4]), pct(sum[5]), pct(sum[6]), pct(sum[7]),
                    exacts[ti][pi], proposal_exacts[ti][pi],
                    new_wins[ti][pi], selected_two[ti][pi],
                    fit_totals[ti][pi] / seeds as f64,
                    n / native_seconds[ti][pi][h],
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
    fn all_teacher_networks_reproduce_their_clean_terminal_labels() {
        for task in Task::ALL {
            let data = dataset(11, task, Panel::Clean, 512);
            let model = make_multi_gate_native(task.width(), &task.teacher_gates()).unwrap();
            assert_eq!(native_errors(&model, &data).unwrap(), 0);
        }
    }

    #[test]
    fn independent_label_noise_never_changes_raw_inputs() {
        for task in Task::ALL {
            let clean = dataset(12, task, Panel::Clean, 512);
            let noisy = dataset(12, task, Panel::LabelNoise, 512);
            assert_eq!(clean.blocks[0].inputs, noisy.blocks[0].inputs);
            assert_ne!(clean.blocks[0].targets, noisy.blocks[0].targets);
        }
    }

    #[test]
    fn preregistration_defines_all_tasks_and_budgets() {
        assert_eq!(Task::ALL.len(), 10);
        assert_eq!(structure_count(128), Some(128_016));
        assert_eq!(structure_count(256), Some(518_160));
        assert_eq!(A6C10_PROBE_STEPS, 64);
        assert_eq!(A6C10_STRUCTURAL_BEAM, 64);
        assert_eq!(A6C10_FACTORS_PER_STRUCTURE, 4);
    }
}

//! A6-C11D: diagnostic of three-hidden-gate search budget vs TRAIN length.
//!
//! Three preregistered teachers in 128-state-bit native ANF ring:
//! 1. Historical failed A6-C10 XOR(j19/x2)->AND(j67/x3)->XOR(j108/x4).
//! 2. Three nonlinear AND(j23/x2)->AND(j69/x3)->AND(j104/x4).
//! 3. Negative FOUR mutations, impossible for the THREE-gate learner.
//!
//! TRAIN lengths 512 and 2048 (strictly nested input and label prefix),
//! common VAL=512, two expensive structure budgets 64 and 640 (10x).
//! 5 seeds x 2 clean/noise1/16 x 3 tasks x 2 train lengths x 2 budgets
//! = 120 frozen model decisions BEFORE independent 4096-step TEST generated.
//! 30 distinct test sets are evaluated across all four configurations.
//! Ground-truth hidden rules are NEVER supplied to the learner: they are
//! generator-only and used retrospectively for diagnostics.
//!
//! All outputs are synthetic Boolean bit-error rates, NOT language quality.

use std::{collections::BTreeMap, error::Error, time::Instant};

use sml_core::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors},
    anf_intermediate_credit::IntermediateRule,
    anf_local_incremental::{LaneBlock, LaneData},
    anf_three_gate_diagnostic::{
        fit_three_gate_diagnostic, retrospective_budget, retrospective_recall, BudgetFit, Source,
        TripleRules, DIAG_BUDGETS, DIAG_SCREEN_FEATURES,
    },
    toy_tasks::SplitMix64,
};

const WIDTH: usize = 128;
const INPUT_BITS: usize = 5;
const LANES: usize = 64;
const TRAIN_LENGTHS: [usize; 2] = [512, 2048];
const VAL_STEPS: usize = 512;
const TEST_STEPS: usize = 4096;
const TASK_COUNT: usize = 3;
const PANEL_COUNT: usize = 2;
const BUDGET_COUNT: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    HistoricalXorAndXor,
    HardAndAndAnd,
    NegativeFour,
}
impl Task {
    const ALL: [Self; TASK_COUNT] = [
        Self::HistoricalXorAndXor,
        Self::HardAndAndAnd,
        Self::NegativeFour,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::HistoricalXorAndXor => "historical_xor_and_xor",
            Self::HardAndAndAnd => "hard_and_and_and",
            Self::NegativeFour => "negative_four",
        }
    }
    fn index(self) -> usize {
        match self {
            Self::HistoricalXorAndXor => 0,
            Self::HardAndAndAnd => 1,
            Self::NegativeFour => 2,
        }
    }
    fn rules(self) -> Vec<IntermediateRule> {
        let r = |gate, raw, truth| IntermediateRule { gate, raw, truth };
        match self {
            Self::HistoricalXorAndXor => vec![r(19, 2, 6), r(67, 3, 8), r(108, 4, 6)],
            Self::HardAndAndAnd => vec![r(23, 2, 8), r(69, 3, 8), r(104, 4, 8)],
            Self::NegativeFour => vec![r(17, 2, 6), r(47, 3, 8), r(81, 4, 6), r(109, 1, 8)],
        }
    }
    fn representable(self) -> bool {
        self != Self::NegativeFour
    }
    fn triple(self) -> Option<TripleRules> {
        let rules = self.rules();
        if rules.len() == 3 {
            Some(TripleRules {
                rules: [rules[0], rules[1], rules[2]],
            })
        } else {
            None
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Clean,
    Noise,
}
impl Panel {
    const ALL: [Self; PANEL_COUNT] = [Self::Clean, Self::Noise];
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

fn seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

/// Independent INPUT and LABEL-NOISE PRNGs ensure the first 512 steps of
/// the 2048-step training trajectory are bit-identical to short TRAIN.
/// The teacher model is a full native ANF network and starts at zero.
fn data(stream_seed: u64, task: Task, panel: Panel, steps: usize) -> LaneData {
    let teacher = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
    let mut input_rng = SplitMix64::new(stream_seed);
    let mut noise_rng = SplitMix64::new(stream_seed ^ 0xa6c1_1d1a_9a11_01cc);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = input_rng.next_u64()
    }
    let mut state = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; WIDTH];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    for (t, target) in targets.iter_mut().enumerate() {
        let begin = t * INPUT_BITS;
        state[WIDTH..].copy_from_slice(&inputs[begin..begin + INPUT_BITS]);
        teacher.step_lanes(&mut state, &mut next, &mut scratch);
        let clean = teacher.readout().evaluate_lanes(&state, &mut scratch);
        let noise = if panel == Panel::Noise {
            noise_rng.next_u64()
                & noise_rng.next_u64()
                & noise_rng.next_u64()
                & noise_rng.next_u64()
        } else {
            0
        };
        *target = clean ^ noise;
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
fn prefix(full: &LaneData, length: usize) -> LaneData {
    let block = &full.blocks[0];
    LaneData {
        input_bits: full.input_bits,
        target_bits: full.target_bits,
        blocks: vec![LaneBlock {
            steps: length,
            inputs: block.inputs[..length * INPUT_BITS].to_vec(),
            targets: block.targets[..length].to_vec(),
            valid: block.valid,
        }],
    }
}
#[derive(Clone)]
struct Frozen {
    task: Task,
    panel: Panel,
    seed: usize,
    train_steps: usize,
    budget: usize,
    fit: BudgetFit,
    fit_seconds: f64,
    top8: usize,
    top24: usize,
    top80: usize,
    true_structure_in_budget: bool,
    proposal_exact: bool,
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 2 {
        return Err("usage: sml-three-gate-diagnostic-bench [seeds:1..=5]".into());
    }
    let nseeds: usize = args.get(1).map_or(Ok(5), |arg| arg.parse())?;
    if !(1..=5).contains(&nseeds) {
        return Err("seed count 1..=5".into());
    }
    println!(
        "A6C11D_CONFIG,seeds={nseeds},tasks=historical_xor_and_xor;hard_and_and_and;negative_four,panels=clean;noise_1_in_16,train_steps=512;2048,val_steps={VAL_STEPS},test_steps={TEST_STEPS},budgets=64;640,width={WIDTH},lanes={LANES}"
    );
    println!(
        "A6C11D_PROTOCOL,three_gate_learner=true,teacher_hidden_rules_not_passed_to_fit=true,past_output_teacher_forced_train_only=true,autonomous_native_holdout=true,feature_scan_shared_across_budgets=true,train_long_nests_train_short=true,all_frozen_before_test_generation=true"
    );
    println!(
        "A6C11D_BUDGET,raw_lag_features={},spectral_screen_top={},structures_full_train_small={},structures_full_train_large={},native_rescores_per_budget=16,truth_factor_classes=520",
        (WIDTH-1)*4,DIAG_SCREEN_FEATURES,DIAG_BUDGETS[0],DIAG_BUDGETS[1]
    );

    let mut frozen = Vec::with_capacity(TASK_COUNT * PANEL_COUNT * nseeds * 4);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                let training_full = data(
                    seed(index, task, panel, 0xa6c1_1d11),
                    task,
                    panel,
                    TRAIN_LENGTHS[1],
                );
                let validation = data(
                    seed(index, task, panel, 0xa6c1_1d22),
                    task,
                    panel,
                    VAL_STEPS,
                );
                for steps in TRAIN_LENGTHS {
                    let training = prefix(&training_full, steps);
                    let start = Instant::now();
                    let report =
                        fit_three_gate_diagnostic(&training, &validation, WIDTH, &DIAG_BUDGETS)?;
                    let fit_seconds = start.elapsed().as_secs_f64();
                    if report.train_samples != (steps * LANES) as u64
                        || report.validation_samples != (VAL_STEPS * LANES) as u64
                        || report.all_lag_feature_count != (WIDTH - 1) * 4
                        || report.distinct_factorizable_masks != 520
                        || report.budgets.len() != BUDGET_COUNT
                    {
                        return Err("A6-C11D preregistered train protocol violated".into());
                    }
                    let (recall, structural) = if let Some(triple) = task.triple() {
                        (
                            retrospective_recall(triple, &report),
                            retrospective_budget(triple, &training, WIDTH, &DIAG_BUDGETS)?,
                        )
                    } else {
                        let expected = task
                            .rules()
                            .into_iter()
                            .map(|rule| Source {
                                gate: rule.gate,
                                raw: rule.raw,
                            })
                            .collect::<Vec<_>>();
                        let count = |top: usize| {
                            expected
                                .iter()
                                .filter(|&&s| {
                                    report
                                        .ranked_features
                                        .iter()
                                        .take(top)
                                        .any(|r| r.source == s)
                                })
                                .count()
                        };
                        ((count(8), count(24), count(80)), vec![false; 2])
                    };
                    for (budget_index, fit) in report.budgets.into_iter().enumerate() {
                        if fit.budget != DIAG_BUDGETS[budget_index]
                            || fit.structures_full_train_scored != fit.budget
                            || fit.native_train_rescores != 16
                        {
                            return Err("A6-C11D budget count violation".into());
                        }
                        let proposal_exact = task
                            .triple()
                            .is_some_and(|triple| fit.proposal_rules == triple);
                        println!(
                            "A6C11D_FROZEN,task={},panel={},seed={},train_steps={},budget={},top8_true={},top24_true={},top80_true={},true_structure_in_budget={},proposal_exact={},accepted={},proposal_sources={:?},proposal_truths={:?},teacher_forced_train_errors={},autonomous_train_errors={},autonomous_val_errors={},baseline_val_errors={},feature_count={},factor_classes={},screen_pool={},native_rescores={},ring_payload_bytes={},fit_seconds={:.6}",
                            task.name(),panel.name(),index,steps,fit.budget,
                            recall.0,recall.1,recall.2,structural[budget_index],
                            proposal_exact,fit.accepted,fit.proposal_sources,
                            fit.proposal_rules.rules.map(|r|r.truth),
                            fit.teacher_forced_train_errors,fit.autonomous_train_errors,
                            fit.autonomous_validation_errors,fit.baseline_validation_errors,
                            report.all_lag_feature_count,report.distinct_factorizable_masks,
                            report.spectral_triplet_pool_count,
                            fit.native_train_rescores,report.scratch_ring_bytes,fit_seconds
                        );
                        frozen.push(Frozen {
                            task,
                            panel,
                            seed: index,
                            train_steps: steps,
                            budget: fit.budget,
                            fit,
                            fit_seconds,
                            top8: recall.0,
                            top24: recall.1,
                            top80: recall.2,
                            true_structure_in_budget: structural[budget_index],
                            proposal_exact,
                        });
                    }
                }
            }
        }
    }

    println!("A6C11D_ALL_FROZEN,true");
    println!(
        "A6C11D_TEST_HEADER,task,panel,seed,train_steps,budget,samples,reference_errors,proposal_errors,committed_errors,true_teacher_oracle_errors,top8_true,top24_true,top80_true,structure_in_budget,proposal_exact,accepted,fit_seconds"
    );

    // All test streams are created AFTER the 120 training choices are frozen.
    // Cache each of 30 distinct test streams for fair 512/2048 & 64/640
    // comparisons. The teacher is an ORACLE only after fitting.
    let mut test_data = BTreeMap::<(usize, usize, usize), LaneData>::new();
    let mut oracle = BTreeMap::<(usize, usize, usize), u64>::new();
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                let key = (task.index(), panel.index(), index);
                let holdout = data(
                    seed(index, task, panel, 0xa6c1_1d33),
                    task,
                    panel,
                    TEST_STEPS,
                );
                let teacher = make_multi_gate_native(WIDTH, &task.rules())?;
                let truth_errors = native_errors(&teacher, &holdout)?;
                oracle.insert(key, truth_errors);
                test_data.insert(key, holdout);
            }
        }
    }
    let mut totals = [[[[[0u64; 4]; BUDGET_COUNT]; 2]; PANEL_COUNT]; TASK_COUNT];
    let mut exact = [[[[0usize; BUDGET_COUNT]; 2]; PANEL_COUNT]; TASK_COUNT];
    let mut structural = [[[[0usize; BUDGET_COUNT]; 2]; PANEL_COUNT]; TASK_COUNT];
    let mut accepted = [[[[0usize; BUDGET_COUNT]; 2]; PANEL_COUNT]; TASK_COUNT];
    let mut recall = [[[[[0usize; 3]; BUDGET_COUNT]; 2]; PANEL_COUNT]; TASK_COUNT];
    let mut time = [[[[0.0f64; BUDGET_COUNT]; 2]; PANEL_COUNT]; TASK_COUNT];
    for frozen_model in &frozen {
        let f = frozen_model;
        let t = f.task.index();
        let p = f.panel.index();
        let li = if f.train_steps == TRAIN_LENGTHS[0] {
            0
        } else {
            1
        };
        let bi = if f.budget == DIAG_BUDGETS[0] { 0 } else { 1 };
        let key = (t, p, f.seed);
        let holdout = test_data.get(&key).ok_or("missing frozen holdout")?;
        let original = make_multi_gate_native(WIDTH, &[])?;
        let baseline = native_errors(&original, holdout)?;
        let proposed = native_errors(&f.fit.proposal, holdout)?;
        let committed = native_errors(&f.fit.committed, holdout)?;
        let reference = *oracle.get(&key).ok_or("missing teacher oracle")?;
        let metrics = [baseline, proposed, committed, reference];
        for (total, value) in totals[t][p][li][bi].iter_mut().zip(metrics) {
            *total += value;
        }
        exact[t][p][li][bi] += usize::from(f.proposal_exact);
        structural[t][p][li][bi] += usize::from(f.true_structure_in_budget);
        accepted[t][p][li][bi] += usize::from(f.fit.accepted);
        for (dst, value) in recall[t][p][li][bi]
            .iter_mut()
            .zip([f.top8, f.top24, f.top80])
        {
            *dst += value;
        }
        time[t][p][li][bi] += f.fit_seconds;
        println!(
            "A6C11D_TEST_RESULT,{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6}",
            f.task.name(),
            f.panel.name(),
            f.seed,
            f.train_steps,
            f.budget,
            TEST_STEPS * LANES,
            baseline,
            proposed,
            committed,
            reference,
            f.top8,
            f.top24,
            f.top80,
            f.true_structure_in_budget,
            f.proposal_exact,
            f.fit.accepted,
            f.fit_seconds
        );
    }

    for task in Task::ALL {
        for panel in Panel::ALL {
            for (li, steps) in TRAIN_LENGTHS.iter().copied().enumerate() {
                for (bi, budget) in DIAG_BUDGETS.iter().copied().enumerate() {
                    let sum = totals[task.index()][panel.index()][li][bi];
                    let n = (nseeds * TEST_STEPS * LANES) as f64;
                    let rate = |v: u64| v as f64 / n;
                    let r = recall[task.index()][panel.index()][li][bi];
                    println!(
                        "A6C11D_SUMMARY,task={},panel={},train_steps={},budget={},seeds={},baseline_error_rate={:.8},proposal_error_rate={:.8},selected_error_rate={:.8},teacher_oracle_error_rate={:.8},source_top8_total={},source_top24_total={},source_top80_total={},true_structures_in_budget={},exact_proposals={},accepted={},mean_fit_seconds={:.6}",
                        task.name(),panel.name(),steps,budget,nseeds,
                        rate(sum[0]),rate(sum[1]),rate(sum[2]),rate(sum[3]),
                        r[0],r[1],r[2],
                        structural[task.index()][panel.index()][li][bi],
                        exact[task.index()][panel.index()][li][bi],
                        accepted[task.index()][panel.index()][li][bi],
                        time[task.index()][panel.index()][li][bi]/nseeds as f64
                    );
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_train_is_an_exact_nested_prefix_of_the_long_train() {
        for task in Task::ALL {
            for panel in Panel::ALL {
                let long = data(123, task, panel, 2048);
                let short = data(123, task, panel, 512);
                assert_eq!(prefix(&long, 512), short);
            }
        }
    }
    #[test]
    fn all_native_teachers_are_exact_on_clean_targets() {
        for task in Task::ALL {
            let sample = data(124, task, Panel::Clean, 400);
            let teacher = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
            assert_eq!(native_errors(&teacher, &sample).unwrap(), 0);
        }
    }
    #[test]
    fn negative_teacher_has_four_hidden_gates() {
        assert_eq!(Task::NegativeFour.rules().len(), 4);
        assert!(Task::NegativeFour.triple().is_none());
        assert!(Task::HistoricalXorAndXor.representable());
    }
}

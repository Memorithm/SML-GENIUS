//! A6-C12a: preregistered adversarial spectral-screen ablations.
//!
//! Four native 128-state-bit three-gate tasks, including parity and masked AND.
//! All three component scores use the same TRAIN; teacher details are used
//! only to synthesize labels and retrospectively record ranks.
//! Held-out TEST streams are created only after all policies have been frozen.
//! This is not an NLP, scaling, or large-ANF-bank benchmark.

use std::{error::Error, time::Instant};

use sml_core::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors},
    anf_intermediate_credit::IntermediateRule,
    anf_local_incremental::{LaneBlock, LaneData},
    anf_three_gate_diagnostic::{
        fit_three_gate_diagnostic, rank_features_ablated, retrospective_source_ranks,
        retrospective_structure_rank, BudgetFit, Source, SpectralAblation, DIAG_BUDGETS,
    },
    toy_tasks::SplitMix64,
};

const WIDTH: usize = 128;
const INPUT_BITS: usize = 5;
const LANES: usize = 64;
const TRAIN_LENGTHS: [usize; 2] = [512, 2048];
const VAL_STEPS: usize = 512;
const TEST_STEPS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    ParityXor3,
    MaskedAndXorXor,
    HistoricalXorAndXor,
    AndAndAnd,
}

impl Family {
    const ALL: [Self; 4] = [
        Self::ParityXor3,
        Self::MaskedAndXorXor,
        Self::HistoricalXorAndXor,
        Self::AndAndAnd,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::ParityXor3 => "parity_xor3",
            Self::MaskedAndXorXor => "masked_and_xor_xor",
            Self::HistoricalXorAndXor => "historical_xor_and_xor",
            Self::AndAndAnd => "and_and_and",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&item| item == self).unwrap()
    }

    fn truth(self) -> [u8; 3] {
        match self {
            Self::ParityXor3 => [6, 6, 6],
            Self::MaskedAndXorXor => [8, 6, 6],
            Self::HistoricalXorAndXor => [6, 8, 6],
            Self::AndAndAnd => [8, 8, 8],
        }
    }

    fn rules(self) -> Vec<IntermediateRule> {
        let truth = self.truth();
        [(19, 2), (67, 3), (108, 4)]
            .into_iter()
            .enumerate()
            .map(|(index, (gate, raw))| IntermediateRule {
                gate,
                raw,
                truth: truth[index],
            })
            .collect()
    }

    fn sources(self) -> [Source; 3] {
        [
            Source { gate: 19, raw: 2 },
            Source { gate: 67, raw: 3 },
            Source { gate: 108, raw: 4 },
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Clean,
    Noise,
}
impl Panel {
    const ALL: [Self; 2] = [Self::Clean, Self::Noise];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Score {
    W1,
    W1Root,
    W1RootPeel,
}
impl Score {
    const ALL: [Self; 3] = [Self::W1, Self::W1Root, Self::W1RootPeel];

    fn name(self) -> &'static str {
        match self {
            Self::W1 => "walsh_1",
            Self::W1Root => "walsh_1_root",
            Self::W1RootPeel => "walsh_1_root_peel",
        }
    }

    fn method(self) -> SpectralAblation {
        match self {
            Self::W1 => SpectralAblation::Marginal,
            Self::W1Root => SpectralAblation::RootInteraction,
            Self::W1RootPeel => SpectralAblation::RootInteractionAndPeeling,
        }
    }
}

fn stream_seed(index: usize, family: Family, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (family.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

fn data(stream: u64, family: Family, panel: Panel, steps: usize) -> LaneData {
    let teacher = make_multi_gate_native(WIDTH, &family.rules()).unwrap();
    let mut input_rng = SplitMix64::new(stream);
    let mut noise_rng = SplitMix64::new(stream ^ 0xa6c1_2a55_aa00_ccee);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = input_rng.next_u64();
    }
    let mut state = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; WIDTH];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    for (t, target) in targets.iter_mut().enumerate() {
        let from = t * INPUT_BITS;
        state[WIDTH..].copy_from_slice(&inputs[from..from + INPUT_BITS]);
        teacher.step_lanes(&mut state, &mut next, &mut scratch);
        let truth = teacher.readout().evaluate_lanes(&state, &mut scratch);
        let noise = if panel == Panel::Noise {
            noise_rng.next_u64()
                & noise_rng.next_u64()
                & noise_rng.next_u64()
                & noise_rng.next_u64()
        } else {
            0
        };
        *target = truth ^ noise;
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
struct Ranked {
    family: Family,
    panel: Panel,
    seed: usize,
    train_steps: usize,
    score: Score,
    source_ranks: [usize; 3],
    structure_rank: Option<usize>,
}

struct Frozen {
    family: Family,
    panel: Panel,
    seed: usize,
    train_steps: usize,
    fit: BudgetFit,
    fit_seconds: f64,
}

fn emit_summaries(records: &[Ranked], nseeds: usize) {
    for family in Family::ALL {
        for panel in Panel::ALL {
            for train_steps in TRAIN_LENGTHS {
                for score in Score::ALL {
                    let group = records
                        .iter()
                        .filter(|r| {
                            r.family == family
                                && r.panel == panel
                                && r.train_steps == train_steps
                                && r.score == score
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(group.len(), nseeds);
                    let within = |rank: usize| {
                        group
                            .iter()
                            .filter(|r| r.source_ranks.iter().all(|&v| v <= rank))
                            .count()
                    };
                    let count_structure = |max_rank: usize| {
                        group
                            .iter()
                            .filter(|r| r.structure_rank.is_some_and(|v| v <= max_rank))
                            .count()
                    };
                    let mut medians = [0usize; 3];
                    let mut worst = [0usize; 3];
                    for index in 0..3 {
                        let mut ranks =
                            group.iter().map(|r| r.source_ranks[index]).collect::<Vec<_>>();
                        ranks.sort_unstable();
                        medians[index] = ranks[ranks.len() / 2];
                        worst[index] = *ranks.last().unwrap();
                    }
                    println!(
                        "A6C12A_RANK_SUMMARY,family={},panel={},train_steps={},score={},seeds={},all_top8={},all_top24={},all_top80={},triple_top64={},triple_top640={},median_ranks={:?},worst_ranks={:?}",
                        family.name(),
                        panel.name(),
                        train_steps,
                        score.name(),
                        group.len(),
                        within(8),
                        within(24),
                        within(80),
                        count_structure(64),
                        count_structure(640),
                        medians,
                        worst
                    );
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() > 2 {
        return Err("usage: sml-spectral-adversarial-bench [seeds:1..=5]".into());
    }
    let nseeds: usize = args.get(1).map_or(Ok(5), |arg| arg.parse())?;
    if !(1..=5).contains(&nseeds) {
        return Err("seed count must be 1..=5".into());
    }
    println!(
        "A6C12A_CONFIG,seeds={nseeds},families=parity_xor3;masked_and_xor_xor;historical_xor_and_xor;and_and_and,panels=clean;noise_1_in_16,train_lengths=512;2048,val_steps={VAL_STEPS},test_steps={TEST_STEPS},width={WIDTH},lanes={LANES},rank_methods=walsh_1;walsh_1_root;walsh_1_root_peel"
    );
    println!(
        "A6C12A_PROTOCOL,preregistered=true,teacher_hidden_rules_never_passed_to_fit=true,ranking_train_only=true,same_streams_for_ablations=true,all_frozen_before_heldout=true"
    );
    let mut ranked = Vec::with_capacity(4 * 2 * nseeds * 2 * 3);
    let mut frozen = Vec::with_capacity(4 * 2 * nseeds * 2 * 2);
    for family in Family::ALL {
        for panel in Panel::ALL {
            for seed in 0..nseeds {
                let full = data(
                    stream_seed(seed, family, panel, 0xa6c1_2a11),
                    family,
                    panel,
                    TRAIN_LENGTHS[1],
                );
                let val = data(
                    stream_seed(seed, family, panel, 0xa6c1_2a22),
                    family,
                    panel,
                    VAL_STEPS,
                );
                for train_steps in TRAIN_LENGTHS {
                    let train = prefix(&full, train_steps);
                    for score in Score::ALL {
                        let candidates = rank_features_ablated(&train, WIDTH, score.method())?;
                        let ranks = retrospective_source_ranks(&candidates, &family.sources());
                        if ranks.len() != 3 || ranks.iter().any(Option::is_none) {
                            return Err("registered source absent from complete candidate space".into());
                        }
                        let source_ranks =
                            [ranks[0].unwrap(), ranks[1].unwrap(), ranks[2].unwrap()];
                        let structure_rank =
                            retrospective_structure_rank(&candidates, family.sources());
                        println!(
                            "A6C12A_RANK,family={},panel={},seed={},train_steps={},score={},rank_j={},rank_k={},rank_l={},triplet_pool_rank={},top24_all={},triplet_top64={},triplet_top640={}",
                            family.name(),
                            panel.name(),
                            seed,
                            train_steps,
                            score.name(),
                            source_ranks[0],
                            source_ranks[1],
                            source_ranks[2],
                            structure_rank.unwrap_or(0),
                            source_ranks.iter().all(|&rank| rank <= 24),
                            structure_rank.is_some_and(|rank| rank <= 64),
                            structure_rank.is_some_and(|rank| rank <= 640)
                        );
                        ranked.push(Ranked {
                            family,
                            panel,
                            seed,
                            train_steps,
                            score,
                            source_ranks,
                            structure_rank,
                        });
                    }
                    let start = Instant::now();
                    let report =
                        fit_three_gate_diagnostic(&train, &val, WIDTH, &DIAG_BUDGETS)?;
                    let seconds = start.elapsed().as_secs_f64();
                    for fit in report.budgets {
                        println!(
                            "A6C12A_FROZEN,family={},panel={},seed={},train_steps={},budget={},proposed_sources={:?},proposed_truths={:?},accepted={},train_proxy_errors={},autonomous_train_errors={},validation_errors={},fit_pair_seconds={:.6}",
                            family.name(),
                            panel.name(),
                            seed,
                            train_steps,
                            fit.budget,
                            fit.proposal_sources,
                            fit.proposal_rules.rules.map(|rule| rule.truth),
                            fit.accepted,
                            fit.teacher_forced_train_errors,
                            fit.autonomous_train_errors,
                            fit.autonomous_validation_errors,
                            seconds
                        );
                        frozen.push(Frozen {
                            family,
                            panel,
                            seed,
                            train_steps,
                            fit,
                            fit_seconds: seconds,
                        });
                    }
                }
            }
        }
    }

    let expected_ranks = 4 * 2 * nseeds * 2 * 3;
    let expected_frozen = 4 * 2 * nseeds * 2 * 2;
    if ranked.len() != expected_ranks || frozen.len() != expected_frozen {
        return Err("unexpected experiment cardinality".into());
    }
    println!("A6C12A_ALL_FROZEN,rank_observations={expected_ranks},policies={expected_frozen}");
    // No TEST data exists until every learned candidate and rank is frozen.
    for family in Family::ALL {
        for panel in Panel::ALL {
            for seed in 0..nseeds {
                let holdout = data(
                    stream_seed(seed, family, panel, 0xa6c1_2a33),
                    family,
                    panel,
                    TEST_STEPS,
                );
                let oracle = make_multi_gate_native(WIDTH, &family.rules())?;
                let oracle_err = native_errors(&oracle, &holdout)?;
                for result in frozen
                    .iter()
                    .filter(|r| r.family == family && r.panel == panel && r.seed == seed)
                {
                    let proposal_err = native_errors(&result.fit.proposal, &holdout)?;
                    let selected_err = native_errors(&result.fit.committed, &holdout)?;
                    let source_exact = result.fit.proposal_sources == family.sources();
                    let truth_exact =
                        result.fit.proposal_rules.rules.map(|r| r.truth) == family.truth();
                    let samples = (TEST_STEPS * LANES) as u64;
                    println!(
                        "A6C12A_TEST,family={},panel={},seed={},train_steps={},budget={},samples={},oracle_err={},proposal_err={},selected_err={},sources_exact={},truth_tables_exact={},accepted={},fit_pair_seconds={:.6}",
                        family.name(),
                        panel.name(),
                        seed,
                        result.train_steps,
                        result.fit.budget,
                        samples,
                        oracle_err,
                        proposal_err,
                        selected_err,
                        source_exact,
                        truth_exact,
                        result.fit.accepted,
                        result.fit_seconds
                    );
                }
            }
        }
    }
    emit_summaries(&ranked, nseeds);
    println!(
        "A6C12A_COMPLETE,rank_observations={},frozen={},independent_test_streams={},summaries={}",
        ranked.len(),
        frozen.len(),
        4 * 2 * nseeds,
        4 * 2 * 2 * 3
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_train_is_bit_identical_and_clean_oracle_exact() {
        for family in Family::ALL {
            let stream = stream_seed(0, family, Panel::Clean, 0xa6c1_2a11);
            let full = data(stream, family, Panel::Clean, TRAIN_LENGTHS[1]);
            let short = data(stream, family, Panel::Clean, TRAIN_LENGTHS[0]);
            assert_eq!(prefix(&full, TRAIN_LENGTHS[0]), short);
            let native = make_multi_gate_native(WIDTH, &family.rules()).unwrap();
            assert_eq!(native_errors(&native, &short).unwrap(), 0);
        }
    }

    #[test]
    fn mixed_case_is_natively_representable() {
        let family = Family::MaskedAndXorXor;
        let teacher = make_multi_gate_native(WIDTH, &family.rules()).unwrap();
        let observed = data(
            stream_seed(1, family, Panel::Noise, 0xa6c1_2a11),
            family,
            Panel::Noise,
            512,
        );
        let errors = native_errors(&teacher, &observed).unwrap();
        assert!(errors > 0);
        assert!(errors < (512 * LANES / 8) as u64);
    }
}

//! A6-C12d: preregistered native correlated inputs and exact alias controls.
//! The exact x1=x2 case is not structurally identifiable on its input domain.
//! Screening uses TRAIN only; held-out TEST is created after all fits freeze.
//! This is a synthetic 3-gate N128 system-ID experiment, not SML-25B.

use std::{
    cmp::Reverse,
    collections::{BTreeSet, HashSet},
    error::Error,
    time::Instant,
};

use sml_core::{
    anf_dual_hidden_credit::{make_multi_gate_native, native_errors},
    anf_intermediate_credit::IntermediateRule,
    anf_local_incremental::{LaneBlock, LaneData},
    anf_three_gate_diagnostic::{fit_three_gate_candidates, BudgetFit, Source},
    toy_tasks::SplitMix64,
};

const WIDTH: usize = 128;
const INPUT_BITS: usize = 5;
const LANES: usize = 64;
const TRAIN_STEPS: usize = 512;
const VAL_STEPS: usize = 512;
const TEST_STEPS: usize = 4096;
const TOP_PAIRS: usize = 8;
const TOP_TRIPLES: usize = 16;
const SKETCH_ROUNDS: usize = 48;
const SKETCH_BITS: usize = 14;
const SKETCH_COARSE_WORDS: usize = 8;
const SKETCH_COARSE_KEEP: usize = 256;
const SKETCH_SEED: u64 = 0xA6C1_2C5C_0123_4567;
const STRESS_WIDTH: usize = 508;
const CORRELATION_SALT: u64 = 0xA6C1_2D00_19A5_1133;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    XorXorXor,
    AndXorXor,
    XorAndXor,
    AndAndAnd,
}
impl Task {
    const ALL: [Self; 2] = [Self::XorXorXor, Self::AndXorXor];
    fn name(self) -> &'static str {
        match self {
            Self::XorXorXor => "xor_xor_xor",
            Self::AndXorXor => "and_xor_xor",
            Self::XorAndXor => "xor_and_xor",
            Self::AndAndAnd => "and_and_and",
        }
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap()
    }
    fn truths(self) -> [u8; 3] {
        match self {
            Self::XorXorXor => [6, 6, 6],
            Self::AndXorXor => [8, 6, 6],
            Self::XorAndXor => [6, 8, 6],
            Self::AndAndAnd => [8, 8, 8],
        }
    }
    fn sources(self) -> [Source; 3] {
        [
            Source { gate: 20, raw: 2 },
            Source { gate: 68, raw: 3 },
            Source { gate: 116, raw: 4 },
        ]
    }
    fn rules(self) -> Vec<IntermediateRule> {
        let sources = self.sources();
        let truths = self.truths();
        (0..3)
            .map(|i| IntermediateRule {
                gate: sources[i].gate,
                raw: sources[i].raw,
                truth: truths[i],
            })
            .collect()
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
enum Method {
    Walsh2Pair,
    SketchMitm,
    Hybrid,
    Walsh3Stream,
}
impl Method {
    const ALL: [Self; 4] = [
        Self::Walsh2Pair,
        Self::SketchMitm,
        Self::Hybrid,
        Self::Walsh3Stream,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Walsh2Pair => "walsh2_pair",
            Self::SketchMitm => "sketch_mitm",
            Self::Hybrid => "hybrid_w2_mitm",
            Self::Walsh3Stream => "walsh3_stream",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Independent,
    NearAlias,
    ExactAlias,
}
impl InputMode {
    const STRESS: [Self; 2] = [Self::NearAlias, Self::ExactAlias];

    fn name(self) -> &'static str {
        match self {
            Self::Independent => "independent",
            Self::NearAlias => "near_alias_1_in_32",
            Self::ExactAlias => "exact_alias",
        }
    }
}

fn one_in_32(rng: &mut SplitMix64) -> u64 {
    (0..5).fold(u64::MAX, |mask, _| mask & rng.next_u64())
}

fn seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

#[cfg(test)]
fn data(stream: u64, task: Task, panel: Panel, steps: usize) -> LaneData {
    data_with_mode(stream, task, panel, steps, InputMode::Independent)
}

fn data_with_mode(
    stream: u64,
    task: Task,
    panel: Panel,
    steps: usize,
    mode: InputMode,
) -> LaneData {
    let teacher = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
    let mut rng = SplitMix64::new(stream);
    let mut noise_rng = SplitMix64::new(stream ^ 0xa6c1_2b66_dada_1177);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    // Correlate inputs BEFORE native teacher simulation. A separate random
    // stream decouples these masks from both raw generation and label noise.
    let mut correlation_rng = SplitMix64::new(stream ^ CORRELATION_SALT);
    for t in 0..steps {
        let offset = t * INPUT_BITS;
        match mode {
            InputMode::Independent => {}
            InputMode::NearAlias => {
                // x1 nearly duplicates x2 at the SAME gate position; likewise
                // x3 nearly duplicates x4. Marginal disagreement = 1/32.
                inputs[offset + 1] = inputs[offset + 2] ^ one_in_32(&mut correlation_rng);
                inputs[offset + 3] = inputs[offset + 4] ^ one_in_32(&mut correlation_rng);
            }
            InputMode::ExactAlias => {
                inputs[offset + 1] = inputs[offset + 2];
            }
        }
    }
    let mut state = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; WIDTH];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    for (t, target) in targets.iter_mut().enumerate() {
        let from = t * INPUT_BITS;
        state[WIDTH..].copy_from_slice(&inputs[from..from + INPUT_BITS]);
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

/// The only search-space prior: a fixed, increasingly dense grid of valid
/// gate positions. The true mutated positions are not provided to the ranker.
fn grid(n: usize) -> Vec<Source> {
    let gates = match n {
        32 => (0..8).map(|i| 4 + 16 * i).collect::<Vec<_>>(),
        128 => (0..31)
            .map(|i| 4 + 4 * i)
            .chain(std::iter::once(127))
            .collect::<Vec<_>>(),
        508 => (1..WIDTH).collect::<Vec<_>>(),
        _ => panic!("only preregistered grid sizes"),
    };
    let sources = gates
        .into_iter()
        .flat_map(|gate| (1..=4).map(move |raw| Source { gate, raw }))
        .collect::<Vec<_>>();
    assert_eq!(sources.len(), n);
    sources
}

/// All source columns are derived only from the observed final TRAIN labels
/// and raw inputs: p = z XOR Rpast. The use of a past noisy R introduces a
/// second independent label-noise bit on the proxy, not on autonomous TEST.
struct Observed {
    p: Vec<u64>,
    columns: Vec<Vec<u64>>,
    sources: Vec<Source>,
}
impl Observed {
    fn new(train: &LaneData, sources: Vec<Source>) -> Self {
        let b = &train.blocks[0];
        assert!(b.steps > WIDTH && b.valid == u64::MAX);
        let get = |t: usize, raw: usize| b.inputs[t * INPUT_BITS + raw];
        let p = (WIDTH..b.steps)
            .map(|t| {
                let r_now = b.targets[t] ^ get(t, 0);
                let r_past = b.targets[t - WIDTH] ^ get(t - WIDTH, 0);
                r_now ^ get(t, 1) ^ r_past
            })
            .collect::<Vec<_>>();
        let columns = sources
            .iter()
            .map(|s| {
                (WIDTH..b.steps)
                    .map(|t| get(t - (WIDTH - s.gate), s.raw))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Self {
            p,
            columns,
            sources,
        }
    }
    fn same_gate(&self, i: usize, j: usize) -> bool {
        self.sources[i].gate == self.sources[j].gate
    }
    fn magnitude(&self, flips: u64) -> u64 {
        let samples = (self.p.len() * LANES) as u64;
        samples.abs_diff(2 * flips)
    }
    fn pair_magnitude(&self, i: usize, j: usize) -> u64 {
        let errors = self
            .p
            .iter()
            .zip(&self.columns[i])
            .zip(&self.columns[j])
            .map(|((&p, &a), &b)| u64::from((p ^ a ^ b).count_ones()))
            .sum();
        self.magnitude(errors)
    }
}

#[derive(Clone, Copy)]
struct PairScore {
    ids: [usize; 2],
    correlation: u64,
}

fn rank_pairs(o: &Observed) -> Vec<PairScore> {
    let mut scores = Vec::new();
    for i in 0..o.sources.len() {
        for j in i + 1..o.sources.len() {
            if o.same_gate(i, j) {
                continue;
            }
            scores.push(PairScore {
                ids: [i, j],
                correlation: o.pair_magnitude(i, j),
            });
        }
    }
    scores.sort_unstable_by(|a, b| {
        b.correlation
            .cmp(&a.correlation)
            .then_with(|| a.ids.cmp(&b.ids))
    });
    scores
}

/// Every admissible triple is fully scored; only top16 kept in memory.
fn rank_triples_stream(o: &Observed) -> Vec<[Source; 3]> {
    let mut top = BTreeSet::<(u64, Reverse<[usize; 3]>)>::new();
    for i in 0..o.sources.len() {
        for j in i + 1..o.sources.len() {
            if o.same_gate(i, j) {
                continue;
            }
            let paired =
                o.p.iter()
                    .zip(&o.columns[i])
                    .zip(&o.columns[j])
                    .map(|((&p, &a), &b)| p ^ a ^ b)
                    .collect::<Vec<_>>();
            for k in j + 1..o.sources.len() {
                if o.same_gate(i, k) || o.same_gate(j, k) {
                    continue;
                }
                let errors: u64 = paired
                    .iter()
                    .zip(&o.columns[k])
                    .map(|(&p, &c)| u64::from((p ^ c).count_ones()))
                    .sum();
                let key = (o.magnitude(errors), Reverse([i, j, k]));
                if top.len() < TOP_TRIPLES || key > *top.first().unwrap() {
                    top.insert(key);
                    if top.len() > TOP_TRIPLES {
                        top.pop_first();
                    }
                }
            }
        }
    }
    top.iter()
        .rev()
        .map(|(_, reverse)| triplet(o, reverse.0))
        .collect()
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
struct SketchStats {
    pair_probes: usize,
    bucket_hits: usize,
    unique: usize,
    coarse_scored: usize,
    full_scored: usize,
}

fn sketch_key(column: &[u64], samples: &[usize; SKETCH_BITS]) -> usize {
    samples
        .iter()
        .enumerate()
        .fold(0usize, |key, (bit, &slot)| {
            key | ((((column[slot / 64] >> (slot % 64)) & 1) as usize) << bit)
        })
}

fn triple_correlation(o: &Observed, ids: [usize; 3], words: usize) -> u64 {
    let length = words.min(o.p.len());
    let errors: u64 = (0..length)
        .map(|t| {
            u64::from(
                (o.p[t] ^ o.columns[ids[0]][t] ^ o.columns[ids[1]][t] ^ o.columns[ids[2]][t])
                    .count_ones(),
            )
        })
        .sum();
    (length as u64 * LANES as u64).abs_diff(2 * errors)
}

/// Probabilistic hash screening. Worst-case bucket multiplicity remains
/// cubic; log actual collisions separately from full-correlation evaluations.
fn rank_sketch_triplets(o: &Observed) -> (Vec<[Source; 3]>, SketchStats) {
    let n = o.sources.len();
    let slots = o.p.len() * LANES;
    assert!(slots >= SKETCH_BITS && o.p.len() >= SKETCH_COARSE_WORDS);
    let mut rng = SplitMix64::new(SKETCH_SEED);
    let mut heads = vec![usize::MAX; 1 << SKETCH_BITS];
    let mut next = vec![usize::MAX; n];
    let mut matched = HashSet::<[usize; 3]>::new();
    let mut stats = SketchStats::default();
    for _round in 0..SKETCH_ROUNDS {
        let mut samples = [0usize; SKETCH_BITS];
        for position in 0..SKETCH_BITS {
            loop {
                let bit = rng.next_u64() as usize % slots;
                if !samples[..position].contains(&bit) {
                    samples[position] = bit;
                    break;
                }
            }
        }
        let keys = o
            .columns
            .iter()
            .map(|column| sketch_key(column, &samples))
            .collect::<Vec<_>>();
        let target = sketch_key(&o.p, &samples);
        heads.fill(usize::MAX);
        for (index, &key) in keys.iter().enumerate() {
            next[index] = heads[key];
            heads[key] = index;
        }
        for i in 0..n {
            for j in i + 1..n {
                if o.same_gate(i, j) {
                    continue;
                }
                stats.pair_probes += 1;
                let key = target ^ keys[i] ^ keys[j];
                let mut k = heads[key];
                while k != usize::MAX {
                    stats.bucket_hits += 1;
                    if !o.same_gate(i, k) && !o.same_gate(j, k) {
                        let mut ids = [i, j, k];
                        ids.sort_unstable();
                        matched.insert(ids);
                    }
                    k = next[k];
                }
            }
        }
    }
    stats.unique = matched.len();
    let mut coarse = matched
        .into_iter()
        .map(|ids| (triple_correlation(o, ids, SKETCH_COARSE_WORDS), ids))
        .collect::<Vec<_>>();
    stats.coarse_scored = coarse.len();
    coarse.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    coarse.truncate(SKETCH_COARSE_KEEP);
    let mut full = coarse
        .into_iter()
        .map(|(_, ids)| (triple_correlation(o, ids, o.p.len()), ids))
        .collect::<Vec<_>>();
    stats.full_scored = full.len();
    full.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    full.truncate(TOP_TRIPLES);
    (
        full.into_iter().map(|(_, ids)| triplet(o, ids)).collect(),
        stats,
    )
}

fn triplet(o: &Observed, ids: [usize; 3]) -> [Source; 3] {
    ids.map(|index| o.sources[index])
}
fn pair_extension(o: &Observed, scores: &[PairScore]) -> Vec<[Source; 3]> {
    let mut out = BTreeSet::new();
    for pair in scores.iter().take(TOP_PAIRS) {
        for k in 0..o.sources.len() {
            if o.same_gate(pair.ids[0], k) || o.same_gate(pair.ids[1], k) {
                continue;
            }
            let mut ids = [pair.ids[0], pair.ids[1], k];
            ids.sort_unstable();
            out.insert(triplet(o, ids));
        }
    }
    out.into_iter().collect()
}
fn count_combinations(o: &Observed) -> (usize, usize) {
    let p = o.sources.len() / 4;
    (p * (p - 1) / 2 * 16, p * (p - 1) * (p - 2) / 6 * 64)
}

struct Frozen {
    task: Task,
    panel: Panel,
    seed: usize,
    n: usize,
    method: Method,
    proposals: usize,
    pair_rank: usize,
    full_pairs: usize,
    full_triples: usize,
    w3_full_correlations: usize,
    sketch: SketchStats,
    screen_seconds: f64,
    factor_seconds: f64,
    fit: Option<BudgetFit>,
}

fn freeze(
    identity: (Task, Panel, usize, usize),
    method: Method,
    train: &LaneData,
    validation: &LaneData,
    observed: &Observed,
) -> Result<Frozen, Box<dyn Error>> {
    let (task, panel, seed_index, n) = identity;
    let expected = task.sources();
    let (full_pairs, full_triples) = count_combinations(observed);
    let mut pair_rank = 0usize;
    let mut stats = SketchStats::default();
    let screen_start = Instant::now();
    let mut hypotheses = BTreeSet::<[Source; 3]>::new();
    match method {
        Method::Walsh2Pair | Method::Hybrid => {
            let pair_scores = rank_pairs(observed);
            if task == Task::AndXorXor {
                pair_rank = pair_scores
                    .iter()
                    .position(|pair| {
                        observed.sources[pair.ids[0]] == expected[1]
                            && observed.sources[pair.ids[1]] == expected[2]
                    })
                    .map_or(0, |index| index + 1);
            }
            hypotheses.extend(pair_extension(observed, &pair_scores));
            if method == Method::Hybrid {
                let (sketched, measured) = rank_sketch_triplets(observed);
                stats = measured;
                hypotheses.extend(sketched);
            }
        }
        Method::SketchMitm => {
            let (sketched, measured) = rank_sketch_triplets(observed);
            stats = measured;
            hypotheses.extend(sketched);
        }
        Method::Walsh3Stream => {
            hypotheses.extend(rank_triples_stream(observed));
        }
    }
    let true_structure_proposed = hypotheses.contains(&expected);
    let proposed = hypotheses.into_iter().collect::<Vec<_>>();
    let screen_seconds = screen_start.elapsed().as_secs_f64();
    let factor_start = Instant::now();
    let fit = if proposed.is_empty() {
        None
    } else {
        Some(fit_three_gate_candidates(
            train, validation, WIDTH, &proposed,
        )?)
    };
    let factor_seconds = factor_start.elapsed().as_secs_f64();
    let w3_full_correlations = if method == Method::Walsh3Stream {
        full_triples
    } else {
        0
    };
    println!(
        "A6C12C_FROZEN,task={},panel={},seed={},n={},method={},proposals={},true_structure_proposed={},pair_rank={},w2_full_pairs={},w3_full_correlations={},sketch_pair_probes={},sketch_bucket_hits={},sketch_unique={},sketch_coarse={},sketch_full={},screen_seconds={:.6},factor_seconds={:.6},sources={:?},truths={:?},accepted={}",
        task.name(),panel.name(),seed_index,n,method.name(),proposed.len(),
        true_structure_proposed,pair_rank,
        if matches!(method, Method::Walsh2Pair | Method::Hybrid) {full_pairs} else {0},
        w3_full_correlations,stats.pair_probes,stats.bucket_hits,
        stats.unique,stats.coarse_scored,stats.full_scored,
        screen_seconds,factor_seconds,
        fit.as_ref().map(|x| x.proposal_sources),
        fit.as_ref().map(|x| x.proposal_rules.rules.map(|r| r.truth)),
        fit.as_ref().is_some_and(|x| x.accepted)
    );
    Ok(Frozen {
        task,
        panel,
        seed: seed_index,
        n,
        method,
        proposals: proposed.len(),
        pair_rank,
        full_pairs,
        full_triples,
        w3_full_correlations,
        sketch: stats,
        screen_seconds,
        factor_seconds,
        fit,
    })
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() > 2 {
        return Err("usage: sml-sketch-mitm-bench [seeds:1..=3]".into());
    }
    let nseeds: usize = args.get(1).map_or(Ok(3), |arg| arg.parse())?;
    if !(1..=3).contains(&nseeds) {
        return Err("seed count must be 1..=3".into());
    }
    println!(
        "A6C12C_CONFIG,seeds={nseeds},tasks=xor_xor_xor;and_xor_xor;xor_and_xor;and_and_and,panels=clean;noise_1_in_16,n=32;128;508,train=512,val=512,test=4096,lanes=64,sketch_rounds={SKETCH_ROUNDS},sketch_bits={SKETCH_BITS},sketch_coarse_words={SKETCH_COARSE_WORDS},sketch_coarse_keep={SKETCH_COARSE_KEEP},sketch_full_keep={TOP_TRIPLES},top_w2_pairs={TOP_PAIRS}"
    );
    println!(
        "A6C12C_PROTOCOL,teacher_rules_not_passed_to_screening=true,validation_gate_only=true,all_frozen_before_test=true,w3_n508_seed0_only=true,hash_seed_fixed=true"
    );
    let mut frozen = Vec::with_capacity(4 * 2 * nseeds * 3 * 4);
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                let train = data(
                    seed(index, task, panel, 0xa6c1_2b11),
                    task,
                    panel,
                    TRAIN_STEPS,
                );
                let validation = data(
                    seed(index, task, panel, 0xa6c1_2b22),
                    task,
                    panel,
                    VAL_STEPS,
                );
                for n in NS {
                    let observed = Observed::new(&train, grid(n));
                    assert_eq!(observed.p.len(), TRAIN_STEPS - WIDTH);
                    assert!(task
                        .sources()
                        .iter()
                        .all(|source| observed.sources.contains(source)));
                    let (pairs, triples) = count_combinations(&observed);
                    println!(
                        "A6C12C_GRID,task={},panel={},seed={},n={},pairs={},triples={},samples={}",
                        task.name(),
                        panel.name(),
                        index,
                        n,
                        pairs,
                        triples,
                        observed.p.len() * LANES
                    );
                    for method in Method::ALL {
                        if method == Method::Walsh3Stream && n == 508 && index != 0 {
                            continue;
                        }
                        frozen.push(freeze(
                            (task, panel, index, n),
                            method,
                            &train,
                            &validation,
                            &observed,
                        )?);
                    }
                }
            }
        }
    }
    let expected_frozen = 4 * 2 * (nseeds * 3 * 3 + nseeds * 2 + 1);
    if frozen.len() != expected_frozen {
        return Err("A6-C12c frozen cardinality violated".into());
    }
    println!(
        "A6C12C_ALL_FROZEN,policies={},streams={}",
        frozen.len(),
        4 * 2 * nseeds
    );
    // TEST generators are invoked only after every policy has been frozen.
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                let test = data(
                    seed(index, task, panel, 0xa6c1_2b33),
                    task,
                    panel,
                    TEST_STEPS,
                );
                let teacher = make_multi_gate_native(WIDTH, &task.rules())?;
                let oracle_errors = native_errors(&teacher, &test)?;
                for result in frozen
                    .iter()
                    .filter(|r| r.task == task && r.panel == panel && r.seed == index)
                {
                    let (sources_exact, truths_exact, proposal_errors, selected_errors, accepted) =
                        if let Some(fit) = &result.fit {
                            (
                                fit.proposal_sources == task.sources(),
                                fit.proposal_rules.rules.map(|rule| rule.truth) == task.truths(),
                                Some(native_errors(&fit.proposal, &test)?),
                                Some(native_errors(&fit.committed, &test)?),
                                fit.accepted,
                            )
                        } else {
                            (false, false, None, None, false)
                        };
                    println!(
                        "A6C12C_TEST,task={},panel={},seed={},n={},method={},oracle_errors={},proposal_errors={:?},selected_errors={:?},sources_exact={},truths_exact={},accepted={},pair_rank={},factor_structures={},w2_full_pairs={},w3_full_correlations={},sketch_pair_probes={},sketch_bucket_hits={},sketch_unique={},sketch_coarse={},sketch_full={},screen_seconds={:.6},factor_seconds={:.6},samples={},triples_admissible={}",
                        task.name(),panel.name(),index,result.n,result.method.name(),
                        oracle_errors,proposal_errors,selected_errors,sources_exact,truths_exact,
                        accepted,result.pair_rank,result.proposals,
                        if matches!(result.method, Method::Walsh2Pair | Method::Hybrid) {result.full_pairs} else {0},
                        result.w3_full_correlations,result.sketch.pair_probes,
                        result.sketch.bucket_hits,result.sketch.unique,
                        result.sketch.coarse_scored,result.sketch.full_scored,
                        result.screen_seconds,result.factor_seconds,TEST_STEPS*LANES,result.full_triples
                    );
                }
            }
        }
    }
    println!(
        "A6C12C_COMPLETE,policies={},independent_test_streams={},grid_sizes=3,methods=4",
        frozen.len(),
        4 * 2 * nseeds
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preregistered_grids_cover_native_sources() {
        for (n, pairs, triples) in [
            (32, 448, 3_584),
            (128, 7_936, 317_440),
            (508, 128_016, 21_336_000),
        ] {
            let observed = Observed::new(
                &data(11, Task::XorXorXor, Panel::Clean, TRAIN_STEPS),
                grid(n),
            );
            assert_eq!(count_combinations(&observed), (pairs, triples));
            assert!(Task::XorXorXor
                .sources()
                .iter()
                .all(|source| observed.sources.contains(source)));
        }
    }

    #[test]
    fn sketch_key_is_stable_and_uses_requested_bits() {
        let mut words = vec![0u64; 32];
        words[1] = u64::MAX;
        words[16] = 0b11;
        let samples: [usize; SKETCH_BITS] = [0, 1, 2, 3, 4, 5, 6, 64, 65, 66, 67, 1024, 1025, 1026];
        let key = sketch_key(&words, &samples);
        assert_eq!(key, sketch_key(&words, &samples));
        assert_eq!(key & (1 << 7), 1 << 7);
        assert_eq!(key & (1 << 8), 1 << 8);
        assert_eq!(key & (1 << 11), 1 << 11);
        assert_eq!(key & (1 << 12), 1 << 12);
    }

    #[test]
    fn clean_parity_is_found_deterministically_and_has_distinct_gates() {
        let observed = Observed::new(
            &data(91, Task::XorXorXor, Panel::Clean, TRAIN_STEPS),
            grid(32),
        );
        let (candidates, stats) = rank_sketch_triplets(&observed);
        assert!(candidates.contains(&Task::XorXorXor.sources()));
        assert!(stats.full_scored <= SKETCH_COARSE_KEEP);
        assert!(stats.coarse_scored >= stats.full_scored);
        assert!(candidates.iter().all(|sources| {
            sources[0].gate < sources[1].gate && sources[1].gate < sources[2].gate
        }));
        assert!(rank_triples_stream(&observed).contains(&Task::XorXorXor.sources()));
        let (again, repeated_stats) = rank_sketch_triplets(&observed);
        assert_eq!(again, candidates);
        assert_eq!(repeated_stats, stats);
    }

    #[test]
    fn same_native_anf_factorizer_fits_screened_clean_parity() {
        let train = data(94, Task::XorXorXor, Panel::Clean, TRAIN_STEPS);
        let val = data(95, Task::XorXorXor, Panel::Clean, VAL_STEPS);
        let observed = Observed::new(&train, grid(32));
        let (candidates, _) = rank_sketch_triplets(&observed);
        let fit = fit_three_gate_candidates(&train, &val, WIDTH, &candidates).unwrap();
        assert_eq!(fit.autonomous_train_errors, 0);
        assert_eq!(fit.proposal_sources, Task::XorXorXor.sources());
    }
}

//! A6-C12g: preregistered noise/sample phase transition, Rust native.
//! C12c screening code carried forward unchanged; new data and evaluation.
//! All TEST streams are created only after every fit is frozen.

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
const TRAIN_STEPS: usize = 640;
const VAL_STEPS: usize = 640;
const TEST_STEPS: usize = 4096;
const TOP_PAIRS: usize = 8;
const TOP_TRIPLES: usize = 16;
const SKETCH_ROUNDS: usize = 48;
const SKETCH_BITS: usize = 14;
const SKETCH_COARSE_WORDS: usize = 8;
const SKETCH_COARSE_KEEP: usize = 256;
const SKETCH_SEED: u64 = 0xA6C1_2C5C_0123_4567;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    XorXorXor,
    AndXorXor,
    XorAndXor,
    AndAndAnd,
}
impl Task {
    const ALL: [Self; 4] = [
        Self::XorXorXor,
        Self::AndXorXor,
        Self::XorAndXor,
        Self::AndAndAnd,
    ];
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

// A6-C12g: the two preregistered teacher families are the first two C12c tasks.
const TASKS: [Task; 2] = [Task::ALL[0], Task::ALL[1]];
const WORD_BUDGETS: [usize; 7] = [8, 16, 32, 64, 128, 256, 512];
const SEEDS: usize = 8;
const REPEATS: usize = 3;
const TEST_STREAMS: usize = 2 * 5 * SEEDS;
const POLICY_COUNT: usize = 2 * 5 * 7 * SEEDS * 2 + 2 * 7 * SEEDS;
const GAUSS_MAX_WORDS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoiseRate {
    Zero,
    OneIn32,
    OneIn16,
    OneIn8,
    OneIn5,
}
impl NoiseRate {
    const ALL: [Self; 5] = [
        Self::Zero,
        Self::OneIn32,
        Self::OneIn16,
        Self::OneIn8,
        Self::OneIn5,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::Zero => "0",
            Self::OneIn32 => "1/32",
            Self::OneIn16 => "1/16",
            Self::OneIn8 => "1/8",
            Self::OneIn5 => "1/5",
        }
    }
    fn eta(self) -> f64 {
        match self {
            Self::Zero => 0.0,
            Self::OneIn32 => 1.0 / 32.0,
            Self::OneIn16 => 1.0 / 16.0,
            Self::OneIn8 => 1.0 / 8.0,
            Self::OneIn5 => 0.2,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    Walsh3Stream,
    Hybrid,
    GaussExact,
}
impl Method {
    const TWO: [Self; 2] = [Self::Walsh3Stream, Self::Hybrid];
    fn name(self) -> &'static str {
        match self {
            Self::Walsh3Stream => "walsh3_stream",
            Self::Hybrid => "hybrid_w2_mitm",
            Self::GaussExact => "gauss_clean",
        }
    }
}
fn seed(index: usize, task: Task, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ stage
}
fn noise_mask(rate: NoiseRate, rng: &mut SplitMix64) -> u64 {
    match rate {
        NoiseRate::Zero => 0,
        NoiseRate::OneIn32 => {
            rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64()
        }
        NoiseRate::OneIn16 => rng.next_u64() & rng.next_u64() & rng.next_u64() & rng.next_u64(),
        NoiseRate::OneIn8 => rng.next_u64() & rng.next_u64() & rng.next_u64(),
        NoiseRate::OneIn5 => {
            let mut bits = 0u64;
            for bit in 0..LANES {
                if rng.next_u64() < u64::MAX / 5 {
                    bits |= 1u64 << bit;
                }
            }
            bits
        }
    }
}
fn data(stream: u64, task: Task, rate: NoiseRate, steps: usize) -> LaneData {
    let teacher = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
    let mut rng = SplitMix64::new(stream);
    let mut noise_rng = SplitMix64::new(stream ^ 0xa6c1_2b66_dada_1177);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
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
        *target = clean ^ noise_mask(rate, &mut noise_rng);
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
fn prefix(data: &LaneData, steps: usize) -> LaneData {
    let b = &data.blocks[0];
    assert!(steps <= b.steps && steps > WIDTH);
    LaneData {
        input_bits: INPUT_BITS,
        target_bits: 1,
        blocks: vec![LaneBlock {
            steps,
            inputs: b.inputs[..steps * INPUT_BITS].to_vec(),
            targets: b.targets[..steps].to_vec(),
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

// Exact GF(2) column elimination. Never executed on noisy labels.
fn lowest_bit(values: &[u64]) -> Option<usize> {
    values
        .iter()
        .enumerate()
        .find(|(_, word)| **word != 0)
        .map(|(index, &word)| index * 64 + word.trailing_zeros() as usize)
}
fn xor_assign(to: &mut [u64], other: &[u64]) {
    for (left, right) in to.iter_mut().zip(other) {
        *left ^= *right;
    }
}
fn solve_gauss(o: &Observed) -> Option<Vec<usize>> {
    if o.p.is_empty() || o.sources.len() > 128 {
        return None;
    }
    let words = o.p.len().min(GAUSS_MAX_WORDS);
    let repwords = o.sources.len().div_ceil(64);
    let mut pivot = vec![None::<(Vec<u64>, Vec<u64>)>; words * LANES];
    for (index, column) in o.columns.iter().enumerate() {
        let mut bits = column[..words].to_vec();
        let mut rep = vec![0u64; repwords];
        rep[index / 64] |= 1u64 << (index % 64);
        while let Some(row) = lowest_bit(&bits) {
            if let Some((basis, coordinates)) = &pivot[row] {
                xor_assign(&mut bits, basis);
                xor_assign(&mut rep, coordinates);
            } else {
                pivot[row] = Some((bits, rep));
                break;
            }
        }
    }
    let mut target = o.p[..words].to_vec();
    let mut rep = vec![0u64; repwords];
    while let Some(row) = lowest_bit(&target) {
        if let Some((basis, coordinates)) = &pivot[row] {
            xor_assign(&mut target, basis);
            xor_assign(&mut rep, coordinates);
        } else {
            return None;
        }
    }
    let selected = (0..o.sources.len())
        .filter(|&i| rep[i / 64] & (1u64 << (i % 64)) != 0)
        .collect::<Vec<_>>();
    if o.p
        .iter()
        .enumerate()
        .any(|(t, &p)| selected.iter().fold(p, |v, &i| v ^ o.columns[i][t]) != 0)
    {
        return None;
    }
    Some(selected)
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Screen {
    hypotheses: Vec<[Source; 3]>,
    stats: SketchStats,
}
fn propose(o: &Observed, method: Method) -> Screen {
    let mut proposals = BTreeSet::new();
    let mut stats = SketchStats::default();
    match method {
        Method::Walsh3Stream => proposals.extend(rank_triples_stream(o)),
        Method::Hybrid => {
            proposals.extend(pair_extension(o, &rank_pairs(o)));
            let (sketched, measured) = rank_sketch_triplets(o);
            proposals.extend(sketched);
            stats = measured;
        }
        Method::GaussExact => {
            if let Some(selected) = solve_gauss(o) {
                if selected.len() == 3 {
                    let found = triplet(o, [selected[0], selected[1], selected[2]]);
                    if found[0].gate < found[1].gate && found[1].gate < found[2].gate {
                        proposals.insert(found);
                    }
                }
            }
        }
    }
    Screen {
        hypotheses: proposals.into_iter().collect(),
        stats,
    }
}
fn median(mut readings: [f64; REPEATS]) -> f64 {
    readings.sort_by(|a, b| a.total_cmp(b));
    readings[REPEATS / 2]
}
fn median_vector(mut readings: Vec<f64>) -> f64 {
    readings.sort_by(|a, b| a.total_cmp(b));
    let len = readings.len();
    assert!(!readings.is_empty());
    (readings[(len - 1) / 2] + readings[len / 2]) / 2.0
}
fn score_margin(
    o: &Observed,
    true_sources: [Source; 3],
    hypotheses: &[[Source; 3]],
) -> (usize, f64, f64, f64) {
    let mut indices = [0usize; 3];
    for (i, source) in true_sources.iter().enumerate() {
        indices[i] = o.sources.iter().position(|value| value == source).unwrap();
    }
    let denom = (o.p.len() * LANES) as f64;
    let true_score = triple_correlation(o, indices, o.p.len()) as f64 / denom;
    let rank = hypotheses
        .iter()
        .position(|triplet| *triplet == true_sources)
        .map_or(0, |i| i + 1);
    let best_false = hypotheses
        .iter()
        .filter(|triplet| **triplet != true_sources)
        .map(|triplet| {
            let ids = triplet.map(|s| o.sources.iter().position(|x| *x == s).unwrap());
            triple_correlation(o, ids, o.p.len()) as f64 / denom
        })
        .fold(0.0f64, f64::max);
    (rank, true_score, best_false, true_score - best_false)
}
struct Frozen {
    task: Task,
    rate: NoiseRate,
    seed: usize,
    words: usize,
    method: Method,
    fit: Option<BudgetFit>,
    proposed: bool,
    exact: bool,
    median_screen_us: f64,
    median_factor_us: f64,
    median_total_us: f64,
}
fn freeze(
    task: Task,
    rate: NoiseRate,
    sample: (usize, usize),
    method: Method,
    train: &LaneData,
    validation: &LaneData,
    observed: &Observed,
) -> Result<Frozen, Box<dyn Error>> {
    let (index, words) = sample;
    let mut times_screen = [0.0f64; REPEATS];
    let mut times_factor = [0.0f64; REPEATS];
    let mut times_total = [0.0f64; REPEATS];
    let mut first_screen: Option<Screen> = None;
    let mut first_fit: Option<BudgetFit> = None;
    for repeat in 0..REPEATS {
        let before = Instant::now();
        let screen = propose(observed, method);
        times_screen[repeat] = before.elapsed().as_secs_f64() * 1e6;
        let factor_before = Instant::now();
        let fit = if screen.hypotheses.is_empty() {
            None
        } else {
            Some(fit_three_gate_candidates(
                train,
                validation,
                WIDTH,
                &screen.hypotheses,
            )?)
        };
        times_factor[repeat] = factor_before.elapsed().as_secs_f64() * 1e6;
        times_total[repeat] = times_screen[repeat] + times_factor[repeat];
        if repeat == 0 {
            first_screen = Some(screen);
            first_fit = fit;
        } else {
            assert_eq!(
                first_screen.as_ref(),
                Some(&screen),
                "screen changed across repetitions"
            );
            assert_eq!(
                first_fit.as_ref(),
                fit.as_ref(),
                "native fit changed across repetitions"
            );
        }
    }
    let screen = first_screen.unwrap();
    let sources = task.sources();
    let exact = first_fit.as_ref().is_some_and(|fit| {
        fit.proposal_sources == sources
            && fit.proposal_rules.rules.map(|r| r.truth) == task.truths()
    });
    let proposed = screen.hypotheses.contains(&sources);
    let (rank, true_score, best_false, margin) = if method == Method::Walsh3Stream {
        score_margin(observed, sources, &screen.hypotheses)
    } else {
        (0, f64::NAN, f64::NAN, f64::NAN)
    };
    let pair_rank = if method == Method::Hybrid && task == Task::AndXorXor {
        rank_pairs(observed)
            .iter()
            .position(|p| {
                observed.sources[p.ids[0]] == sources[1] && observed.sources[p.ids[1]] == sources[2]
            })
            .map_or(0, |i| i + 1)
    } else {
        0
    };
    let ms = median(times_screen);
    let mf = median(times_factor);
    let mt = median(times_total);
    println!(
        "A6C12G_FROZEN,task={},seed={},rate={},eta={:.8},words={},proxy_bits={},method={},status={},proposals={},proposed={},exact={},accepted={},rank_true_top16={},true_w3_abs={:.8},best_false_w3_abs={:.8},w3_signed_margin={:.8},pair_rank={},sketch_pair_probes={},sketch_bucket_hits={},sketch_unique={},sketch_coarse={},sketch_full={},screen_us={:.3};{:.3};{:.3},factor_us={:.3};{:.3};{:.3},total_us={:.3};{:.3};{:.3},median_screen_us={:.3},median_factor_us={:.3},median_total_us={:.3}",
        task.name(),index,rate.name(),rate.eta(),words,words*LANES,method.name(),
        if first_fit.is_some() {"FITTED"} else {"NO_EXACT_GAUSS_OR_EMPTY"},
        screen.hypotheses.len(), proposed, exact,
        first_fit.as_ref().is_some_and(|fit| fit.accepted),
        rank, true_score, best_false, margin, pair_rank,
        screen.stats.pair_probes, screen.stats.bucket_hits, screen.stats.unique,
        screen.stats.coarse_scored, screen.stats.full_scored,
        times_screen[0], times_screen[1], times_screen[2],
        times_factor[0], times_factor[1], times_factor[2],
        times_total[0], times_total[1], times_total[2],
        ms, mf, mt
    );
    Ok(Frozen {
        task,
        rate,
        seed: index,
        words,
        method,
        fit: first_fit,
        proposed,
        exact,
        median_screen_us: ms,
        median_factor_us: mf,
        median_total_us: mt,
    })
}
fn native_prediction_disagreements(
    a: &sml_core::anf_local::LocalAnfNetwork,
    b: &sml_core::anf_local::LocalAnfNetwork,
    data: &LaneData,
) -> u64 {
    let mut total = 0u64;
    let mut state_a = vec![0u64; a.source_width()];
    let mut state_b = vec![0u64; b.source_width()];
    let mut next_a = vec![0u64; WIDTH];
    let mut next_b = vec![0u64; WIDTH];
    let mut scratch_a = vec![0u64; a.scratch_len()];
    let mut scratch_b = vec![0u64; b.scratch_len()];
    for block in &data.blocks {
        state_a.fill(0);
        state_b.fill(0);
        for t in 0..block.steps {
            let from = t * INPUT_BITS;
            let raw = &block.inputs[from..from + INPUT_BITS];
            state_a[WIDTH..].copy_from_slice(raw);
            state_b[WIDTH..].copy_from_slice(raw);
            a.step_lanes(&mut state_a, &mut next_a, &mut scratch_a);
            b.step_lanes(&mut state_b, &mut next_b, &mut scratch_b);
            let y_a = a.readout().evaluate_lanes(&state_a, &mut scratch_a);
            let y_b = b.readout().evaluate_lanes(&state_b, &mut scratch_b);
            total += u64::from(((y_a ^ y_b) & block.valid).count_ones());
        }
    }
    total
}
fn wilson(success: usize, trials: usize) -> (f64, f64) {
    let n = trials as f64;
    let z = 1.959_963_984_540_054f64;
    let p = success as f64 / n;
    let z2 = z * z;
    let base = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let radius = z / (1.0 + z2 / n) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((base - radius).max(0.0), (base + radius).min(1.0))
}
fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        return Err("usage: sml-noise-sample-phase-bench (fixed preregistered matrix)".into());
    }
    let sources = grid(128);
    let sample = Observed::new(
        &data(0x1234, Task::XorXorXor, NoiseRate::Zero, TRAIN_STEPS),
        sources,
    );
    assert_eq!(count_combinations(&sample), (7_936, 317_440));
    assert_eq!(TEST_STREAMS, 80);
    assert_eq!(POLICY_COUNT, 1232);
    println!("A6C12G_CONFIG,seeds=8,tasks=xor_xor_xor;and_xor_xor,rates=0;1/32;1/16;1/8;1/5,words=8;16;32;64;128;256;512,n=128,train_max=640,val=640,test=4096,lanes=64,repeats=3,policy_count=1232,test_datasets=80");
    println!("A6C12G_THEORY,admissible_triples=317440,train_proxy_uses_two_noisy_labels=true,noise_proxy=2eta(1-eta),rho_proxy=(1-2eta)^2,gauss_noisy_skipped=true,test_after_freeze=true");
    let mut frozen = Vec::with_capacity(POLICY_COUNT);
    for task in TASKS {
        for index in 0..SEEDS {
            for rate in NoiseRate::ALL {
                let full_train = data(seed(index, task, 0xa6c1_2b11), task, rate, TRAIN_STEPS);
                let validation = data(seed(index, task, 0xa6c1_2b22), task, rate, VAL_STEPS);
                for words in WORD_BUDGETS {
                    let train = prefix(&full_train, WIDTH + words);
                    let observed = Observed::new(&train, grid(128));
                    assert_eq!(observed.p.len(), words);
                    for method in Method::TWO {
                        frozen.push(freeze(
                            task,
                            rate,
                            (index, words),
                            method,
                            &train,
                            &validation,
                            &observed,
                        )?);
                    }
                    if rate == NoiseRate::Zero {
                        frozen.push(freeze(
                            task,
                            rate,
                            (index, words),
                            Method::GaussExact,
                            &train,
                            &validation,
                            &observed,
                        )?);
                    }
                }
            }
        }
    }
    if frozen.len() != POLICY_COUNT {
        return Err("A6-C12g incorrect frozen policy census".into());
    }
    println!("A6C12G_ALL_FROZEN,policies=1232,test_datasets=80");

    // Only TEST outcomes, not TRAIN/VAL exactness alone, define empirical success.
    // TEST data are generated exclusively after the full 1232-fit freeze.
    let mut test_outcomes = vec![None; POLICY_COUNT];
    let mut test_rows = 0usize;
    let mut distinct_test_datasets = 0usize;
    for task in TASKS {
        for index in 0..SEEDS {
            for rate in NoiseRate::ALL {
                let test = data(seed(index, task, 0xa6c1_2b33), task, rate, TEST_STEPS);
                distinct_test_datasets += 1;
                let teacher = make_multi_gate_native(WIDTH, &task.rules())?;
                let teacher_errors = native_errors(&teacher, &test)?;
                for (case_idx, case) in frozen
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.task == task && f.seed == index && f.rate == rate)
                {
                    let (
                        proposal_errors,
                        committed_errors,
                        proposal_disagreements,
                        committed_disagreements,
                        accepted,
                    ) = if let Some(fit) = &case.fit {
                        (
                            Some(native_errors(&fit.proposal, &test)?),
                            Some(native_errors(&fit.committed, &test)?),
                            Some(native_prediction_disagreements(
                                &fit.proposal,
                                &teacher,
                                &test,
                            )),
                            Some(native_prediction_disagreements(
                                &fit.committed,
                                &teacher,
                                &test,
                            )),
                            fit.accepted,
                        )
                    } else {
                        (None, None, None, None, false)
                    };
                    let success = case.exact
                        && proposal_disagreements == Some(0)
                        && accepted
                        && committed_disagreements == Some(0);
                    test_outcomes[case_idx] = Some(success);
                    println!(
                        "A6C12G_TEST,task={},seed={},rate={},words={},method={},proposed={},exact={},accepted={},native_success={},oracle_errors={},proposal_errors={:?},committed_errors={:?},proposal_bit_disagreements={:?},committed_bit_disagreements={:?},samples={}",
                        task.name(),index,rate.name(),case.words,case.method.name(),
                        case.proposed,case.exact,accepted,success,teacher_errors,proposal_errors,committed_errors,
                        proposal_disagreements,committed_disagreements,TEST_STEPS*LANES
                    );
                    test_rows += 1;
                }
            }
        }
    }
    if test_rows != POLICY_COUNT
        || distinct_test_datasets != TEST_STREAMS
        || test_outcomes.iter().any(Option::is_none)
    {
        return Err("A6-C12g TEST census mismatch".into());
    }

    for task in TASKS {
        for rate in NoiseRate::ALL {
            let eta = rate.eta();
            let rho = (1.0 - 2.0 * eta).powi(2);
            for method in [Method::Walsh3Stream, Method::Hybrid, Method::GaussExact] {
                if method == Method::GaussExact && rate != NoiseRate::Zero {
                    continue;
                }
                let mut first_empirical_threshold = None;
                for words in WORD_BUDGETS {
                    let rows = frozen
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| {
                            r.task == task
                                && r.rate == rate
                                && r.method == method
                                && r.words == words
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(rows.len(), SEEDS);
                    let train_exact_accepted = rows
                        .iter()
                        .filter(|(_, r)| r.exact && r.fit.as_ref().is_some_and(|fit| fit.accepted))
                        .count();
                    let successes = rows
                        .iter()
                        .filter(|(i, _)| test_outcomes[*i] == Some(true))
                        .count();
                    let (lower, upper) = wilson(successes, SEEDS);
                    if successes >= 7 && first_empirical_threshold.is_none() {
                        first_empirical_threshold = Some(words);
                    }
                    let screen =
                        median_vector(rows.iter().map(|(_, r)| r.median_screen_us).collect());
                    let factor =
                        median_vector(rows.iter().map(|(_, r)| r.median_factor_us).collect());
                    let total =
                        median_vector(rows.iter().map(|(_, r)| r.median_total_us).collect());
                    println!(
                        "A6C12G_SUMMARY,task={},rate={},words={},method={},train_exact_and_accepted={}/{},native_success={}/{},wilson95_lower={:.6},wilson95_upper={:.6},median_screen_us={:.3},median_factor_us={:.3},median_total_us={:.3},rho_proxy={:.6},ln_m_over_rho2={:.3}",
                        task.name(), rate.name(), words, method.name(), train_exact_accepted, SEEDS, successes, SEEDS, lower, upper,
                        screen, factor, total, rho, (317_440f64).ln() / (rho*rho)
                    );
                }
                println!(
                    "A6C12G_THRESHOLD,task={},rate={},method={},first_words_ge7_of8={:?}",
                    task.name(),
                    rate.name(),
                    method.name(),
                    first_empirical_threshold
                );
            }
        }
    }
    println!("A6C12G_COMPLETE,policies=1232,test_rows=1232,distinct_test_datasets=80,noise_rates=5,word_budgets=7,seed_count=8,fit_repeats=3");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_grid_and_noise_stages() {
        assert_eq!(POLICY_COUNT, 1232);
        let observed = Observed::new(
            &data(19, Task::XorXorXor, NoiseRate::Zero, TRAIN_STEPS),
            grid(128),
        );
        assert_eq!(count_combinations(&observed), (7_936, 317_440));
        assert!(Task::XorXorXor
            .sources()
            .iter()
            .all(|source| observed.sources.contains(source)));
        let train = data(20, Task::XorXorXor, NoiseRate::Zero, TRAIN_STEPS);
        for words in WORD_BUDGETS {
            assert_eq!(
                Observed::new(&prefix(&train, WIDTH + words), grid(128))
                    .p
                    .len(),
                words
            );
        }
    }
    #[test]
    fn noise_rate_and_wilson_are_valid() {
        assert!((NoiseRate::OneIn5.eta() - 0.2).abs() < 1e-12);
        for rate in NoiseRate::ALL {
            let eta = rate.eta();
            let rho = (1.0 - 2.0 * eta).powi(2);
            assert!(rho >= 0.36 - 1e-10);
        }
        assert!((wilson(8, 8).0 - 0.6755924).abs() < 1e-4);
        assert!(wilson(0, 8).1 > 0.3);
    }
    #[test]
    fn clean_gauss_solve_noisy_is_never_invoked() {
        let train = data(47, Task::XorXorXor, NoiseRate::Zero, TRAIN_STEPS);
        let o = Observed::new(&train, grid(128));
        let solved = solve_gauss(&o).unwrap();
        assert_eq!(
            triplet(&o, [solved[0], solved[1], solved[2]]),
            Task::XorXorXor.sources()
        );
    }
}

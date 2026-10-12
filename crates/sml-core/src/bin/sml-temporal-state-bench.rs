//! A6-C12e: preregistered temporal-lag input ambiguity and state-transition
//! corruption; comparison with the unchanged n508 W2/Sketch-MITM/W3 engines.
//! Native three-gate state N128. TEST is generated only after fitted policies
//! have frozen. No claims for free graphs, general LPN or SML-25B.

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
const TEMPORAL_SALT: u64 = 0xA6C1_2E11_5EED_0101;
const HIDDEN_STATE_SALT: u64 = 0xA6C1_2E22_5EED_0202;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    XorXorXor,
    AndXorXor,
}
impl Task {
    const ALL: [Self; 2] = [Self::XorXorXor, Self::AndXorXor];
    fn name(self) -> &'static str {
        match self {
            Self::XorXorXor => "xor_xor_xor",
            Self::AndXorXor => "and_xor_xor",
        }
    }
    fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap()
    }
    fn truths(self) -> [u8; 3] {
        match self {
            Self::XorXorXor => [6, 6, 6],
            Self::AndXorXor => [8, 6, 6],
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
    NearLag,
    ExactLag,
    StateFlip,
}
impl InputMode {
    const STRESS: [Self; 3] = [Self::NearLag, Self::ExactLag, Self::StateFlip];
    fn name(self) -> &'static str {
        match self {
            Self::NearLag => "near_lag_1_in_32",
            Self::ExactLag => "exact_lag",
            Self::StateFlip => "hidden_state_flip_1_in_256",
        }
    }
}

fn one_in_32(rng: &mut SplitMix64) -> u64 {
    (0..5).fold(u64::MAX, |mask, _| mask & rng.next_u64())
}

fn one_in_256(rng: &mut SplitMix64) -> u64 {
    (0..8).fold(u64::MAX, |mask, _| mask & rng.next_u64())
}

fn seed(index: usize, task: Task, panel: Panel, stage: u64) -> u64 {
    (index as u64 + 1).wrapping_mul(0xd134_2543_de82_ef95)
        ^ (task.index() as u64 + 1).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (panel.index() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ stage
}

/// The hidden disturbance is injected after recurrent transition and carried
/// into the next step. It is a true state perturbation, not post-hoc label noise.
fn data_with_mode(
    stream: u64,
    task: Task,
    panel: Panel,
    steps: usize,
    mode: InputMode,
) -> (LaneData, u64) {
    let teacher = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
    let mut rng = SplitMix64::new(stream);
    let mut label_noise_rng = SplitMix64::new(stream ^ 0xa6c1_2b66_dada_1177);
    let mut temporal_rng = SplitMix64::new(stream ^ TEMPORAL_SALT);
    let mut hidden_rng = SplitMix64::new(stream ^ HIDDEN_STATE_SALT);
    let mut inputs = vec![0u64; steps * INPUT_BITS];
    for word in &mut inputs {
        *word = rng.next_u64();
    }
    // Complete all correlated features before advancing the native teacher.
    // At t=0, the virtual historical source is zero in the initial state.
    for t in 0..steps {
        let offset = t * INPUT_BITS;
        let prior_x2 = if t == 0 {
            0
        } else {
            inputs[(t - 1) * INPUT_BITS + 2]
        };
        let prior_x4 = if t == 0 {
            0
        } else {
            inputs[(t - 1) * INPUT_BITS + 4]
        };
        match mode {
            InputMode::NearLag => {
                inputs[offset + 1] = prior_x2 ^ one_in_32(&mut temporal_rng);
                inputs[offset + 3] = prior_x4 ^ one_in_32(&mut temporal_rng);
            }
            InputMode::ExactLag => {
                inputs[offset + 1] = prior_x2;
                inputs[offset + 3] = prior_x4;
            }
            InputMode::StateFlip => {}
        }
    }
    let mut state = vec![0u64; teacher.source_width()];
    let mut next = vec![0u64; WIDTH];
    let mut scratch = vec![0u64; teacher.scratch_len()];
    let mut targets = vec![0u64; steps];
    let mut injected_state_flips = 0u64;
    for (t, target) in targets.iter_mut().enumerate() {
        let from = t * INPUT_BITS;
        state[WIDTH..].copy_from_slice(&inputs[from..from + INPUT_BITS]);
        teacher.step_lanes(&mut state, &mut next, &mut scratch);
        if mode == InputMode::StateFlip {
            let disturbance = one_in_256(&mut hidden_rng);
            state[WIDTH - 1] ^= disturbance;
            injected_state_flips += u64::from(disturbance.count_ones());
        }
        let true_output = teacher.readout().evaluate_lanes(&state, &mut scratch);
        let label_error = if panel == Panel::Noise {
            label_noise_rng.next_u64()
                & label_noise_rng.next_u64()
                & label_noise_rng.next_u64()
                & label_noise_rng.next_u64()
        } else {
            0
        };
        *target = true_output ^ label_error;
    }
    (
        LaneData {
            input_bits: INPUT_BITS,
            target_bits: 1,
            blocks: vec![LaneBlock {
                steps,
                inputs,
                targets,
                valid: u64::MAX,
            }],
        },
        injected_state_flips,
    )
}

/// Diagnostic only: compare a channel to its source one time step earlier.
fn delayed_channel_mismatches(split: &LaneData) -> (u64, u64) {
    let block = &split.blocks[0];
    let mut x1_vs_prior_x2 = 0u64;
    let mut x3_vs_prior_x4 = 0u64;
    for t in 0..block.steps {
        let current = t * INPUT_BITS;
        let earlier_x2 = if t == 0 {
            0
        } else {
            block.inputs[(t - 1) * INPUT_BITS + 2]
        };
        let earlier_x4 = if t == 0 {
            0
        } else {
            block.inputs[(t - 1) * INPUT_BITS + 4]
        };
        x1_vs_prior_x2 += u64::from((block.inputs[current + 1] ^ earlier_x2).count_ones());
        x3_vs_prior_x4 += u64::from((block.inputs[current + 3] ^ earlier_x4).count_ones());
    }
    (x1_vs_prior_x2, x3_vs_prior_x4)
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

struct FrozenStress {
    mode: InputMode,
    fitted: Frozen,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() > 2 {
        return Err("usage: sml-temporal-state-bench [seeds:1..=3]".into());
    }
    let nseeds: usize = args.get(1).map_or(Ok(3), |arg| arg.parse())?;
    if !(1..=3).contains(&nseeds) {
        return Err("seed count must be 1..=3".into());
    }
    println!(
        "A6C12E_CONFIG,seeds={nseeds},tasks=xor_xor_xor;and_xor_xor,panels=clean;noise_1_in_16,modes=near_lag_1_in_32;exact_lag;hidden_state_flip_1_in_256,n=508,train=512,val=512,test=4096,lanes=64,sketch_rounds={SKETCH_ROUNDS},sketch_bits={SKETCH_BITS},top_w2_pairs={TOP_PAIRS}"
    );
    println!(
        "A6C12E_PROTOCOL,three_gate_N128=true,temporal_alias_shift=one_step,hidden_state_noise_slot=127,hidden_flip_prob=1_in_256,all_frozen_before_test=true,w3_seed0_only=true"
    );

    let mut frozen = Vec::<FrozenStress>::new();
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                for mode in InputMode::STRESS {
                    let (train, train_hidden_flips) = data_with_mode(
                        seed(index, task, panel, 0xa6c1_2b11),
                        task,
                        panel,
                        TRAIN_STEPS,
                        mode,
                    );
                    let (validation, val_hidden_flips) = data_with_mode(
                        seed(index, task, panel, 0xa6c1_2b22),
                        task,
                        panel,
                        VAL_STEPS,
                        mode,
                    );
                    let (lagged_x1_x2, lagged_x3_x4) = delayed_channel_mismatches(&train);
                    if mode == InputMode::ExactLag && (lagged_x1_x2 != 0 || lagged_x3_x4 != 0) {
                        return Err(
                            "exact temporal source generator violated one-step identity".into()
                        );
                    }
                    if mode == InputMode::StateFlip && train_hidden_flips == 0 {
                        return Err("state disturbance did not flip any bits on TRAIN".into());
                    }
                    if mode != InputMode::StateFlip
                        && (train_hidden_flips != 0 || val_hidden_flips != 0)
                    {
                        return Err("unexpected hidden transition disturbances".into());
                    }
                    let observed = Observed::new(&train, grid(STRESS_WIDTH));
                    if !task
                        .sources()
                        .iter()
                        .all(|source| observed.sources.contains(source))
                    {
                        return Err("native source absent from full search space".into());
                    }
                    println!(
                        "A6C12E_INPUT,task={},panel={},seed={},mode={},lagged_x1_x2_mismatches={},lagged_x3_x4_mismatches={},train_hidden_flips={},validation_hidden_flips={},samples={}",
                        task.name(),
                        panel.name(),
                        index,
                        mode.name(),
                        lagged_x1_x2,
                        lagged_x3_x4,
                        train_hidden_flips,
                        val_hidden_flips,
                        TRAIN_STEPS * LANES
                    );
                    for method in Method::ALL {
                        if method == Method::Walsh3Stream && index != 0 {
                            continue;
                        }
                        let fit = freeze(
                            (task, panel, index, STRESS_WIDTH),
                            method,
                            &train,
                            &validation,
                            &observed,
                        )?;
                        println!(
                            "A6C12E_FROZEN,task={},panel={},seed={},mode={},method={},factor_structures={},screen_seconds={:.6},factor_seconds={:.6},sketch_bucket_hits={},sketch_unique={},sketch_full={},w3_full_correlations={}",
                            task.name(),
                            panel.name(),
                            index,
                            mode.name(),
                            method.name(),
                            fit.proposals,
                            fit.screen_seconds,
                            fit.factor_seconds,
                            fit.sketch.bucket_hits,
                            fit.sketch.unique,
                            fit.sketch.full_scored,
                            fit.w3_full_correlations
                        );
                        frozen.push(FrozenStress { mode, fitted: fit });
                    }
                }
            }
        }
    }
    // 2 tasks x 2 panels x 3 modes x (3*nseeds + 1 seed0 reference).
    let expected_policies =
        Task::ALL.len() * Panel::ALL.len() * InputMode::STRESS.len() * (3 * nseeds + 1);
    let distinct_datasets = Task::ALL.len() * Panel::ALL.len() * InputMode::STRESS.len() * nseeds;
    if frozen.len() != expected_policies {
        return Err("frozen policy count differs from preregistered plan".into());
    }
    println!(
        "A6C12E_ALL_FROZEN,policies={},datasets={}",
        frozen.len(),
        distinct_datasets
    );

    // No TEST data or scores exist until after every source selection is frozen.
    for task in Task::ALL {
        for panel in Panel::ALL {
            for index in 0..nseeds {
                for mode in InputMode::STRESS {
                    let heldout_seed = seed(index, task, panel, 0xa6c1_2b33);
                    let (test, test_hidden_flips) =
                        data_with_mode(heldout_seed, task, panel, TEST_STEPS, mode);
                    let (clean, clean_hidden_flips) =
                        data_with_mode(heldout_seed, task, Panel::Clean, TEST_STEPS, mode);
                    if test.blocks[0].inputs != clean.blocks[0].inputs {
                        return Err("label corruption altered raw TEST inputs".into());
                    }
                    if test_hidden_flips != clean_hidden_flips {
                        return Err("hidden state disturbance depended on label noise panel".into());
                    }
                    let true_model = make_multi_gate_native(WIDTH, &task.rules())?;
                    let deterministic_clean_errors = native_errors(&true_model, &clean)?;
                    let deterministic_test_errors = native_errors(&true_model, &test)?;
                    match mode {
                        InputMode::StateFlip => {
                            if test_hidden_flips == 0 || deterministic_clean_errors == 0 {
                                return Err("state noise did not cause a real native transition disturbance".into());
                            }
                        }
                        InputMode::NearLag | InputMode::ExactLag => {
                            if test_hidden_flips != 0 || deterministic_clean_errors != 0 {
                                return Err(
                                    "noiseless native teacher failed temporal oracle".into()
                                );
                            }
                        }
                    }
                    // An independent alternative native circuit moves the
                    // intermediate XOR gate j68/x3 to j67/x4. This is an
                    // observational-equivalence witness when x3(t)=x4(t-1).
                    let lagged_alternative_difference = if mode == InputMode::StateFlip {
                        None
                    } else {
                        let mut alternative_rules = task.rules();
                        alternative_rules[1].gate = 67;
                        alternative_rules[1].raw = 4;
                        let alternate_model = make_multi_gate_native(WIDTH, &alternative_rules)?;
                        let disagreements = native_errors(&alternate_model, &clean)?;
                        if mode == InputMode::ExactLag && disagreements != 0 {
                            return Err(
                                "exact temporal observational-equivalence witness failed".into()
                            );
                        }
                        if mode == InputMode::NearLag && disagreements == 0 {
                            return Err(
                                "near-lag controls accidentally became indistinguishable".into()
                            );
                        }
                        Some(disagreements)
                    };
                    println!(
                        "A6C12E_WITNESS,task={},panel={},seed={},mode={},alternative_native_errors={:?},deterministic_clean_errors={},deterministic_test_errors={},test_hidden_flips={},samples={}",
                        task.name(),
                        panel.name(),
                        index,
                        mode.name(),
                        lagged_alternative_difference,
                        deterministic_clean_errors,
                        deterministic_test_errors,
                        test_hidden_flips,
                        TEST_STEPS * LANES
                    );

                    for row in frozen.iter().filter(|row| {
                        row.mode == mode
                            && row.fitted.task == task
                            && row.fitted.panel == panel
                            && row.fitted.seed == index
                    }) {
                        let fit = &row.fitted;
                        let (
                            sources_exact,
                            truths_exact,
                            proposal_errors,
                            selected_errors,
                            accepted,
                        ) = if let Some(model) = &fit.fit {
                            (
                                model.proposal_sources == task.sources(),
                                model.proposal_rules.rules.map(|rule| rule.truth) == task.truths(),
                                Some(native_errors(&model.proposal, &test)?),
                                Some(native_errors(&model.committed, &test)?),
                                model.accepted,
                            )
                        } else {
                            (false, false, None, None, false)
                        };
                        println!(
                            "A6C12E_TEST,task={},panel={},seed={},mode={},method={},n={},deterministic_teacher_errors={},deterministic_clean_errors={},test_hidden_flips={},proposal_errors={:?},selected_errors={:?},sources_exact={},truths_exact={},accepted={},alternative_native_errors={:?},factor_structures={},pair_rank={},pair_candidates={},triple_candidates={},sketch_bucket_hits={},sketch_unique={},sketch_full={},screen_seconds={:.6},factor_seconds={:.6},w3_full_correlations={},samples={}",
                            task.name(),
                            panel.name(),
                            index,
                            mode.name(),
                            fit.method.name(),
                            fit.n,
                            deterministic_test_errors,
                            deterministic_clean_errors,
                            test_hidden_flips,
                            proposal_errors,
                            selected_errors,
                            sources_exact,
                            truths_exact,
                            accepted,
                            lagged_alternative_difference,
                            fit.proposals,
                            fit.pair_rank,
                            fit.full_pairs,
                            fit.full_triples,
                            fit.sketch.bucket_hits,
                            fit.sketch.unique,
                            fit.sketch.full_scored,
                            fit.screen_seconds,
                            fit.factor_seconds,
                            fit.w3_full_correlations,
                            TEST_STEPS * LANES
                        );
                    }
                }
            }
        }
    }
    println!(
        "A6C12E_COMPLETE,policies={},distinct_test_datasets={},exact_lag_native_witness_checked=true,hidden_state_transition_noise_checked=true,modes=3,methods=4",
        frozen.len(),
        distinct_datasets
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_grid_and_preregistered_cardinality_are_exact() {
        let (train, injected) = data_with_mode(
            11,
            Task::XorXorXor,
            Panel::Clean,
            TRAIN_STEPS,
            InputMode::ExactLag,
        );
        assert_eq!(injected, 0);
        let observed = Observed::new(&train, grid(STRESS_WIDTH));
        assert_eq!(count_combinations(&observed), (128_016, 21_336_000));
        assert!(Task::XorXorXor
            .sources()
            .iter()
            .all(|source| observed.sources.contains(source)));
        assert_eq!(
            Task::ALL.len() * Panel::ALL.len() * InputMode::STRESS.len() * (3 * 3 + 1),
            120
        );
    }

    #[test]
    fn exact_one_step_lag_has_a_distinct_native_equivalent_circuit() {
        for task in Task::ALL {
            let (clean, flips) =
                data_with_mode(51, task, Panel::Clean, TEST_STEPS, InputMode::ExactLag);
            assert_eq!(flips, 0);
            assert_eq!(delayed_channel_mismatches(&clean), (0, 0));
            let true_model = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
            let mut alternate = task.rules();
            alternate[1].gate = 67;
            alternate[1].raw = 4;
            assert_ne!(alternate, task.rules());
            assert_eq!(native_errors(&true_model, &clean).unwrap(), 0);
            assert_eq!(
                native_errors(&make_multi_gate_native(WIDTH, &alternate).unwrap(), &clean).unwrap(),
                0
            );
        }
    }

    #[test]
    fn imperfect_one_step_lag_remains_observably_distinguishable() {
        for task in Task::ALL {
            let (clean, flips) =
                data_with_mode(57, task, Panel::Clean, TEST_STEPS, InputMode::NearLag);
            assert_eq!(flips, 0);
            let (a, b) = delayed_channel_mismatches(&clean);
            assert!(a > 0 && b > 0);
            let mut alternative = task.rules();
            alternative[1].gate = 67;
            alternative[1].raw = 4;
            assert_eq!(
                native_errors(
                    &make_multi_gate_native(WIDTH, &task.rules()).unwrap(),
                    &clean
                )
                .unwrap(),
                0
            );
            assert!(
                native_errors(
                    &make_multi_gate_native(WIDTH, &alternative).unwrap(),
                    &clean
                )
                .unwrap()
                    > 0
            );
        }
    }

    #[test]
    fn hidden_transition_corruption_is_real_and_not_label_noise() {
        for task in Task::ALL {
            let (train, flips) =
                data_with_mode(63, task, Panel::Clean, TRAIN_STEPS, InputMode::StateFlip);
            assert!(flips > 0);
            let correct_deterministic = make_multi_gate_native(WIDTH, &task.rules()).unwrap();
            // All labels in this panel are clean *observations* of a teacher
            // whose recurrent hidden state was perturbed, hence even the
            // identical deterministic circuit is not a perfect predictor.
            assert!(native_errors(&correct_deterministic, &train).unwrap() > 0);
            let (with_label_noise, noise_flips) =
                data_with_mode(63, task, Panel::Noise, TRAIN_STEPS, InputMode::StateFlip);
            assert_eq!(flips, noise_flips);
            assert_eq!(train.blocks[0].inputs, with_label_noise.blocks[0].inputs);
            assert_ne!(train.blocks[0].targets, with_label_noise.blocks[0].targets);
        }
    }

    #[test]
    fn label_noise_cannot_change_raw_inputs_or_hidden_disturbance_stream() {
        for mode in InputMode::STRESS {
            let (clean, flips) =
                data_with_mode(72, Task::XorXorXor, Panel::Clean, TRAIN_STEPS, mode);
            let (noisy, other_flips) =
                data_with_mode(72, Task::XorXorXor, Panel::Noise, TRAIN_STEPS, mode);
            assert_eq!(clean.blocks[0].inputs, noisy.blocks[0].inputs);
            assert_ne!(clean.blocks[0].targets, noisy.blocks[0].targets);
            assert_eq!(flips, other_flips);
            if mode == InputMode::StateFlip {
                assert!(flips > 0);
            } else {
                assert_eq!(flips, 0);
            }
        }
    }

    #[test]
    fn sketch_probe_and_full_corr_are_accounted_separately() {
        let (train, _) = data_with_mode(
            93,
            Task::XorXorXor,
            Panel::Clean,
            TRAIN_STEPS,
            InputMode::NearLag,
        );
        let observed = Observed::new(&train, grid(32));
        let (triples, stats) = rank_sketch_triplets(&observed);
        assert!(stats.pair_probes > 0);
        assert!(stats.unique >= triples.len());
        assert!(stats.coarse_scored >= stats.full_scored);
        assert!(stats.full_scored <= SKETCH_COARSE_KEEP);
        let (again, stats2) = rank_sketch_triplets(&observed);
        assert_eq!(again, triples);
        assert_eq!(stats2, stats);
    }
}

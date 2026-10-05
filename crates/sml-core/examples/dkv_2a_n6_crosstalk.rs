//! DKV-2a-N6 terminal-write crosstalk decomposition.
//!
//! Protocol: docs/SML-DKV-2A-N6-CROSSTALK-PREREGISTRATION.md.
//! Existing ActiveOnly + outer PageId(5)-derived seed only; no new projector mechanism.

use std::{collections::BTreeMap, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::MorDecision,
    semantic::SemanticState,
    PageId,
};

type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const LOAD: usize = 4;
const MIN_NODE_DISTANCE: u32 = 16;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const MOR_PAGE: u32 = 5;
const KEY_WIDTHS: [usize; 3] = [64, 256, 1024];
const MATCH_REPLICATES: usize = 4;
const ARMS: [&str; 4] = ["SELF", "SELF_SAME", "SELF_CROSS_MATCHED", "FULL"];
const A_SELF: usize = 0;
const A_SAME: usize = 1;
const A_CROSS: usize = 2;
const A_FULL: usize = 3;
const GEO_KINDS: [&str; 2] = ["same", "cross"];
const GEO_SAME: usize = 0;
const GEO_CROSS: usize = 1;

const MATCH_C1: u64 = 0xd6e8_feb8_6659_fd93;
const MATCH_C2: u64 = 0xc2b2_ae3d_27d4_eb4f;
const MATCH_C3: u64 = 0x1656_67b1_9e37_79f9;

#[derive(Clone, Copy)]
struct Stream(u64);

impl Stream {
    fn new(base: u64) -> Self {
        let mut stream = Self(base | 1);
        for _ in 0..8 {
            stream.next();
        }
        stream
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        ((self.next() >> 11) % bound as u64) as usize
    }
}

fn tagged(seed: u64, a: u64, b: u64, c: u64) -> Stream {
    Stream::new(
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ 0xA5A5_A5A5_A5A5_A5A5
            ^ (a + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ (b + 1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (c + 1).wrapping_mul(0x1656_67B1_9E37_79F9),
    )
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn build_state(value: u64, meaning: u64, routing: u64) -> State {
    let mut state = State::zeroed().expect("state");
    for bit in 0..BITS {
        state
            .value
            .set(bit, (value >> bit) & 1 == 1)
            .expect("value");
        state
            .meaning_mask
            .set(bit, (meaning >> bit) & 1 == 1)
            .expect("meaning");
        state
            .routing
            .set(bit, (routing >> bit) & 1 == 1)
            .expect("routing");
    }
    state
}

fn decision() -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(MOR_PAGE)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn target_q12(code: u64) -> Vec<i16> {
    (0..BITS)
        .map(|bit| {
            if (code >> bit) & 1 == 1 {
                DELTA_KV_ONE
            } else {
                -DELTA_KV_ONE
            }
        })
        .collect()
}

fn projector<const K: usize>(seed: u64) -> DeltaKvSemanticProjector<K> {
    let page_seed = mix64(seed ^ u64::from(MOR_PAGE).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        page_seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector")
}

struct Chain {
    length: usize,
    meaning: u64,
    routing: u64,
    codes: Vec<u64>,
}

fn build_chain(seed: u64, index: usize) -> Chain {
    let length = index % 4;
    let mut stream = tagged(seed, LOAD as u64, index as u64, 0);
    let mut codes = Vec::new();
    while codes.len() < length + 1 {
        let candidate = stream.next();
        if codes
            .iter()
            .all(|code: &u64| (*code ^ candidate).count_ones() >= MIN_NODE_DISTANCE)
        {
            codes.push(candidate);
        }
    }
    Chain {
        length,
        meaning: stream.next(),
        routing: stream.next(),
        codes,
    }
}

#[derive(Clone)]
struct WriteFact {
    id: usize,
    chain: usize,
    source_hop: usize,
    terminal: bool,
    global_pos: usize,
    state: State,
    target: u64,
}

fn build_writes(seed: u64, chains: &[Chain]) -> Vec<WriteFact> {
    let mut facts = Vec::new();
    for (chain_index, chain) in chains.iter().enumerate() {
        for hop in 0..chain.length {
            facts.push(WriteFact {
                id: chain_index * 4 + hop,
                chain: chain_index,
                source_hop: hop,
                terminal: false,
                global_pos: 0,
                state: build_state(chain.codes[hop], chain.meaning, chain.routing),
                target: chain.codes[hop + 1],
            });
        }
        facts.push(WriteFact {
            id: chain_index * 4 + chain.length,
            chain: chain_index,
            source_hop: chain.length,
            terminal: true,
            global_pos: 0,
            state: build_state(
                chain.codes[chain.length],
                chain.meaning,
                chain.routing,
            ),
            target: chain.codes[chain.length],
        });
    }

    let mut shuffle = tagged(seed, LOAD as u64, 999, 1);
    for index in (1..facts.len()).rev() {
        let swap_with = shuffle.below(index + 1);
        facts.swap(index, swap_with);
    }
    for (global_pos, fact) in facts.iter_mut().enumerate() {
        fact.global_pos = global_pos;
    }
    facts
}

fn signs_match(values: &[i32], target: u64) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(bit, value)| (*value > 0) == ((target >> bit) & 1 == 1))
}

fn wrong_coordinates(values: &[i32], target: u64) -> u64 {
    values
        .iter()
        .enumerate()
        .filter(|(bit, value)| (**value > 0) != ((target >> *bit) & 1 == 1))
        .count() as u64
}

fn read_clear(values: &[i32]) -> bool {
    values.iter().all(|value| value.unsigned_abs() > 128)
}

fn min_abs(values: &[i32]) -> u32 {
    values
        .iter()
        .map(|value| value.unsigned_abs())
        .min()
        .expect("read")
}

#[derive(Debug, Clone, Copy)]
struct ReadResult {
    sign_exact: bool,
    exact_clear: bool,
    wrong: u64,
    min_abs: u32,
}

fn evaluate_subset<const K: usize>(
    projector: DeltaKvSemanticProjector<K>,
    query: &WriteFact,
    subset: &[&WriteFact],
) -> ReadResult {
    let mut ordered = subset.to_vec();
    ordered.sort_by_key(|fact| fact.global_pos);

    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    for fact in ordered {
        let key = projector
            .project(&fact.state, &decision(), None)
            .expect("project")
            .key_q12;
        memory
            .update(&key, &target_q12(fact.target), FULL_RATE, &mut counters)
            .expect("update");
    }

    let query_key = projector
        .project(&query.state, &decision(), None)
        .expect("query project")
        .key_q12;
    let values = memory.read(&query_key, &mut counters).expect("read");
    let sign_exact = signs_match(&values, query.target);
    ReadResult {
        sign_exact,
        exact_clear: sign_exact && read_clear(&values),
        wrong: wrong_coordinates(&values, query.target),
        min_abs: min_abs(&values),
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ArmTally {
    observations: u64,
    sign_exact: u64,
    exact_clear: u64,
    wrong_coordinates: u64,
    min_abs: u32,
}

impl ArmTally {
    fn add(&mut self, read: ReadResult) {
        self.observations += 1;
        self.sign_exact += u64::from(read.sign_exact);
        self.exact_clear += u64::from(read.exact_clear);
        self.wrong_coordinates = self.wrong_coordinates.saturating_add(read.wrong);
        if self.observations == 1 {
            self.min_abs = read.min_abs;
        } else {
            self.min_abs = self.min_abs.min(read.min_abs);
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct GeometryTally {
    pairs: u64,
    cosine_sum: f64,
    abs_cosine_sum: f64,
    max_abs_cosine: f64,
    equal_keys: u64,
}

impl GeometryTally {
    fn add(&mut self, cosine: f64, equal: bool) {
        self.pairs += 1;
        self.cosine_sum += cosine;
        self.abs_cosine_sum += cosine.abs();
        self.max_abs_cosine = self.max_abs_cosine.max(cosine.abs());
        self.equal_keys += u64::from(equal);
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ReproductionTally {
    full_fail_l23: u64,
    same_fail_when_full_l23: u64,
}

type ArmCells = BTreeMap<(usize, usize, usize), ArmTally>;
type GeometryCells = BTreeMap<(usize, usize, usize), GeometryTally>;
type ReproductionCells = BTreeMap<usize, ReproductionTally>;

fn cosine(left: &[i16], right: &[i16]) -> f64 {
    let dot: f64 = left
        .iter()
        .zip(right)
        .map(|(a, b)| f64::from(*a) * f64::from(*b))
        .sum();
    let left_norm = left
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    let right_norm = right
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    dot / (left_norm * right_norm)
}

fn matched_cross<'a>(
    seed: u64,
    query: &WriteFact,
    facts: &'a [WriteFact],
    count: usize,
    replicate: usize,
) -> Vec<&'a WriteFact> {
    let mut candidates: Vec<(u64, &WriteFact)> = facts
        .iter()
        .filter(|fact| fact.chain != query.chain)
        .map(|fact| {
            let score = mix64(
                seed
                    ^ (query.id as u64).wrapping_mul(MATCH_C1)
                    ^ (replicate as u64).wrapping_mul(MATCH_C2)
                    ^ (fact.id as u64).wrapping_mul(MATCH_C3),
            );
            (score, fact)
        })
        .collect();
    candidates.sort_by_key(|(score, fact)| (*score, fact.id));
    candidates
        .into_iter()
        .take(count)
        .map(|(_, fact)| fact)
        .collect()
}

fn run_width<const K: usize>(
    arms: &mut ArmCells,
    geometry: &mut GeometryCells,
    reproduction: &mut ReproductionCells,
) {
    for seed in 0..SEEDS {
        let chains: Vec<Chain> = (0..LOAD).map(|index| build_chain(seed, index)).collect();
        let facts = build_writes(seed, &chains);
        let projector = projector::<K>(seed * 1000 + 17);

        let keys: BTreeMap<usize, Vec<i16>> = facts
            .iter()
            .map(|fact| {
                (
                    fact.id,
                    projector
                        .project(&fact.state, &decision(), None)
                        .expect("key")
                        .key_q12,
                )
            })
            .collect();

        for query in facts.iter().filter(|fact| fact.terminal) {
            let length = query.source_hop;

            let self_subset = vec![query];
            let same_subset: Vec<&WriteFact> = facts
                .iter()
                .filter(|fact| fact.chain == query.chain)
                .collect();
            let full_subset: Vec<&WriteFact> = facts.iter().collect();

            let self_read = evaluate_subset(projector, query, &self_subset);
            let same_read = evaluate_subset(projector, query, &same_subset);
            let full_read = evaluate_subset(projector, query, &full_subset);

            arms.entry((K, length, A_SELF)).or_default().add(self_read);
            arms.entry((K, length, A_SAME)).or_default().add(same_read);
            arms.entry((K, length, A_FULL)).or_default().add(full_read);

            for replicate in 0..MATCH_REPLICATES {
                let mut cross = matched_cross(seed, query, &facts, length, replicate);
                cross.push(query);
                let read = evaluate_subset(projector, query, &cross);
                arms.entry((K, length, A_CROSS)).or_default().add(read);
            }

            if length >= 2 && !full_read.exact_clear {
                let repro = reproduction.entry(K).or_default();
                repro.full_fail_l23 += 1;
                repro.same_fail_when_full_l23 += u64::from(!same_read.exact_clear);
            }

            let anchor = &keys[&query.id];
            for fact in facts.iter().filter(|fact| fact.id != query.id) {
                let kind = if fact.chain == query.chain {
                    GEO_SAME
                } else {
                    GEO_CROSS
                };
                let other = &keys[&fact.id];
                geometry
                    .entry((K, length, kind))
                    .or_default()
                    .add(cosine(anchor, other), anchor == other);
            }
        }
    }
}

fn ppm(numerator: u64, denominator: u64) -> u64 {
    numerator.saturating_mul(1_000_000) / denominator
}

fn main() {
    let mut arms = ArmCells::new();
    let mut geometry = GeometryCells::new();
    let mut reproduction = ReproductionCells::new();

    run_width::<64>(&mut arms, &mut geometry, &mut reproduction);
    run_width::<256>(&mut arms, &mut geometry, &mut reproduction);
    run_width::<1024>(&mut arms, &mut geometry, &mut reproduction);

    let mut out = String::new();

    for &k in &KEY_WIDTHS {
        for length in 0..4 {
            for (arm, arm_name) in ARMS.iter().enumerate() {
                let tally = arms[&(k, length, arm)];
                writeln!(
                    out,
                    "N6_ARM,K={k},L={length},arm={arm_name},observations={},sign_exact={},sign_ppm={},exact_clear={},clear_ppm={},wrong_coordinates={},min_abs={}",
                    tally.observations,
                    tally.sign_exact,
                    ppm(tally.sign_exact, tally.observations),
                    tally.exact_clear,
                    ppm(tally.exact_clear, tally.observations),
                    tally.wrong_coordinates,
                    tally.min_abs,
                )
                .expect("write");
            }

            for (kind, kind_name) in GEO_KINDS.iter().enumerate() {
                if let Some(tally) = geometry.get(&(k, length, kind)) {
                    writeln!(
                        out,
                        "N6_GEOMETRY,K={k},L={length},kind={kind_name},pairs={},mean_cosine={:.6},mean_abs_cosine={:.6},max_abs_cosine={:.6},equal_keys={}",
                        tally.pairs,
                        tally.cosine_sum / tally.pairs as f64,
                        tally.abs_cosine_sum / tally.pairs as f64,
                        tally.max_abs_cosine,
                        tally.equal_keys,
                    )
                    .expect("write");
                }
            }
        }

        let repro = reproduction[&k];
        writeln!(
            out,
            "N6_REPRO,K={k},full_fail_l23={},same_fail_when_full_l23={}",
            repro.full_fail_l23,
            repro.same_fail_when_full_l23,
        )
        .expect("write");
    }

    let h1 = KEY_WIDTHS.iter().all(|&k| {
        (0..4).all(|length| {
            let tally = arms[&(k, length, A_SELF)];
            tally.exact_clear == tally.observations
        })
    });

    let h2 = (1..=3).all(|length| {
        let same = geometry[&(1024, length, GEO_SAME)];
        let cross = geometry[&(1024, length, GEO_CROSS)];
        let same_abs = same.abs_cosine_sum / same.pairs as f64;
        let cross_abs = cross.abs_cosine_sum / cross.pairs as f64;
        same_abs >= cross_abs + 0.20
    });

    let l2_same = arms[&(1024, 2, A_SAME)];
    let l2_cross = arms[&(1024, 2, A_CROSS)];
    let l3_same = arms[&(1024, 3, A_SAME)];
    let l3_cross = arms[&(1024, 3, A_CROSS)];
    let h3 = ppm(l2_cross.exact_clear, l2_cross.observations)
        >= ppm(l2_same.exact_clear, l2_same.observations) + 50_000
        && ppm(l3_cross.exact_clear, l3_cross.observations)
            >= ppm(l3_same.exact_clear, l3_same.observations) + 200_000;

    let repro = reproduction[&1024];
    let h4 = repro.full_fail_l23 > 0
        && repro.same_fail_when_full_l23 * 10 >= repro.full_fail_l23 * 8;

    let h5 = ppm(l3_same.exact_clear, l3_same.observations) < 900_000;

    let h6 = arms[&(1024, 0, A_FULL)].exact_clear == 16
        && arms[&(1024, 1, A_FULL)].exact_clear == 16
        && arms[&(1024, 2, A_FULL)].exact_clear == 13
        && arms[&(1024, 3, A_FULL)].exact_clear == 8;

    for (name, holds) in [
        ("H1_SELF_PERFECT", h1),
        ("H2_SAME_CHAIN_GEOMETRY", h2),
        ("H3_MATCHED_SAME_CHAIN_DAMAGE", h3),
        ("H4_FULL_FAILURE_REPRODUCED_BY_SAME", h4),
        ("H5_WIDENING_NOT_ENOUGH", h5),
        ("H6_FULL_CONTROL", h6),
    ] {
        writeln!(
            out,
            "N6_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .expect("write");
    }
    writeln!(out, "PROMOTION,BLOCKED,n6_diagnostic_only").expect("write");

    print!("{out}");
}

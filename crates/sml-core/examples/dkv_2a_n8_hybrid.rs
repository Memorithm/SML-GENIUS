//! DKV-2a-N8 fixed-width PAGE/PAGE_VALUE hybrid-domain tradeoff.
//!
//! Protocol: docs/SML-DKV-2A-N8-HYBRID-PREREGISTRATION.md.
//! Harness-only: no core projector or Delta-KV memory implementation changes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticProjectionCounters,
        DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::MorDecision,
    semantic::SemanticState,
    PageId,
};

type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const MIN_NODE_DISTANCE: u32 = 16;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const MOR_PAGE: u32 = 5;
const PAGE_C: u64 = 0x9e37_79b9_7f4a_7c15;
const VALUE_C: u64 = 0xd6e8_feb8_6659_fd93;
const KEY_WIDTHS: [usize; 2] = [256, 1024];
const LOADS: [usize; 2] = [4, 16];
const FRACTIONS: [usize; 5] = [0, 1, 2, 3, 4];
const PERTURB_H: [usize; 3] = [1, 2, 4];
const SUBSETS: [&str; 2] = ["SELF_SAME", "FULL"];
const S_SAME: usize = 0;
const S_FULL: usize = 1;

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

fn value_bits(state: &State) -> u64 {
    (0..BITS).fold(0_u64, |acc, bit| {
        acc | (u64::from(state.value.get(bit).expect("value")) << bit)
    })
}

fn ambiguity_clear(state: &State) -> bool {
    (0..BITS).all(|bit| !state.ambiguity.get(bit).expect("ambiguity"))
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

struct Chain {
    length: usize,
    meaning: u64,
    routing: u64,
    codes: Vec<u64>,
}

fn build_chain(seed: u64, load: usize, index: usize) -> Chain {
    let length = index % 4;
    let mut stream = tagged(seed, load as u64, index as u64, 0);
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

fn build_writes(seed: u64, load: usize, chains: &[Chain]) -> Vec<WriteFact> {
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
            state: build_state(chain.codes[chain.length], chain.meaning, chain.routing),
            target: chain.codes[chain.length],
        });
    }

    let mut shuffle = tagged(seed, load as u64, 999, 1);
    for index in (1..facts.len()).rev() {
        let swap_with = shuffle.below(index + 1);
        facts.swap(index, swap_with);
    }
    for (global_pos, fact) in facts.iter_mut().enumerate() {
        fact.global_pos = global_pos;
    }
    facts
}

fn page_seed(base_seed: u64) -> u64 {
    mix64(base_seed ^ u64::from(MOR_PAGE).wrapping_mul(PAGE_C))
}

fn source_projection<const K: usize>(
    value_domain: bool,
    base_seed: u64,
    state: &State,
) -> (Vec<i16>, DeltaKvSemanticProjectionCounters) {
    let seed = if value_domain {
        mix64(page_seed(base_seed) ^ value_bits(state).wrapping_mul(VALUE_C))
    } else {
        page_seed(base_seed)
    };
    let evidence = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector")
    .project(state, &decision(), None)
    .expect("projection");
    (evidence.key_q12, evidence.counters)
}

fn hybrid_key<const K: usize>(
    q: usize,
    base_seed: u64,
    state: &State,
) -> (Vec<i16>, DeltaKvSemanticProjectionCounters, DeltaKvSemanticProjectionCounters) {
    let (page, page_work) = source_projection::<K>(false, base_seed, state);
    let (value, value_work) = source_projection::<K>(true, base_seed, state);
    let mut key = Vec::with_capacity(K);
    for index in 0..K {
        key.push(if index % 4 < q {
            value[index]
        } else {
            page[index]
        });
    }
    (key, page_work, value_work)
}

fn signs_match(values: &[i32], target: u64) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(bit, value)| (*value > 0) == ((target >> bit) & 1 == 1))
}

fn read_clear(values: &[i32]) -> bool {
    values.iter().all(|value| value.unsigned_abs() > 128)
}

fn wrong_coordinates(values: &[i32], target: u64) -> u64 {
    values
        .iter()
        .enumerate()
        .filter(|(bit, value)| (**value > 0) != ((target >> *bit) & 1 == 1))
        .count() as u64
}

fn min_abs(values: &[i32]) -> u32 {
    values
        .iter()
        .map(|value| value.unsigned_abs())
        .min()
        .expect("read")
}

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

fn norm_sq(key: &[i16]) -> u128 {
    key.iter()
        .map(|value| {
            let value = i128::from(*value);
            u128::try_from(value * value).expect("square")
        })
        .sum()
}

#[derive(Debug, Default, Clone, Copy)]
struct RecallTally {
    observations: u64,
    exact_clear: u64,
    sign_exact: u64,
    wrong_coordinates: u64,
    min_abs: u32,
}

impl RecallTally {
    fn add(&mut self, values: &[i32], target: u64) {
        let sign = signs_match(values, target);
        let clear = sign && read_clear(values);
        let minimum = min_abs(values);
        self.observations += 1;
        self.sign_exact += u64::from(sign);
        self.exact_clear += u64::from(clear);
        self.wrong_coordinates = self
            .wrong_coordinates
            .saturating_add(wrong_coordinates(values, target));
        if self.observations == 1 {
            self.min_abs = minimum;
        } else {
            self.min_abs = self.min_abs.min(minimum);
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct GeometryTally {
    pairs: u64,
    abs_cosine_sum: f64,
    max_abs_cosine: f64,
    equal_keys: u64,
}

impl GeometryTally {
    fn add(&mut self, left: &[i16], right: &[i16]) {
        let value = cosine(left, right).abs();
        self.pairs += 1;
        self.abs_cosine_sum += value;
        self.max_abs_cosine = self.max_abs_cosine.max(value);
        self.equal_keys += u64::from(left == right);
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct NormTally {
    observations: u64,
    norm_sq_sum: u128,
}

#[derive(Debug, Default, Clone, Copy)]
struct ChainTally {
    queries: u64,
    f1: u64,
    f2: u64,
    f3: u64,
    f6: u64,
    oracle: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct PerturbTally {
    observations: u64,
    abs_cosine_sum: f64,
    equal_keys: u64,
    exact_clear: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct WorkTally {
    projections: u64,
    semantic_coordinates: u64,
    bucket_accumulations: u64,
    quantized_coordinates: u64,
    final_key_width: u64,
}

impl WorkTally {
    fn add(
        &mut self,
        page: DeltaKvSemanticProjectionCounters,
        value: DeltaKvSemanticProjectionCounters,
        key_width: usize,
    ) {
        self.projections += 1;
        self.semantic_coordinates = self
            .semantic_coordinates
            .saturating_add(page.semantic_coordinates_read)
            .saturating_add(value.semantic_coordinates_read);
        self.bucket_accumulations = self
            .bucket_accumulations
            .saturating_add(page.bucket_accumulations)
            .saturating_add(value.bucket_accumulations);
        self.quantized_coordinates = self
            .quantized_coordinates
            .saturating_add(page.quantized_key_coordinates)
            .saturating_add(value.quantized_key_coordinates);
        self.final_key_width = key_width as u64;
    }
}

type P1Cells = BTreeMap<(usize, usize, usize, usize), RecallTally>;
type GeometryCells = BTreeMap<(usize, usize, usize, usize), GeometryTally>;
type NormCells = BTreeMap<(usize, usize, usize), NormTally>;
type TeacherCells = BTreeMap<(usize, usize, usize, usize, bool), RecallTally>;
type ChainCells = BTreeMap<(usize, usize, usize), ChainTally>;
type PerturbCells = BTreeMap<(usize, usize, usize), PerturbTally>;
type CollisionCells = BTreeMap<(usize, usize, usize), u64>;
type WorkCells = BTreeMap<(usize, usize), WorkTally>;

fn build_memory<const K: usize>(
    q: usize,
    base_seed: u64,
    facts: &[WriteFact],
) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    let mut ordered: Vec<&WriteFact> = facts.iter().collect();
    ordered.sort_by_key(|fact| fact.global_pos);
    for fact in ordered {
        let (key, _, _) = hybrid_key::<K>(q, base_seed, &fact.state);
        memory
            .update(&key, &target_q12(fact.target), FULL_RATE, &mut counters)
            .expect("update");
    }
    memory
}

fn build_subset_memory<const K: usize>(
    q: usize,
    base_seed: u64,
    subset: &[&WriteFact],
) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    let mut ordered = subset.to_vec();
    ordered.sort_by_key(|fact| fact.global_pos);
    for fact in ordered {
        let (key, _, _) = hybrid_key::<K>(q, base_seed, &fact.state);
        memory
            .update(&key, &target_q12(fact.target), FULL_RATE, &mut counters)
            .expect("update");
    }
    memory
}

fn read_state<const K: usize>(
    q: usize,
    base_seed: u64,
    memory: &DeltaKvMemory,
    state: &State,
) -> Vec<i32> {
    let (key, _, _) = hybrid_key::<K>(q, base_seed, state);
    let mut counters = DeltaKvCounters::default();
    memory.read(&key, &mut counters).expect("read")
}

fn rewrite_once(state: &State, values: &[i32]) -> State {
    let mut next = state.clone();
    for (bit, value) in values.iter().enumerate() {
        if value.unsigned_abs() <= 128 {
            next.ambiguity.set(bit, true).expect("ambiguity");
        } else {
            next.value.set(bit, *value > 0).expect("value");
            next.ambiguity.set(bit, false).expect("ambiguity");
        }
    }
    next
}

fn chain_correct(state: &State, expected: u64) -> bool {
    value_bits(state) == expected && ambiguity_clear(state)
}

fn perturb_state(state: &State, seed: u64, fact_id: usize, h: usize) -> State {
    let mut out = state.clone();
    let mut stream = tagged(seed, fact_id as u64, h as u64, 77);
    let mut pool: Vec<usize> = (0..BITS).collect();
    for index in 0..h {
        let swap_with = index + stream.below(BITS - index);
        pool.swap(index, swap_with);
    }
    for &bit in &pool[..h] {
        let current = out.value.get(bit).expect("value");
        out.value.set(bit, !current).expect("value");
    }
    out
}

fn run_width<const K: usize>(
    p1: &mut P1Cells,
    geometry: &mut GeometryCells,
    norms: &mut NormCells,
    teacher: &mut TeacherCells,
    chains_out: &mut ChainCells,
    perturb: &mut PerturbCells,
    collisions: &mut CollisionCells,
) {
    for &load in &LOADS {
        for seed in 0..SEEDS {
            let chains: Vec<Chain> = (0..load)
                .map(|index| build_chain(seed, load, index))
                .collect();
            let facts = build_writes(seed, load, &chains);
            let base_seed = seed * 1000 + 17;

            for &q in &FRACTIONS {
                let full = build_memory::<K>(q, base_seed, &facts);

                let mut unique = BTreeSet::<Vec<i16>>::new();
                let mut collision_count = 0_u64;
                for fact in &facts {
                    let (key, _, _) = hybrid_key::<K>(q, base_seed, &fact.state);
                    collision_count += u64::from(!unique.insert(key));
                }
                *collisions.entry((K, load, q)).or_default() += collision_count;

                if load == 4 {
                    let keys: BTreeMap<usize, Vec<i16>> = facts
                        .iter()
                        .map(|fact| {
                            let (key, _, _) = hybrid_key::<K>(q, base_seed, &fact.state);
                            (fact.id, key)
                        })
                        .collect();

                    for query in facts.iter().filter(|fact| fact.terminal) {
                        let length = query.source_hop;
                        let same_subset: Vec<&WriteFact> = facts
                            .iter()
                            .filter(|fact| fact.chain == query.chain)
                            .collect();

                        let same_memory =
                            build_subset_memory::<K>(q, base_seed, &same_subset);
                        let same_values =
                            read_state::<K>(q, base_seed, &same_memory, &query.state);
                        p1.entry((K, q, length, S_SAME))
                            .or_default()
                            .add(&same_values, query.target);

                        let full_values =
                            read_state::<K>(q, base_seed, &full, &query.state);
                        p1.entry((K, q, length, S_FULL))
                            .or_default()
                            .add(&full_values, query.target);

                        let anchor = &keys[&query.id];
                        let norm = norms.entry((K, q, length)).or_default();
                        norm.observations += 1;
                        norm.norm_sq_sum += norm_sq(anchor);

                        for fact in facts.iter().filter(|fact| fact.id != query.id) {
                            let kind = usize::from(fact.chain != query.chain);
                            geometry
                                .entry((K, q, length, kind))
                                .or_default()
                                .add(anchor, &keys[&fact.id]);
                        }
                    }
                }

                for chain in &chains {
                    for hop in 0..=chain.length {
                        let state = build_state(chain.codes[hop], chain.meaning, chain.routing);
                        let terminal = hop == chain.length;
                        let target = if terminal {
                            chain.codes[hop]
                        } else {
                            chain.codes[hop + 1]
                        };
                        let values = read_state::<K>(q, base_seed, &full, &state);
                        teacher
                            .entry((K, load, q, hop, terminal))
                            .or_default()
                            .add(&values, target);
                    }
                }

                for chain in &chains {
                    let expected = chain.codes[chain.length];
                    let mut state = build_state(chain.codes[0], chain.meaning, chain.routing);
                    let mut hits = [false; 6];
                    for hit in &mut hits {
                        let values = read_state::<K>(q, base_seed, &full, &state);
                        state = rewrite_once(&state, &values);
                        *hit = chain_correct(&state, expected);
                    }
                    let tally = chains_out.entry((K, load, q)).or_default();
                    tally.queries += 1;
                    tally.f1 += u64::from(hits[0]);
                    tally.f2 += u64::from(hits[1]);
                    tally.f3 += u64::from(hits[2]);
                    tally.f6 += u64::from(hits[5]);
                    tally.oracle += u64::from(hits.iter().any(|hit| *hit));
                }

                if load == 4 {
                    for fact in facts.iter().filter(|fact| !fact.terminal) {
                        let (exact_key, _, _) = hybrid_key::<K>(q, base_seed, &fact.state);
                        for &h in &PERTURB_H {
                            let perturbed = perturb_state(&fact.state, seed, fact.id, h);
                            let (perturbed_key, _, _) =
                                hybrid_key::<K>(q, base_seed, &perturbed);
                            let values = read_state::<K>(q, base_seed, &full, &perturbed);
                            let entry = perturb.entry((K, q, h)).or_default();
                            entry.observations += 1;
                            entry.abs_cosine_sum += cosine(&exact_key, &perturbed_key).abs();
                            entry.equal_keys += u64::from(exact_key == perturbed_key);
                            entry.exact_clear +=
                                u64::from(signs_match(&values, fact.target) && read_clear(&values));
                        }
                    }
                }
            }
        }
    }
}

fn run_work<const K: usize>(work: &mut WorkCells) {
    for &q in &FRACTIONS {
        let mut tally = WorkTally::default();
        for seed in 0..SEEDS {
            let mut stream = tagged(seed, K as u64, q as u64, 888);
            let state = build_state(stream.next(), stream.next(), stream.next());
            let (_, page, value) = hybrid_key::<K>(q, seed * 1000 + 17, &state);
            tally.add(page, value, K);
        }
        work.insert((K, q), tally);
    }
}

fn ppm(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator.saturating_mul(1_000_000) / denominator
    }
}

fn mean_abs_geometry(
    geometry: &GeometryCells,
    k: usize,
    q: usize,
    length: usize,
    kind: usize,
) -> f64 {
    let tally = geometry[&(k, q, length, kind)];
    tally.abs_cosine_sum / tally.pairs as f64
}

fn advancing_min_ppm(teacher: &TeacherCells, k: usize, load: usize, q: usize) -> u64 {
    (0..=2)
        .map(|hop| {
            let tally = teacher[&(k, load, q, hop, false)];
            ppm(tally.exact_clear, tally.observations)
        })
        .min()
        .expect("advancing hops")
}

fn main() {
    let mut p1 = P1Cells::new();
    let mut geometry = GeometryCells::new();
    let mut norms = NormCells::new();
    let mut teacher = TeacherCells::new();
    let mut chains = ChainCells::new();
    let mut perturb = PerturbCells::new();
    let mut collisions = CollisionCells::new();
    let mut work = WorkCells::new();

    run_width::<256>(
        &mut p1,
        &mut geometry,
        &mut norms,
        &mut teacher,
        &mut chains,
        &mut perturb,
        &mut collisions,
    );
    run_width::<1024>(
        &mut p1,
        &mut geometry,
        &mut norms,
        &mut teacher,
        &mut chains,
        &mut perturb,
        &mut collisions,
    );
    run_work::<256>(&mut work);
    run_work::<1024>(&mut work);

    let mut out = String::new();

    for &k in &KEY_WIDTHS {
        for &q in &FRACTIONS {
            for length in 0..4 {
                for subset in 0..2 {
                    let tally = p1[&(k, q, length, subset)];
                    writeln!(
                        out,
                        "N8_P1,K={k},q={q},L={length},subset={},observations={},exact_clear={},clear_ppm={},sign_exact={},wrong_coordinates={},min_abs={}",
                        SUBSETS[subset],
                        tally.observations,
                        tally.exact_clear,
                        ppm(tally.exact_clear, tally.observations),
                        tally.sign_exact,
                        tally.wrong_coordinates,
                        tally.min_abs,
                    )
                    .expect("write");
                }
                for kind in 0..2 {
                    if let Some(tally) = geometry.get(&(k, q, length, kind)) {
                        writeln!(
                            out,
                            "N8_GEOMETRY,K={k},q={q},L={length},kind={},pairs={},mean_abs_cosine={:.6},max_abs_cosine={:.6},equal_keys={}",
                            ["same", "cross"][kind],
                            tally.pairs,
                            tally.abs_cosine_sum / tally.pairs as f64,
                            tally.max_abs_cosine,
                            tally.equal_keys,
                        )
                        .expect("write");
                    }
                }
                if let Some(tally) = norms.get(&(k, q, length)) {
                    writeln!(
                        out,
                        "N8_NORM,K={k},q={q},L={length},observations={},mean_norm_sq={}",
                        tally.observations,
                        tally.norm_sq_sum / u128::from(tally.observations),
                    )
                    .expect("write");
                }
            }
        }
    }

    for &k in &KEY_WIDTHS {
        for &load in &LOADS {
            for &q in &FRACTIONS {
                for hop in 0..4 {
                    for terminal in [false, true] {
                        if let Some(tally) = teacher.get(&(k, load, q, hop, terminal)) {
                            writeln!(
                                out,
                                "N8_TEACHER,K={k},G={load},q={q},hop={hop},terminal={terminal},observations={},exact_clear={},clear_ppm={},sign_exact={},wrong_coordinates={},min_abs={}",
                                tally.observations,
                                tally.exact_clear,
                                ppm(tally.exact_clear, tally.observations),
                                tally.sign_exact,
                                tally.wrong_coordinates,
                                tally.min_abs,
                            )
                            .expect("write");
                        }
                    }
                }
                let tally = chains[&(k, load, q)];
                writeln!(
                    out,
                    "N8_CHAIN,K={k},G={load},q={q},queries={},F1={},F1_ppm={},F2={},F2_ppm={},F3={},F3_ppm={},F6={},F6_ppm={},ORACLE={},ORACLE_ppm={}",
                    tally.queries,
                    tally.f1,
                    ppm(tally.f1, tally.queries),
                    tally.f2,
                    ppm(tally.f2, tally.queries),
                    tally.f3,
                    ppm(tally.f3, tally.queries),
                    tally.f6,
                    ppm(tally.f6, tally.queries),
                    tally.oracle,
                    ppm(tally.oracle, tally.queries),
                )
                .expect("write");
                writeln!(
                    out,
                    "N8_COLLISIONS,K={k},G={load},q={q},count={}",
                    collisions[&(k, load, q)],
                )
                .expect("write");
            }
        }
    }

    for &k in &KEY_WIDTHS {
        for &q in &FRACTIONS {
            for &h in &PERTURB_H {
                let tally = perturb[&(k, q, h)];
                writeln!(
                    out,
                    "N8_PERTURB,K={k},q={q},h={h},observations={},mean_abs_cosine={:.6},equal_keys={},exact_clear={},clear_ppm={}",
                    tally.observations,
                    tally.abs_cosine_sum / tally.observations as f64,
                    tally.equal_keys,
                    tally.exact_clear,
                    ppm(tally.exact_clear, tally.observations),
                )
                .expect("write");
            }
            let tally = work[&(k, q)];
            writeln!(
                out,
                "N8_WORK,K={k},q={q},hybrid_keys={},semantic_coordinates={},bucket_accumulations={},quantized_coordinates={},final_key_width={}",
                tally.projections,
                tally.semantic_coordinates,
                tally.bucket_accumulations,
                tally.quantized_coordinates,
                tally.final_key_width,
            )
            .expect("write");
        }
    }

    let n7_page_same = [16_u64, 16, 14, 8];
    let n7_page_full = [16_u64, 16, 13, 8];
    let h1_page = (0..4).all(|length| {
        p1[&(1024, 0, length, S_SAME)].exact_clear == n7_page_same[length]
            && p1[&(1024, 0, length, S_FULL)].exact_clear == n7_page_full[length]
    });
    let q4_h1 = perturb[&(1024, 4, 1)];
    let h1_value = (0..4).all(|length| p1[&(1024, 4, length, S_FULL)].exact_clear == 16)
        && q4_h1.exact_clear == 0
        && (q4_h1.abs_cosine_sum / q4_h1.observations as f64 - 0.023576).abs() < 0.000001;
    let h1 = h1_page && h1_value;

    let h2 = (1..=3).all(|length| {
        FRACTIONS.windows(2).all(|pair| {
            mean_abs_geometry(&geometry, 1024, pair[1], length, 0)
                <= mean_abs_geometry(&geometry, 1024, pair[0], length, 0) + 0.000001
        })
    });

    let h3 = FRACTIONS.windows(2).all(|pair| {
        let left = perturb[&(1024, pair[0], 1)];
        let right = perturb[&(1024, pair[1], 1)];
        right.abs_cosine_sum / right.observations as f64
            <= left.abs_cosine_sum / left.observations as f64 + 0.000001
    });

    let qualifies = |load: usize, q: usize| {
        let chain = chains[&(1024, load, q)];
        let h1_perturb = perturb[&(1024, q, 1)];
        ppm(chain.f6, chain.queries) >= 900_000
            && advancing_min_ppm(&teacher, 1024, load, q) >= 900_000
            && ppm(h1_perturb.exact_clear, h1_perturb.observations) >= 200_000
    };

    let low_candidate = [1_usize, 2, 3]
        .into_iter()
        .find(|q| qualifies(4, *q));
    let high_candidate = [1_usize, 2, 3]
        .into_iter()
        .find(|q| qualifies(16, *q));

    let h4 = low_candidate.is_some();
    let h5 = high_candidate.is_some();

    let h6 = collisions.values().all(|count| *count == 0);
    let h7 = KEY_WIDTHS.iter().all(|&k| {
        let baseline = work[&(k, 0)];
        FRACTIONS.iter().all(|&q| work[&(k, q)] == baseline)
    });

    for (name, holds) in [
        ("H1_ENDPOINTS_REPRODUCE_N7", h1),
        ("H2_SAME_CHAIN_COSINE_MONOTONE", h2),
        ("H3_ONE_BIT_COSINE_MONOTONE", h3),
        ("H4_LOW_LOAD_INTERIOR_COMPROMISE", h4),
        ("H5_HIGH_LOAD_INTERIOR_COMPROMISE", h5),
        ("H6_NO_COLLISIONS", h6),
        ("H7_EQUAL_PROJECTOR_WORK", h7),
    ] {
        writeln!(
            out,
            "N8_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .expect("write");
    }

    writeln!(
        out,
        "N8_SELECTION,low_load={},high_load={}",
        low_candidate
            .map(|q| q.to_string())
            .unwrap_or_else(|| "NONE".to_string()),
        high_candidate
            .map(|q| q.to_string())
            .unwrap_or_else(|| "NONE".to_string()),
    )
    .expect("write");
    writeln!(out, "PROMOTION,BLOCKED,n8_harness_only").expect("write");

    print!("{out}");
}

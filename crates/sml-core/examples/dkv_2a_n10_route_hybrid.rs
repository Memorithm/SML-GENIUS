//! DKV-2a-N10 fixed-width PRIMARY/ORDERED route-domain hybrid.
//!
//! Protocol: docs/SML-DKV-2A-N10-ROUTE-HYBRID-PREREGISTRATION.md.
//! Harness-only: q=3 remains fixed and no core API is introduced.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticKeyEvidence,
        DeltaKvSemanticProjectionCounters, DeltaKvSemanticProjectionPolicy,
        DeltaKvSemanticProjector,
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
const KEY_WIDTHS: [usize; 2] = [256, 1024];
const LOADS: [usize; 2] = [4, 16];
const ROUTE_FRACTIONS: [usize; 5] = [0, 1, 2, 3, 4];
const Q: usize = 3;
const P2_FACTS: usize = 16;
const P1_PAIRS: usize = 200;

const PAGE_C: u64 = 0x9e37_79b9_7f4a_7c15;
const VALUE_C: u64 = 0xd6e8_feb8_6659_fd93;
const ROUTE_TAG: u64 = 0x6d75_6c74_695f_6d6f;
const ROUTE_C: u64 = 0xc2b2_ae3d_27d4_eb4f;
const RANK_C: u64 = 0x1656_67b1_9e37_79f9;
const LEN_C: u64 = 0x85eb_ca77_c2b2_ae63;

const R1: &[u32] = &[5];
const R2: &[u32] = &[5, 7];
const R3: &[u32] = &[5, 8];
const R4: &[u32] = &[5, 7, 9];
const R6: &[u32] = &[7, 5];

const P_PRIMARY: usize = 0;
const P_ORDERED: usize = 1;

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

fn decision(route: &[u32]) -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: route.iter().copied().map(PageId).collect(),
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

fn route_seed(policy: usize, base_seed: u64, route: &[u32]) -> u64 {
    assert!(!route.is_empty());
    let mut seed = mix64(base_seed ^ u64::from(route[0]).wrapping_mul(PAGE_C));
    if policy == P_ORDERED && route.len() > 1 {
        for (rank, page) in route.iter().copied().enumerate().skip(1) {
            seed = mix64(
                seed ^ ROUTE_TAG
                    ^ u64::from(page).wrapping_mul(ROUTE_C)
                    ^ (rank as u64).wrapping_mul(RANK_C),
            );
        }
        seed = mix64(seed ^ (route.len() as u64).wrapping_mul(LEN_C));
    }
    seed
}

fn source_projection<const K: usize>(
    policy: usize,
    value_domain: bool,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> DeltaKvSemanticKeyEvidence {
    let route_seed = route_seed(policy, base_seed, route);
    let seed = if value_domain {
        mix64(route_seed ^ value_bits(state).wrapping_mul(VALUE_C))
    } else {
        route_seed
    };
    let evidence = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector")
    .project(state, &decision(route), None)
    .expect("projection");
    let expected: Vec<PageId> = route.iter().copied().map(PageId).collect();
    assert_eq!(evidence.selected_experts, expected);
    evidence
}

fn endpoint_key<const K: usize>(
    policy: usize,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> (
    Vec<i16>,
    DeltaKvSemanticProjectionCounters,
    DeltaKvSemanticProjectionCounters,
) {
    let page = source_projection::<K>(policy, false, base_seed, state, route);
    let value = source_projection::<K>(policy, true, base_seed, state, route);
    let key = (0..K)
        .map(|index| {
            if index % 4 < Q {
                value.key_q12[index]
            } else {
                page.key_q12[index]
            }
        })
        .collect();
    (key, page.counters, value.counters)
}

fn route_hybrid_key<const K: usize>(
    r: usize,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> (
    Vec<i16>,
    [DeltaKvSemanticProjectionCounters; 4],
) {
    let (primary, primary_page, primary_value) =
        endpoint_key::<K>(P_PRIMARY, base_seed, state, route);
    let (ordered, ordered_page, ordered_value) =
        endpoint_key::<K>(P_ORDERED, base_seed, state, route);
    let key = (0..K)
        .map(|index| {
            if index % 4 < r {
                ordered[index]
            } else {
                primary[index]
            }
        })
        .collect();
    (
        key,
        [primary_page, primary_value, ordered_page, ordered_value],
    )
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

fn exact_clear(values: &[i32], target: u64) -> bool {
    signs_match(values, target) && read_clear(values)
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

fn perturb_one(state: &State, seed: u64, fact_id: usize) -> State {
    let mut out = state.clone();
    let mut stream = tagged(seed, fact_id as u64, 1, 77);
    let bit = stream.below(BITS);
    let current = out.value.get(bit).expect("value");
    out.value.set(bit, !current).expect("value");
    out
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
struct RecallTally {
    observations: u64,
    exact_clear: u64,
}

impl RecallTally {
    fn add(&mut self, values: &[i32], target: u64) {
        self.observations += 1;
        self.exact_clear += u64::from(exact_clear(values, target));
    }
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
    constructions: u64,
    semantic_coordinates: u64,
    bucket_accumulations: u64,
    quantized_coordinates: u64,
    final_key_width: u64,
}

impl WorkTally {
    fn add(&mut self, counters: [DeltaKvSemanticProjectionCounters; 4], k: usize) {
        self.constructions += 1;
        for item in counters {
            self.semantic_coordinates = self
                .semantic_coordinates
                .saturating_add(item.semantic_coordinates_read);
            self.bucket_accumulations = self
                .bucket_accumulations
                .saturating_add(item.bucket_accumulations);
            self.quantized_coordinates = self
                .quantized_coordinates
                .saturating_add(item.quantized_key_coordinates);
        }
        self.final_key_width = k as u64;
    }
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
    global_pos: usize,
    state: State,
    target: u64,
}

fn chain_writes(seed: u64, load: usize, chains: &[Chain]) -> Vec<WriteFact> {
    let mut facts = Vec::new();
    for (chain_index, chain) in chains.iter().enumerate() {
        for hop in 0..chain.length {
            facts.push(WriteFact {
                id: chain_index * 4 + hop,
                global_pos: 0,
                state: build_state(chain.codes[hop], chain.meaning, chain.routing),
                target: chain.codes[hop + 1],
            });
        }
        facts.push(WriteFact {
            id: chain_index * 4 + chain.length,
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

fn p2_facts(seed: u64) -> Vec<(State, u64)> {
    let mut stream = tagged(seed, P2_FACTS as u64, 2, 911);
    (0..P2_FACTS)
        .map(|_| {
            (
                build_state(stream.next(), stream.next(), stream.next()),
                stream.next(),
            )
        })
        .collect()
}

fn read_state<const K: usize>(
    r: usize,
    base_seed: u64,
    route: &[u32],
    memory: &DeltaKvMemory,
    state: &State,
) -> Vec<i32> {
    let (key, _) = route_hybrid_key::<K>(r, base_seed, state, route);
    let mut counters = DeltaKvCounters::default();
    memory.read(&key, &mut counters).expect("read")
}

fn build_memory<const K: usize>(
    r: usize,
    base_seed: u64,
    route: &[u32],
    facts: &[(State, u64, usize)],
    collision_count: &mut u64,
) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    let mut ordered: Vec<&(State, u64, usize)> = facts.iter().collect();
    ordered.sort_by_key(|fact| fact.2);
    let mut unique = BTreeSet::new();
    for (state, target, _) in ordered {
        let (key, _) = route_hybrid_key::<K>(r, base_seed, state, route);
        *collision_count += u64::from(!unique.insert(key.clone()));
        memory
            .update(&key, &target_q12(*target), FULL_RATE, &mut counters)
            .expect("update");
    }
    memory
}

type GeometryCells = BTreeMap<(usize, usize, usize), GeometryTally>;
type ContinuityCells = BTreeMap<(usize, usize, usize), RecallTally>;
type ConflictCells = BTreeMap<(usize, usize, usize), RecallTally>;
type TeacherCells = BTreeMap<(usize, usize, usize, usize), RecallTally>;
type ChainCells = BTreeMap<(usize, usize, usize), ChainTally>;
type PerturbCells = BTreeMap<(usize, usize), PerturbTally>;
type CollisionCells = BTreeMap<(usize, usize, usize), u64>;
type WorkCells = BTreeMap<(usize, usize), WorkTally>;

fn compatibility<const K: usize>() -> u64 {
    let mut checks = 0_u64;
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let mut stream = tagged(seed, K as u64, 10, 999);
        for _ in 0..50 {
            let state = build_state(stream.next(), stream.next(), stream.next());
            let (primary, _, _) = endpoint_key::<K>(P_PRIMARY, base_seed, &state, R2);
            let (ordered, _, _) = endpoint_key::<K>(P_ORDERED, base_seed, &state, R2);
            let (r0, _) = route_hybrid_key::<K>(0, base_seed, &state, R2);
            let (r4, _) = route_hybrid_key::<K>(4, base_seed, &state, R2);
            assert_eq!(r0, primary, "r0 PRIMARY endpoint");
            assert_eq!(r4, ordered, "r4 ORDERED endpoint");

            let (r1_single, _) = route_hybrid_key::<K>(1, base_seed, &state, R1);
            let (r3_single, _) = route_hybrid_key::<K>(3, base_seed, &state, R1);
            assert_eq!(r1_single, r3_single, "singleton route policies collapse");

            let (again, _) = route_hybrid_key::<K>(3, base_seed, &state, R2);
            let (first, _) = route_hybrid_key::<K>(3, base_seed, &state, R2);
            assert_eq!(first, again, "deterministic");
            checks += 4;
        }
    }
    checks
}

fn run_geometry<const K: usize>(geometry: &mut GeometryCells) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let mut stream = tagged(seed, K as u64, 11, 1000);
        for _ in 0..P1_PAIRS {
            let state = build_state(stream.next(), stream.next(), stream.next());
            for &r in &ROUTE_FRACTIONS {
                let (anchor, _) = route_hybrid_key::<K>(r, base_seed, &state, R2);
                for (route_slot, route) in [(0_usize, R3), (1_usize, R4), (2_usize, R6)] {
                    let (other, _) = route_hybrid_key::<K>(r, base_seed, &state, route);
                    geometry
                        .entry((K, r, route_slot))
                        .or_default()
                        .add(&anchor, &other);
                }
            }
        }
    }
}

fn run_continuity<const K: usize>(
    continuity: &mut ContinuityCells,
    collisions: &mut CollisionCells,
) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let facts = p2_facts(seed);
        for &r in &ROUTE_FRACTIONS {
            let records: Vec<(State, u64, usize)> = facts
                .iter()
                .enumerate()
                .map(|(index, (state, target))| (state.clone(), *target, index))
                .collect();
            let collision = collisions.entry((K, r, 0)).or_default();
            let memory = build_memory::<K>(r, base_seed, R2, &records, collision);
            for (state, target) in &facts {
                for (route_slot, route) in [(0_usize, R2), (1_usize, R3), (2_usize, R4)] {
                    let values = read_state::<K>(r, base_seed, route, &memory, state);
                    continuity
                        .entry((K, r, route_slot))
                        .or_default()
                        .add(&values, *target);
                }
            }
        }
    }
}

fn conflict_targets(stream: &mut Stream) -> (u64, u64) {
    let first = stream.next();
    let mut second = stream.next();
    while second == first {
        second = stream.next();
    }
    (first, second)
}

fn run_conflict<const K: usize>(
    conflicts: &mut ConflictCells,
    collisions: &mut CollisionCells,
) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        for &r in &ROUTE_FRACTIONS {
            for (kind, route_b) in [(0_usize, R3), (1_usize, R6)] {
                let mut stream = tagged(seed, K as u64, r as u64, 1200 + kind as u64);
                let mut records: Vec<(State, u64, &[u32])> = Vec::new();
                let mut reads: Vec<(State, u64, &[u32])> = Vec::new();
                for _ in 0..P2_FACTS {
                    let state = build_state(stream.next(), stream.next(), stream.next());
                    let (a, b) = conflict_targets(&mut stream);
                    records.push((state.clone(), a, R2));
                    records.push((state.clone(), b, route_b));
                    reads.push((state.clone(), a, R2));
                    reads.push((state, b, route_b));
                }
                for index in (1..records.len()).rev() {
                    let swap_with = stream.below(index + 1);
                    records.swap(index, swap_with);
                }

                let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
                let mut counters = DeltaKvCounters::default();
                let mut unique = BTreeSet::new();
                for (state, target, route) in &records {
                    let (key, _) = route_hybrid_key::<K>(r, base_seed, state, route);
                    *collisions.entry((K, r, 1 + kind)).or_default() +=
                        u64::from(!unique.insert(key.clone()));
                    memory
                        .update(&key, &target_q12(*target), FULL_RATE, &mut counters)
                        .expect("update");
                }

                let tally = conflicts.entry((K, r, kind)).or_default();
                for (state, target, route) in &reads {
                    let values = read_state::<K>(r, base_seed, route, &memory, state);
                    tally.add(&values, *target);
                }
            }
        }
    }
}

fn run_chains<const K: usize>(
    teacher: &mut TeacherCells,
    chains_out: &mut ChainCells,
    perturb: &mut PerturbCells,
    collisions: &mut CollisionCells,
) {
    for &load in &LOADS {
        for seed in 0..SEEDS {
            let base_seed = seed * 1000 + 17;
            let chains: Vec<Chain> = (0..load)
                .map(|index| build_chain(seed, load, index))
                .collect();
            let writes = chain_writes(seed, load, &chains);
            let records: Vec<(State, u64, usize)> = writes
                .iter()
                .map(|fact| (fact.state.clone(), fact.target, fact.global_pos))
                .collect();

            for &r in &ROUTE_FRACTIONS {
                let collision = collisions.entry((K, r, 3 + load)).or_default();
                let memory = build_memory::<K>(r, base_seed, R2, &records, collision);

                for chain in &chains {
                    for hop in 0..chain.length {
                        let state = build_state(chain.codes[hop], chain.meaning, chain.routing);
                        let values = read_state::<K>(r, base_seed, R2, &memory, &state);
                        teacher
                            .entry((K, load, r, hop))
                            .or_default()
                            .add(&values, chain.codes[hop + 1]);
                    }
                }

                for chain in &chains {
                    let expected = chain.codes[chain.length];
                    let mut state = build_state(chain.codes[0], chain.meaning, chain.routing);
                    let mut hits = [false; 6];
                    for hit in &mut hits {
                        let values = read_state::<K>(r, base_seed, R2, &memory, &state);
                        state = rewrite_once(&state, &values);
                        *hit = chain_correct(&state, expected);
                    }
                    let tally = chains_out.entry((K, load, r)).or_default();
                    tally.queries += 1;
                    tally.f1 += u64::from(hits[0]);
                    tally.f2 += u64::from(hits[1]);
                    tally.f3 += u64::from(hits[2]);
                    tally.f6 += u64::from(hits[5]);
                    tally.oracle += u64::from(hits.iter().any(|hit| *hit));
                }

                if load == 4 {
                    for fact in writes.iter().filter(|fact| {
                        let chain_index = fact.id / 4;
                        let hop = fact.id % 4;
                        hop < chains[chain_index].length
                    }) {
                        let perturbed = perturb_one(&fact.state, seed, fact.id);
                        let (exact_key, _) = route_hybrid_key::<K>(r, base_seed, &fact.state, R2);
                        let (perturbed_key, _) =
                            route_hybrid_key::<K>(r, base_seed, &perturbed, R2);
                        let values = read_state::<K>(r, base_seed, R2, &memory, &perturbed);
                        let tally = perturb.entry((K, r)).or_default();
                        tally.observations += 1;
                        tally.abs_cosine_sum += cosine(&exact_key, &perturbed_key).abs();
                        tally.equal_keys += u64::from(exact_key == perturbed_key);
                        tally.exact_clear += u64::from(exact_clear(&values, fact.target));
                    }
                }
            }
        }
    }
}

fn run_work<const K: usize>(work: &mut WorkCells) {
    for &r in &ROUTE_FRACTIONS {
        let mut tally = WorkTally::default();
        for seed in 0..SEEDS {
            let mut stream = tagged(seed, K as u64, r as u64, 1300);
            let state = build_state(stream.next(), stream.next(), stream.next());
            let (_, counters) = route_hybrid_key::<K>(r, seed * 1000 + 17, &state, R2);
            tally.add(counters, K);
        }
        work.insert((K, r), tally);
    }
}

fn ppm(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator.saturating_mul(1_000_000) / denominator
    }
}

fn mean_cosine(geometry: &GeometryCells, k: usize, r: usize, route_slot: usize) -> f64 {
    let tally = geometry[&(k, r, route_slot)];
    tally.abs_cosine_sum / tally.pairs as f64
}

fn continuity_ppm(continuity: &ContinuityCells, k: usize, r: usize, route_slot: usize) -> u64 {
    let tally = continuity[&(k, r, route_slot)];
    ppm(tally.exact_clear, tally.observations)
}

fn conflict_ppm(conflicts: &ConflictCells, k: usize, r: usize, kind: usize) -> u64 {
    let tally = conflicts[&(k, r, kind)];
    ppm(tally.exact_clear, tally.observations)
}

fn min_teacher_ppm(teacher: &TeacherCells, k: usize, load: usize, r: usize) -> u64 {
    (0..=2)
        .map(|hop| {
            let tally = teacher
                .get(&(k, load, r, hop))
                .copied()
                .unwrap_or_default();
            ppm(tally.exact_clear, tally.observations)
        })
        .min()
        .expect("teacher hops")
}

fn main() {
    let compatibility_checks = compatibility::<256>() + compatibility::<1024>();
    let mut geometry = GeometryCells::new();
    let mut continuity = ContinuityCells::new();
    let mut conflicts = ConflictCells::new();
    let mut teacher = TeacherCells::new();
    let mut chains = ChainCells::new();
    let mut perturb = PerturbCells::new();
    let mut collisions = CollisionCells::new();
    let mut work = WorkCells::new();

    run_geometry::<256>(&mut geometry);
    run_geometry::<1024>(&mut geometry);
    run_continuity::<256>(&mut continuity, &mut collisions);
    run_continuity::<1024>(&mut continuity, &mut collisions);
    run_conflict::<256>(&mut conflicts, &mut collisions);
    run_conflict::<1024>(&mut conflicts, &mut collisions);
    run_chains::<256>(&mut teacher, &mut chains, &mut perturb, &mut collisions);
    run_chains::<1024>(&mut teacher, &mut chains, &mut perturb, &mut collisions);
    run_work::<256>(&mut work);
    run_work::<1024>(&mut work);

    let mut out = String::new();
    writeln!(out, "N10_CONTROL,compatibility_assertions={compatibility_checks}").expect("write");

    for &k in &KEY_WIDTHS {
        for &r in &ROUTE_FRACTIONS {
            for route_slot in 0..3 {
                let tally = geometry[&(k, r, route_slot)];
                writeln!(
                    out,
                    "N10_GEOMETRY,K={k},r={r},from=R2,to={},pairs={},mean_abs_cosine={:.6},max_abs_cosine={:.6},equal_keys={}",
                    ["R3", "R4", "R6"][route_slot],
                    tally.pairs,
                    tally.abs_cosine_sum / tally.pairs as f64,
                    tally.max_abs_cosine,
                    tally.equal_keys,
                )
                .expect("write");
            }

            for route_slot in 0..3 {
                let tally = continuity[&(k, r, route_slot)];
                writeln!(
                    out,
                    "N10_CONTINUITY,K={k},r={r},write=R2,query={},observations={},exact_clear={},clear_ppm={}",
                    ["R2", "R3", "R4"][route_slot],
                    tally.observations,
                    tally.exact_clear,
                    ppm(tally.exact_clear, tally.observations),
                )
                .expect("write");
            }

            for kind in 0..2 {
                let tally = conflicts[&(k, r, kind)];
                writeln!(
                    out,
                    "N10_CONFLICT,K={k},r={r},kind={},observations={},exact_clear={},clear_ppm={},collisions={}",
                    ["secondary_R2_R3", "primary_R2_R6"][kind],
                    tally.observations,
                    tally.exact_clear,
                    ppm(tally.exact_clear, tally.observations),
                    collisions[&(k, r, 1 + kind)],
                )
                .expect("write");
            }

            for &load in &LOADS {
                for hop in 0..=2 {
                    if let Some(tally) = teacher.get(&(k, load, r, hop)) {
                        writeln!(
                            out,
                            "N10_TEACHER,K={k},G={load},r={r},hop={hop},observations={},exact_clear={},clear_ppm={}",
                            tally.observations,
                            tally.exact_clear,
                            ppm(tally.exact_clear, tally.observations),
                        )
                        .expect("write");
                    }
                }
                let tally = chains[&(k, load, r)];
                writeln!(
                    out,
                    "N10_CHAIN,K={k},G={load},r={r},queries={},F1={},F1_ppm={},F2={},F2_ppm={},F3={},F3_ppm={},F6={},F6_ppm={},ORACLE={},ORACLE_ppm={},collisions={}",
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
                    collisions[&(k, r, 3 + load)],
                )
                .expect("write");
            }

            let p = perturb[&(k, r)];
            writeln!(
                out,
                "N10_PERTURB,K={k},r={r},h=1,observations={},mean_abs_cosine={:.6},equal_keys={},exact_clear={},clear_ppm={}",
                p.observations,
                p.abs_cosine_sum / p.observations as f64,
                p.equal_keys,
                p.exact_clear,
                ppm(p.exact_clear, p.observations),
            )
            .expect("write");

            let w = work[&(k, r)];
            writeln!(
                out,
                "N10_WORK,K={k},r={r},constructions={},semantic_coordinates={},bucket_accumulations={},quantized_coordinates={},final_key_width={}",
                w.constructions,
                w.semantic_coordinates,
                w.bucket_accumulations,
                w.quantized_coordinates,
                w.final_key_width,
            )
            .expect("write");
        }
    }

    let h1 = compatibility_checks > 0
        && continuity_ppm(&continuity, 1024, 0, 1) == 1_000_000
        && continuity_ppm(&continuity, 1024, 4, 1) == 0
        && conflict_ppm(&conflicts, 1024, 0, 0) == 500_000
        && conflict_ppm(&conflicts, 1024, 4, 0) == 1_000_000;

    let h2 = [0_usize, 1].iter().all(|&route_slot| {
        ROUTE_FRACTIONS.windows(2).all(|pair| {
            mean_cosine(&geometry, 1024, pair[1], route_slot)
                <= mean_cosine(&geometry, 1024, pair[0], route_slot) + 0.000001
        })
    });

    let h3 = [1_usize, 2].iter().all(|&route_slot| {
        ROUTE_FRACTIONS.windows(2).all(|pair| {
            continuity_ppm(&continuity, 1024, pair[1], route_slot)
                <= continuity_ppm(&continuity, 1024, pair[0], route_slot)
        })
    });

    let h4 = ROUTE_FRACTIONS.windows(2).all(|pair| {
        conflict_ppm(&conflicts, 1024, pair[1], 0)
            >= conflict_ppm(&conflicts, 1024, pair[0], 0)
    });

    let route_qualifies = |r: usize| {
        conflict_ppm(&conflicts, 1024, r, 0) >= 850_000
            && continuity_ppm(&continuity, 1024, r, 1) >= 200_000
            && continuity_ppm(&continuity, 1024, r, 2) >= 200_000
            && collisions[&(1024, r, 1)] == 0
            && conflict_ppm(&conflicts, 1024, r, 1) >= 850_000
            && mean_cosine(&geometry, 1024, r, 2) <= 0.10
    };

    let capability_qualifies = |r: usize| {
        let low = chains[&(1024, 4, r)];
        let high = chains[&(1024, 16, r)];
        let p = perturb[&(1024, r)];
        ppm(low.f6, low.queries) >= 900_000
            && ppm(high.f6, high.queries) >= 850_000
            && min_teacher_ppm(&teacher, 1024, 4, r) >= 900_000
            && ppm(p.exact_clear, p.observations) >= 200_000
    };

    let route_candidates: Vec<usize> = [1_usize, 2, 3]
        .into_iter()
        .filter(|r| route_qualifies(*r))
        .collect();
    let selected = route_candidates
        .iter()
        .copied()
        .find(|r| capability_qualifies(*r));

    let h5 = !route_candidates.is_empty();
    let h6 = selected.is_some();

    let h7_status = if let Some(r) = selected {
        collisions[&(1024, r, 0)] == 0
            && collisions[&(1024, r, 7)] == 0
            && collisions[&(1024, r, 19)] == 0
    } else {
        false
    };

    let h8 = KEY_WIDTHS.iter().all(|&k| {
        let baseline = work[&(k, 0)];
        ROUTE_FRACTIONS.iter().all(|&r| work[&(k, r)] == baseline)
    });

    for (name, holds) in [
        ("H1_ENDPOINT_COMPATIBILITY", h1),
        ("H2_SECONDARY_COSINE_MONOTONE", h2),
        ("H3_SECONDARY_CONTINUITY_MONOTONE", h3),
        ("H4_SECONDARY_CONFLICT_MONOTONE", h4),
        ("H5_INTERIOR_ROUTE_TRADEOFF", h5),
        ("H6_INTERIOR_CAPABILITY", h6),
        ("H7_SELECTED_NO_COLLISIONS", h7_status),
        ("H8_EQUAL_LOGICAL_WORK", h8),
    ] {
        writeln!(
            out,
            "N10_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .expect("write");
    }

    writeln!(
        out,
        "N10_SELECTION,route_fraction={}",
        selected
            .map(|r| r.to_string())
            .unwrap_or_else(|| "NONE".to_string())
    )
    .expect("write");
    writeln!(out, "PROMOTION,BLOCKED,n10_harness_only").expect("write");

    print!("{out}");
}

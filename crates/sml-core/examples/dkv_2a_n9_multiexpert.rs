//! DKV-2a-N9 canonical multi-expert domain semantics.
//!
//! Protocol: docs/SML-DKV-2A-N9-MULTIEXPERT-PREREGISTRATION.md.
//! Harness-only: no core projector or Delta-KV memory implementation changes.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticKeyEvidence, DeltaKvSemanticProjectionPolicy,
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
const P2_FACTS: usize = 16;
const P1_PAIRS: usize = 200;
const Q: usize = 3;

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
const R5: &[u32] = &[5, 9, 7];
const R6: &[u32] = &[7, 5];
const ROUTES: [&[u32]; 6] = [R1, R2, R3, R4, R5, R6];
const ROUTE_NAMES: [&str; 6] = ["R1", "R2", "R3", "R4", "R5", "R6"];

const POLICIES: [&str; 2] = ["PRIMARY_ANCHORED", "ORDERED_ROUTE"];
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
    assert!(!route.is_empty(), "frozen routes are non-empty");
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
    assert_eq!(evidence.selected_experts, expected, "full route evidence");
    evidence
}

fn hybrid_key<const K: usize>(
    policy: usize,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> Vec<i16> {
    let page = source_projection::<K>(policy, false, base_seed, state, route);
    let value = source_projection::<K>(policy, true, base_seed, state, route);
    (0..K)
        .map(|index| {
            if index % 4 < Q {
                value.key_q12[index]
            } else {
                page.key_q12[index]
            }
        })
        .collect()
}

fn n8_singleton_key<const K: usize>(base_seed: u64, state: &State) -> Vec<i16> {
    let page_seed = mix64(base_seed ^ u64::from(5_u32).wrapping_mul(PAGE_C));
    let value_seed = mix64(page_seed ^ value_bits(state).wrapping_mul(VALUE_C));
    let page = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        page_seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("N8 page projector")
    .project(state, &decision(R1), None)
    .expect("N8 page projection");
    let value = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        value_seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("N8 value projector")
    .project(state, &decision(R1), None)
    .expect("N8 value projection");
    (0..K)
        .map(|index| {
            if index % 4 < Q {
                value.key_q12[index]
            } else {
                page.key_q12[index]
            }
        })
        .collect()
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
struct ConflictTally {
    reads: u64,
    exact_clear: u64,
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
    exact_clear: u64,
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

fn build_memory<const K: usize>(
    policy: usize,
    base_seed: u64,
    route: &[u32],
    facts: &[(State, u64, usize)],
    collisions: &mut u64,
) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    let mut ordered: Vec<&(State, u64, usize)> = facts.iter().collect();
    ordered.sort_by_key(|fact| fact.2);
    let mut unique = BTreeSet::new();
    for (state, target, _) in ordered {
        let key = hybrid_key::<K>(policy, base_seed, state, route);
        *collisions += u64::from(!unique.insert(key.clone()));
        memory
            .update(&key, &target_q12(*target), FULL_RATE, &mut counters)
            .expect("update");
    }
    memory
}

fn read_state<const K: usize>(
    policy: usize,
    base_seed: u64,
    route: &[u32],
    memory: &DeltaKvMemory,
    state: &State,
) -> Vec<i32> {
    let key = hybrid_key::<K>(policy, base_seed, state, route);
    let mut counters = DeltaKvCounters::default();
    memory.read(&key, &mut counters).expect("read")
}

type GeometryCells = BTreeMap<(usize, usize, usize), GeometryTally>;
type ContinuityCells = BTreeMap<(usize, usize, usize), RecallTally>;
type ConflictCells = BTreeMap<(usize, usize, usize), ConflictTally>;
type TeacherCells = BTreeMap<(usize, usize, usize, usize), RecallTally>;
type ChainCells = BTreeMap<(usize, usize, usize), ChainTally>;
type PerturbCells = BTreeMap<(usize, usize), PerturbTally>;
type CollisionCells = BTreeMap<(usize, usize, usize), u64>;

fn compatibility<const K: usize>() -> u64 {
    let mut checks = 0_u64;
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let mut stream = tagged(seed, K as u64, 0, 909);
        for _ in 0..50 {
            let state = build_state(stream.next(), stream.next(), stream.next());
            let n8 = n8_singleton_key::<K>(base_seed, &state);
            for policy in 0..2 {
                let first = hybrid_key::<K>(policy, base_seed, &state, R1);
                let second = hybrid_key::<K>(policy, base_seed, &state, R1);
                assert_eq!(first, n8, "H1 singleton compatibility");
                assert_eq!(first, second, "determinism");
                checks += 2;
            }
        }

        let duplicate = decision(&[5, 5]);
        let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
            DeltaKvSemanticProjectionPolicy::default(),
            base_seed,
            DeltaKvAmbiguityProjection::ActiveOnly,
        )
        .expect("projector");
        let state = build_state(stream.next(), stream.next(), stream.next());
        assert!(projector.project(&state, &duplicate, None).is_err());
        checks += 1;
    }
    checks
}

fn run_geometry<const K: usize>(geometry: &mut GeometryCells) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let mut stream = tagged(seed, K as u64, 1, 910);
        for _ in 0..P1_PAIRS {
            let state = build_state(stream.next(), stream.next(), stream.next());
            for policy in 0..2 {
                let anchor = hybrid_key::<K>(policy, base_seed, &state, R2);
                for route_index in [2_usize, 3, 4, 5] {
                    let other = hybrid_key::<K>(policy, base_seed, &state, ROUTES[route_index]);
                    geometry
                        .entry((K, policy, route_index))
                        .or_default()
                        .add(&anchor, &other);
                }
            }
        }
    }
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

fn run_continuity<const K: usize>(
    continuity: &mut ContinuityCells,
    collisions: &mut CollisionCells,
) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let facts = p2_facts(seed);
        for policy in 0..2 {
            let writes: Vec<(State, u64, usize)> = facts
                .iter()
                .enumerate()
                .map(|(index, (state, target))| (state.clone(), *target, index))
                .collect();
            let collision = collisions.entry((K, policy, 0)).or_default();
            let memory = build_memory::<K>(policy, base_seed, R2, &writes, collision);
            for (state, target) in &facts {
                for (route_slot, route) in [(1_usize, R2), (2, R3), (3, R4)] {
                    let values = read_state::<K>(policy, base_seed, route, &memory, state);
                    continuity
                        .entry((K, policy, route_slot))
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

fn run_conflict<const K: usize>(conflicts: &mut ConflictCells, collisions: &mut CollisionCells) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        for policy in 0..2 {
            for (conflict_kind, route_b) in [(0_usize, R3), (1_usize, R6)] {
                let mut stream = tagged(seed, K as u64, policy as u64, 920 + conflict_kind as u64);
                let mut records = Vec::new();
                let mut reads = Vec::new();
                for index in 0..P2_FACTS {
                    let state = build_state(stream.next(), stream.next(), stream.next());
                    let (target_a, target_b) = conflict_targets(&mut stream);
                    records.push((state.clone(), target_a, R2, index * 2));
                    records.push((state.clone(), target_b, route_b, index * 2 + 1));
                    reads.push((state.clone(), target_a, R2));
                    reads.push((state, target_b, route_b));
                }
                for index in (1..records.len()).rev() {
                    let swap_with = stream.below(index + 1);
                    records.swap(index, swap_with);
                }

                let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
                let mut counters = DeltaKvCounters::default();
                let mut unique = BTreeSet::new();
                for (state, target, route, _) in &records {
                    let key = hybrid_key::<K>(policy, base_seed, state, route);
                    *collisions
                        .entry((K, policy, 1 + conflict_kind))
                        .or_default() += u64::from(!unique.insert(key.clone()));
                    memory
                        .update(&key, &target_q12(*target), FULL_RATE, &mut counters)
                        .expect("update");
                }

                let tally = conflicts.entry((K, policy, conflict_kind)).or_default();
                for (state, target, route) in &reads {
                    let values = read_state::<K>(policy, base_seed, route, &memory, state);
                    tally.reads += 1;
                    tally.exact_clear += u64::from(exact_clear(&values, *target));
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

            for policy in 0..2 {
                let records: Vec<(State, u64, usize)> = writes
                    .iter()
                    .map(|fact| (fact.state.clone(), fact.target, fact.global_pos))
                    .collect();
                let collision = collisions.entry((K, policy, 3 + load)).or_default();
                let memory = build_memory::<K>(policy, base_seed, R2, &records, collision);

                for chain in &chains {
                    for hop in 0..chain.length {
                        let state = build_state(chain.codes[hop], chain.meaning, chain.routing);
                        let values = read_state::<K>(policy, base_seed, R2, &memory, &state);
                        teacher
                            .entry((K, load, policy, hop))
                            .or_default()
                            .add(&values, chain.codes[hop + 1]);
                    }
                }

                for chain in &chains {
                    let expected = chain.codes[chain.length];
                    let mut state = build_state(chain.codes[0], chain.meaning, chain.routing);
                    let mut hits = [false; 6];
                    for hit in &mut hits {
                        let values = read_state::<K>(policy, base_seed, R2, &memory, &state);
                        state = rewrite_once(&state, &values);
                        *hit = chain_correct(&state, expected);
                    }
                    let tally = chains_out.entry((K, load, policy)).or_default();
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
                        let exact_key = hybrid_key::<K>(policy, base_seed, &fact.state, R2);
                        let perturbed_key = hybrid_key::<K>(policy, base_seed, &perturbed, R2);
                        let values = read_state::<K>(policy, base_seed, R2, &memory, &perturbed);
                        let tally = perturb.entry((K, policy)).or_default();
                        tally.observations += 1;
                        tally.abs_cosine_sum += cosine(&exact_key, &perturbed_key).abs();
                        tally.exact_clear += u64::from(exact_clear(&values, fact.target));
                    }
                }
            }
        }
    }
}

fn ppm(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator.saturating_mul(1_000_000) / denominator
    }
}

fn mean_cosine(geometry: &GeometryCells, k: usize, policy: usize, route_index: usize) -> f64 {
    let tally = geometry[&(k, policy, route_index)];
    tally.abs_cosine_sum / tally.pairs as f64
}

fn min_teacher_ppm(teacher: &TeacherCells, k: usize, load: usize, policy: usize) -> u64 {
    (0..=2)
        .map(|hop| {
            let tally = teacher
                .get(&(k, load, policy, hop))
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

    run_geometry::<256>(&mut geometry);
    run_geometry::<1024>(&mut geometry);
    run_continuity::<256>(&mut continuity, &mut collisions);
    run_continuity::<1024>(&mut continuity, &mut collisions);
    run_conflict::<256>(&mut conflicts, &mut collisions);
    run_conflict::<1024>(&mut conflicts, &mut collisions);
    run_chains::<256>(&mut teacher, &mut chains, &mut perturb, &mut collisions);
    run_chains::<1024>(&mut teacher, &mut chains, &mut perturb, &mut collisions);

    let mut out = String::new();
    writeln!(
        out,
        "N9_CONTROL,compatibility_assertions={compatibility_checks}"
    )
    .expect("write");

    for &k in &KEY_WIDTHS {
        for policy in 0..2 {
            for route_index in [2_usize, 3, 4, 5] {
                let tally = geometry[&(k, policy, route_index)];
                writeln!(
                    out,
                    "N9_GEOMETRY,K={k},policy={},from=R2,to={},pairs={},mean_abs_cosine={:.6},max_abs_cosine={:.6},equal_keys={}",
                    POLICIES[policy],
                    ROUTE_NAMES[route_index],
                    tally.pairs,
                    tally.abs_cosine_sum / tally.pairs as f64,
                    tally.max_abs_cosine,
                    tally.equal_keys,
                )
                .expect("write");
            }

            for route_slot in [1_usize, 2, 3] {
                let tally = continuity[&(k, policy, route_slot)];
                writeln!(
                    out,
                    "N9_CONTINUITY,K={k},policy={},write=R2,query={},observations={},exact_clear={},clear_ppm={}",
                    POLICIES[policy],
                    ROUTE_NAMES[route_slot],
                    tally.observations,
                    tally.exact_clear,
                    ppm(tally.exact_clear, tally.observations),
                )
                .expect("write");
            }

            for conflict_kind in 0..2 {
                let tally = conflicts[&(k, policy, conflict_kind)];
                writeln!(
                    out,
                    "N9_CONFLICT,K={k},policy={},kind={},reads={},exact_clear={},clear_ppm={}",
                    POLICIES[policy],
                    ["secondary_R2_R3", "primary_R2_R6"][conflict_kind],
                    tally.reads,
                    tally.exact_clear,
                    ppm(tally.exact_clear, tally.reads),
                )
                .expect("write");
            }

            for &load in &LOADS {
                for hop in 0..=2 {
                    if let Some(tally) = teacher.get(&(k, load, policy, hop)) {
                        writeln!(
                            out,
                            "N9_TEACHER,K={k},G={load},policy={},route=R2,hop={hop},observations={},exact_clear={},clear_ppm={}",
                            POLICIES[policy],
                            tally.observations,
                            tally.exact_clear,
                            ppm(tally.exact_clear, tally.observations),
                        )
                        .expect("write");
                    }
                }
                let tally = chains[&(k, load, policy)];
                writeln!(
                    out,
                    "N9_CHAIN,K={k},G={load},policy={},route=R2,queries={},F1={},F1_ppm={},F2={},F2_ppm={},F3={},F3_ppm={},F6={},F6_ppm={},ORACLE={},ORACLE_ppm={}",
                    POLICIES[policy],
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
            }

            let p = perturb[&(k, policy)];
            writeln!(
                out,
                "N9_PERTURB,K={k},policy={},route=R2,h=1,observations={},mean_abs_cosine={:.6},exact_clear={},clear_ppm={}",
                POLICIES[policy],
                p.observations,
                p.abs_cosine_sum / p.observations as f64,
                p.exact_clear,
                ppm(p.exact_clear, p.observations),
            )
            .expect("write");
        }
    }

    for ((k, policy, cell), count) in &collisions {
        writeln!(
            out,
            "N9_COLLISIONS,K={k},policy={},cell={cell},count={count}",
            POLICIES[*policy]
        )
        .expect("write");
    }

    let h1 = compatibility_checks > 0;

    let primary_r3 = continuity[&(1024, P_PRIMARY, 2)];
    let primary_r4 = continuity[&(1024, P_PRIMARY, 3)];
    let h2 = mean_cosine(&geometry, 1024, P_PRIMARY, 2) >= 0.15
        && mean_cosine(&geometry, 1024, P_PRIMARY, 3) >= 0.15
        && ppm(primary_r3.exact_clear, primary_r3.observations) >= 200_000
        && ppm(primary_r4.exact_clear, primary_r4.observations) >= 200_000;

    let primary_switch = conflicts[&(1024, P_PRIMARY, 1)];
    let h3 = mean_cosine(&geometry, 1024, P_PRIMARY, 5) <= 0.10
        && ppm(primary_switch.exact_clear, primary_switch.reads) >= 850_000;

    let ordered_secondary = conflicts[&(1024, P_ORDERED, 0)];
    let h4 = ppm(ordered_secondary.exact_clear, ordered_secondary.reads) >= 850_000;

    let ordered_r3 = continuity[&(1024, P_ORDERED, 2)];
    let h5 = ppm(ordered_r3.exact_clear, ordered_r3.observations) <= 100_000;

    let primary_g4 = chains[&(1024, 4, P_PRIMARY)];
    let primary_g16 = chains[&(1024, 16, P_PRIMARY)];
    let h6 = ppm(primary_g4.f6, primary_g4.queries) >= 900_000
        && ppm(primary_g16.f6, primary_g16.queries) >= 850_000;

    let h7 = min_teacher_ppm(&teacher, 1024, 4, P_PRIMARY) >= 900_000;

    let primary_perturb = perturb[&(1024, P_PRIMARY)];
    let h8 = ppm(primary_perturb.exact_clear, primary_perturb.observations) >= 200_000;

    let h9 = collisions.values().all(|count| *count == 0);

    for (name, holds) in [
        ("H1_SINGLETON_COMPATIBILITY", h1),
        ("H2_PRIMARY_SECONDARY_CONTINUITY", h2),
        ("H3_PRIMARY_SWITCH_ISOLATION", h3),
        ("H4_ORDERED_SECONDARY_ISOLATION", h4),
        ("H5_ORDERED_SECONDARY_BRITTLENESS", h5),
        ("H6_PRIMARY_CHAIN_CAPABILITY", h6),
        ("H7_PRIMARY_TEACHER_CAPABILITY", h7),
        ("H8_PRIMARY_HAMMING1_ROBUSTNESS", h8),
        ("H9_NO_COLLISIONS", h9),
    ] {
        writeln!(
            out,
            "N9_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .expect("write");
    }

    let selected = h1 && h2 && h3 && h6 && h7 && h8 && h9;
    writeln!(
        out,
        "N9_SELECTION,canonical={}",
        if selected { "PRIMARY_ANCHORED" } else { "NONE" }
    )
    .expect("write");
    writeln!(out, "PROMOTION,BLOCKED,n9_development_semantics_only").expect("write");

    print!("{out}");
}

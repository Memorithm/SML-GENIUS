//! DKV-2a-N7 harness-only within-page value-domain conditioning.
//!
//! Protocol: docs/SML-DKV-2A-N7-VALUE-DOMAIN-PREREGISTRATION.md.

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
const ARMS: [&str; 2] = ["PAGE", "PAGE_VALUE"];
const A_PAGE: usize = 0;
const A_VALUE: usize = 1;
const SUBSETS: [&str; 3] = ["SELF", "SELF_SAME", "FULL"];
const S_SELF: usize = 0;
const S_SAME: usize = 1;
const S_FULL: usize = 2;
const PERTURB_H: [usize; 3] = [1, 2, 4];

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
        acc | (u64::from(state.value.get(bit).expect("value bit")) << bit)
    })
}

fn ambiguity_clear(state: &State) -> bool {
    (0..BITS).all(|bit| !state.ambiguity.get(bit).expect("ambiguity bit"))
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

fn arm_seed(arm: usize, base_seed: u64, state: &State) -> u64 {
    let page_seed = mix64(base_seed ^ u64::from(MOR_PAGE).wrapping_mul(PAGE_C));
    match arm {
        A_PAGE => page_seed,
        A_VALUE => mix64(page_seed ^ value_bits(state).wrapping_mul(VALUE_C)),
        _ => unreachable!("two arms"),
    }
}

fn projector_for<const K: usize>(
    arm: usize,
    base_seed: u64,
    state: &State,
) -> DeltaKvSemanticProjector<K> {
    DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        arm_seed(arm, base_seed, state),
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector")
}

fn project<const K: usize>(
    arm: usize,
    base_seed: u64,
    state: &State,
) -> (Vec<i16>, DeltaKvSemanticProjectionCounters) {
    let evidence = projector_for::<K>(arm, base_seed, state)
        .project(state, &decision(), None)
        .expect("project");
    (evidence.key_q12, evidence.counters)
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

type P1Cells = BTreeMap<(usize, usize, usize, usize), RecallTally>;
type GeometryCells = BTreeMap<(usize, usize, usize, usize), GeometryTally>;
type TeacherCells = BTreeMap<(usize, usize, usize, usize, bool), RecallTally>;
type ChainCells = BTreeMap<(usize, usize, usize), ChainTally>;
type PerturbCells = BTreeMap<(usize, usize, usize), PerturbTally>;

fn build_memory<const K: usize>(arm: usize, base_seed: u64, facts: &[WriteFact]) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    let mut ordered: Vec<&WriteFact> = facts.iter().collect();
    ordered.sort_by_key(|fact| fact.global_pos);
    for fact in ordered {
        let (key, _) = project::<K>(arm, base_seed, &fact.state);
        memory
            .update(&key, &target_q12(fact.target), FULL_RATE, &mut counters)
            .expect("update");
    }
    memory
}

fn build_subset_memory<const K: usize>(
    arm: usize,
    base_seed: u64,
    subset: &[&WriteFact],
) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    let mut ordered = subset.to_vec();
    ordered.sort_by_key(|fact| fact.global_pos);
    for fact in ordered {
        let (key, _) = project::<K>(arm, base_seed, &fact.state);
        memory
            .update(&key, &target_q12(fact.target), FULL_RATE, &mut counters)
            .expect("update");
    }
    memory
}

fn read_state<const K: usize>(
    arm: usize,
    base_seed: u64,
    memory: &DeltaKvMemory,
    state: &State,
) -> Vec<i32> {
    let (key, _) = project::<K>(arm, base_seed, state);
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
    teacher: &mut TeacherCells,
    chains_out: &mut ChainCells,
    perturb: &mut PerturbCells,
    collisions: &mut BTreeMap<(usize, usize, usize), u64>,
) {
    for &load in &LOADS {
        for seed in 0..SEEDS {
            let chains: Vec<Chain> = (0..load)
                .map(|index| build_chain(seed, load, index))
                .collect();
            let facts = build_writes(seed, load, &chains);
            let base_seed = seed * 1000 + 17;

            for arm in 0..2 {
                let full = build_memory::<K>(arm, base_seed, &facts);

                // Written-state key collision check.
                let mut unique = BTreeSet::<Vec<i16>>::new();
                let mut collision_count = 0_u64;
                for fact in &facts {
                    let (key, first_counters) = project::<K>(arm, base_seed, &fact.state);
                    let (key_again, second_counters) = project::<K>(arm, base_seed, &fact.state);
                    assert_eq!(key, key_again, "H7 deterministic key");
                    assert_eq!(first_counters, second_counters, "H7 deterministic counters");
                    collision_count += u64::from(!unique.insert(key));
                }
                *collisions.entry((K, load, arm)).or_default() += collision_count;

                // P1 + geometry, G4 only.
                if load == 4 {
                    let keys: BTreeMap<usize, Vec<i16>> = facts
                        .iter()
                        .map(|fact| {
                            let (key, _) = project::<K>(arm, base_seed, &fact.state);
                            (fact.id, key)
                        })
                        .collect();

                    for query in facts.iter().filter(|fact| fact.terminal) {
                        let length = query.source_hop;
                        let self_subset = vec![query];
                        let same_subset: Vec<&WriteFact> = facts
                            .iter()
                            .filter(|fact| fact.chain == query.chain)
                            .collect();

                        for (subset_index, subset) in [(S_SELF, self_subset), (S_SAME, same_subset)]
                        {
                            let memory = build_subset_memory::<K>(arm, base_seed, &subset);
                            let values = read_state::<K>(arm, base_seed, &memory, &query.state);
                            p1.entry((K, arm, length, subset_index))
                                .or_default()
                                .add(&values, query.target);
                        }
                        let full_values = read_state::<K>(arm, base_seed, &full, &query.state);
                        p1.entry((K, arm, length, S_FULL))
                            .or_default()
                            .add(&full_values, query.target);

                        let anchor = &keys[&query.id];
                        for fact in facts.iter().filter(|fact| fact.id != query.id) {
                            let kind = usize::from(fact.chain != query.chain);
                            geometry
                                .entry((K, arm, length, kind))
                                .or_default()
                                .add(anchor, &keys[&fact.id]);
                        }
                    }
                }

                // P2 teacher-forced exact true nodes.
                for chain in &chains {
                    for hop in 0..=chain.length {
                        let state = build_state(chain.codes[hop], chain.meaning, chain.routing);
                        let target = if hop == chain.length {
                            chain.codes[hop]
                        } else {
                            chain.codes[hop + 1]
                        };
                        let values = read_state::<K>(arm, base_seed, &full, &state);
                        let terminal = hop == chain.length;
                        teacher
                            .entry((K, load, arm, hop, terminal))
                            .or_default()
                            .add(&values, target);
                    }
                }

                // P3 six-cycle free-running replay.
                for chain in &chains {
                    let expected = chain.codes[chain.length];
                    let mut state = build_state(chain.codes[0], chain.meaning, chain.routing);
                    let mut hits = [false; 6];
                    for cycle in 0..6 {
                        let values = read_state::<K>(arm, base_seed, &full, &state);
                        state = rewrite_once(&state, &values);
                        hits[cycle] = chain_correct(&state, expected);
                    }
                    let tally = chains_out.entry((K, load, arm)).or_default();
                    tally.queries += 1;
                    tally.f1 += u64::from(hits[0]);
                    tally.f2 += u64::from(hits[1]);
                    tally.f3 += u64::from(hits[2]);
                    tally.f6 += u64::from(hits[5]);
                    tally.oracle += u64::from(hits.iter().any(|hit| *hit));
                }

                // P4 near-state brittleness, exact non-terminal written sources at G4.
                if load == 4 {
                    for fact in facts.iter().filter(|fact| !fact.terminal) {
                        let (exact_key, _) = project::<K>(arm, base_seed, &fact.state);
                        for &h in &PERTURB_H {
                            let perturbed = perturb_state(&fact.state, seed, fact.id, h);
                            let (perturbed_key, _) = project::<K>(arm, base_seed, &perturbed);
                            let values = read_state::<K>(arm, base_seed, &full, &perturbed);
                            let entry = perturb.entry((K, arm, h)).or_default();
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

fn ppm(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator.saturating_mul(1_000_000) / denominator
    }
}

fn main() {
    let mut p1 = P1Cells::new();
    let mut geometry = GeometryCells::new();
    let mut teacher = TeacherCells::new();
    let mut chains = ChainCells::new();
    let mut perturb = PerturbCells::new();
    let mut collisions = BTreeMap::new();

    run_width::<256>(
        &mut p1,
        &mut geometry,
        &mut teacher,
        &mut chains,
        &mut perturb,
        &mut collisions,
    );
    run_width::<1024>(
        &mut p1,
        &mut geometry,
        &mut teacher,
        &mut chains,
        &mut perturb,
        &mut collisions,
    );

    let mut out = String::new();

    for &k in &KEY_WIDTHS {
        for arm in 0..2 {
            for length in 0..4 {
                for subset in 0..3 {
                    let tally = p1[&(k, arm, length, subset)];
                    writeln!(
                        out,
                        "N7_P1,K={k},arm={},L={length},subset={},observations={},exact_clear={},clear_ppm={},sign_exact={},wrong_coordinates={},min_abs={}",
                        ARMS[arm],
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
                    if let Some(tally) = geometry.get(&(k, arm, length, kind)) {
                        writeln!(
                            out,
                            "N7_GEOMETRY,K={k},arm={},L={length},kind={},pairs={},mean_abs_cosine={:.6},max_abs_cosine={:.6},equal_keys={}",
                            ARMS[arm],
                            ["same", "cross"][kind],
                            tally.pairs,
                            tally.abs_cosine_sum / tally.pairs as f64,
                            tally.max_abs_cosine,
                            tally.equal_keys,
                        )
                        .expect("write");
                    }
                }
            }
        }
    }

    for &k in &KEY_WIDTHS {
        for &load in &LOADS {
            for arm in 0..2 {
                for hop in 0..4 {
                    for terminal in [false, true] {
                        if let Some(tally) = teacher.get(&(k, load, arm, hop, terminal)) {
                            writeln!(
                                out,
                                "N7_TEACHER,K={k},G={load},arm={},hop={hop},terminal={terminal},observations={},exact_clear={},clear_ppm={},sign_exact={},wrong_coordinates={},min_abs={}",
                                ARMS[arm],
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
                let tally = chains[&(k, load, arm)];
                writeln!(
                    out,
                    "N7_CHAIN,K={k},G={load},arm={},queries={},F1={},F1_ppm={},F2={},F2_ppm={},F3={},F3_ppm={},F6={},F6_ppm={},ORACLE={},ORACLE_ppm={}",
                    ARMS[arm],
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
                    "N7_COLLISIONS,K={k},G={load},arm={},count={}",
                    ARMS[arm],
                    collisions[&(k, load, arm)],
                )
                .expect("write");
            }
        }
    }

    for &k in &KEY_WIDTHS {
        for arm in 0..2 {
            for &h in &PERTURB_H {
                let tally = perturb[&(k, arm, h)];
                writeln!(
                    out,
                    "N7_PERTURB,K={k},arm={},h={h},observations={},mean_abs_cosine={:.6},equal_keys={},exact_clear={},clear_ppm={}",
                    ARMS[arm],
                    tally.observations,
                    tally.abs_cosine_sum / tally.observations as f64,
                    tally.equal_keys,
                    tally.exact_clear,
                    ppm(tally.exact_clear, tally.observations),
                )
                .expect("write");
            }
        }
    }

    let h1 = p1[&(1024, A_PAGE, 0, S_SAME)].exact_clear == 16
        && p1[&(1024, A_PAGE, 1, S_SAME)].exact_clear == 16
        && p1[&(1024, A_PAGE, 2, S_SAME)].exact_clear == 14
        && p1[&(1024, A_PAGE, 3, S_SAME)].exact_clear == 8
        && p1[&(1024, A_PAGE, 0, S_FULL)].exact_clear == 16
        && p1[&(1024, A_PAGE, 1, S_FULL)].exact_clear == 16
        && p1[&(1024, A_PAGE, 2, S_FULL)].exact_clear == 13
        && p1[&(1024, A_PAGE, 3, S_FULL)].exact_clear == 8;

    let h2 = (1..=3).all(|length| {
        let tally = geometry[&(1024, A_VALUE, length, 0)];
        tally.abs_cosine_sum / tally.pairs as f64 <= 0.10
    });

    let h3 = [2, 3].iter().all(|&length| {
        let tally = p1[&(1024, A_VALUE, length, S_SAME)];
        ppm(tally.exact_clear, tally.observations) >= 950_000
    });

    let h4 = (0..=2).all(|hop| {
        let tally = teacher[&(1024, 4, A_VALUE, hop, false)];
        ppm(tally.exact_clear, tally.observations) >= 900_000
    });

    let chain = chains[&(1024, 4, A_VALUE)];
    let h5 = ppm(chain.f6, chain.queries) >= 900_000;

    let perturb_h1 = perturb[&(1024, A_VALUE, 1)];
    let h6 = perturb_h1.abs_cosine_sum / perturb_h1.observations as f64 <= 0.10
        && ppm(perturb_h1.exact_clear, perturb_h1.observations) <= 200_000;

    let h7 = true; // enforced by assertions during every written-state projection.
    let h8 = KEY_WIDTHS.iter().all(|&k| {
        LOADS
            .iter()
            .all(|&load| collisions[&(k, load, A_VALUE)] == 0)
    });

    for (name, holds) in [
        ("H1_PAGE_CONTROL", h1),
        ("H2_VALUE_DOMAIN_ORTHOGONALIZES", h2),
        ("H3_SELF_SAME_RESTORED", h3),
        ("H4_TEACHER_CAPABILITY", h4),
        ("H5_FREE_RUNNING_CAPABILITY", h5),
        ("H6_ONE_BIT_BRITTLENESS", h6),
        ("H7_DETERMINISTIC", h7),
        ("H8_NO_COLLISIONS", h8),
    ] {
        writeln!(
            out,
            "N7_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .expect("write");
    }
    writeln!(out, "PROMOTION,BLOCKED,n7_harness_only").expect("write");

    print!("{out}");
}

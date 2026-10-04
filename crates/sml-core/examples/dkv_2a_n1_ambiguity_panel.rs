//! DKV-2a-N1 current-stack ambiguity projection panel.
//!
//! Protocol: docs/SML-DKV-2A-N1-CURRENT-PANEL-PROTOCOL.md.
//! Both arms call DeltaKvSemanticProjector directly.

use std::{collections::BTreeMap, fmt::Write as _};

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
const WIDTHS: [usize; 3] = [64, 256, 1024];
const LOADS: [usize; 5] = [1, 8, 16, 32, 64];
const ARM_NAMES: [&str; 2] = ["Bipolar", "ActiveOnly"];
const DIST_NAMES: [&str; 3] = ["ZERO", "SPARSE", "DENSE_CONTROL"];
const NEAR_H: [usize; 4] = [1, 2, 4, 8];
const BASES: usize = 40;
const CLEAR_ABOVE: u32 = 128;
const PAIRS: usize = 200;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;

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

fn tagged(seed: u64, a: u64, b: u64) -> Stream {
    Stream::new(
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ 0x3C3C_3C3C_3C3C_3C3C
            ^ (a + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ (b + 1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
    )
}

fn decision() -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(5)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn random_state(stream: &mut Stream, dist: usize) -> State {
    let (value, meaning, routing) = (stream.next(), stream.next(), stream.next());
    let mut state = State::zeroed().expect("valid state width");
    for bit in 0..BITS {
        state.value.set(bit, (value >> bit) & 1 == 1).unwrap();
        state
            .meaning_mask
            .set(bit, (meaning >> bit) & 1 == 1)
            .unwrap();
        state.routing.set(bit, (routing >> bit) & 1 == 1).unwrap();
    }
    match dist {
        1 => {
            let first = stream.below(BITS);
            let mut second = stream.below(BITS);
            while second == first {
                second = stream.below(BITS);
            }
            state.ambiguity.set(first, true).unwrap();
            state.ambiguity.set(second, true).unwrap();
        }
        2 => {
            let ambiguity = stream.next();
            for bit in 0..BITS {
                state
                    .ambiguity
                    .set(bit, (ambiguity >> bit) & 1 == 1)
                    .unwrap();
            }
        }
        _ => {}
    }
    state
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

fn cosine(a: &[i16], b: &[i16]) -> f64 {
    let dot: f64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| f64::from(*x) * f64::from(*y))
        .sum();
    let na: f64 = a.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    let nb: f64 = b.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>().sqrt();
    dot / (na * nb)
}

fn mode(arm: usize) -> DeltaKvAmbiguityProjection {
    match arm {
        0 => DeltaKvAmbiguityProjection::Bipolar,
        1 => DeltaKvAmbiguityProjection::ActiveOnly,
        _ => unreachable!("two frozen arms"),
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ProjectionWork {
    coordinates: u64,
    fields: u64,
    mor_experts: u64,
    buckets: u64,
    quantized: u64,
}

impl ProjectionWork {
    fn add(&mut self, counters: DeltaKvSemanticProjectionCounters) {
        self.coordinates = self
            .coordinates
            .saturating_add(counters.semantic_coordinates_read);
        self.fields = self.fields.saturating_add(counters.semantic_fields_read);
        self.mor_experts = self
            .mor_experts
            .saturating_add(counters.mor_selected_experts_read);
        self.buckets = self.buckets.saturating_add(counters.bucket_accumulations);
        self.quantized = self
            .quantized
            .saturating_add(counters.quantized_key_coordinates);
    }

    fn merge(&mut self, other: Self) {
        self.coordinates = self.coordinates.saturating_add(other.coordinates);
        self.fields = self.fields.saturating_add(other.fields);
        self.mor_experts = self.mor_experts.saturating_add(other.mor_experts);
        self.buckets = self.buckets.saturating_add(other.buckets);
        self.quantized = self.quantized.saturating_add(other.quantized);
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct RecallTally {
    facts: u64,
    strict: u64,
    sign: u64,
    wrong_coordinates: u64,
    projection: ProjectionWork,
    memory: DeltaKvCounters,
    matrix_semantic_bits: u128,
}

#[derive(Debug, Default, Clone, Copy)]
struct GeometryTally {
    cosine_sum: f64,
    pairs: u64,
    projection: ProjectionWork,
}

#[derive(Debug, Default)]
struct Results {
    recall: BTreeMap<(usize, usize, usize, usize), RecallTally>,
    geometry: BTreeMap<(usize, usize, usize), GeometryTally>,
    alias: BTreeMap<(usize, usize, usize), (u64, u64)>,
    compatibility_checked: u64,
    compatibility_equal: u64,
}

fn add_memory(dst: &mut DeltaKvCounters, src: DeltaKvCounters) {
    dst.reads = dst.reads.saturating_add(src.reads);
    dst.updates = dst.updates.saturating_add(src.updates);
    dst.read_macs = dst.read_macs.saturating_add(src.read_macs);
    dst.weight_updates = dst.weight_updates.saturating_add(src.weight_updates);
    dst.saturations = dst.saturations.saturating_add(src.saturations);
}

fn run_width<const K: usize>(results: &mut Results) {
    let mor = decision();
    for seed in 0..SEEDS {
        let projector_seed = seed * 1000 + 17;
        let legacy = DeltaKvSemanticProjector::<K>::new(
            DeltaKvSemanticProjectionPolicy::default(),
            projector_seed,
        )
        .unwrap();
        let explicit = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
            DeltaKvSemanticProjectionPolicy::default(),
            projector_seed,
            DeltaKvAmbiguityProjection::Bipolar,
        )
        .unwrap();

        let mut compatibility = tagged(seed, 3, K as u64);
        for _ in 0..PAIRS {
            let state = random_state(&mut compatibility, 2);
            let left = legacy.project(&state, &mor, None).unwrap();
            let right = explicit.project(&state, &mor, None).unwrap();
            results.compatibility_checked += 1;
            if left == right {
                results.compatibility_equal += 1;
            }
        }

        for arm in 0..2 {
            let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
                DeltaKvSemanticProjectionPolicy::default(),
                projector_seed,
                mode(arm),
            )
            .unwrap();

            for dist in 0..3 {
                let mut pairs = tagged(seed, 7 + dist as u64, K as u64);
                let geometry = results.geometry.entry((K, arm, dist)).or_default();
                for _ in 0..PAIRS {
                    let state_a = random_state(&mut pairs, dist);
                    let state_b = random_state(&mut pairs, dist);
                    let left = projector.project(&state_a, &mor, None).unwrap();
                    let right = projector.project(&state_b, &mor, None).unwrap();
                    geometry.cosine_sum += cosine(&left.key_q12, &right.key_q12);
                    geometry.pairs += 1;
                    geometry.projection.add(left.counters);
                    geometry.projection.add(right.counters);
                }

                for &load in &LOADS {
                    let mut stream = tagged(seed, 100 + load as u64, dist as u64);
                    let mut facts: Vec<(State, u64)> = (0..load)
                        .map(|_| (random_state(&mut stream, dist), stream.next()))
                        .collect();
                    for i in (1..facts.len()).rev() {
                        let j = stream.below(i + 1);
                        facts.swap(i, j);
                    }

                    let mut memory = DeltaKvMemory::new(K, BITS).unwrap();
                    let matrix_semantic_bits = memory.semantic_bits();
                    let mut counters = DeltaKvCounters::default();
                    let mut projection = ProjectionWork::default();

                    for (state, code) in &facts {
                        let evidence = projector.project(state, &mor, None).unwrap();
                        projection.add(evidence.counters);
                        memory
                            .update(
                                &evidence.key_q12,
                                &target_q12(*code),
                                FULL_RATE,
                                &mut counters,
                            )
                            .unwrap();
                    }

                    let mut local = RecallTally {
                        matrix_semantic_bits,
                        ..RecallTally::default()
                    };
                    for (state, code) in &facts {
                        let evidence = projector.project(state, &mor, None).unwrap();
                        projection.add(evidence.counters);
                        let values = memory.read(&evidence.key_q12, &mut counters).unwrap();
                        let mut clear = true;
                        let mut wrong = 0_u64;
                        for (bit, value) in values.iter().enumerate() {
                            if value.unsigned_abs() <= CLEAR_ABOVE {
                                clear = false;
                            }
                            if (*value > 0) != ((code >> bit) & 1 == 1) {
                                wrong += 1;
                            }
                        }
                        local.facts += 1;
                        local.wrong_coordinates = local.wrong_coordinates.saturating_add(wrong);
                        if wrong == 0 {
                            local.sign += 1;
                            if clear {
                                local.strict += 1;
                            }
                        }
                    }
                    local.projection = projection;
                    local.memory = counters;

                    let tally = results.recall.entry((K, arm, dist, load)).or_default();
                    tally.facts = tally.facts.saturating_add(local.facts);
                    tally.strict = tally.strict.saturating_add(local.strict);
                    tally.sign = tally.sign.saturating_add(local.sign);
                    tally.wrong_coordinates = tally
                        .wrong_coordinates
                        .saturating_add(local.wrong_coordinates);
                    tally.projection.merge(local.projection);
                    add_memory(&mut tally.memory, local.memory);
                    tally.matrix_semantic_bits = local.matrix_semantic_bits;
                }
            }

            for base_index in 0..BASES {
                let mut stream = tagged(seed, 900 + base_index as u64, K as u64);
                let base = random_state(&mut stream, 0);
                let base_key = projector.project(&base, &mor, None).unwrap().key_q12;
                for &h in &NEAR_H {
                    let mut pool: Vec<usize> = (0..3 * BITS).collect();
                    for index in 0..h {
                        let swap_with = index + stream.below(3 * BITS - index);
                        pool.swap(index, swap_with);
                    }
                    let mut near = base.clone();
                    for &position in &pool[..h] {
                        let bit = position % BITS;
                        let field = match position / BITS {
                            0 => &mut near.value,
                            1 => &mut near.meaning_mask,
                            _ => &mut near.routing,
                        };
                        let current = field.get(bit).unwrap();
                        field.set(bit, !current).unwrap();
                    }
                    let near_key = projector.project(&near, &mor, None).unwrap().key_q12;
                    let entry = results.alias.entry((K, arm, h)).or_default();
                    entry.1 += 1;
                    if near_key == base_key {
                        entry.0 += 1;
                    }
                }
            }
        }
    }
}

fn run_grid() -> Results {
    let mut results = Results::default();
    run_width::<64>(&mut results);
    run_width::<256>(&mut results);
    run_width::<1024>(&mut results);
    results
}

fn strict_ppm(results: &Results, k: usize, arm: usize, dist: usize, load: usize) -> u64 {
    let tally = results.recall[&(k, arm, dist, load)];
    tally.strict.saturating_mul(1_000_000) / tally.facts
}

fn sign_ppm(results: &Results, k: usize, arm: usize, dist: usize, load: usize) -> u64 {
    let tally = results.recall[&(k, arm, dist, load)];
    tally.sign.saturating_mul(1_000_000) / tally.facts
}

fn mean_cosine(results: &Results, k: usize, arm: usize, dist: usize) -> f64 {
    let tally = results.geometry[&(k, arm, dist)];
    tally.cosine_sum / tally.pairs as f64
}

fn alias_ppm(results: &Results, k: usize, arm: usize, h: usize) -> u64 {
    let (same, total) = results.alias[&(k, arm, h)];
    same.saturating_mul(1_000_000) / total
}

fn build_report() -> String {
    let results = run_grid();
    let mut out = String::new();

    writeln!(
        out,
        "CONTROL,compatibility,checked={},equal={}",
        results.compatibility_checked, results.compatibility_equal
    )
    .unwrap();

    for (dist, dist_name) in DIST_NAMES.iter().enumerate() {
        for (arm, arm_name) in ARM_NAMES.iter().enumerate() {
            for &k in &WIDTHS {
                let geometry = results.geometry[&(k, arm, dist)];
                writeln!(
                    out,
                    "GEOMETRY,K={k},arm={arm_name},dist={dist_name},pairs={},mean_cosine={:.6},semantic_coordinates={},semantic_fields={},mor_experts={},bucket_accumulations={},quantized_coordinates={}",
                    geometry.pairs,
                    mean_cosine(&results, k, arm, dist),
                    geometry.projection.coordinates,
                    geometry.projection.fields,
                    geometry.projection.mor_experts,
                    geometry.projection.buckets,
                    geometry.projection.quantized,
                )
                .unwrap();

                for &load in &LOADS {
                    let tally = results.recall[&(k, arm, dist, load)];
                    writeln!(
                        out,
                        "RECALL,K={k},arm={arm_name},dist={dist_name},W={load},facts={},strict={},strict_ppm={},sign={},sign_ppm={},wrong_coordinates={},semantic_coordinates={},bucket_accumulations={},reads={},read_macs={},updates={},weight_updates={},saturations={},matrix_semantic_bits={}",
                        tally.facts,
                        tally.strict,
                        strict_ppm(&results, k, arm, dist, load),
                        tally.sign,
                        sign_ppm(&results, k, arm, dist, load),
                        tally.wrong_coordinates,
                        tally.projection.coordinates,
                        tally.projection.buckets,
                        tally.memory.reads,
                        tally.memory.read_macs,
                        tally.memory.updates,
                        tally.memory.weight_updates,
                        tally.memory.saturations,
                        tally.matrix_semantic_bits,
                    )
                    .unwrap();
                }
            }
        }
    }

    for (arm, arm_name) in ARM_NAMES.iter().enumerate() {
        for &k in &WIDTHS {
            for &h in &NEAR_H {
                let (same, total) = results.alias[&(k, arm, h)];
                writeln!(
                    out,
                    "ALIAS,K={k},arm={arm_name},h={h},same={same},total={total},rate_ppm={}",
                    alias_ppm(&results, k, arm, h)
                )
                .unwrap();
            }
        }
    }

    let h1 = results.compatibility_checked == results.compatibility_equal;
    let projections = SEEDS * PAIRS as u64 * 2;
    let h2 = WIDTHS.iter().all(|&k| {
        results.geometry[&(k, 0, 0)].projection.buckets == projections * (4 * BITS as u64 + 1)
            && results.geometry[&(k, 1, 0)].projection.buckets
                == projections * (3 * BITS as u64 + 1)
    });
    let h3 = WIDTHS
        .iter()
        .all(|&k| mean_cosine(&results, k, 1, 0) < mean_cosine(&results, k, 0, 0));
    let h4 = [256, 1024]
        .iter()
        .all(|&k| strict_ppm(&results, k, 1, 0, 16) > strict_ppm(&results, k, 0, 0, 16));
    let h5 = [32, 64]
        .iter()
        .all(|&load| strict_ppm(&results, 64, 1, 0, load) < strict_ppm(&results, 64, 1, 0, 16));
    let h6 = WIDTHS
        .iter()
        .all(|&k| results.recall.contains_key(&(k, 1, 1, 16)));
    let h8 = WIDTHS.iter().all(|&k| {
        NEAR_H.iter().all(|&h| {
            alias_ppm(&results, k, 1, h) <= alias_ppm(&results, k, 0, h).saturating_add(20_000)
        })
    });

    for (name, value) in [
        ("H1_COMPATIBILITY", h1),
        ("H2_ZERO_WORK_REDUCTION", h2),
        ("H3_ZERO_GEOMETRY", h3),
        ("H4_ZERO_W16_RECALL", h4),
        ("H5_FINITE_CAPACITY", h5),
        ("H6_SPARSE_MEASURED", h6),
        ("H8_NEAR_ALIAS_BOUND", h8),
    ] {
        writeln!(
            out,
            "HYPOTHESIS,{name},{}",
            if value { "HOLDS" } else { "REFUTED" }
        )
        .unwrap();
    }

    writeln!(out, "SCOPE,H7_AUTHORITY_UNCHANGED,TRUE").unwrap();
    writeln!(out, "PROMOTION,BLOCKED,current_stack_panel_only").unwrap();
    out
}

fn main() {
    print!("{}", build_report());
}

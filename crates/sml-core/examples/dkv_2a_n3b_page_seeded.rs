//! DKV-2a-N3B current-stack replay: page-conditioned projection.
//!
//! Protocol: docs/SML-DKV-2A-N3B-PAGE-SEEDED-REPLAY.md.

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
const WIDTHS: [usize; 2] = [256, 1024];
const PAGES: usize = 8;
const E1_LOAD: usize = 16;
const E2_LOAD: usize = 8;
const PAIRS: usize = 200;
const CLEAR_ABOVE: u32 = 128;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const ARM_NAMES: [&str; 2] = ["Additive", "PageSeeded"];
const PAGE_SEED_MIX: u64 = 0x9e37_79b9_7f4a_7c15;

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
            ^ 0x7E7E_7E7E_7E7E_7E7E
            ^ (a + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93)
            ^ (b + 1).wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
    )
}

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn page_seed(base: u64, page: u32) -> u64 {
    mix64(base ^ u64::from(page).wrapping_mul(PAGE_SEED_MIX))
}

fn decision(page: u32) -> MorDecision {
    MorDecision {
        active_routers: Vec::new(),
        ranked_experts: Vec::new(),
        selected_experts: vec![PageId(page)],
        noise_ppm: 0,
        structural_disagreement_ppm: 0,
        expert_mass_threshold_ppm: 0,
        request_precision_escalation: false,
        hysteresis_retained_previous_primary: false,
    }
}

fn random_state(stream: &mut Stream) -> State {
    let (value, meaning, routing) = (stream.next(), stream.next(), stream.next());
    let mut state = State::zeroed().expect("state");
    for bit in 0..BITS {
        state.value.set(bit, (value >> bit) & 1 == 1).expect("v");
        state
            .meaning_mask
            .set(bit, (meaning >> bit) & 1 == 1)
            .expect("m");
        state.routing.set(bit, (routing >> bit) & 1 == 1).expect("r");
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

fn strict_recall(values: &[i32], code: u64) -> bool {
    values.iter().enumerate().all(|(bit, value)| {
        value.unsigned_abs() > CLEAR_ABOVE && (*value > 0) == ((code >> bit) & 1 == 1)
    })
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

fn projection_seed(base_seed: u64, page: u32, arm: usize) -> u64 {
    match arm {
        0 => base_seed,
        1 => page_seed(base_seed, page),
        _ => unreachable!("two arms"),
    }
}

fn project<const K: usize>(
    base_seed: u64,
    state: &State,
    page: u32,
    arm: usize,
) -> (Vec<i16>, DeltaKvSemanticProjectionCounters) {
    let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        projection_seed(base_seed, page, arm),
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector");
    let evidence = projector
        .project(state, &decision(page), None)
        .expect("project");
    (evidence.key_q12, evidence.counters)
}

#[derive(Debug, Clone, Copy, Default)]
struct E1 {
    facts: u64,
    own: u64,
    wrong: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct E2 {
    reads: u64,
    strict: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct E3 {
    same_sum: f64,
    cross_sum: f64,
    pairs: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Work {
    projections: u64,
    semantic_coordinates: u64,
    bucket_accumulations: u64,
    quantized_coordinates: u64,
}

impl Work {
    fn add(&mut self, counters: DeltaKvSemanticProjectionCounters) {
        self.projections += 1;
        self.semantic_coordinates = self
            .semantic_coordinates
            .saturating_add(counters.semantic_coordinates_read);
        self.bucket_accumulations = self
            .bucket_accumulations
            .saturating_add(counters.bucket_accumulations);
        self.quantized_coordinates = self
            .quantized_coordinates
            .saturating_add(counters.quantized_key_coordinates);
    }
}

#[derive(Debug, Default)]
struct Results {
    e1: BTreeMap<(usize, usize), E1>,
    e2: BTreeMap<(usize, usize), E2>,
    e3: BTreeMap<(usize, usize), E3>,
    work: BTreeMap<(usize, usize), Work>,
}

fn run_width<const K: usize>(results: &mut Results) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        for arm in 0..2 {
            let mut e1_stream = tagged(seed, 10 + E1_LOAD as u64, 0);
            let facts: Vec<(State, u64, u32)> = (0..E1_LOAD)
                .map(|_| {
                    let state = random_state(&mut e1_stream);
                    let code = e1_stream.next();
                    let page = 1 + e1_stream.below(PAGES) as u32;
                    (state, code, page)
                })
                .collect();
            let mut order: Vec<usize> = (0..E1_LOAD).collect();
            for index in (1..E1_LOAD).rev() {
                let swap_with = e1_stream.below(index + 1);
                order.swap(index, swap_with);
            }

            let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
            let mut counters = DeltaKvCounters::default();
            for &index in &order {
                let (state, code, page) = &facts[index];
                let (key, key_counters) = project::<K>(base_seed, state, *page, arm);
                results.work.entry((K, arm)).or_default().add(key_counters);
                memory
                    .update(&key, &target_q12(*code), FULL_RATE, &mut counters)
                    .expect("update");
            }
            let e1 = results.e1.entry((K, arm)).or_default();
            for (state, code, page) in &facts {
                let (own_key, own_counters) = project::<K>(base_seed, state, *page, arm);
                results.work.entry((K, arm)).or_default().add(own_counters);
                let own = memory.read(&own_key, &mut counters).expect("read");

                let other_page =
                    1 + ((*page as usize - 1) + 1 + e1_stream.below(PAGES - 1)) % PAGES;
                let (wrong_key, wrong_counters) =
                    project::<K>(base_seed, state, other_page as u32, arm);
                results
                    .work
                    .entry((K, arm))
                    .or_default()
                    .add(wrong_counters);
                let wrong = memory.read(&wrong_key, &mut counters).expect("read");

                e1.facts += 1;
                e1.own += u64::from(strict_recall(&own, *code));
                e1.wrong += u64::from(strict_recall(&wrong, *code));
            }

            let mut e2_stream = tagged(seed, 20 + E2_LOAD as u64, 0);
            let mut writes: Vec<(State, u64, u32)> = Vec::new();
            for _ in 0..E2_LOAD {
                let state = random_state(&mut e2_stream);
                let first_page = 1 + e2_stream.below(PAGES) as u32;
                let second_page =
                    1 + ((first_page as usize - 1) + 1 + e2_stream.below(PAGES - 1)) % PAGES;
                writes.push((state.clone(), e2_stream.next(), first_page));
                writes.push((state, e2_stream.next(), second_page as u32));
            }
            let reads = writes.clone();
            for index in (1..writes.len()).rev() {
                let swap_with = e2_stream.below(index + 1);
                writes.swap(index, swap_with);
            }

            let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
            let mut counters = DeltaKvCounters::default();
            for (state, code, page) in &writes {
                let (key, key_counters) = project::<K>(base_seed, state, *page, arm);
                results.work.entry((K, arm)).or_default().add(key_counters);
                memory
                    .update(&key, &target_q12(*code), FULL_RATE, &mut counters)
                    .expect("update");
            }
            let e2 = results.e2.entry((K, arm)).or_default();
            for (state, code, page) in &reads {
                let (key, key_counters) = project::<K>(base_seed, state, *page, arm);
                results.work.entry((K, arm)).or_default().add(key_counters);
                let values = memory.read(&key, &mut counters).expect("read");
                e2.reads += 1;
                e2.strict += u64::from(strict_recall(&values, *code));
            }

            let mut e3_stream = tagged(seed, 40, K as u64);
            let e3 = results.e3.entry((K, arm)).or_default();
            for _ in 0..PAIRS {
                let a = random_state(&mut e3_stream);
                let b = random_state(&mut e3_stream);
                let page = 1 + e3_stream.below(PAGES) as u32;
                let other_page =
                    1 + ((page as usize - 1) + 1 + e3_stream.below(PAGES - 1)) % PAGES;
                let (a_page, a_counters) = project::<K>(base_seed, &a, page, arm);
                let (b_page, b_counters) = project::<K>(base_seed, &b, page, arm);
                let (a_other, cross_counters) =
                    project::<K>(base_seed, &a, other_page as u32, arm);
                let work = results.work.entry((K, arm)).or_default();
                work.add(a_counters);
                work.add(b_counters);
                work.add(cross_counters);
                e3.same_sum += cosine(&a_page, &b_page);
                e3.cross_sum += cosine(&a_page, &a_other);
                e3.pairs += 1;
            }
        }
    }
}

fn run_grid() -> Results {
    let mut results = Results::default();
    run_width::<256>(&mut results);
    run_width::<1024>(&mut results);
    results
}

fn ppm(numerator: u64, denominator: u64) -> u64 {
    numerator.saturating_mul(1_000_000) / denominator
}

fn report() -> String {
    let results = run_grid();
    let mut out = String::new();

    for &k in &WIDTHS {
        for (arm, arm_name) in ARM_NAMES.iter().enumerate() {
            let e1 = results.e1[&(k, arm)];
            let e2 = results.e2[&(k, arm)];
            let e3 = results.e3[&(k, arm)];
            let work = results.work[&(k, arm)];
            writeln!(
                out,
                "N3B,K={k},arm={arm_name},e1_facts={},own={},own_ppm={},wrong={},wrong_ppm={},e2_reads={},e2_strict={},e2_ppm={},same_page_cosine={:.6},cross_page_cosine={:.6},projections={},semantic_coordinates={},bucket_accumulations={},quantized_coordinates={}",
                e1.facts,
                e1.own,
                ppm(e1.own, e1.facts),
                e1.wrong,
                ppm(e1.wrong, e1.facts),
                e2.reads,
                e2.strict,
                ppm(e2.strict, e2.reads),
                e3.same_sum / e3.pairs as f64,
                e3.cross_sum / e3.pairs as f64,
                work.projections,
                work.semantic_coordinates,
                work.bucket_accumulations,
                work.quantized_coordinates,
            )
            .unwrap();
        }
    }

    let e1 = |k: usize, arm: usize| {
        let value = results.e1[&(k, arm)];
        (ppm(value.own, value.facts), ppm(value.wrong, value.facts))
    };
    let e2 = |k: usize, arm: usize| {
        let value = results.e2[&(k, arm)];
        ppm(value.strict, value.reads)
    };
    let e3 = |k: usize, arm: usize| {
        let value = results.e3[&(k, arm)];
        (
            value.same_sum / value.pairs as f64,
            value.cross_sum / value.pairs as f64,
        )
    };

    let h0 = e1(1024, 0) == (984_375, 984_375) && e2(1024, 0) == 496_093;
    let h1 = WIDTHS.iter().all(|&k| e1(k, 1).1 <= 50_000);
    let h2 = WIDTHS.iter().all(|&k| e2(k, 1) >= 850_000);
    let h3 = WIDTHS.iter().all(|&k| {
        let additive = e1(k, 0).0 as i64;
        let seeded = e1(k, 1).0 as i64;
        (seeded - additive).unsigned_abs() <= 30_000
    });
    let h4 = WIDTHS.iter().all(|&k| {
        let (same, cross) = e3(k, 1);
        same.abs() <= 0.05 && cross.abs() <= 0.1
    });
    let work_equal = WIDTHS.iter().all(|&k| {
        let additive = results.work[&(k, 0)];
        let seeded = results.work[&(k, 1)];
        additive.projections == seeded.projections
            && additive.semantic_coordinates == seeded.semantic_coordinates
            && additive.bucket_accumulations == seeded.bucket_accumulations
            && additive.quantized_coordinates == seeded.quantized_coordinates
    });

    for (name, holds) in [
        ("H0_CONTROL", h0),
        ("H1_ISOLATION", h1),
        ("H2_CONFLICT_RESOLVES", h2),
        ("H3_OWN_RECALL_WITHIN_3PT", h3),
        ("H4_GEOMETRY", h4),
        ("WORK_COUNTERS_EQUAL", work_equal),
    ] {
        writeln!(
            out,
            "N3B_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .unwrap();
    }
    writeln!(out, "PROMOTION,BLOCKED,n3b_single_page_only").unwrap();
    out
}

fn main() {
    print!("{}", report());
}

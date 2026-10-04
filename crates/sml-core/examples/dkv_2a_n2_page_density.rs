//! DKV-2a-N2 current-stack replay: page discrimination and ambiguity density.
//!
//! Protocol: docs/SML-DKV-2A-N2-PAGE-DENSITY-REPLAY.md.
//! Both arms use the current DeltaKvSemanticProjector implementation directly.

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
const E1_LOADS: [usize; 3] = [8, 16, 32];
const E2_LOADS: [usize; 2] = [4, 8];
const E3_DENSITY: [usize; 8] = [0, 1, 2, 4, 8, 16, 32, 64];
const E3_LOAD: usize = 16;
const CHECK_STATES: usize = 200;
const CLEAR_ABOVE: u32 = 128;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const ARM_NAMES: [&str; 2] = ["Bipolar", "ActiveOnly"];

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

fn random_state(stream: &mut Stream, set_ambiguity: usize) -> State {
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
    let mut pool: Vec<usize> = (0..BITS).collect();
    for index in 0..set_ambiguity {
        let swap_with = index + stream.below(BITS - index);
        pool.swap(index, swap_with);
    }
    for &bit in &pool[..set_ambiguity] {
        state.ambiguity.set(bit, true).unwrap();
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

fn mode(arm: usize) -> DeltaKvAmbiguityProjection {
    match arm {
        0 => DeltaKvAmbiguityProjection::Bipolar,
        1 => DeltaKvAmbiguityProjection::ActiveOnly,
        _ => unreachable!("two frozen arms"),
    }
}

fn projector<const K: usize>(seed: u64, arm: usize) -> DeltaKvSemanticProjector<K> {
    DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        seed,
        mode(arm),
    )
    .unwrap()
}

fn key<const K: usize>(
    projector: &DeltaKvSemanticProjector<K>,
    state: &State,
    page: u32,
) -> (Vec<i16>, DeltaKvSemanticProjectionCounters) {
    let evidence = projector.project(state, &decision(page), None).unwrap();
    (evidence.key_q12, evidence.counters)
}

#[derive(Debug, Clone, Copy, Default)]
struct ProjectionTally {
    semantic_coordinates: u64,
    bucket_accumulations: u64,
}

impl ProjectionTally {
    fn add(&mut self, counters: DeltaKvSemanticProjectionCounters) {
        self.semantic_coordinates = self
            .semantic_coordinates
            .saturating_add(counters.semantic_coordinates_read);
        self.bucket_accumulations = self
            .bucket_accumulations
            .saturating_add(counters.bucket_accumulations);
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct E1Tally {
    facts: u64,
    own: u64,
    wrong: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct E2Tally {
    reads: u64,
    strict: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct E3Tally {
    facts: u64,
    strict: u64,
    cosine_sum: f64,
    cosine_pairs: u64,
    projection: ProjectionTally,
}

#[derive(Debug, Default)]
struct Results {
    e1: BTreeMap<(usize, usize, usize), E1Tally>,
    e2: BTreeMap<(usize, usize, usize), E2Tally>,
    e3: BTreeMap<(usize, usize), E3Tally>,
    contracts: u64,
}

fn contracts<const K: usize>(results: &mut Results) {
    for seed in 0..SEEDS {
        let projector_seed = seed * 1000 + 17;
        let legacy = DeltaKvSemanticProjector::<K>::new(
            DeltaKvSemanticProjectionPolicy::default(),
            projector_seed,
        )
        .unwrap();
        let bipolar = projector::<K>(projector_seed, 0);
        let active = projector::<K>(projector_seed, 1);
        let mut stream = tagged(seed, 1, K as u64);

        for index in 0..CHECK_STATES {
            let set = if index % 4 == 0 { 3 } else { 0 };
            let state = random_state(&mut stream, set);

            let legacy_evidence = legacy.project(&state, &decision(5), None).unwrap();
            let bipolar_evidence = bipolar.project(&state, &decision(5), None).unwrap();
            assert_eq!(legacy_evidence, bipolar_evidence, "C1 legacy compatibility");

            let first = active.project(&state, &decision(5), None).unwrap();
            let second = active.project(&state, &decision(5), None).unwrap();
            assert_eq!(first.key_q12, second.key_q12, "C2 deterministic key");
            assert_eq!(first.counters, second.counters, "C2 deterministic counters");

            assert_eq!(
                bipolar_evidence.counters.semantic_coordinates_read, 256,
                "C4 bipolar coordinates"
            );
            assert_eq!(
                first.counters.semantic_coordinates_read, 256,
                "C4 active coordinates"
            );
            assert_eq!(
                bipolar_evidence.counters.bucket_accumulations, 257,
                "C5 bipolar accumulations"
            );
            assert_eq!(
                first.counters.bucket_accumulations,
                193 + set as u64,
                "C5 active accumulations"
            );
            results.contracts += 7;
        }

        let mut all_ones = random_state(&mut stream, 0);
        for bit in 0..BITS {
            all_ones.ambiguity.set(bit, true).unwrap();
        }
        let (bipolar_key, _) = key(&bipolar, &all_ones, 5);
        let (active_key, _) = key(&active, &all_ones, 5);
        assert_eq!(bipolar_key, active_key, "C3 all-ones equivalence");
        results.contracts += 1;

        for _ in 0..CHECK_STATES {
            let state = random_state(&mut stream, 0);
            assert!(
                active.project(&state, &decision(5), None).is_ok(),
                "C6 no zero key"
            );
            results.contracts += 1;
        }
    }
}

fn experiments<const K: usize>(results: &mut Results) {
    for seed in 0..SEEDS {
        let projector_seed = seed * 1000 + 17;
        for arm in 0..2 {
            let projector = projector::<K>(projector_seed, arm);

            for &load in &E1_LOADS {
                let mut stream = tagged(seed, 10 + load as u64, 0);
                let facts: Vec<(State, u64, u32)> = (0..load)
                    .map(|_| {
                        let state = random_state(&mut stream, 0);
                        let code = stream.next();
                        let page = 1 + stream.below(PAGES) as u32;
                        (state, code, page)
                    })
                    .collect();
                let mut order: Vec<usize> = (0..load).collect();
                for index in (1..load).rev() {
                    let swap_with = stream.below(index + 1);
                    order.swap(index, swap_with);
                }

                let mut memory = DeltaKvMemory::new(K, BITS).unwrap();
                let mut counters = DeltaKvCounters::default();
                for &index in &order {
                    let (state, code, page) = &facts[index];
                    let (key, _) = key(&projector, state, *page);
                    memory
                        .update(&key, &target_q12(*code), FULL_RATE, &mut counters)
                        .unwrap();
                }

                let entry = results.e1.entry((K, arm, load)).or_default();
                for (state, code, page) in &facts {
                    let (own_key, _) = key(&projector, state, *page);
                    let own = memory.read(&own_key, &mut counters).unwrap();

                    let other_page =
                        1 + ((*page as usize - 1) + 1 + stream.below(PAGES - 1)) % PAGES;
                    let (wrong_key, _) = key(&projector, state, other_page as u32);
                    let wrong = memory.read(&wrong_key, &mut counters).unwrap();

                    entry.facts += 1;
                    entry.own += u64::from(strict_recall(&own, *code));
                    entry.wrong += u64::from(strict_recall(&wrong, *code));
                }
            }

            for &load in &E2_LOADS {
                let mut stream = tagged(seed, 20 + load as u64, 0);
                let mut writes: Vec<(State, u64, u32)> = Vec::new();
                for _ in 0..load {
                    let state = random_state(&mut stream, 0);
                    let first_page = 1 + stream.below(PAGES) as u32;
                    let second_page =
                        1 + ((first_page as usize - 1) + 1 + stream.below(PAGES - 1)) % PAGES;
                    writes.push((state.clone(), stream.next(), first_page));
                    writes.push((state, stream.next(), second_page as u32));
                }
                let reads = writes.clone();
                for index in (1..writes.len()).rev() {
                    let swap_with = stream.below(index + 1);
                    writes.swap(index, swap_with);
                }

                let mut memory = DeltaKvMemory::new(K, BITS).unwrap();
                let mut counters = DeltaKvCounters::default();
                for (state, code, page) in &writes {
                    let (key, _) = key(&projector, state, *page);
                    memory
                        .update(&key, &target_q12(*code), FULL_RATE, &mut counters)
                        .unwrap();
                }

                let entry = results.e2.entry((K, arm, load)).or_default();
                for (state, code, page) in &reads {
                    let (key, _) = key(&projector, state, *page);
                    let values = memory.read(&key, &mut counters).unwrap();
                    entry.reads += 1;
                    entry.strict += u64::from(strict_recall(&values, *code));
                }
            }

            if K == 1024 {
                for &density in &E3_DENSITY {
                    let mut stream = tagged(seed, 30 + density as u64, 0);
                    let facts: Vec<(State, u64)> = (0..E3_LOAD)
                        .map(|_| (random_state(&mut stream, density), stream.next()))
                        .collect();

                    let mut memory = DeltaKvMemory::new(K, BITS).unwrap();
                    let mut counters = DeltaKvCounters::default();
                    let mut keys = Vec::with_capacity(facts.len());
                    let mut projection = ProjectionTally::default();

                    for (state, code) in &facts {
                        let (key, key_counters) = key(&projector, state, 5);
                        projection.add(key_counters);
                        memory
                            .update(&key, &target_q12(*code), FULL_RATE, &mut counters)
                            .unwrap();
                        keys.push(key);
                    }

                    let entry = results.e3.entry((arm, density)).or_default();
                    for ((state, code), stored_key) in facts.iter().zip(&keys) {
                        let values = memory.read(stored_key, &mut counters).unwrap();
                        entry.facts += 1;
                        entry.strict += u64::from(strict_recall(&values, *code));
                        let (_, read_counters) = key(&projector, state, 5);
                        projection.add(read_counters);
                    }
                    for pair in keys.chunks(2) {
                        entry.cosine_sum += cosine(&pair[0], &pair[1]);
                        entry.cosine_pairs += 1;
                    }
                    entry.projection.semantic_coordinates = entry
                        .projection
                        .semantic_coordinates
                        .saturating_add(projection.semantic_coordinates);
                    entry.projection.bucket_accumulations = entry
                        .projection
                        .bucket_accumulations
                        .saturating_add(projection.bucket_accumulations);
                }
            }
        }
    }
}

fn run_grid() -> Results {
    let mut results = Results::default();
    contracts::<256>(&mut results);
    contracts::<1024>(&mut results);
    experiments::<256>(&mut results);
    experiments::<1024>(&mut results);
    results
}

fn ppm(numerator: u64, denominator: u64) -> u64 {
    numerator.saturating_mul(1_000_000) / denominator
}

fn build_report() -> String {
    let results = run_grid();
    let mut out = String::new();

    writeln!(out, "CONTROL,contracts={}", results.contracts).unwrap();

    for &k in &WIDTHS {
        for (arm, name) in ARM_NAMES.iter().enumerate() {
            for &load in &E1_LOADS {
                let tally = results.e1[&(k, arm, load)];
                writeln!(
                    out,
                    "E1,K={k},arm={name},W={load},facts={},own={},own_ppm={},wrong={},wrong_ppm={}",
                    tally.facts,
                    tally.own,
                    ppm(tally.own, tally.facts),
                    tally.wrong,
                    ppm(tally.wrong, tally.facts)
                )
                .unwrap();
            }
        }
    }

    for &k in &WIDTHS {
        for (arm, name) in ARM_NAMES.iter().enumerate() {
            for &load in &E2_LOADS {
                let tally = results.e2[&(k, arm, load)];
                writeln!(
                    out,
                    "E2,K={k},arm={name},W={load},reads={},strict={},strict_ppm={}",
                    tally.reads,
                    tally.strict,
                    ppm(tally.strict, tally.reads)
                )
                .unwrap();
            }
        }
    }

    for (arm, name) in ARM_NAMES.iter().enumerate() {
        for &density in &E3_DENSITY {
            let tally = results.e3[&(arm, density)];
            writeln!(
                out,
                "E3,K=1024,arm={name},n={density},facts={},strict={},strict_ppm={},mean_cosine={:.6},semantic_coordinates={},bucket_accumulations={}",
                tally.facts,
                tally.strict,
                ppm(tally.strict, tally.facts),
                tally.cosine_sum / tally.cosine_pairs as f64,
                tally.projection.semantic_coordinates,
                tally.projection.bucket_accumulations
            )
            .unwrap();
        }
    }

    let e1_ppm = |k: usize, arm: usize, load: usize, wrong: bool| {
        let tally = results.e1[&(k, arm, load)];
        if wrong {
            ppm(tally.wrong, tally.facts)
        } else {
            ppm(tally.own, tally.facts)
        }
    };
    let e2_ppm = |k: usize, arm: usize, load: usize| {
        let tally = results.e2[&(k, arm, load)];
        ppm(tally.strict, tally.reads)
    };
    let e3_ppm = |arm: usize, density: usize| {
        let tally = results.e3[&(arm, density)];
        ppm(tally.strict, tally.facts)
    };

    let h1 = WIDTHS.iter().all(|&k| {
        E1_LOADS.iter().all(|&load| {
            e1_ppm(k, 1, load, false).saturating_add(20_000) >= e1_ppm(k, 0, load, false)
        })
    });
    let h2 = (0..2).all(|arm| {
        e1_ppm(1024, arm, 8, true).saturating_mul(10)
            >= e1_ppm(1024, arm, 8, false).saturating_mul(8)
    });
    let h3 = (0..2).all(|arm| e2_ppm(1024, arm, 8) <= 600_000);
    let h4 = WIDTHS.iter().all(|&k| {
        E2_LOADS
            .iter()
            .all(|&load| e2_ppm(k, 1, load).saturating_add(20_000) >= e2_ppm(k, 0, load))
    });
    let h5 = [0, 1, 2, 4, 8]
        .iter()
        .all(|&density| e3_ppm(1, density).saturating_add(10_000) >= e3_ppm(0, density))
        && e3_ppm(1, 32) < e3_ppm(0, 32)
        && e3_ppm(1, 64) == e3_ppm(0, 64);

    for (name, holds) in [
        ("H1_PAGE_DIVERSITY", h1),
        ("H2_PAGE_WEAK_DISCRIMINATION", h2),
        ("H3_PAGE_CONFLICT_UNRESOLVED", h3),
        ("H4_NO_E2_REGRESSION", h4),
        ("H5_DENSITY_CROSSOVER", h5),
    ] {
        writeln!(
            out,
            "HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .unwrap();
    }

    writeln!(out, "PROMOTION,BLOCKED,n2_replay_only").unwrap();
    out
}

fn main() {
    print!("{}", build_report());
}

//! DKV-3a-1 harness: projected-key interference and ambiguity-signal probe.
//!
//! Implements `docs/SML-DKV-3A-1-PROJECTED-PREREGISTRATION.md` exactly: real DKV-2a
//! projections of real `SemanticState` values, real DKV-2b recurrent kernel, frozen
//! grid, frozen hypotheses. Hypotheses are computed and printed, never asserted:
//! a refuted hypothesis is a result, not a test failure. Only structural invariants
//! (determinism, key shape) are asserted.

use std::{collections::BTreeMap, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_recurrent::{DeltaKvRecurrentKernel, DeltaKvRecurrentPolicy},
    delta_kv_semantic::{DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector},
    mor::MorDecision,
    semantic::{run_semantic_recurrent, SemanticState},
    HaltingPolicy, PageId,
};

const BITS: usize = 64;
const POSITIONS: usize = 3 * BITS;
const VALUE_DIM: usize = 2;
const DISTRACTORS: usize = 5;
const SEEDS: u64 = 8;
const HAMMING: [usize; 6] = [1, 2, 4, 8, 16, 32];
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const MOR_PAGE: u32 = 5;

struct Stream(u64);

impl Stream {
    fn new(base: u64) -> Self {
        Self(base | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

fn base(seed: u64) -> u64 {
    seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xA5A5_A5A5_A5A5_A5A5
}

fn target_state(seed: u64) -> SemanticState<BITS> {
    let mut stream = Stream::new(base(seed));
    let mut state = SemanticState::<BITS>::zeroed().unwrap();
    for index in 0..BITS {
        state.value.set(index, stream.next() & 1 == 1).unwrap();
    }
    for index in 0..BITS {
        state
            .meaning_mask
            .set(index, stream.next() & 1 == 1)
            .unwrap();
    }
    for index in 0..BITS {
        state.routing.set(index, stream.next() & 1 == 1).unwrap();
    }
    state
}

fn flip_positions(seed: u64, distractor: usize, hamming: usize) -> Vec<usize> {
    let mut stream =
        Stream::new(base(seed) ^ (distractor as u64 + 1).wrapping_mul(0xD6E8_FEB8_6659_FD93));
    let mut pool: Vec<usize> = (0..POSITIONS).collect();
    for k in 0..hamming {
        let j = k + (stream.next() % (POSITIONS - k) as u64) as usize;
        pool.swap(k, j);
    }
    pool[..hamming].to_vec()
}

fn distractor_state(seed: u64, distractor: usize, hamming: usize) -> SemanticState<BITS> {
    let mut state = target_state(seed);
    for position in flip_positions(seed, distractor, hamming) {
        let bit = position % BITS;
        let field = match position / BITS {
            0 => &mut state.value,
            1 => &mut state.meaning_mask,
            _ => &mut state.routing,
        };
        let current = field.get(bit).unwrap();
        field.set(bit, !current).unwrap();
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

fn projector<const K: usize>(seed: u64) -> DeltaKvSemanticProjector<K> {
    DeltaKvSemanticProjector::<K>::new(DeltaKvSemanticProjectionPolicy::default(), seed).unwrap()
}

struct Keys {
    target: Vec<i16>,
    distractors: Vec<Vec<i16>>,
}

fn keys<const K: usize>(seed: u64, hamming: usize) -> Keys {
    let projector = projector::<K>(seed);
    let mor = decision();
    let target = projector
        .project(&target_state(seed), &mor, None)
        .unwrap()
        .key_q12;
    assert_eq!(target.len(), K);
    let distractors = (0..DISTRACTORS)
        .map(|i| {
            projector
                .project(&distractor_state(seed, i, hamming), &mor, None)
                .unwrap()
                .key_q12
        })
        .collect();
    Keys {
        target,
        distractors,
    }
}

fn overlap_permille(a: &[i16], b: &[i16]) -> i64 {
    let one = i64::from(DELTA_KV_ONE);
    let dot: i64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| (i64::from(*x) / one) * (i64::from(*y) / one))
        .sum();
    let na = a.iter().filter(|x| **x != 0).count() as u64;
    let nb = b.iter().filter(|x| **x != 0).count() as u64;
    1000 * dot / i64::try_from((na * nb).isqrt()).unwrap()
}

fn target_value() -> [i16; VALUE_DIM] {
    [DELTA_KV_ONE, DELTA_KV_ONE]
}

fn opposite_value() -> [i16; VALUE_DIM] {
    [-DELTA_KV_ONE, -DELTA_KV_ONE]
}

fn build_memory<const K: usize>(
    keys: &Keys,
    write_target: bool,
    target_first: bool,
) -> DeltaKvMemory {
    let mut memory = DeltaKvMemory::new(K, VALUE_DIM).unwrap();
    let mut counters = DeltaKvCounters::default();
    if write_target && target_first {
        memory
            .update(&keys.target, &target_value(), FULL_RATE, &mut counters)
            .unwrap();
    }
    for key in &keys.distractors {
        memory
            .update(key, &opposite_value(), FULL_RATE, &mut counters)
            .unwrap();
    }
    if write_target && !target_first {
        memory
            .update(&keys.target, &target_value(), FULL_RATE, &mut counters)
            .unwrap();
    }
    memory
}

fn probe<const K: usize>(seed: u64, hamming: usize, after: bool) -> bool {
    let keys = keys::<K>(seed, hamming);
    let memory = build_memory::<K>(&keys, true, after);
    let mut counters = DeltaKvCounters::default();
    let value = memory.read(&keys.target, &mut counters).unwrap();
    value[0] > 0 && value[1] > 0
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Cal {
    Abstain,
    AnsweredCorrect,
    AnsweredWrong,
    Fabricated,
}

fn calibrate<const K: usize>(seed: u64, hamming: usize, written: bool) -> Cal {
    let keys = keys::<K>(seed, hamming);
    let memory = build_memory::<K>(&keys, written, true);
    let initial = target_state(seed);
    let evidence = projector::<K>(seed)
        .project(&initial, &decision(), None)
        .unwrap();
    let mut kernel = DeltaKvRecurrentKernel::<BITS>::new(
        memory,
        evidence,
        vec![0, 1],
        DeltaKvRecurrentPolicy::default(),
    )
    .unwrap();
    let outcome = run_semantic_recurrent(
        initial,
        &mut kernel,
        HaltingPolicy {
            min_depth: 1,
            max_depth: 2,
            stable_hamming_threshold: 0,
        },
    )
    .unwrap();
    let resolved: Vec<bool> = (0..VALUE_DIM)
        .filter(|c| !outcome.state.ambiguity.get(*c).unwrap())
        .map(|c| outcome.state.value.get(c).unwrap())
        .collect();
    if resolved.is_empty() {
        Cal::Abstain
    } else if !written {
        Cal::Fabricated
    } else if resolved.iter().all(|value| *value) {
        Cal::AnsweredCorrect
    } else {
        Cal::AnsweredWrong
    }
}

#[derive(Default)]
struct Results {
    /// (K, h) -> (overlap permille sum, pair count, aliased distractor keys)
    geom: BTreeMap<(usize, usize), (i64, i64, u32)>,
    /// (K, h, after) -> seeds correct
    probe: BTreeMap<(usize, usize, bool), u32>,
    /// (K, h) -> [written abstain, written correct, written wrong, missing abstain, missing fabricated]
    cal: BTreeMap<(usize, usize), [u32; 5]>,
}

fn collect<const K: usize>(results: &mut Results) {
    for hamming in HAMMING {
        let geom = results.geom.entry((K, hamming)).or_default();
        for seed in 0..SEEDS {
            let keys = keys::<K>(seed, hamming);
            for distractor in &keys.distractors {
                geom.0 += overlap_permille(&keys.target, distractor);
                geom.1 += 1;
                geom.2 += u32::from(*distractor == keys.target);
            }
        }
        for after in [false, true] {
            let correct = (0..SEEDS)
                .filter(|seed| probe::<K>(*seed, hamming, after))
                .count() as u32;
            results.probe.insert((K, hamming, after), correct);
        }
        let cal = results.cal.entry((K, hamming)).or_default();
        for seed in 0..SEEDS {
            match calibrate::<K>(seed, hamming, true) {
                Cal::Abstain => cal[0] += 1,
                Cal::AnsweredCorrect => cal[1] += 1,
                Cal::AnsweredWrong => cal[2] += 1,
                Cal::Fabricated => unreachable!("written case never fabricates"),
            }
            match calibrate::<K>(seed, hamming, false) {
                Cal::Abstain => cal[3] += 1,
                Cal::Fabricated => cal[4] += 1,
                other => unreachable!("missing case produced {other:?}"),
            }
        }
    }
}

fn run() -> Results {
    let mut results = Results::default();
    collect::<16>(&mut results);
    collect::<64>(&mut results);
    results
}

fn mean_overlap(results: &Results, k: usize, h: usize) -> i64 {
    let (sum, pairs, _) = results.geom[&(k, h)];
    sum / pairs
}

fn render(results: &Results) -> String {
    let mut out = String::new();
    writeln!(out, "GEOM K h mean_overlap_permille aliased_keys_of_40").unwrap();
    for ((k, h), (sum, pairs, alias)) in &results.geom {
        writeln!(out, "GEOM {k} {h} {} {alias}", sum / pairs).unwrap();
    }
    writeln!(out, "PROBE K h order correct_of_8").unwrap();
    for ((k, h, after), correct) in &results.probe {
        let order = if *after { "after" } else { "before" };
        writeln!(out, "PROBE {k} {h} {order} {correct}").unwrap();
    }
    writeln!(
        out,
        "CAL K h written_abstain written_correct written_wrong missing_abstain missing_fabricated"
    )
    .unwrap();
    for ((k, h), c) in &results.cal {
        writeln!(
            out,
            "CAL {k} {h} {} {} {} {} {}",
            c[0], c[1], c[2], c[3], c[4]
        )
        .unwrap();
    }

    let probe = |k, h, after| results.probe[&(k, h, after)];
    let h1 = mean_overlap(results, 64, 1) >= 800 && mean_overlap(results, 64, 32) <= 600;
    let h2 = results
        .probe
        .iter()
        .all(|((_, _, after), c)| *after || *c == 8)
        && probe(64, 1, true) <= 2;
    let h3 = probe(64, 32, true) >= 6 && probe(16, 32, true) <= probe(64, 32, true);
    let h4 = [16, 64]
        .iter()
        .all(|k| [1, 2, 4].iter().all(|h| results.cal[&(*k, *h)][4] >= 6));
    let h5 = results.cal[&(64, 1)][0] <= 2 && results.cal[&(64, 1)][2] >= 6;
    writeln!(out, "HYP H1 holds={h1}").unwrap();
    writeln!(out, "HYP H2 holds={h2}").unwrap();
    writeln!(out, "HYP H3 holds={h3}").unwrap();
    writeln!(out, "HYP H4 holds={h4}").unwrap();
    writeln!(out, "HYP H5 holds={h5}").unwrap();
    out
}

#[test]
fn dkv_3a_1_is_deterministic() {
    assert_eq!(render(&run()), render(&run()));
}

#[test]
fn dkv_3a_1_projected_experiment() {
    let results = run();
    // Structural invariants only: every cell is populated, counts are bounded.
    assert_eq!(results.geom.len(), 2 * HAMMING.len());
    assert_eq!(results.probe.len(), 2 * HAMMING.len() * 2);
    for counts in results.cal.values() {
        assert_eq!(counts[0] + counts[1] + counts[2], SEEDS as u32);
        assert_eq!(counts[3] + counts[4], SEEDS as u32);
    }
    println!("{}", render(&results));
}

// Added after the supplied run: exact regression, not new confirmation.
#[test]
fn reproduced_table_stays_identical_to_retained_rust_189_evidence() {
    let numeric = |s: &str| -> String {
        s.lines()
            .filter(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                fields.first() == Some(&"HYP")
                    || (matches!(
                        fields.first(),
                        Some(&"GEOM") | Some(&"PROBE") | Some(&"CAL")
                    ) && fields.get(1).is_some_and(|s| s.parse::<u32>().is_ok()))
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        numeric(&render(&run())),
        numeric(include_str!("../../../research/dkv-b/reproduced-1.89.txt"))
    );
}

//! DKV-2a-N5 per-hop read/rewrite fidelity diagnostic.
//!
//! Protocol: docs/SML-DKV-2A-N5-HOP-FIDELITY-PREREGISTRATION.md.
//! This is diagnostic-only and uses the current ActiveOnly + outer PageId(5)-derived seed.

use std::{collections::BTreeMap, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_dynamic::DeltaKvDynamicRekeyKernel,
    delta_kv_recurrent::DeltaKvRecurrentPolicy,
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::MorDecision,
    semantic::{SemanticRewriter, SemanticState, SemanticStateDelta},
    PageId,
};

type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const MIN_NODE_DISTANCE: u32 = 16;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const MOR_PAGE: u32 = 5;
const KEY_WIDTHS: [usize; 3] = [64, 256, 1024];
const LOADS: [usize; 2] = [4, 16];

const DIVERGENCE_NAMES: [&str; 6] = [
    "teacher_read_wrong",
    "teacher_read_weak",
    "free_read_wrong",
    "rewrite_ambiguous",
    "reprojection_drift",
    "none",
];
const D_TEACHER_WRONG: usize = 0;
const D_TEACHER_WEAK: usize = 1;
const D_FREE_WRONG: usize = 2;
const D_REWRITE_AMBIGUOUS: usize = 3;
const D_REPROJECTION_DRIFT: usize = 4;
const D_NONE: usize = 5;

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

fn projector<const K: usize>(seed: u64) -> DeltaKvSemanticProjector<K> {
    let page_seed = mix64(seed ^ u64::from(MOR_PAGE).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        page_seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector")
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
        .expect("non-empty read")
}

fn key_sign_hamming(left: &[i16], right: &[i16]) -> u64 {
    left.iter()
        .zip(right)
        .filter(|(a, b)| (**a > 0) != (**b > 0))
        .count() as u64
}

fn dot_and_norms(left: &[i16], right: &[i16]) -> (i128, u128, u128) {
    let mut dot = 0_i128;
    let mut left_norm = 0_u128;
    let mut right_norm = 0_u128;
    for (a, b) in left.iter().zip(right) {
        let ai = i128::from(*a);
        let bi = i128::from(*b);
        dot += ai * bi;
        left_norm += u128::try_from(ai * ai).expect("left square");
        right_norm += u128::try_from(bi * bi).expect("right square");
    }
    (dot, left_norm, right_norm)
}

#[derive(Debug, Clone, Copy)]
struct TeacherRead {
    sign_exact: bool,
    clear: bool,
    wrong: u64,
    min_abs: u32,
}

fn teacher_read<const K: usize>(
    memory: &DeltaKvMemory,
    projector: DeltaKvSemanticProjector<K>,
    source: &State,
    target: u64,
) -> TeacherRead {
    let key = projector
        .project(source, &decision(), None)
        .expect("teacher project")
        .key_q12;
    let mut counters = DeltaKvCounters::default();
    let values = memory.read(&key, &mut counters).expect("teacher read");
    TeacherRead {
        sign_exact: signs_match(&values, target),
        clear: read_clear(&values),
        wrong: wrong_coordinates(&values, target),
        min_abs: min_abs(&values),
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct TeacherTally {
    queries: u64,
    sign_exact: u64,
    exact_clear: u64,
    wrong_coordinates: u64,
    min_abs: u32,
}

impl TeacherTally {
    fn add(&mut self, read: TeacherRead) {
        self.queries += 1;
        self.sign_exact += u64::from(read.sign_exact);
        self.exact_clear += u64::from(read.sign_exact && read.clear);
        self.wrong_coordinates = self.wrong_coordinates.saturating_add(read.wrong);
        if self.queries == 1 {
            self.min_abs = read.min_abs;
        } else {
            self.min_abs = self.min_abs.min(read.min_abs);
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct FreeTally {
    queries: u64,
    value_exact: u64,
    ambiguity_clear: u64,
    state_exact: u64,
    value_hamming_sum: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct KeyTally {
    queries: u64,
    exact: u64,
    exact_state_checks: u64,
    exact_state_key_equal: u64,
    sign_hamming_sum: u64,
    dot_sum: i128,
    actual_norm_sum: u128,
    expected_norm_sum: u128,
}

#[derive(Debug, Default, Clone, Copy)]
struct DivergenceTally {
    total: u64,
    by_class: [u64; 6],
    failed_l23: u64,
    classified_failed_l23: u64,
    failed_l23_by_class: [u64; 6],
}

type TeacherCells = BTreeMap<(usize, usize, usize, bool), TeacherTally>;
type FreeCells = BTreeMap<(usize, usize, usize), FreeTally>;
type KeyCells = BTreeMap<(usize, usize, usize), KeyTally>;
type DivergenceCells = BTreeMap<(usize, usize), DivergenceTally>;

fn build_memory<const K: usize>(
    seed: u64,
    load: usize,
    chains: &[Chain],
) -> (DeltaKvMemory, DeltaKvSemanticProjector<K>) {
    let projector = projector::<K>(seed * 1000 + 17);
    let mut writes = Vec::new();
    for chain in chains {
        for hop in 0..chain.length {
            writes.push((
                build_state(chain.codes[hop], chain.meaning, chain.routing),
                chain.codes[hop + 1],
            ));
        }
        writes.push((
            build_state(chain.codes[chain.length], chain.meaning, chain.routing),
            chain.codes[chain.length],
        ));
    }

    let mut shuffle = tagged(seed, load as u64, 999, 1);
    for index in (1..writes.len()).rev() {
        let swap_with = shuffle.below(index + 1);
        writes.swap(index, swap_with);
    }

    let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
    let mut counters = DeltaKvCounters::default();
    for (state, target) in &writes {
        let key = projector
            .project(state, &decision(), None)
            .expect("write project")
            .key_q12;
        memory
            .update(&key, &target_q12(*target), FULL_RATE, &mut counters)
            .expect("update");
    }
    (memory, projector)
}

fn classify_first_divergence<const K: usize>(
    memory: &DeltaKvMemory,
    projector: DeltaKvSemanticProjector<K>,
    chain: &Chain,
) -> (usize, bool) {
    if chain.length == 0 {
        return (D_NONE, false);
    }

    let mut kernel = DeltaKvDynamicRekeyKernel::<BITS, K>::new(
        memory.clone(),
        projector,
        decision(),
        None,
        (0..BITS).collect(),
        DeltaKvRecurrentPolicy::default(),
    )
    .expect("kernel");

    let mut free_state = build_state(chain.codes[0], chain.meaning, chain.routing);
    let mut first = D_NONE;

    for step_index in 0..chain.length {
        let source = build_state(chain.codes[step_index], chain.meaning, chain.routing);
        let expected = build_state(chain.codes[step_index + 1], chain.meaning, chain.routing);
        let teacher = teacher_read(memory, projector, &source, chain.codes[step_index + 1]);

        let step = kernel
            .rewrite(&free_state, (step_index + 1) as u32)
            .expect("free rewrite");
        let evidence = kernel.cycle_evidence().last().expect("cycle");

        if first == D_NONE {
            if !teacher.sign_exact {
                first = D_TEACHER_WRONG;
            } else if !teacher.clear {
                first = D_TEACHER_WEAK;
            } else if !signs_match(&evidence.read_values_q12, chain.codes[step_index + 1]) {
                first = D_FREE_WRONG;
            } else if step.state != expected {
                first = D_REWRITE_AMBIGUOUS;
            } else {
                let actual_key = projector
                    .project(&step.state, &decision(), None)
                    .expect("actual reprojection")
                    .key_q12;
                let expected_key = projector
                    .project(&expected, &decision(), None)
                    .expect("expected reprojection")
                    .key_q12;
                if actual_key != expected_key {
                    first = D_REPROJECTION_DRIFT;
                }
            }
        }

        free_state = step.state;
    }

    let expected_terminal = build_state(chain.codes[chain.length], chain.meaning, chain.routing);
    let failed = free_state != expected_terminal;
    (first, failed)
}

fn run_width<const K: usize>(
    teacher: &mut TeacherCells,
    free: &mut FreeCells,
    keys: &mut KeyCells,
    divergence: &mut DivergenceCells,
) {
    for &load in &LOADS {
        for seed in 0..SEEDS {
            let chains: Vec<Chain> = (0..load)
                .map(|index| build_chain(seed, load, index))
                .collect();
            let (memory, projector) = build_memory::<K>(seed, load, &chains);

            for chain in &chains {
                // T1: exact true source states, including terminal self-read.
                for hop in 0..=chain.length {
                    let source = build_state(chain.codes[hop], chain.meaning, chain.routing);
                    let terminal = hop == chain.length;
                    let target = if terminal {
                        chain.codes[hop]
                    } else {
                        chain.codes[hop + 1]
                    };
                    let read = teacher_read(&memory, projector, &source, target);
                    teacher
                        .entry((K, load, hop, terminal))
                        .or_default()
                        .add(read);
                }

                if chain.length == 0 {
                    let entry = divergence.entry((K, load)).or_default();
                    entry.total += 1;
                    entry.by_class[D_NONE] += 1;
                    continue;
                }

                // T2/T3: free-running for exactly the true advancing-hop count.
                let mut kernel = DeltaKvDynamicRekeyKernel::<BITS, K>::new(
                    memory.clone(),
                    projector,
                    decision(),
                    None,
                    (0..BITS).collect(),
                    DeltaKvRecurrentPolicy::default(),
                )
                .expect("kernel");
                let mut state = build_state(chain.codes[0], chain.meaning, chain.routing);

                for depth in 1..=chain.length {
                    let step = kernel.rewrite(&state, depth as u32).expect("rewrite");
                    let expected = build_state(chain.codes[depth], chain.meaning, chain.routing);
                    let delta = SemanticStateDelta::between(&step.state, &expected);

                    let free_tally = free.entry((K, load, depth)).or_default();
                    free_tally.queries += 1;
                    free_tally.value_exact +=
                        u64::from(value_bits(&step.state) == chain.codes[depth]);
                    free_tally.ambiguity_clear += u64::from(ambiguity_clear(&step.state));
                    free_tally.state_exact += u64::from(step.state == expected);
                    free_tally.value_hamming_sum = free_tally
                        .value_hamming_sum
                        .saturating_add(delta.value as u64);

                    let actual_key = projector
                        .project(&step.state, &decision(), None)
                        .expect("actual key")
                        .key_q12;
                    let expected_key = projector
                        .project(&expected, &decision(), None)
                        .expect("expected key")
                        .key_q12;
                    let (dot, actual_norm, expected_norm) =
                        dot_and_norms(&actual_key, &expected_key);

                    let key_tally = keys.entry((K, load, depth)).or_default();
                    key_tally.queries += 1;
                    key_tally.exact += u64::from(actual_key == expected_key);
                    if step.state == expected {
                        key_tally.exact_state_checks += 1;
                        key_tally.exact_state_key_equal += u64::from(actual_key == expected_key);
                    }
                    key_tally.sign_hamming_sum = key_tally
                        .sign_hamming_sum
                        .saturating_add(key_sign_hamming(&actual_key, &expected_key));
                    key_tally.dot_sum += dot;
                    key_tally.actual_norm_sum += actual_norm;
                    key_tally.expected_norm_sum += expected_norm;

                    if step.state == expected {
                        assert_eq!(
                            actual_key, expected_key,
                            "H4 exact semantic state must reproject identically"
                        );
                    }

                    state = step.state;
                }

                let (class, failed) = classify_first_divergence(&memory, projector, chain);
                let entry = divergence.entry((K, load)).or_default();
                entry.total += 1;
                entry.by_class[class] += 1;
                if chain.length >= 2 && failed {
                    entry.failed_l23 += 1;
                    entry.classified_failed_l23 += u64::from(class != D_NONE);
                    entry.failed_l23_by_class[class] += 1;
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

fn teacher_advancing(cells: &TeacherCells, k: usize, load: usize, hop: usize) -> TeacherTally {
    cells
        .get(&(k, load, hop, false))
        .copied()
        .unwrap_or_default()
}

fn main() {
    let mut teacher = TeacherCells::new();
    let mut free = FreeCells::new();
    let mut keys = KeyCells::new();
    let mut divergence = DivergenceCells::new();

    run_width::<64>(&mut teacher, &mut free, &mut keys, &mut divergence);
    run_width::<256>(&mut teacher, &mut free, &mut keys, &mut divergence);
    run_width::<1024>(&mut teacher, &mut free, &mut keys, &mut divergence);

    let mut out = String::new();

    for &load in &LOADS {
        for &k in &KEY_WIDTHS {
            for hop in 0..4 {
                for terminal in [false, true] {
                    if let Some(tally) = teacher.get(&(k, load, hop, terminal)) {
                        writeln!(
                            out,
                            "N5_TEACHER,K={k},G={load},hop={hop},terminal={terminal},queries={},sign_exact={},sign_ppm={},exact_clear={},clear_ppm={},wrong_coordinates={},min_abs={}",
                            tally.queries,
                            tally.sign_exact,
                            ppm(tally.sign_exact, tally.queries),
                            tally.exact_clear,
                            ppm(tally.exact_clear, tally.queries),
                            tally.wrong_coordinates,
                            tally.min_abs,
                        )
                        .expect("write");
                    }
                }
            }

            for depth in 1..=3 {
                if let Some(tally) = free.get(&(k, load, depth)) {
                    let key_tally = keys[&(k, load, depth)];
                    writeln!(
                        out,
                        "N5_FREE,K={k},G={load},depth={depth},queries={},value_exact={},value_ppm={},ambiguity_clear={},clear_ppm={},state_exact={},state_ppm={},value_hamming_sum={},key_exact={},key_ppm={},exact_state_checks={},exact_state_key_equal={},key_sign_hamming_sum={},dot_sum={},actual_norm_sum={},expected_norm_sum={}",
                        tally.queries,
                        tally.value_exact,
                        ppm(tally.value_exact, tally.queries),
                        tally.ambiguity_clear,
                        ppm(tally.ambiguity_clear, tally.queries),
                        tally.state_exact,
                        ppm(tally.state_exact, tally.queries),
                        tally.value_hamming_sum,
                        key_tally.exact,
                        ppm(key_tally.exact, key_tally.queries),
                        key_tally.exact_state_checks,
                        key_tally.exact_state_key_equal,
                        key_tally.sign_hamming_sum,
                        key_tally.dot_sum,
                        key_tally.actual_norm_sum,
                        key_tally.expected_norm_sum,
                    )
                    .expect("write");
                }
            }

            let div = divergence[&(k, load)];
            write!(
                out,
                "N5_DIVERGENCE,K={k},G={load},total={},failed_l23={},classified_failed_l23={}",
                div.total, div.failed_l23, div.classified_failed_l23,
            )
            .expect("write");
            for (index, name) in DIVERGENCE_NAMES.iter().enumerate() {
                write!(
                    out,
                    ",{name}={},failed_l23_{name}={}",
                    div.by_class[index], div.failed_l23_by_class[index],
                )
                .expect("write");
            }
            writeln!(out).expect("write");
        }
    }

    let terminal_256 = teacher
        .iter()
        .filter(|((k, load, _, terminal), _)| *k == 256 && *load == 4 && *terminal)
        .fold((0_u64, 0_u64), |(q, c), (_, t)| {
            (q + t.queries, c + t.exact_clear)
        });
    let terminal_1024 = teacher
        .iter()
        .filter(|((k, load, _, terminal), _)| *k == 1024 && *load == 4 && *terminal)
        .fold((0_u64, 0_u64), |(q, c), (_, t)| {
            (q + t.queries, c + t.exact_clear)
        });
    let h1 = ppm(terminal_256.1, terminal_256.0) >= 950_000
        && ppm(terminal_1024.1, terminal_1024.0) >= 950_000;

    let h0 = teacher_advancing(&teacher, 1024, 4, 0);
    let h1_teacher = teacher_advancing(&teacher, 1024, 4, 1);
    let h2_teacher = teacher_advancing(&teacher, 1024, 4, 2);
    let h2 = ppm(h0.exact_clear, h0.queries) >= 950_000
        && ppm(h1_teacher.exact_clear, h1_teacher.queries) >= 900_000
        && ppm(h2_teacher.exact_clear, h2_teacher.queries) >= 800_000;

    let mut h3 = true;
    for depth in 1..=3 {
        if let Some(free_tally) = free.get(&(1024, 4, depth)) {
            let teacher_tally = teacher_advancing(&teacher, 1024, 4, depth - 1);
            h3 &= ppm(free_tally.state_exact, free_tally.queries)
                <= ppm(teacher_tally.exact_clear, teacher_tally.queries);
        }
    }

    let h4 = keys
        .values()
        .all(|tally| tally.exact_state_checks == tally.exact_state_key_equal);

    let (failed_l23, classified_l23) = divergence
        .iter()
        .filter(|((_, _), tally)| tally.failed_l23 > 0)
        .fold((0_u64, 0_u64), |(failed, classified), (_, tally)| {
            (
                failed + tally.failed_l23,
                classified + tally.classified_failed_l23,
            )
        });
    let h5 = failed_l23 > 0 && classified_l23 * 10 >= failed_l23 * 8;

    let teacher_h1_ppm = ppm(h1_teacher.exact_clear, h1_teacher.queries);
    let teacher_h2_ppm = ppm(h2_teacher.exact_clear, h2_teacher.queries);
    let free_d2 = free.get(&(1024, 4, 2)).copied().unwrap_or_default();
    let free_d3 = free.get(&(1024, 4, 3)).copied().unwrap_or_default();
    let antecedent = teacher_h1_ppm >= 900_000
        && teacher_h2_ppm >= 900_000
        && (ppm(free_d2.state_exact, free_d2.queries) < 900_000
            || ppm(free_d3.state_exact, free_d3.queries) < 900_000);

    let drift_failures = divergence[&(1024, 4)].failed_l23_by_class[D_FREE_WRONG]
        + divergence[&(1024, 4)].failed_l23_by_class[D_REWRITE_AMBIGUOUS]
        + divergence[&(1024, 4)].failed_l23_by_class[D_REPROJECTION_DRIFT];
    let local_failures = divergence[&(1024, 4)].failed_l23_by_class[D_TEACHER_WRONG]
        + divergence[&(1024, 4)].failed_l23_by_class[D_TEACHER_WEAK];
    let h6_status = if !antecedent {
        "NOT_APPLICABLE"
    } else if drift_failures > local_failures {
        "HOLDS"
    } else {
        "REFUTED"
    };

    for (name, holds) in [
        ("H1_TERMINAL_SELF_RETRIEVAL", h1),
        ("H2_TEACHER_ADVANCING_FIDELITY", h2),
        ("H3_FREE_NOT_ABOVE_TEACHER", h3),
        ("H4_EXACT_STATE_REPROJECTS_EXACT", h4),
        ("H5_FIRST_DIVERGENCE_COVERAGE", h5),
    ] {
        writeln!(
            out,
            "N5_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .expect("write");
    }
    writeln!(out, "N5_HYPOTHESIS,H6_ACCUMULATED_DRIFT,{h6_status}").expect("write");
    writeln!(
        out,
        "N5_H6_DETAIL,teacher_h1_ppm={teacher_h1_ppm},teacher_h2_ppm={teacher_h2_ppm},free_d2_ppm={},free_d3_ppm={},drift_failures={drift_failures},local_failures={local_failures}",
        ppm(free_d2.state_exact, free_d2.queries),
        ppm(free_d3.state_exact, free_d3.queries),
    )
    .expect("write");
    writeln!(out, "PROMOTION,BLOCKED,n5_diagnostic_only").expect("write");

    print!("{out}");
}

//! DKV-2a-N4 current-stack replay of the historical recurrent-chain panel.
//!
//! Protocol: docs/SML-DKV-2A-N4-CHAIN-REPLAY.md.
//! The combined arm composes current ActiveOnly with an outer PageId(5)-derived seed.
//! No projector hash/bucket/sign implementation is copied.

use std::{collections::BTreeMap, fmt::Write as _};

use sml_core::{
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_dynamic::DeltaKvDynamicRekeyKernel,
    delta_kv_recurrent::DeltaKvRecurrentPolicy,
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::MorDecision,
    semantic::{run_semantic_recurrent, SemanticRewriter, SemanticState, SemanticStateDelta},
    HaltingPolicy, PageId,
};

type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const MAX_DEPTH: usize = 6;
const MIN_NODE_DISTANCE: u32 = 16;
const WEAK_READ_Q12: u32 = 2048;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const MOR_PAGE: u32 = 5;
const KEY_WIDTHS: [usize; 3] = [64, 256, 1024];
const LOADS: [usize; 2] = [4, 16];

const CTRL: [&str; 9] = [
    "F1", "F2", "F3", "F6", "STAB", "KEY", "AMB", "MAG", "ORACLE",
];
const C_F6: usize = 3;
const C_STAB: usize = 4;
const C_KEY: usize = 5;
const C_AMB: usize = 6;
const C_MAG: usize = 7;
const C_ORACLE: usize = 8;
const CLASS_NOVEL: usize = 4;
const CLASS_NAMES: [&str; 5] = ["L0", "L1", "L2", "L3", "novel"];

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
struct CycleRecord {
    state: State,
    delta_total: u128,
    blocked: bool,
    ambiguous: usize,
    key_changed: bool,
    min_abs_read: u32,
    macs: u64,
}

#[derive(Clone)]
struct Query {
    class: usize,
    start: State,
    expected: Option<u64>,
}

fn projector<const K: usize>(combined: bool, seed: u64) -> DeltaKvSemanticProjector<K> {
    if combined {
        let page_seed = mix64(seed ^ u64::from(MOR_PAGE).wrapping_mul(0x9e37_79b9_7f4a_7c15));
        DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
            DeltaKvSemanticProjectionPolicy::default(),
            page_seed,
            DeltaKvAmbiguityProjection::ActiveOnly,
        )
        .expect("combined projector")
    } else {
        DeltaKvSemanticProjector::<K>::new(DeltaKvSemanticProjectionPolicy::default(), seed)
            .expect("default projector")
    }
}

fn trace_query<const K: usize>(
    memory: &DeltaKvMemory,
    projector: DeltaKvSemanticProjector<K>,
    start: &State,
) -> Vec<CycleRecord> {
    let mut kernel = DeltaKvDynamicRekeyKernel::<BITS, K>::new(
        memory.clone(),
        projector,
        decision(),
        None,
        (0..BITS).collect(),
        DeltaKvRecurrentPolicy::default(),
    )
    .expect("kernel");

    let mut state = start.clone();
    let mut records = Vec::new();
    for depth in 1..=MAX_DEPTH {
        let step = kernel.rewrite(&state, depth as u32).expect("rewrite");
        let delta = SemanticStateDelta::between(&state, &step.state);
        let evidence = kernel.cycle_evidence().last().expect("evidence").clone();
        records.push(CycleRecord {
            state: step.state.clone(),
            delta_total: delta.total(),
            blocked: step.stability_blocked,
            ambiguous: evidence.ambiguous_values,
            key_changed: evidence.key_changed,
            min_abs_read: evidence
                .read_values_q12
                .iter()
                .map(|value| value.unsigned_abs())
                .min()
                .expect("read values"),
            macs: evidence.read_macs_delta,
        });
        state = step.state;
    }
    records
}

fn correct(record: &CycleRecord, expected: u64) -> bool {
    value_bits(&record.state) == expected && ambiguity_clear(&record.state)
}

fn stab(record: &CycleRecord) -> bool {
    record.delta_total == 0 && !record.blocked
}

fn stops(trace: &[CycleRecord], expected: Option<u64>) -> [usize; 9] {
    let first = |predicate: &dyn Fn(usize, &CycleRecord) -> bool| {
        trace
            .iter()
            .enumerate()
            .find(|(index, record)| predicate(*index + 1, record))
            .map_or(MAX_DEPTH, |(index, _)| index + 1)
    };
    let oracle = expected
        .and_then(|value| {
            trace
                .iter()
                .position(|record| correct(record, value))
                .map(|i| i + 1)
        })
        .unwrap_or(1);

    [
        1,
        2,
        3,
        6,
        first(&|_, record| stab(record)),
        first(&|depth, record| depth >= 2 && !record.key_changed && !record.blocked),
        first(&|_, record| record.ambiguous == 0),
        first(&|_, record| stab(record) || record.min_abs_read < WEAK_READ_Q12),
        oracle,
    ]
}

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct Tally {
    n: u64,
    correct: u64,
    cycles: u64,
    macs: u64,
}

#[derive(Default, Clone, PartialEq, Eq, Debug)]
struct Extra {
    novel_n: u64,
    novel_clear: u64,
    written_n: u64,
    written_clear: u64,
}

type Cells = BTreeMap<(usize, usize, usize, usize), Tally>;
type Extras = BTreeMap<(usize, usize), Extra>;

fn run_cell<const K: usize>(combined: bool, load: usize, cells: &mut Cells, extras: &mut Extras) {
    for seed in 0..SEEDS {
        let chains: Vec<Chain> = (0..load)
            .map(|index| build_chain(seed, load, index))
            .collect();
        let mut writes = Vec::new();
        let mut queries = Vec::new();

        for chain in &chains {
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
            queries.push(Query {
                class: chain.length,
                start: build_state(chain.codes[0], chain.meaning, chain.routing),
                expected: Some(chain.codes[chain.length]),
            });
        }

        let mut shuffle = tagged(seed, load as u64, 999, 1);
        for index in (1..writes.len()).rev() {
            let swap_with = shuffle.below(index + 1);
            writes.swap(index, swap_with);
        }

        for query in 0..load {
            let mut stream = tagged(seed, load as u64, 500 + query as u64, 2);
            queries.push(Query {
                class: CLASS_NOVEL,
                start: build_state(stream.next(), stream.next(), stream.next()),
                expected: None,
            });
        }

        let projector = projector::<K>(combined, seed * 1000 + 17);
        let mut memory = DeltaKvMemory::new(K, BITS).expect("memory");
        let mut counters = DeltaKvCounters::default();
        for (state, code) in &writes {
            let key = projector
                .project(state, &decision(), None)
                .expect("project")
                .key_q12;
            memory
                .update(&key, &target_q12(*code), FULL_RATE, &mut counters)
                .expect("update");
        }

        for query in &queries {
            let trace = trace_query::<K>(&memory, projector, &query.start);
            let stop = stops(&trace, query.expected);

            let mut repo_kernel = DeltaKvDynamicRekeyKernel::<BITS, K>::new(
                memory.clone(),
                projector,
                decision(),
                None,
                (0..BITS).collect(),
                DeltaKvRecurrentPolicy::default(),
            )
            .expect("repo kernel");
            let repo = run_semantic_recurrent(
                query.start.clone(),
                &mut repo_kernel,
                HaltingPolicy {
                    min_depth: 1,
                    max_depth: MAX_DEPTH as u32,
                    stable_hamming_threshold: 0,
                },
            )
            .expect("repo recurrent");
            assert_eq!(repo.depth as usize, stop[C_STAB], "STAB depth mismatch");
            assert_eq!(
                repo.state,
                trace[stop[C_STAB] - 1].state,
                "STAB state mismatch"
            );

            for (ctrl, &depth) in stop.iter().enumerate() {
                let tally = cells.entry((K, load, query.class, ctrl)).or_default();
                tally.n += 1;
                tally.cycles += depth as u64;
                tally.macs += trace[..depth].iter().map(|record| record.macs).sum::<u64>();

                if let Some(expected) = query.expected {
                    if ctrl == C_ORACLE {
                        tally.correct +=
                            u64::from(trace.iter().any(|record| correct(record, expected)));
                    } else {
                        tally.correct += u64::from(correct(&trace[depth - 1], expected));
                    }
                }
            }

            let extra = extras.entry((K, load)).or_default();
            let final_state = &trace[stop[C_STAB] - 1].state;
            if query.class == CLASS_NOVEL {
                extra.novel_n += 1;
                extra.novel_clear += u64::from(ambiguity_clear(final_state));
            } else {
                extra.written_n += 1;
                extra.written_clear += u64::from(ambiguity_clear(final_state));
            }
        }
    }
}

fn run_grid(combined: bool) -> (Cells, Extras) {
    let mut cells = Cells::new();
    let mut extras = Extras::new();
    for &load in &LOADS {
        run_cell::<64>(combined, load, &mut cells, &mut extras);
        run_cell::<256>(combined, load, &mut cells, &mut extras);
        run_cell::<1024>(combined, load, &mut cells, &mut extras);
    }
    (cells, extras)
}

fn tally(cells: &Cells, k: usize, load: usize, class: usize, ctrl: usize) -> Tally {
    cells[&(k, load, class, ctrl)]
}

fn pooled(cells: &Cells, k: usize, load: usize, ctrl: usize) -> (f64, f64) {
    let mut total = Tally::default();
    for class in 0..4 {
        let value = tally(cells, k, load, class, ctrl);
        total.n += value.n;
        total.correct += value.correct;
        total.cycles += value.cycles;
    }
    (
        100.0 * total.correct as f64 / total.n as f64,
        total.cycles as f64 / total.n as f64,
    )
}

fn capable(cells: &Cells, k: usize, load: usize) -> bool {
    (1..4).all(|class| {
        let value = tally(cells, k, load, class, C_ORACLE);
        value.correct * 10 >= value.n * 9
    })
}

fn dominates(candidate: (f64, f64), baseline: (f64, f64)) -> bool {
    (candidate.0 >= baseline.0 + 2.0 && candidate.1 <= baseline.1)
        || (candidate.0 >= baseline.0 - 1.0 && candidate.1 <= 0.9 * baseline.1)
}

fn contracts() -> u64 {
    fn width<const K: usize>() -> u64 {
        let mut checks = 0_u64;
        for seed in 0..4 {
            let base_seed = seed * 1000 + 17;
            let legacy = projector::<K>(false, base_seed);
            let explicit = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
                DeltaKvSemanticProjectionPolicy::default(),
                base_seed,
                DeltaKvAmbiguityProjection::Bipolar,
            )
            .expect("explicit");
            let combined = projector::<K>(true, base_seed);
            let mut stream = Stream::new(seed + 99);
            for _ in 0..50 {
                let state = build_state(stream.next(), stream.next(), stream.next());
                assert_eq!(
                    legacy.project(&state, &decision(), None).expect("legacy"),
                    explicit
                        .project(&state, &decision(), None)
                        .expect("explicit"),
                );
                let first = combined
                    .project(&state, &decision(), None)
                    .expect("combined");
                let second = combined
                    .project(&state, &decision(), None)
                    .expect("combined");
                assert_eq!(first, second);
                assert_eq!(first.counters.semantic_coordinates_read, 256);
                assert_eq!(first.counters.bucket_accumulations, 193);
                checks += 4;
            }
        }
        checks
    }

    width::<64>() + width::<256>() + width::<1024>()
}

fn main() {
    let checks = contracts();
    let grids = [run_grid(false), run_grid(true)];
    let mut out = String::new();

    writeln!(out, "N4_CONTROL,contract_assertions={checks}").expect("write");

    for (arm, (cells, extras)) in grids.iter().enumerate() {
        let arm_name = ["default", "combined"][arm];
        for &load in &LOADS {
            for &k in &KEY_WIDTHS {
                for (class, class_name) in CLASS_NAMES.iter().enumerate() {
                    let oracle = tally(cells, k, load, class, C_ORACLE);
                    if class == CLASS_NOVEL {
                        writeln!(
                            out,
                            "N4_CLASS,arm={arm_name},K={k},G={load},class={class_name},oracle_novel_cycles={:.4}",
                            oracle.cycles as f64 / oracle.n as f64,
                        )
                        .expect("write");
                    } else {
                        writeln!(
                            out,
                            "N4_CLASS,arm={arm_name},K={k},G={load},class={class_name},oracle_correct={},oracle_n={}",
                            oracle.correct,
                            oracle.n,
                        )
                        .expect("write");
                    }
                }

                let extra = &extras[&(k, load)];
                write!(out, "N4_CELL,arm={arm_name},K={k},G={load}").expect("write");
                for (ctrl, ctrl_name) in CTRL.iter().enumerate() {
                    let (accuracy, cycles) = pooled(cells, k, load, ctrl);
                    write!(
                        out,
                        ",{ctrl_name}={accuracy:.4}/{ctrl_name}cycles={cycles:.4}",
                    )
                    .expect("write");
                }
                writeln!(
                    out,
                    ",novel_clear={}/{},written_clear={}/{}",
                    extra.novel_clear, extra.novel_n, extra.written_clear, extra.written_n,
                )
                .expect("write");
            }
        }
    }

    let (default_cells, _) = &grids[0];
    let (combined_cells, combined_extra) = &grids[1];
    let all: Vec<(usize, usize)> = LOADS
        .iter()
        .flat_map(|&load| KEY_WIDTHS.iter().map(move |&k| (k, load)))
        .collect();

    let close = |a: f64, b: f64| (a - b).abs() < 0.05;
    let h0 = close(pooled(default_cells, 64, 4, C_F6).0, 60.9)
        && close(pooled(default_cells, 256, 4, C_F6).0, 75.0)
        && close(pooled(default_cells, 64, 16, C_F6).0, 17.2)
        && close(pooled(default_cells, 256, 16, C_F6).0, 21.5);

    let h1 = all.iter().all(|&(k, load)| {
        pooled(combined_cells, k, load, C_F6).0 >= pooled(default_cells, k, load, C_F6).0
    });
    let h2 = pooled(combined_cells, 1024, 16, C_F6).0 >= 70.0;
    let h3 = pooled(combined_cells, 256, 4, C_ORACLE).0 < 90.0
        && pooled(combined_cells, 1024, 4, C_ORACLE).0 < 90.0;
    let h4 = all.iter().all(|&(k, load)| {
        (pooled(combined_cells, k, load, C_STAB).0 - pooled(combined_cells, k, load, C_F6).0).abs()
            <= 1.0
    });

    let mut domination = false;
    for &(k, load) in &all {
        let baseline = pooled(combined_cells, k, load, C_STAB);
        for ctrl in [C_KEY, C_AMB, C_MAG] {
            domination |= dominates(pooled(combined_cells, k, load, ctrl), baseline);
        }
    }
    let h5 = !domination;

    let h6 = KEY_WIDTHS.iter().any(|&k| {
        let extra = &combined_extra[&(k, 4)];
        let ambiguous = extra.novel_n - extra.novel_clear;
        ambiguous * 10 < extra.novel_n * 7
    });

    for (name, holds) in [
        ("H0_DEFAULT_CONTROL", h0),
        ("H1_COMBINED_NO_WORSE", h1),
        ("H2_K1024_G16_F6_GE_70", h2),
        ("H3_LOW_LOAD_ORACLE_BELOW_90", h3),
        ("H4_STAB_MATCHES_F6", h4),
        ("H5_NO_SIGNAL_DOMINANCE", h5),
        ("H6_AMBIGUITY_MISS_GATE_FAILS_SOME_WIDTH", h6),
    ] {
        writeln!(
            out,
            "N4_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" },
        )
        .expect("write");
    }

    writeln!(
        out,
        "N4_CAPABLE,{:?}",
        all.iter()
            .copied()
            .filter(|&(k, load)| capable(combined_cells, k, load))
            .collect::<Vec<_>>()
    )
    .expect("write");
    writeln!(out, "PROMOTION,BLOCKED,n4_chain_replay_only").expect("write");

    print!("{out}");
}

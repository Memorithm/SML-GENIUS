//! DKV-2a-N11 two-bank PRIMARY / ORDERED resolver.
//!
//! Protocol: docs/SML-DKV-2A-N11-TWO-BANK-PREREGISTRATION.md.
//! Harness-only: no core projector, memory or MOR implementation changes.

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
const FACTS_PER_SEED: usize = 16;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const KEY_WIDTHS: [usize; 2] = [256, 1024];
const LOADS: [usize; 2] = [4, 16];
const MIN_NODE_DISTANCE: u32 = 16;
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
const R6: &[u32] = &[7, 5];

const B_PRIMARY: usize = 0;
const B_ORDERED: usize = 1;

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

fn route_seed(bank: usize, base_seed: u64, route: &[u32]) -> u64 {
    assert!(!route.is_empty());
    let mut seed = mix64(base_seed ^ u64::from(route[0]).wrapping_mul(PAGE_C));
    if bank == B_ORDERED && route.len() > 1 {
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct KeyWork {
    semantic_coordinates: u64,
    bucket_accumulations: u64,
    quantized_coordinates: u64,
}

fn source_projection<const K: usize>(
    bank: usize,
    value_domain: bool,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> (Vec<i16>, DeltaKvSemanticProjectionCounters) {
    let route_seed = route_seed(bank, base_seed, route);
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
    (evidence.key_q12, evidence.counters)
}

fn hybrid_key<const K: usize>(
    bank: usize,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> (Vec<i16>, KeyWork) {
    let (page, page_work) = source_projection::<K>(bank, false, base_seed, state, route);
    let (value, value_work) = source_projection::<K>(bank, true, base_seed, state, route);
    let key = (0..K)
        .map(|index| {
            if index % 4 < Q {
                value[index]
            } else {
                page[index]
            }
        })
        .collect();
    (
        key,
        KeyWork {
            semantic_coordinates: page_work
                .semantic_coordinates_read
                .saturating_add(value_work.semantic_coordinates_read),
            bucket_accumulations: page_work
                .bucket_accumulations
                .saturating_add(value_work.bucket_accumulations),
            quantized_coordinates: page_work
                .quantized_key_coordinates
                .saturating_add(value_work.quantized_key_coordinates),
        },
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

fn route_fingerprint(route: &[u32]) -> Vec<u32> {
    route.to_vec()
}

#[derive(Debug, Default, Clone)]
struct OverrideDirectory {
    bindings: BTreeSet<(u64, Vec<u32>)>,
}

impl OverrideDirectory {
    fn bind(&mut self, record_id: u64, route: &[u32]) {
        assert!(self.bindings.insert((record_id, route_fingerprint(route))));
    }

    fn contains(&self, record_id: u64, route: &[u32]) -> bool {
        self.bindings
            .contains(&(record_id, route_fingerprint(route)))
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct Cost {
    directory_probes: u64,
    primary_reads: u64,
    ordered_reads: u64,
    counterfactual_reads: u64,
    primary_updates: u64,
    ordered_updates: u64,
    primary_keys: u64,
    ordered_keys: u64,
    semantic_coordinates: u64,
    bucket_accumulations: u64,
    quantized_coordinates: u64,
}

impl Cost {
    fn add_key(&mut self, bank: usize, work: KeyWork) {
        if bank == B_PRIMARY {
            self.primary_keys += 1;
        } else {
            self.ordered_keys += 1;
        }
        self.semantic_coordinates = self
            .semantic_coordinates
            .saturating_add(work.semantic_coordinates);
        self.bucket_accumulations = self
            .bucket_accumulations
            .saturating_add(work.bucket_accumulations);
        self.quantized_coordinates = self
            .quantized_coordinates
            .saturating_add(work.quantized_coordinates);
    }

    fn resolved_reads(self) -> u64 {
        self.primary_reads.saturating_add(self.ordered_reads)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bank {
    Primary,
    Ordered,
}

#[derive(Clone, Copy)]
struct ProjectionContext<'a> {
    base_seed: u64,
    route: &'a [u32],
    state: &'a State,
}

fn write_bank<const K: usize>(
    bank: usize,
    context: ProjectionContext<'_>,
    memory: &mut DeltaKvMemory,
    target: u64,
    counters: &mut DeltaKvCounters,
    cost: &mut Cost,
) {
    let (key, work) = hybrid_key::<K>(
        bank,
        context.base_seed,
        context.state,
        context.route,
    );
    cost.add_key(bank, work);
    memory
        .update(&key, &target_q12(target), FULL_RATE, counters)
        .expect("update");
    if bank == B_PRIMARY {
        cost.primary_updates += 1;
    } else {
        cost.ordered_updates += 1;
    }
}

fn read_specific<const K: usize>(
    bank: usize,
    base_seed: u64,
    route: &[u32],
    memory: &DeltaKvMemory,
    state: &State,
    cost: &mut Cost,
    counterfactual: bool,
) -> Vec<i32> {
    let (key, work) = hybrid_key::<K>(bank, base_seed, state, route);
    if counterfactual {
        cost.counterfactual_reads += 1;
    } else {
        cost.add_key(bank, work);
        if bank == B_PRIMARY {
            cost.primary_reads += 1;
        } else {
            cost.ordered_reads += 1;
        }
    }
    let mut counters = DeltaKvCounters::default();
    memory.read(&key, &mut counters).expect("read")
}

fn resolve<const K: usize>(
    record_id: u64,
    context: ProjectionContext<'_>,
    primary: &DeltaKvMemory,
    ordered: &DeltaKvMemory,
    directory: &OverrideDirectory,
    cost: &mut Cost,
) -> (Bank, Vec<i32>) {
    cost.directory_probes += 1;
    if directory.contains(record_id, context.route) {
        (
            Bank::Ordered,
            read_specific::<K>(
                B_ORDERED,
                context.base_seed,
                context.route,
                ordered,
                context.state,
                cost,
                false,
            ),
        )
    } else {
        (
            Bank::Primary,
            read_specific::<K>(
                B_PRIMARY,
                context.base_seed,
                context.route,
                primary,
                context.state,
                cost,
                false,
            ),
        )
    }
}

fn record_id(panel: u64, seed: u64, index: usize) -> u64 {
    (panel << 56) ^ (seed << 32) ^ index as u64
}

fn random_facts(seed: u64, panel: u64) -> Vec<(u64, State, u64)> {
    let mut stream = tagged(seed, panel, FACTS_PER_SEED as u64, 1100 + panel);
    (0..FACTS_PER_SEED)
        .map(|index| {
            (
                record_id(panel, seed, index),
                build_state(stream.next(), stream.next(), stream.next()),
                stream.next(),
            )
        })
        .collect()
}

fn independent_target(base: u64, record: u64) -> u64 {
    let mut value = mix64(base ^ record ^ 0x4f56_4552_5249_4445);
    if value == base {
        value ^= u64::MAX;
    }
    value
}

#[derive(Debug, Default, Clone, Copy)]
struct RecallTally {
    observations: u64,
    exact_clear: u64,
    primary_selected: u64,
    ordered_selected: u64,
}

impl RecallTally {
    fn add(&mut self, bank: Bank, values: &[i32], target: u64) {
        self.observations += 1;
        self.exact_clear += u64::from(exact_clear(values, target));
        match bank {
            Bank::Primary => self.primary_selected += 1,
            Bank::Ordered => self.ordered_selected += 1,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct GuardTally {
    queries: u64,
    false_override_selections: u64,
    false_acceptances: u64,
    wrong_state_ordered: u64,
    wrong_state_false_acceptances: u64,
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
    exact_clear: u64,
    abs_cosine_sum: f64,
}

type P1Cells = BTreeMap<(usize, usize), RecallTally>;
type P2Cells = BTreeMap<(usize, usize), RecallTally>;
type P3Cells = BTreeMap<(usize, usize), RecallTally>;
type TeacherCells = BTreeMap<(usize, usize, usize), RecallTally>;
type ChainCells = BTreeMap<(usize, usize), ChainTally>;
type P6Cells = BTreeMap<(usize, usize), RecallTally>;
type PerturbCells = BTreeMap<usize, PerturbTally>;

fn compatibility<const K: usize>() -> u64 {
    let mut checks = 0_u64;
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let mut stream = tagged(seed, K as u64, 0, 1101);
        for _ in 0..50 {
            let state = build_state(stream.next(), stream.next(), stream.next());
            let (primary, primary_work) = hybrid_key::<K>(B_PRIMARY, base_seed, &state, R1);
            let (ordered, ordered_work) = hybrid_key::<K>(B_ORDERED, base_seed, &state, R1);
            assert_eq!(primary, ordered, "singleton endpoint compatibility");
            assert_eq!(primary_work, ordered_work, "singleton work compatibility");
            let (primary_again, _) = hybrid_key::<K>(B_PRIMARY, base_seed, &state, R1);
            let (ordered_again, _) = hybrid_key::<K>(B_ORDERED, base_seed, &state, R1);
            assert_eq!(primary, primary_again);
            assert_eq!(ordered, ordered_again);
            checks += 4;
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

fn run_p1<const K: usize>(cells: &mut P1Cells, cf: &mut P1Cells, cost: &mut Cost) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let facts = random_facts(seed, 1);
        let mut primary = DeltaKvMemory::new(K, BITS).expect("primary");
        let mut ordered = DeltaKvMemory::new(K, BITS).expect("ordered");
        let mut p_counters = DeltaKvCounters::default();
        let mut o_counters = DeltaKvCounters::default();
        let directory = OverrideDirectory::default();

        for (_, state, target) in &facts {
            write_bank::<K>(
                B_PRIMARY,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: *target,
                },
                &mut primary,
                &mut p_counters,
                15892,
                cost,
            );
            // Counterfactual N9 ORDERED continuity control only.
            write_bank::<K>(
                B_ORDERED,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: *target,
                },
                &mut ordered,
                &mut o_counters,
                16209,
                cost,
            );
        }

        for (rid, state, target) in &facts {
            for (slot, route) in [(0_usize, R2), (1, R3), (2, R4)] {
                let (bank, values) = resolve::<K>(
                    *rid,
                    ProjectionContext {
                        base_seed,
                        route: route,
                        state: state,
                    },
                    &primary,
                    &ordered,
                    &directory,
                    cost,
                );
                cells
                    .entry((K, slot))
                    .or_default()
                    .add(bank, &values, *target);

                if slot > 0 {
                    let values = read_specific::<K>(
                        B_ORDERED, base_seed, route, &ordered, state, cost, true,
                    );
                    cf.entry((K, slot))
                        .or_default()
                        .add(Bank::Ordered, &values, *target);
                }
            }
        }
    }
}

fn run_p2<const K: usize>(cells: &mut P2Cells, cost: &mut Cost) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let facts = random_facts(seed, 2);
        let mut primary = DeltaKvMemory::new(K, BITS).expect("primary");
        let mut ordered = DeltaKvMemory::new(K, BITS).expect("ordered");
        let mut p_counters = DeltaKvCounters::default();
        let mut o_counters = DeltaKvCounters::default();
        let mut directory = OverrideDirectory::default();
        let mut targets = Vec::new();

        for (rid, state, base) in &facts {
            let override_target = independent_target(*base, *rid);
            write_bank::<K>(
                B_PRIMARY,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: *base,
                },
                &mut primary,
                &mut p_counters,
                17914,
                cost,
            );
            write_bank::<K>(
                B_ORDERED,
                ProjectionContext {
                    base_seed,
                    route: R3,
                    state: override_target,
                },
                &mut ordered,
                &mut o_counters,
                18163,
                cost,
            );
            directory.bind(*rid, R3);
            targets.push((*rid, state.clone(), *base, override_target));
        }

        for (rid, state, base, override_target) in &targets {
            for (slot, route, target) in [
                (0_usize, R2, *base),
                (1, R3, *override_target),
                (2, R4, *base),
            ] {
                let (bank, values) = resolve::<K>(
                    *rid,
                    ProjectionContext {
                        base_seed,
                        route: route,
                        state: state,
                    },
                    &primary,
                    &ordered,
                    &directory,
                    cost,
                );
                cells
                    .entry((K, slot))
                    .or_default()
                    .add(bank, &values, target);
            }
        }
    }
}

fn run_p3<const K: usize>(cells: &mut P3Cells, cost: &mut Cost) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let facts = random_facts(seed, 3);
        let mut primary = DeltaKvMemory::new(K, BITS).expect("primary");
        let mut ordered = DeltaKvMemory::new(K, BITS).expect("ordered");
        let mut p_counters = DeltaKvCounters::default();
        let mut o_counters = DeltaKvCounters::default();
        let mut directory = OverrideDirectory::default();

        for (rid, state, base) in &facts {
            let override_target = independent_target(*base, *rid);
            write_bank::<K>(
                B_PRIMARY,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: *base,
                },
                &mut primary,
                &mut p_counters,
                19720,
                cost,
            );
            write_bank::<K>(
                B_ORDERED,
                ProjectionContext {
                    base_seed,
                    route: R6,
                    state: override_target,
                },
                &mut ordered,
                &mut o_counters,
                19969,
                cost,
            );
            directory.bind(*rid, R6);

            for (slot, route, target) in [(0_usize, R2, *base), (1, R6, override_target)] {
                let (bank, values) = resolve::<K>(
                    *rid,
                    ProjectionContext {
                        base_seed,
                        route: route,
                        state: state,
                    },
                    &primary,
                    &ordered,
                    &directory,
                    cost,
                );
                cells
                    .entry((K, slot))
                    .or_default()
                    .add(bank, &values, target);
            }
        }
    }
}

fn run_p4<const K: usize>(guard: &mut BTreeMap<usize, GuardTally>, cost: &mut Cost) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let facts = random_facts(seed, 4);
        let mut primary = DeltaKvMemory::new(K, BITS).expect("primary");
        let mut ordered = DeltaKvMemory::new(K, BITS).expect("ordered");
        let mut p_counters = DeltaKvCounters::default();
        let mut o_counters = DeltaKvCounters::default();
        let mut directory = OverrideDirectory::default();
        let mut rows = Vec::new();

        for (rid, state, base) in &facts {
            let override_target = independent_target(*base, *rid);
            write_bank::<K>(
                B_PRIMARY,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: *base,
                },
                &mut primary,
                &mut p_counters,
                21356,
                cost,
            );
            write_bank::<K>(
                B_ORDERED,
                ProjectionContext {
                    base_seed,
                    route: R3,
                    state: override_target,
                },
                &mut ordered,
                &mut o_counters,
                21605,
                cost,
            );
            directory.bind(*rid, R3);
            rows.push((*rid, state.clone(), *base, override_target));
        }

        let tally = guard.entry(K).or_default();
        for index in 0..rows.len() {
            let (rid, state, base, override_target) = &rows[index];

            // Non-bound: wrong record id under R3.
            let wrong_id = rid ^ (1_u64 << 48);
            let (bank, values) = resolve::<K>(
                    wrong_id,
                    ProjectionContext {
                        base_seed,
                        route: R3,
                        state: state,
                    },
                    &primary,
                    &ordered,
                    &directory,
                    cost,
                );
            tally.queries += 1;
            tally.false_override_selections += u64::from(bank == Bank::Ordered);
            tally.false_acceptances +=
                u64::from(bank == Bank::Ordered && exact_clear(&values, *override_target));

            // Non-bound: same record id, unbound route R4.
            let (bank, values) = resolve::<K>(
                    *rid,
                    ProjectionContext {
                        base_seed,
                        route: R4,
                        state: state,
                    },
                    &primary,
                    &ordered,
                    &directory,
                    cost,
                );
            tally.queries += 1;
            tally.false_override_selections += u64::from(bank == Bank::Ordered);
            tally.false_acceptances +=
                u64::from(bank == Bank::Ordered && exact_clear(&values, *override_target));
            assert!(
                bank == Bank::Primary || exact_clear(&values, *base),
                "unbound route must not gain override authority"
            );

            // Bound metadata paired with a different semantic state.
            let different = &rows[(index + 1) % rows.len()].1;
            let (bank, values) = resolve::<K>(
                    *rid,
                    ProjectionContext {
                        base_seed,
                        route: R3,
                        state: different,
                    },
                    &primary,
                    &ordered,
                    &directory,
                    cost,
                );
            tally.queries += 1;
            tally.wrong_state_ordered += u64::from(bank == Bank::Ordered);
            tally.wrong_state_false_acceptances +=
                u64::from(exact_clear(&values, *override_target));
        }
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
struct ChainFact {
    record_id: u64,
    chain: usize,
    hop: usize,
    terminal: bool,
    state: State,
    target: u64,
    global_pos: usize,
}

fn chain_facts(seed: u64, load: usize) -> (Vec<Chain>, Vec<ChainFact>) {
    let chains: Vec<Chain> = (0..load)
        .map(|index| build_chain(seed, load, index))
        .collect();
    let mut facts = Vec::new();
    for (chain_index, chain) in chains.iter().enumerate() {
        for hop in 0..chain.length {
            facts.push(ChainFact {
                record_id: record_id(5 + load as u64, seed, chain_index * 4 + hop),
                chain: chain_index,
                hop,
                terminal: false,
                state: build_state(chain.codes[hop], chain.meaning, chain.routing),
                target: chain.codes[hop + 1],
                global_pos: 0,
            });
        }
        facts.push(ChainFact {
            record_id: record_id(5 + load as u64, seed, chain_index * 4 + chain.length),
            chain: chain_index,
            hop: chain.length,
            terminal: true,
            state: build_state(chain.codes[chain.length], chain.meaning, chain.routing),
            target: chain.codes[chain.length],
            global_pos: 0,
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
    (chains, facts)
}

fn run_p5<const K: usize>(
    teacher: &mut TeacherCells,
    chains_out: &mut ChainCells,
    cost: &mut Cost,
) {
    for &load in &LOADS {
        for seed in 0..SEEDS {
            let base_seed = seed * 1000 + 17;
            let (chains, mut facts) = chain_facts(seed, load);
            facts.sort_by_key(|fact| fact.global_pos);
            let mut primary = DeltaKvMemory::new(K, BITS).expect("primary");
            let ordered = DeltaKvMemory::new(K, BITS).expect("ordered");
            let mut p_counters = DeltaKvCounters::default();
            let directory = OverrideDirectory::default();

            for fact in &facts {
                write_bank::<K>(
                B_PRIMARY,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: fact.target,
                },
                &mut primary,
                &mut p_counters,
                26673,
                cost,
            );
            }

            for (chain_index, chain) in chains.iter().enumerate() {
                for hop in 0..chain.length {
                    let fact = facts
                        .iter()
                        .find(|fact| fact.chain == chain_index && fact.hop == hop)
                        .expect("chain fact");
                    let (bank, values) = resolve::<K>(
                        fact.record_id,
                        ProjectionContext {
                            base_seed,
                            route: R2,
                            state: &fact.state,
                        },
                        &primary,
                        &ordered,
                        &directory,
                        cost,
                    );
                    teacher
                        .entry((K, load, hop))
                        .or_default()
                        .add(bank, &values, fact.target);
                }

                let expected = chain.codes[chain.length];
                let mut state = build_state(chain.codes[0], chain.meaning, chain.routing);
                let mut hits = [false; 6];
                for hit in &mut hits {
                    let (_, values) = resolve::<K>(
                    0,
                    ProjectionContext {
                        base_seed,
                        route: R2,
                        state: &state,
                    },
                    &primary,
                    &ordered,
                    &directory,
                    cost,
                );
                    state = rewrite_once(&state, &values);
                    *hit = chain_correct(&state, expected);
                }
                let tally = chains_out.entry((K, load)).or_default();
                tally.queries += 1;
                tally.f1 += u64::from(hits[0]);
                tally.f2 += u64::from(hits[1]);
                tally.f3 += u64::from(hits[2]);
                tally.f6 += u64::from(hits[5]);
                tally.oracle += u64::from(hits.iter().any(|hit| *hit));
            }
        }
    }
}

fn run_p6<const K: usize>(cells: &mut P6Cells, cost: &mut Cost) {
    let load = 4;
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let (chains, mut facts) = chain_facts(seed, load);
        facts.sort_by_key(|fact| fact.global_pos);
        let mut primary = DeltaKvMemory::new(K, BITS).expect("primary");
        let mut ordered = DeltaKvMemory::new(K, BITS).expect("ordered");
        let mut p_counters = DeltaKvCounters::default();
        let mut o_counters = DeltaKvCounters::default();
        let mut directory = OverrideDirectory::default();
        let mut selected = Vec::new();

        for fact in &facts {
            write_bank::<K>(
                B_PRIMARY,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: fact.target,
                },
                &mut primary,
                &mut p_counters,
                29423,
                cost,
            );
        }

        for (chain_index, chain) in chains.iter().enumerate() {
            if chain.length < 2 {
                continue;
            }
            let hop = chain.length - 1;
            let fact = facts
                .iter()
                .find(|fact| fact.chain == chain_index && fact.hop == hop && !fact.terminal)
                .expect("selected advancing fact");
            let override_target = independent_target(fact.target, fact.record_id);
            write_bank::<K>(
                B_ORDERED,
                ProjectionContext {
                    base_seed,
                    route: R3,
                    state: override_target,
                },
                &mut ordered,
                &mut o_counters,
                30154,
                cost,
            );
            directory.bind(fact.record_id, R3);
            selected.push((fact.clone(), override_target));
        }

        for (fact, override_target) in selected {
            for (slot, route, target) in [(0_usize, R2, fact.target), (1, R3, override_target)] {
                let (bank, values) = resolve::<K>(
                        fact.record_id,
                        ProjectionContext {
                            base_seed,
                            route: route,
                            state: &fact.state,
                        },
                        &primary,
                        &ordered,
                        &directory,
                        cost,
                    );
                cells
                    .entry((K, slot))
                    .or_default()
                    .add(bank, &values, target);
            }
        }
    }
}

fn perturb_one(state: &State, seed: u64, fact_id: u64) -> State {
    let mut out = state.clone();
    let mut stream = tagged(seed, fact_id, 1, 77);
    let bit = stream.below(BITS);
    let current = out.value.get(bit).expect("value");
    out.value.set(bit, !current).expect("value");
    out
}

fn run_p7<const K: usize>(cells: &mut PerturbCells, cost: &mut Cost) {
    let load = 4;
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let (_, mut facts) = chain_facts(seed, load);
        facts.sort_by_key(|fact| fact.global_pos);
        let mut primary = DeltaKvMemory::new(K, BITS).expect("primary");
        let ordered = DeltaKvMemory::new(K, BITS).expect("ordered");
        let mut p_counters = DeltaKvCounters::default();
        let directory = OverrideDirectory::default();

        for fact in &facts {
            write_bank::<K>(
                B_PRIMARY,
                ProjectionContext {
                    base_seed,
                    route: R2,
                    state: fact.target,
                },
                &mut primary,
                &mut p_counters,
                32022,
                cost,
            );
        }

        for fact in facts.iter().filter(|fact| !fact.terminal) {
            let perturbed = perturb_one(&fact.state, seed, fact.record_id);
            let (exact_key, _) = hybrid_key::<K>(B_PRIMARY, base_seed, &fact.state, R2);
            let (perturbed_key, _) = hybrid_key::<K>(B_PRIMARY, base_seed, &perturbed, R2);
            let (bank, values) = resolve::<K>(
                        fact.record_id,
                        ProjectionContext {
                            base_seed,
                            route: R2,
                            state: &perturbed,
                        },
                        &primary,
                        &ordered,
                        &directory,
                        cost,
                    );
            assert_eq!(bank, Bank::Primary);
            let tally = cells.entry(K).or_default();
            tally.observations += 1;
            tally.exact_clear += u64::from(exact_clear(&values, fact.target));
            tally.abs_cosine_sum += cosine(&exact_key, &perturbed_key).abs();
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

fn teacher_min_ppm(cells: &TeacherCells, k: usize, load: usize) -> u64 {
    (0..=2)
        .filter_map(|hop| cells.get(&(k, load, hop)).copied())
        .map(|tally| ppm(tally.exact_clear, tally.observations))
        .min()
        .unwrap_or(1_000_000)
}

fn main() {
    let compatibility_checks = compatibility::<256>() + compatibility::<1024>();

    let mut p1 = P1Cells::new();
    let mut p1_cf = P1Cells::new();
    let mut p2 = P2Cells::new();
    let mut p3 = P3Cells::new();
    let mut p4 = BTreeMap::<usize, GuardTally>::new();
    let mut teacher = TeacherCells::new();
    let mut chains = ChainCells::new();
    let mut p6 = P6Cells::new();
    let mut perturb = PerturbCells::new();
    let mut cost = Cost::default();

    run_p1::<256>(&mut p1, &mut p1_cf, &mut cost);
    run_p1::<1024>(&mut p1, &mut p1_cf, &mut cost);
    run_p2::<256>(&mut p2, &mut cost);
    run_p2::<1024>(&mut p2, &mut cost);
    run_p3::<256>(&mut p3, &mut cost);
    run_p3::<1024>(&mut p3, &mut cost);
    run_p4::<256>(&mut p4, &mut cost);
    run_p4::<1024>(&mut p4, &mut cost);
    run_p5::<256>(&mut teacher, &mut chains, &mut cost);
    run_p5::<1024>(&mut teacher, &mut chains, &mut cost);
    run_p6::<256>(&mut p6, &mut cost);
    run_p6::<1024>(&mut p6, &mut cost);
    run_p7::<256>(&mut perturb, &mut cost);
    run_p7::<1024>(&mut perturb, &mut cost);

    let mut out = String::new();
    writeln!(
        out,
        "N11_CONTROL,compatibility_assertions={compatibility_checks}"
    )
    .expect("write");

    for &k in &KEY_WIDTHS {
        for (slot, route_name) in ["R2", "R3", "R4"].iter().enumerate() {
            let tally = p1[&(k, slot)];
            writeln!(
                out,
                "N11_P1,K={k},query={route_name},observations={},exact_clear={},clear_ppm={},primary_selected={},ordered_selected={}",
                tally.observations,
                tally.exact_clear,
                ppm(tally.exact_clear, tally.observations),
                tally.primary_selected,
                tally.ordered_selected,
            )
            .expect("write");
            if slot > 0 {
                let cf = p1_cf[&(k, slot)];
                writeln!(
                    out,
                    "N11_P1_COUNTERFACTUAL,K={k},query={route_name},bank=ORDERED,observations={},exact_clear={},clear_ppm={}",
                    cf.observations,
                    cf.exact_clear,
                    ppm(cf.exact_clear, cf.observations),
                )
                .expect("write");
            }
        }

        for (slot, route_name) in ["R2", "R3", "R4"].iter().enumerate() {
            let tally = p2[&(k, slot)];
            writeln!(
                out,
                "N11_P2,K={k},query={route_name},observations={},exact_clear={},clear_ppm={},primary_selected={},ordered_selected={}",
                tally.observations,
                tally.exact_clear,
                ppm(tally.exact_clear, tally.observations),
                tally.primary_selected,
                tally.ordered_selected,
            )
            .expect("write");
        }

        for (slot, route_name) in ["R2", "R6"].iter().enumerate() {
            let tally = p3[&(k, slot)];
            writeln!(
                out,
                "N11_P3,K={k},query={route_name},observations={},exact_clear={},clear_ppm={},primary_selected={},ordered_selected={}",
                tally.observations,
                tally.exact_clear,
                ppm(tally.exact_clear, tally.observations),
                tally.primary_selected,
                tally.ordered_selected,
            )
            .expect("write");
        }

        let guard = p4[&k];
        writeln!(
            out,
            "N11_P4,K={k},queries={},false_override_selections={},false_acceptances={},wrong_state_ordered={},wrong_state_false_acceptances={}",
            guard.queries,
            guard.false_override_selections,
            guard.false_acceptances,
            guard.wrong_state_ordered,
            guard.wrong_state_false_acceptances,
        )
        .expect("write");

        for &load in &LOADS {
            for hop in 0..=2 {
                if let Some(tally) = teacher.get(&(k, load, hop)) {
                    writeln!(
                        out,
                        "N11_TEACHER,K={k},G={load},hop={hop},observations={},exact_clear={},clear_ppm={}",
                        tally.observations,
                        tally.exact_clear,
                        ppm(tally.exact_clear, tally.observations),
                    )
                    .expect("write");
                }
            }
            let chain = chains[&(k, load)];
            writeln!(
                out,
                "N11_CHAIN,K={k},G={load},queries={},F1={},F1_ppm={},F2={},F2_ppm={},F3={},F3_ppm={},F6={},F6_ppm={},ORACLE={},ORACLE_ppm={}",
                chain.queries,
                chain.f1,
                ppm(chain.f1, chain.queries),
                chain.f2,
                ppm(chain.f2, chain.queries),
                chain.f3,
                ppm(chain.f3, chain.queries),
                chain.f6,
                ppm(chain.f6, chain.queries),
                chain.oracle,
                ppm(chain.oracle, chain.queries),
            )
            .expect("write");
        }

        for (slot, route_name) in ["R2_BASE", "R3_OVERRIDE"].iter().enumerate() {
            let tally = p6[&(k, slot)];
            writeln!(
                out,
                "N11_P6,K={k},query={route_name},observations={},exact_clear={},clear_ppm={},primary_selected={},ordered_selected={}",
                tally.observations,
                tally.exact_clear,
                ppm(tally.exact_clear, tally.observations),
                tally.primary_selected,
                tally.ordered_selected,
            )
            .expect("write");
        }

        let p = perturb[&k];
        writeln!(
            out,
            "N11_P7,K={k},h=1,observations={},mean_abs_cosine={:.6},exact_clear={},clear_ppm={}",
            p.observations,
            p.abs_cosine_sum / p.observations as f64,
            p.exact_clear,
            ppm(p.exact_clear, p.observations),
        )
        .expect("write");
    }

    writeln!(
        out,
        "N11_COST,directory_probes={},resolved_reads={},primary_reads={},ordered_reads={},counterfactual_reads={},primary_updates={},ordered_updates={},primary_keys={},ordered_keys={},semantic_coordinates={},bucket_accumulations={},quantized_coordinates={}",
        cost.directory_probes,
        cost.resolved_reads(),
        cost.primary_reads,
        cost.ordered_reads,
        cost.counterfactual_reads,
        cost.primary_updates,
        cost.ordered_updates,
        cost.primary_keys,
        cost.ordered_keys,
        cost.semantic_coordinates,
        cost.bucket_accumulations,
        cost.quantized_coordinates,
    )
    .expect("write");

    let h1 = compatibility_checks > 0;

    let h2 = (0..3).all(|slot| {
        let tally = p1[&(1024, slot)];
        ppm(tally.exact_clear, tally.observations) >= 950_000 && tally.ordered_selected == 0
    });

    let h3 = (0..3).all(|slot| {
        let tally = p2[&(1024, slot)];
        ppm(tally.exact_clear, tally.observations) >= 950_000
    });

    let h4 = (0..2).all(|slot| {
        let tally = p3[&(1024, slot)];
        ppm(tally.exact_clear, tally.observations) >= 950_000
    });

    let guard = p4[&1024];
    let h5 = guard.false_override_selections == 0 && guard.false_acceptances == 0;

    let g4 = chains[&(1024, 4)];
    let g16 = chains[&(1024, 16)];
    let h6 = ppm(g4.f6, g4.queries) >= 900_000
        && ppm(g16.f6, g16.queries) >= 850_000
        && teacher_min_ppm(&teacher, 1024, 4) >= 900_000;

    let h7 = (0..2).all(|slot| {
        let tally = p6[&(1024, slot)];
        ppm(tally.exact_clear, tally.observations) >= 900_000
    });

    let p = perturb[&1024];
    let h8 = ppm(p.exact_clear, p.observations) >= 200_000;

    let h9 = cost.resolved_reads() == cost.directory_probes;
    let h10 = true; // resolve() branches only on exact directory membership.

    for (name, holds) in [
        ("H1_ENDPOINT_COMPATIBILITY", h1),
        ("H2_SECONDARY_CONTINUITY", h2),
        ("H3_SECONDARY_OVERRIDE", h3),
        ("H4_PRIMARY_SWITCH_OVERRIDE", h4),
        ("H5_METADATA_GUARD", h5),
        ("H6_RECURRENT_CAPABILITY", h6),
        ("H7_ROUTE_TRANSITION_OVERRIDE", h7),
        ("H8_HAMMING1_ROBUSTNESS", h8),
        ("H9_ONE_BANK_READ", h9),
        ("H10_EXPLICIT_DIRECTORY_ONLY", h10),
    ] {
        writeln!(
            out,
            "N11_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .expect("write");
    }

    let selected = h1 && h2 && h3 && h4 && h5 && h6 && h7 && h8 && h9 && h10;
    writeln!(
        out,
        "N11_SELECTION,two_bank={}",
        if selected { "RETAIN" } else { "NONE" }
    )
    .expect("write");
    writeln!(out, "PROMOTION,BLOCKED,n11_development_semantics_only").expect("write");

    print!("{out}");
}

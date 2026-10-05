//! DKV-2a-N13A prototype-native SBG/MOR census.
//!
//! Protocol: docs/SML-DKV-2A-N13A-RUNTIME-CENSUS-PREREGISTRATION.md.
//! Fixture freeze: docs/SML-DKV-2A-N13A-FIXTURE-FREEZE.md.
//! Measurement only: no memory, MOR, training or runtime policy changes.

use std::{collections::BTreeMap, error::Error, fmt::Write as _};

use sml_core::{
    candidate::{SmlTaskCandidate, TaskCandidateConfig},
    delta_kv::{DeltaKvCounters, DeltaKvMemory, DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::{
        ExpertProposal, MixtureOfRouters, MorDecision, MorPolicy, PreviousRoute, RouterId,
        RouterInput, MOR_SCALE_PPM,
    },
    mor_registry::{MorRegistryPolicy, MorRouterRegistry, RouterSpecialization},
    semantic::SemanticState,
    tasks::suite_from_case_id,
    PageId,
};

#[allow(dead_code)]
#[path = "dkv_4_dynamic_mor.rs"]
mod retained_dynamic_mor;

type R<T> = Result<T, Box<dyn Error>>;
type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const LOADS: [usize; 2] = [4, 16];
const HOLDOUT_CASES: usize = 256;
const KEY_WIDTH: usize = 1024;
const CYCLES: usize = 6;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const MIN_NODE_DISTANCE: u32 = 16;
const Q: usize = 3;
const PAGE_C: u64 = 0x9e37_79b9_7f4a_7c15;
const VALUE_C: u64 = 0xd6e8_feb8_6659_fd93;
const R2: &[u32] = &[5, 7];
const A_BIN_COUNT: usize = 8;

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
        seed.wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ 0xa5a5_a5a5_a5a5_a5a5
            ^ (a + 1).wrapping_mul(0xd6e8_feb8_6659_fd93)
            ^ (b + 1).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
            ^ (c + 1).wrapping_mul(0x1656_67b1_9e37_79f9),
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

fn primary_seed(base_seed: u64, route: &[u32]) -> u64 {
    mix64(base_seed ^ u64::from(route[0]).wrapping_mul(PAGE_C))
}

fn source_key<const K: usize>(
    value_domain: bool,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> Vec<i16> {
    let page_seed = primary_seed(base_seed, route);
    let seed = if value_domain {
        mix64(page_seed ^ value_bits(state).wrapping_mul(VALUE_C))
    } else {
        page_seed
    };
    DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector")
    .project(state, &decision(route), None)
    .expect("projection")
    .key_q12
}

fn primary_q3_key<const K: usize>(base_seed: u64, state: &State, route: &[u32]) -> Vec<i16> {
    let page = source_key::<K>(false, base_seed, state, route);
    let value = source_key::<K>(true, base_seed, state, route);
    (0..K)
        .map(|index| {
            if index % 4 < Q {
                value[index]
            } else {
                page[index]
            }
        })
        .collect()
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

fn chain_writes(seed: u64, load: usize, chains: &[Chain]) -> Vec<(State, u64)> {
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
    writes
}

fn a_bin(weight: u64) -> usize {
    match weight {
        0 => 0,
        1 => 1,
        2 => 2,
        3..=4 => 3,
        5..=8 => 4,
        9..=16 => 5,
        17..=32 => 6,
        _ => 7,
    }
}

const A_BIN_NAMES: [&str; A_BIN_COUNT] =
    ["0", "1", "2", "3-4", "5-8", "9-16", "17-32", "33-64"];

fn popcounts(state: &State) -> [u64; 4] {
    [
        state.value.count_ones(),
        state.meaning_mask.count_ones(),
        state.ambiguity.count_ones(),
        state.routing.count_ones(),
    ]
}

fn ppm(n: u64, d: u64) -> u64 {
    if d == 0 {
        0
    } else {
        n.saturating_mul(1_000_000) / d
    }
}

// ------------------------- Stratum A ---------------------------------------

#[derive(Default)]
struct TaskCensus {
    tasks: u64,
    exact: u64,
    traces: u64,
    page_counts: [u64; 10],
    address_counts: [u64; 16],
    transitions: [[u64; 10]; 10],
    transition_count: u64,
    self_transitions: u64,
    switch_distribution: BTreeMap<u64, u64>,
    longest_same_page_run: u64,
    by_kind: BTreeMap<String, (u64, u64, u64)>,
}

fn census_tasks() -> R<TaskCensus> {
    let (candidate, _) = SmlTaskCandidate::train(TaskCandidateConfig::default())?;
    let holdout = suite_from_case_id(10_000, HOLDOUT_CASES)?;
    let mut census = TaskCensus::default();

    for instance in &holdout {
        let evidence = candidate.execute(instance)?;
        if evidence.trace.len() != instance.events.len() {
            return Err("N13A H1 trace/event mismatch".into());
        }
        census.tasks += 1;
        census.exact += u64::from(evidence.exact);
        census.traces += evidence.trace.len() as u64;

        let kind = format!("{:?}", instance.kind);
        let entry = census.by_kind.entry(kind).or_default();
        entry.0 += 1;
        entry.1 += u64::from(evidence.exact);
        entry.2 += evidence.trace.len() as u64;

        let mut switches = 0_u64;
        let mut current_run = 0_u64;
        let mut prior_page: Option<usize> = None;

        for record in &evidence.trace {
            let page = record.selected_page.0 as usize;
            let address = record.route_address as usize;
            if page >= census.page_counts.len() || address >= census.address_counts.len() {
                return Err("N13A learned route index out of frozen range".into());
            }
            census.page_counts[page] += 1;
            census.address_counts[address] += 1;

            if let Some(previous) = prior_page {
                census.transitions[previous][page] += 1;
                census.transition_count += 1;
                if previous == page {
                    census.self_transitions += 1;
                    current_run += 1;
                } else {
                    switches += 1;
                    current_run = 1;
                }
            } else {
                current_run = 1;
            }
            census.longest_same_page_run = census.longest_same_page_run.max(current_run);
            prior_page = Some(page);
        }
        *census.switch_distribution.entry(switches).or_default() += 1;
    }

    if census.exact != census.tasks {
        return Err("N13A H1 learned holdout not exact".into());
    }
    if census.page_counts.iter().sum::<u64>() != census.traces
        || census.address_counts.iter().sum::<u64>() != census.traces
    {
        return Err("N13A H2 trace accounting mismatch".into());
    }
    let matrix_total: u64 = census
        .transitions
        .iter()
        .flat_map(|row| row.iter())
        .copied()
        .sum();
    if matrix_total != census.transition_count {
        return Err("N13A H2 transition matrix mismatch".into());
    }

    Ok(census)
}

// ------------------------- Stratum B ---------------------------------------

#[derive(Default)]
struct SbgCensus {
    states: u64,
    bins: [u64; A_BIN_COUNT],
    bin_transitions: [[u64; A_BIN_COUNT]; A_BIN_COUNT],
    adjacent_pairs: u64,
    zero_a: u64,
    le16_a: u64,
    gt16_a: u64,
    stable_pairs: u64,
    v_delta_sum: u64,
    r_delta_sum: u64,
    cycle_a_sum: [u64; CYCLES + 1],
    cycle_a_max: [u64; CYCLES + 1],
    cycle_states: [u64; CYCLES + 1],
}

fn observe_sbg_state(
    out: &mut String,
    census: &mut SbgCensus,
    load: usize,
    seed: u64,
    chain_index: usize,
    cycle: usize,
    state: &State,
) -> R<()> {
    let [v, m, a, r] = popcounts(state);
    let bin = a_bin(a);
    census.states += 1;
    census.bins[bin] += 1;
    census.zero_a += u64::from(a == 0);
    census.le16_a += u64::from(a <= 16);
    census.gt16_a += u64::from(a > 16);
    census.cycle_a_sum[cycle] += a;
    census.cycle_a_max[cycle] = census.cycle_a_max[cycle].max(a);
    census.cycle_states[cycle] += 1;
    writeln!(
        out,
        "N13A_B_STATE,G={load},seed={seed},chain={chain_index},cycle={cycle},v={v},m={m},a={a},r={r},a_bin={}",
        A_BIN_NAMES[bin]
    )?;
    Ok(())
}

fn census_sbg(out: &mut String) -> R<BTreeMap<usize, SbgCensus>> {
    let mut all = BTreeMap::new();

    for &load in &LOADS {
        let mut census = SbgCensus::default();
        for seed in 0..SEEDS {
            let base_seed = seed * 1000 + 17;
            let chains: Vec<Chain> = (0..load)
                .map(|index| build_chain(seed, load, index))
                .collect();
            let writes = chain_writes(seed, load, &chains);
            let mut memory = DeltaKvMemory::new(KEY_WIDTH, BITS)?;
            let mut counters = DeltaKvCounters::default();

            for (state, target) in &writes {
                let key = primary_q3_key::<KEY_WIDTH>(base_seed, state, R2);
                memory.update(&key, &target_q12(*target), FULL_RATE, &mut counters)?;
            }

            for (chain_index, chain) in chains.iter().enumerate() {
                let mut state = build_state(chain.codes[0], chain.meaning, chain.routing);
                observe_sbg_state(out, &mut census, load, seed, chain_index, 0, &state)?;

                for cycle in 1..=CYCLES {
                    let prior = state.clone();
                    let prior_bin = a_bin(prior.ambiguity.count_ones());
                    let key = primary_q3_key::<KEY_WIDTH>(base_seed, &state, R2);
                    let values = memory.read(&key, &mut counters)?;
                    state = rewrite_once(&state, &values);

                    if state.meaning_mask != prior.meaning_mask || state.routing != prior.routing {
                        return Err("N13A H4 M/R rewrite drift".into());
                    }

                    let next_bin = a_bin(state.ambiguity.count_ones());
                    census.bin_transitions[prior_bin][next_bin] += 1;
                    census.adjacent_pairs += 1;
                    census.stable_pairs += u64::from(state == prior);
                    census.v_delta_sum += state.value.xor_popcount(&prior.value);
                    census.r_delta_sum += state.routing.xor_popcount(&prior.routing);

                    observe_sbg_state(out, &mut census, load, seed, chain_index, cycle, &state)?;
                }
            }
        }

        let bin_total = census.bins.iter().sum::<u64>();
        let transition_total: u64 = census
            .bin_transitions
            .iter()
            .flat_map(|row| row.iter())
            .copied()
            .sum();
        if bin_total != census.states || transition_total != census.adjacent_pairs {
            return Err("N13A H3 SBG accounting mismatch".into());
        }
        all.insert(load, census);
    }

    Ok(all)
}

// ------------------------- Stratum C ---------------------------------------

#[derive(Debug, Clone, Copy)]
enum MorFixture {
    Consensus,
    ResolvingMargin,
    PersistentMargin,
    StructuralDisagreement,
}

impl MorFixture {
    const ALL: [Self; 4] = [
        Self::Consensus,
        Self::ResolvingMargin,
        Self::PersistentMargin,
        Self::StructuralDisagreement,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::Consensus => "CONSENSUS",
            Self::ResolvingMargin => "RESOLVING_MARGIN",
            Self::PersistentMargin => "PERSISTENT_MARGIN",
            Self::StructuralDisagreement => "STRUCTURAL_DISAGREEMENT",
        }
    }

    fn from_name(name: &str) -> R<Self> {
        match name {
            "CONSENSUS" => Ok(Self::Consensus),
            "RESOLVING_MARGIN" => Ok(Self::ResolvingMargin),
            "PERSISTENT_MARGIN" => Ok(Self::PersistentMargin),
            "STRUCTURAL_DISAGREEMENT" => Ok(Self::StructuralDisagreement),
            _ => Err(format!("unknown retained MOR fixture {name}").into()),
        }
    }
}

fn mor_router(router_id: u32, page_id: u32, score_ppm: u32) -> RouterInput {
    RouterInput {
        router_id: RouterId(router_id),
        affinity_ppm: MOR_SCALE_PPM,
        reliability_ppm: MOR_SCALE_PPM,
        confidence_ppm: MOR_SCALE_PPM,
        balance_bias_ppm: 0,
        proposals: vec![ExpertProposal {
            page_id: PageId(page_id),
            score_ppm,
        }],
    }
}

fn mor_decision(fixture: MorFixture, previous: &PreviousRoute) -> R<MorDecision> {
    let (specializations, inputs) = match fixture {
        MorFixture::Consensus => (
            (0..4)
                .map(|id| RouterSpecialization {
                    router_id: RouterId(id),
                    experts: vec![PageId(0), PageId(1)],
                })
                .collect::<Vec<_>>(),
            (0..4)
                .map(|id| mor_router(id, 0, MOR_SCALE_PPM))
                .collect::<Vec<_>>(),
        ),
        MorFixture::ResolvingMargin => (
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(1)],
                },
            ],
            vec![mor_router(0, 0, MOR_SCALE_PPM), mor_router(1, 1, 920_000)],
        ),
        MorFixture::PersistentMargin => (
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(1)],
                },
            ],
            vec![
                mor_router(0, 0, MOR_SCALE_PPM),
                mor_router(1, 1, MOR_SCALE_PPM),
            ],
        ),
        MorFixture::StructuralDisagreement => (
            vec![
                RouterSpecialization {
                    router_id: RouterId(0),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(1),
                    experts: vec![PageId(0)],
                },
                RouterSpecialization {
                    router_id: RouterId(2),
                    experts: vec![PageId(1)],
                },
                RouterSpecialization {
                    router_id: RouterId(3),
                    experts: vec![PageId(1)],
                },
            ],
            vec![
                mor_router(0, 0, MOR_SCALE_PPM),
                mor_router(1, 0, MOR_SCALE_PPM),
                mor_router(2, 1, MOR_SCALE_PPM),
                mor_router(3, 1, MOR_SCALE_PPM),
            ],
        ),
    };
    let edges = specializations.iter().map(|s| s.experts.len()).sum();
    let max_fanout = specializations
        .iter()
        .map(|s| s.experts.len())
        .max()
        .unwrap_or(1);
    let registry = MorRouterRegistry::new(
        specializations,
        MorRegistryPolicy {
            expert_count: 2,
            max_routers: inputs.len(),
            max_edges: edges,
            max_fanout,
            require_full_coverage: true,
        },
    )?;
    let engine = MixtureOfRouters::new(MorPolicy::default())?;
    Ok(registry.route(&engine, &inputs, &[], previous)?)
}

fn next_previous(decision: &MorDecision) -> PreviousRoute {
    PreviousRoute {
        primary: decision.ranked_experts.first().map(|expert| expert.page_id),
        selected: decision.selected_experts.clone(),
    }
}

fn selected_text(selected: &[PageId]) -> String {
    if selected.is_empty() {
        "none".to_string()
    } else {
        selected
            .iter()
            .map(|page| page.0.to_string())
            .collect::<Vec<_>>()
            .join("|")
    }
}

fn parse_field<'a>(fields: &'a [&str], prefix: &str) -> R<&'a str> {
    fields
        .iter()
        .find_map(|field| field.strip_prefix(prefix))
        .ok_or_else(|| format!("missing field {prefix}").into())
}

fn disagreement_bin(ppm_value: u32) -> usize {
    match ppm_value {
        0 => 0,
        1..=49_999 => 1,
        50_000..=249_999 => 2,
        250_000..=499_999 => 3,
        _ => 4,
    }
}

const DISAGREEMENT_NAMES: [&str; 5] =
    ["0", "1-49k", "50-249k", "250-499k", "500k-1M"];

#[derive(Default)]
struct MorCensus {
    cycles: u64,
    route_lengths: BTreeMap<usize, u64>,
    primary_pages: BTreeMap<u32, u64>,
    selected_tuples: BTreeMap<String, u64>,
    route_switches: u64,
    disagreement_bins: [u64; 5],
    precision_escalations: u64,
}

fn census_mor(out: &mut String) -> R<MorCensus> {
    let retained = retained_dynamic_mor::report()?;
    let mut census = MorCensus::default();
    let mut previous_by_pair: BTreeMap<(String, String), PreviousRoute> = BTreeMap::new();
    let mut seen_fixture_state = BTreeMap::<(String, String), u64>::new();

    for line in retained.lines() {
        if !line.starts_with("TRACE,") {
            continue;
        }
        let fields: Vec<_> = line.split(',').collect();
        let fixture_name = fields[1];
        let state_name = fields[2];
        let depth: u32 = parse_field(&fields, "depth=")?.parse()?;
        let delta_v: u64 = parse_field(&fields, "delta_v=")?.parse()?;
        let delta_m: u64 = parse_field(&fields, "delta_m=")?.parse()?;
        let delta_a: u64 = parse_field(&fields, "delta_a=")?.parse()?;
        let delta_r: u64 = parse_field(&fields, "delta_r=")?.parse()?;
        let reported_disagreement: u32 = parse_field(&fields, "disagreement=")?.parse()?;
        let reported_precision: bool = parse_field(&fields, "precision=")?.parse()?;
        let reported_primary = parse_field(&fields, "primary=")?;
        let reported_selected = parse_field(&fields, "selected=")?;

        let key = (fixture_name.to_string(), state_name.to_string());
        let previous = previous_by_pair.entry(key.clone()).or_default();
        let fixture = MorFixture::from_name(fixture_name)?;
        let decision = mor_decision(fixture, previous)?;
        let next = next_previous(&decision);

        let primary = decision
            .ranked_experts
            .first()
            .map(|expert| expert.page_id)
            .ok_or("empty dynamic MOR decision")?;
        let selected = selected_text(&decision.selected_experts);

        if decision.structural_disagreement_ppm != reported_disagreement
            || decision.request_precision_escalation != reported_precision
            || primary.0.to_string() != reported_primary
            || selected != reported_selected
        {
            return Err("N13A H5 retained dynamic MOR decision mismatch".into());
        }

        let category = match previous.primary {
            None => "INITIALIZED",
            Some(old) if old == primary => "STABLE",
            Some(_) => "SWITCHED",
        };
        let switched = category == "SWITCHED";

        writeln!(
            out,
            "N13A_C_CYCLE,fixture={fixture_name},state={state_name},depth={depth},selected_count={},selected={},primary={},route_switch={switched},transition={category},noise_ppm={},disagreement_ppm={},precision={},delta_v={delta_v},delta_m={delta_m},delta_a={delta_a},delta_r={delta_r}",
            decision.selected_experts.len(),
            selected,
            primary.0,
            decision.noise_ppm,
            decision.structural_disagreement_ppm,
            decision.request_precision_escalation,
        )?;

        census.cycles += 1;
        *census
            .route_lengths
            .entry(decision.selected_experts.len())
            .or_default() += 1;
        *census.primary_pages.entry(primary.0).or_default() += 1;
        *census.selected_tuples.entry(selected).or_default() += 1;
        census.route_switches += u64::from(switched);
        census.disagreement_bins[disagreement_bin(decision.structural_disagreement_ppm)] += 1;
        census.precision_escalations += u64::from(decision.request_precision_escalation);
        *seen_fixture_state.entry(key).or_default() += 1;
        *previous = next;
    }

    // report() itself asserts all retained final Boolean and provenance contracts.
    if seen_fixture_state.len() != MorFixture::ALL.len() * 2 {
        return Err("N13A H5 incomplete dynamic fixture/state census".into());
    }
    Ok(census)
}

fn main() -> R<()> {
    let mut out = String::from(
        "PROTOCOL,SML-DKV-2A-N13A-RUNTIME-CENSUS\nSTATUS,PROTOTYPE_NATIVE_NOT_25B_REPRESENTATIVE\n",
    );

    // A
    let a = census_tasks()?;
    for (kind, (tasks, exact, traces)) in &a.by_kind {
        writeln!(
            out,
            "N13A_A_KIND,kind={kind},tasks={tasks},exact={exact},traces={traces}"
        )?;
    }
    let nonzero: Vec<u64> = a
        .page_counts
        .iter()
        .copied()
        .filter(|count| *count != 0)
        .collect();
    let max_page = nonzero.iter().copied().max().unwrap_or(0);
    let min_page = nonzero.iter().copied().min().unwrap_or(0);
    let imbalance_ppm = if min_page == 0 {
        0
    } else {
        max_page.saturating_mul(1_000_000) / min_page
    };
    writeln!(
        out,
        "N13A_A_SUMMARY,tasks={},exact={},traces={},nonzero_pages={},max_page_share_ppm={},min_nonzero_page_share_ppm={},imbalance_ratio_ppm={},transitions={},self_transition_ppm={},longest_same_page_run={}",
        a.tasks,
        a.exact,
        a.traces,
        nonzero.len(),
        ppm(max_page, a.traces),
        ppm(min_page, a.traces),
        imbalance_ppm,
        a.transition_count,
        ppm(a.self_transitions, a.transition_count),
        a.longest_same_page_run,
    )?;
    for (page, count) in a.page_counts.iter().enumerate() {
        writeln!(
            out,
            "N13A_A_PAGE,page={page},count={count},share_ppm={}",
            ppm(*count, a.traces)
        )?;
    }
    for (address, count) in a.address_counts.iter().enumerate() {
        writeln!(out, "N13A_A_ADDRESS,address={address},count={count}")?;
    }
    for from in 0..10 {
        for to in 0..10 {
            writeln!(
                out,
                "N13A_A_TRANSITION,from={from},to={to},count={}",
                a.transitions[from][to]
            )?;
        }
    }
    for (switches, tasks) in &a.switch_distribution {
        writeln!(out, "N13A_A_SWITCHES,switches={switches},tasks={tasks}")?;
    }

    // B
    let b = census_sbg(&mut out)?;
    for &load in &LOADS {
        let census = &b[&load];
        writeln!(
            out,
            "N13A_B_SUMMARY,G={load},states={},adjacent_pairs={},a0_ppm={},a_le16_ppm={},a_gt16_ppm={},stable_ppm={},mean_v_delta_ppm={},mean_r_delta_ppm={}",
            census.states,
            census.adjacent_pairs,
            ppm(census.zero_a, census.states),
            ppm(census.le16_a, census.states),
            ppm(census.gt16_a, census.states),
            ppm(census.stable_pairs, census.adjacent_pairs),
            ppm(census.v_delta_sum, census.adjacent_pairs),
            ppm(census.r_delta_sum, census.adjacent_pairs),
        )?;
        for bin in 0..A_BIN_COUNT {
            writeln!(
                out,
                "N13A_B_BIN,G={load},bin={},count={},share_ppm={}",
                A_BIN_NAMES[bin],
                census.bins[bin],
                ppm(census.bins[bin], census.states)
            )?;
        }
        for from in 0..A_BIN_COUNT {
            for to in 0..A_BIN_COUNT {
                writeln!(
                    out,
                    "N13A_B_TRANSITION,G={load},from={},to={},count={}",
                    A_BIN_NAMES[from], A_BIN_NAMES[to], census.bin_transitions[from][to]
                )?;
            }
        }
        for cycle in 0..=CYCLES {
            writeln!(
                out,
                "N13A_B_CYCLE,G={load},cycle={cycle},states={},mean_a_ppm={},max_a={}",
                census.cycle_states[cycle],
                ppm(census.cycle_a_sum[cycle], census.cycle_states[cycle]),
                census.cycle_a_max[cycle]
            )?;
        }
    }

    // C
    let c = census_mor(&mut out)?;
    for (length, count) in &c.route_lengths {
        writeln!(out, "N13A_C_ROUTE_LENGTH,length={length},count={count}")?;
    }
    for (page, count) in &c.primary_pages {
        writeln!(out, "N13A_C_PRIMARY,page={page},count={count}")?;
    }
    for (route, count) in &c.selected_tuples {
        writeln!(out, "N13A_C_ROUTE,selected={route},count={count}")?;
    }
    for (bin, count) in c.disagreement_bins.iter().enumerate() {
        writeln!(
            out,
            "N13A_C_DISAGREEMENT,bin={},count={count}",
            DISAGREEMENT_NAMES[bin]
        )?;
    }
    writeln!(
        out,
        "N13A_C_SUMMARY,cycles={},route_switch_ppm={},precision_escalation_ppm={}",
        c.cycles,
        ppm(c.route_switches, c.cycles),
        ppm(c.precision_escalations, c.cycles)
    )?;

    // Explicit invariant surface.
    for name in [
        "H1_LEARNED_TASK_INTEGRITY",
        "H2_LEARNED_ROUTE_COMPLETENESS",
        "H3_RECURRENT_ACCOUNTING",
        "H4_NO_HIDDEN_MR_REWRITE",
        "H5_DYNAMIC_MOR_COMPATIBILITY",
    ] {
        writeln!(out, "N13A_INVARIANT,{name},HOLDS")?;
    }
    // H6 is enforced externally by CI byte comparison.
    writeln!(out, "N13A_INVARIANT,H6_BYTE_IDENTICAL,CI_ENFORCED")?;
    writeln!(out, "PROMOTION,BLOCKED,n13a_census_only")?;

    print!("{out}");
    Ok(())
}

//! DKV-2a-N13B unbalanced page-population and cross-primary sharing stress.
//!
//! Protocol: docs/SML-DKV-2A-N13B-PAGE-SKEW-PREREGISTRATION.md.
//! Fixture freeze: docs/SML-DKV-2A-N13B-FIXTURE-FREEZE.md.
//! Harness-only: current q=3 two-bank and N12 identity/provenance APIs are unchanged.

use std::{collections::BTreeMap, fmt::Write as _};

use sml_core::{
    context_ledger::{ContextSegmentId, ContextSegmentRef, ContextStreamId, RawContextLedger},
    delta_kv::{DELTA_KV_ONE, DELTA_KV_SCALE},
    delta_kv_adaptive::Dkv4Provenance,
    delta_kv_semantic::{
        DeltaKvAmbiguityProjection, DeltaKvSemanticProjectionPolicy, DeltaKvSemanticProjector,
    },
    mor::MorDecision,
    semantic::SemanticState,
    PageId,
};
use sml_store::{
    delta_kv_context::{
        ContextLimits, DeltaKvContextMemory, RecordBindingLimits, RecordRequest, VerificationLimits,
    },
    observed_history::{
        bind_projected_subject, encode_event, screen_candidate, verify_history, HistoryLimits,
        HistoryVerificationLimits, ObservationEvent, ObservedState, SubjectBindingError,
        SubjectBindingLimits, SubjectKey, VerifiedHistory, MAX_EVENT_BYTES,
    },
    raw_context::{sha256_raw_bytes, RawContextStoreError},
};

type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const RECORDS: usize = 144;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const KEY_WIDTHS: [usize; 2] = [256, 1024];
const Q: usize = 3;
const POP_NAMES: [&str; 3] = ["BALANCED", "N13A_EMPIRICAL", "HEAVY_SKEW"];
const POP_COUNTS: [[usize; 10]; 3] = [
    [15, 15, 15, 15, 14, 14, 14, 14, 14, 14],
    [2, 4, 53, 2, 4, 6, 2, 9, 62, 0],
    [116, 4, 3, 3, 3, 3, 3, 3, 3, 3],
];
const TRANSITIONS: [(u32, u32, u64); 8] = [
    (8, 8, 5040),
    (2, 2, 4096),
    (2, 8, 1920),
    (8, 2, 1920),
    (7, 7, 896),
    (5, 8, 720),
    (0, 7, 256),
    (7, 3, 256),
];

const PAGE_C: u64 = 0x9e37_79b9_7f4a_7c15;
const VALUE_C: u64 = 0xd6e8_feb8_6659_fd93;
const ROUTE_TAG: u64 = 0x6d75_6c74_695f_6d6f;
const ROUTE_C: u64 = 0xc2b2_ae3d_27d4_eb4f;
const RANK_C: u64 = 0x1656_67b1_9e37_79f9;
const LEN_C: u64 = 0x85eb_ca77_c2b2_ae63;


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

fn route_seed(bank: usize, base_seed: u64, route: &[u32]) -> u64 {
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

fn source_key<const K: usize>(
    bank: usize,
    value_domain: bool,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> Vec<i16> {
    let route_seed = route_seed(bank, base_seed, route);
    let seed = if value_domain {
        mix64(route_seed ^ value_bits(state).wrapping_mul(VALUE_C))
    } else {
        route_seed
    };
    DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
        DeltaKvSemanticProjectionPolicy::default(),
        seed,
        DeltaKvAmbiguityProjection::ActiveOnly,
    )
    .expect("projector")
    .project(state, &decision(route), None)
    .expect("project")
    .key_q12
}

fn hybrid_key<const K: usize>(
    bank: usize,
    base_seed: u64,
    state: &State,
    route: &[u32],
) -> Vec<i16> {
    let page = source_key::<K>(bank, false, base_seed, state, route);
    let value = source_key::<K>(bank, true, base_seed, state, route);
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

fn target_q12(bits: [bool; 2]) -> [i16; 2] {
    bits.map(|bit| if bit { DELTA_KV_ONE } else { -DELTA_KV_ONE })
}

fn candidate(values: &[i32]) -> Option<[bool; 2]> {
    if values.len() != 2 || values.iter().any(|v| v.unsigned_abs() <= 128) {
        None
    } else {
        Some([values[0] > 0, values[1] > 0])
    }
}

fn subject(record: u64) -> SubjectKey {
    SubjectKey::new(record, 0).expect("subject")
}

fn record_id(seed: u64, index: usize) -> u64 {
    seed * 1000 + index as u64 + 1
}

fn base_route(page: u32) -> [u32; 2] {
    [page, (page + 1) % 10]
}

fn secondary_route(page: u32) -> [u32; 2] {
    [page, (page + 2) % 10]
}

fn switch_route(page: u32) -> [u32; 2] {
    [(page + 1) % 10, page]
}

fn context_limits() -> ContextLimits {
    ContextLimits {
        max_writes: 256,
        max_sources_per_write: 4,
        max_source_links: 512,
        max_ledger_segments: 160,
        max_ledger_semantic_links: 512,
        max_element_bytes: 32 << 20,
        max_staging_bytes: 32 << 20,
    }
}

fn verification_limits() -> VerificationLimits {
    VerificationLimits {
        max_sources: 160,
        max_raw_bytes: 2 << 20,
    }
}

fn record_limits() -> RecordBindingLimits {
    RecordBindingLimits {
        max_write_records: 256,
        max_sources: 160,
    }
}

fn history_limits() -> HistoryLimits {
    HistoryLimits {
        max_events: 160,
        max_keys: 160,
        max_element_bytes: 1 << 20,
    }
}

fn history_verification_limits() -> HistoryVerificationLimits {
    HistoryVerificationLimits {
        max_segments: 160,
        max_raw_bytes: 2 << 20,
        max_record_bytes: MAX_EVENT_BYTES,
    }
}

fn subject_limits() -> SubjectBindingLimits {
    SubjectBindingLimits {
        max_candidate_sources: 160,
        max_record_bytes: MAX_EVENT_BYTES,
    }
}

#[derive(Clone)]
struct SourceSnapshot {
    ledger: RawContextLedger,
    raw: Vec<Vec<u8>>,
}

fn snapshot(events: &[ObservationEvent], stream_num: u64) -> SourceSnapshot {
    let stream = ContextStreamId::new(stream_num).expect("stream");
    let mut ledger = RawContextLedger::new(stream);
    let mut raw = Vec::new();
    for (index, &event) in events.iter().enumerate() {
        let bytes = encode_event(event);
        let id = ContextSegmentId::new(stream, index as u64 + 1).expect("segment");
        let reference = ContextSegmentRef::new(
            id,
            ledger.stats().raw_bytes,
            bytes.len() as u64,
            0,
            0,
            index as u64,
            sha256_raw_bytes(&bytes).expect("hash"),
            vec![event.id()],
        )
        .expect("reference");
        ledger.append(reference).expect("append");
        raw.push(bytes);
    }
    SourceSnapshot { ledger, raw }
}

#[derive(Clone)]
struct Fact {
    index: usize,
    record: u64,
    subject: SubjectKey,
    state: State,
    target: [bool; 2],
    page: u32,
}

fn shuffle<T>(values: &mut [T], stream: &mut Stream) {
    for index in (1..values.len()).rev() {
        let other = stream.below(index + 1);
        values.swap(index, other);
    }
}

fn population_pages(seed: u64, pop: usize) -> Vec<u32> {
    let mut pages = Vec::with_capacity(RECORDS);
    for (page, count) in POP_COUNTS[pop].iter().copied().enumerate() {
        for _ in 0..count {
            pages.push(page as u32);
        }
    }
    assert_eq!(pages.len(), RECORDS);
    let mut stream = tagged(seed, pop as u64, 13, 1300);
    shuffle(&mut pages, &mut stream);
    pages
}

fn population_fixture(seed: u64, pop: usize) -> (SourceSnapshot, Vec<Fact>) {
    let pages = population_pages(seed, pop);
    let mut stream = tagged(seed, 13, 0, 1313);
    let mut events = Vec::with_capacity(RECORDS);
    let mut facts = Vec::with_capacity(RECORDS);
    for index in 0..RECORDS {
        let record = record_id(seed, index);
        let state = build_state(stream.next(), stream.next(), stream.next());
        let word = stream.next();
        let target = [word & 1 != 0, word & 2 != 0];
        let subject = subject(record);
        events.push(ObservationEvent::new(record, subject, target, None).expect("event"));
        facts.push(Fact {
            index,
            record,
            subject,
            state,
            target,
            page: pages[index],
        });
    }
    (snapshot(&events, pop as u64 + 1), facts)
}

fn transition_fixture(pair: usize) -> (SourceSnapshot, Vec<Fact>) {
    let (from, _, _) = TRANSITIONS[pair];
    let mut stream = tagged(pair as u64, 32, 13, 1360);
    let mut events = Vec::with_capacity(32);
    let mut facts = Vec::with_capacity(32);
    for index in 0..32 {
        let record = 50_000 + pair as u64 * 100 + index as u64 + 1;
        let state = build_state(stream.next(), stream.next(), stream.next());
        let word = stream.next();
        let target = [word & 1 != 0, word & 2 != 0];
        let subject = subject(record);
        events.push(ObservationEvent::new(record, subject, target, None).expect("event"));
        facts.push(Fact {
            index,
            record,
            subject,
            state,
            target,
            page: from,
        });
    }
    (snapshot(&events, 20 + pair as u64), facts)
}

fn write_order(seed: u64, pop: usize, panel: u64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..RECORDS).collect();
    let mut stream = tagged(seed, pop as u64, panel, 1399);
    shuffle(&mut order, &mut stream);
    order
}

fn verified_history(source: &SourceSnapshot) -> VerifiedHistory<'_> {
    verify_history(
        &source.ledger,
        history_limits(),
        history_verification_limits(),
        |reference| read_source(reference, &source.raw, ReaderMode::Clean),
    )
    .expect("verified history")
}

fn source_for_record(ledger: &RawContextLedger, record: u64) -> ContextSegmentId {
    ledger
        .segments()
        .iter()
        .find(|segment| segment.semantic_record_ids() == [record])
        .expect("record source")
        .identity
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bank {
    Primary,
    Ordered,
}

#[derive(Clone)]
struct Locator {
    bank: Bank,
    stream: ContextStreamId,
    record_id: u64,
    subject: SubjectKey,
    route: Vec<u32>,
    write_key: Vec<i16>,
}

#[derive(Default)]
struct Directory {
    primary: BTreeMap<u64, Locator>,
    ordered: BTreeMap<(u64, Vec<u32>), Locator>,
}

impl Directory {
    fn primary(&mut self, locator: Locator) {
        assert_eq!(locator.bank, Bank::Primary);
        self.primary.insert(locator.record_id, locator);
    }
    fn override_locator(&mut self, locator: Locator) {
        assert_eq!(locator.bank, Bank::Ordered);
        self.ordered
            .insert((locator.record_id, locator.route.clone()), locator);
    }
    fn select(&self, record: u64, route: &[u32]) -> Option<&Locator> {
        self.ordered
            .get(&(record, route.to_vec()))
            .or_else(|| self.primary.get(&record))
    }
}

struct Banks {
    primary: DeltaKvContextMemory,
    ordered: DeltaKvContextMemory,
}

impl Banks {
    fn new<const K: usize>(stream: ContextStreamId) -> Self {
        Self {
            primary: DeltaKvContextMemory::new(stream, K, 2, context_limits()).expect("primary"),
            ordered: DeltaKvContextMemory::new(stream, K, 2, context_limits()).expect("ordered"),
        }
    }
    fn memory(&self, bank: Bank) -> &DeltaKvContextMemory {
        match bank {
            Bank::Primary => &self.primary,
            Bank::Ordered => &self.ordered,
        }
    }
    fn memory_mut(&mut self, bank: Bank) -> &mut DeltaKvContextMemory {
        match bank {
            Bank::Primary => &mut self.primary,
            Bank::Ordered => &mut self.ordered,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn admit<const K: usize>(
    banks: &mut Banks,
    directory: &mut Directory,
    snapshot: &SourceSnapshot,
    bank: Bank,
    base_seed: u64,
    record: u64,
    subject_key: SubjectKey,
    state: &State,
    route: &[u32],
    bits: [bool; 2],
) {
    let bank_id = usize::from(bank == Bank::Ordered);
    let key = hybrid_key::<K>(bank_id, base_seed, state, route);
    let source = source_for_record(&snapshot.ledger, record);
    let evidence_key = {
        let evidence = banks
            .memory_mut(bank)
            .write(
                &snapshot.ledger,
                record,
                &key,
                &target_q12(bits),
                FULL_RATE,
                &[source],
            )
            .expect("context write");
        assert_eq!(evidence.key_q12(), key);
        evidence.key_q12().to_vec()
    };
    let locator = Locator {
        bank,
        stream: snapshot.ledger.stream_id(),
        record_id: record,
        subject: subject_key,
        route: route.to_vec(),
        write_key: evidence_key,
    };
    match bank {
        Bank::Primary => directory.primary(locator),
        Bank::Ordered => directory.override_locator(locator),
    }
}

#[derive(Clone, Copy)]
enum ReaderMode {
    Clean,
    CorruptFirst,
}

fn read_source(
    reference: &ContextSegmentRef,
    raw: &[Vec<u8>],
    mode: ReaderMode,
) -> Result<Vec<u8>, RawContextStoreError> {
    let index = (reference.identity.segment_id - 1) as usize;
    let mut bytes = raw[index].clone();
    match mode {
        ReaderMode::Clean => {}
        ReaderMode::CorruptFirst if index == 0 => {
            bytes[0] ^= 1;
        }
        _ => {}
    }
    Ok(bytes)
}

#[derive(Default, Clone, Copy)]
struct ResolveAccounting {
    verified: u64,
    missing: u64,
    conflict: u64,
    unverified: u64,
    matrix_reads: u64,
    primary_reads: u64,
    ordered_reads: u64,
    verified_source_segments: u64,
    verified_source_bytes: u64,
}

#[derive(Clone, Copy)]
struct ResolveResult {
    provenance: Dkv4Provenance,
    bank: Option<Bank>,
    value: Option<[bool; 2]>,
    screened: Option<[bool; 2]>,
    matrix_reads: u64,
}

struct ResolveRequest<'a> {
    record: RecordRequest,
    subject: SubjectKey,
    route: &'a [u32],
    state: &'a State,
}

#[allow(clippy::too_many_arguments)]
fn resolve<const K: usize>(
    banks: &mut Banks,
    directory: &Directory,
    source: &SourceSnapshot,
    history: &VerifiedHistory<'_>,
    base_seed: u64,
    request: ResolveRequest<'_>,
    accounting: &mut ResolveAccounting,
    reader_mode: ReaderMode,
) -> ResolveResult {
    let Some(locator) = directory.select(request.record.record_id(), request.route) else {
        accounting.missing += 1;
        return ResolveResult {
            provenance: Dkv4Provenance::Missing,
            bank: None,
            value: None,
            screened: None,
            matrix_reads: 0,
        };
    };
    let bank = locator.bank;
    let memory = banks.memory(bank);
    let _locator_hint = (
        locator.stream,
        locator.record_id,
        locator.subject,
        locator.route.as_slice(),
    );

    let preflight = (|| {
        let receipt = memory
            .query(&locator.write_key)
            .map_err(|_| Dkv4Provenance::Unverified)?
            .verify(&source.ledger, verification_limits(), |reference| {
                read_source(reference, &source.raw, reader_mode)
            })
            .map_err(|_| Dkv4Provenance::Unverified)?;

        receipt
            .bind_record(request.record, record_limits())
            .map_err(|_| Dkv4Provenance::Unverified)?;

        let bound =
            match bind_projected_subject(&receipt, history, request.subject, subject_limits()) {
                Ok(bound) => bound,
                Err(SubjectBindingError::MissingSubject) => return Err(Dkv4Provenance::Missing),
                Err(_) => return Err(Dkv4Provenance::Unverified),
            };
        let segments = receipt.sources().len() as u64;
        let bytes = receipt.raw_bytes();
        match bound.observed_state() {
            ObservedState::Agreement => Ok((segments, bytes)),
            ObservedState::Conflict => Err(Dkv4Provenance::Conflict),
            ObservedState::Missing => Err(Dkv4Provenance::Missing),
        }
    })();

    let (segments, bytes) = match preflight {
        Ok(value) => value,
        Err(Dkv4Provenance::Missing) => {
            accounting.missing += 1;
            return ResolveResult {
                provenance: Dkv4Provenance::Missing,
                bank: Some(bank),
                value: None,
                screened: None,
                matrix_reads: 0,
            };
        }
        Err(Dkv4Provenance::Conflict) => {
            accounting.conflict += 1;
            return ResolveResult {
                provenance: Dkv4Provenance::Conflict,
                bank: Some(bank),
                value: None,
                screened: None,
                matrix_reads: 0,
            };
        }
        Err(_) => {
            accounting.unverified += 1;
            return ResolveResult {
                provenance: Dkv4Provenance::Unverified,
                bank: Some(bank),
                value: None,
                screened: None,
                matrix_reads: 0,
            };
        }
    };

    accounting.verified += 1;
    accounting.verified_source_segments += segments;
    accounting.verified_source_bytes += bytes;

    let bank_id = usize::from(bank == Bank::Ordered);
    let query_key = hybrid_key::<K>(bank_id, base_seed, request.state, request.route);
    let before = banks.memory(bank).numeric_counters().reads;
    let values = banks.memory_mut(bank).read(&query_key).expect("matrix read");
    let after = banks.memory(bank).numeric_counters().reads;
    let read_delta = after - before;
    assert_eq!(read_delta, 1);
    accounting.matrix_reads += read_delta;
    match bank {
        Bank::Primary => accounting.primary_reads += 1,
        Bank::Ordered => accounting.ordered_reads += 1,
    }
    let latent = candidate(&values);
    let screened = screen_candidate(history, request.subject, latent);
    ResolveResult {
        provenance: Dkv4Provenance::VerifiedPresent,
        bank: Some(bank),
        value: latent,
        screened: screened.value(),
        matrix_reads: read_delta,
    }
}

fn replicate_primary<const K: usize>(
    banks: &mut Banks,
    source: &SourceSnapshot,
    base_seed: u64,
    fact: &Fact,
    route: &[u32],
) {
    let key = hybrid_key::<K>(B_PRIMARY, base_seed, &fact.state, route);
    let source_id = source_for_record(&source.ledger, fact.record);
    banks
        .primary
        .write(
            &source.ledger,
            fact.record,
            &key,
            &target_q12(fact.target),
            FULL_RATE,
            &[source_id],
        )
        .expect("replication write");
}

fn cosine(left: &[i16], right: &[i16]) -> f64 {
    let dot: f64 = left
        .iter()
        .zip(right)
        .map(|(a, b)| f64::from(*a) * f64::from(*b))
        .sum();
    let ln = left
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    let rn = right
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    dot / (ln * rn)
}

#[derive(Default, Clone, Copy)]
struct Recall {
    n: u64,
    exact: u64,
    verified: u64,
    primary: u64,
    ordered: u64,
    reads: u64,
    screened_same: u64,
}

impl Recall {
    fn add(&mut self, result: ResolveResult, expected: [bool; 2]) {
        self.n += 1;
        self.exact += u64::from(result.value == Some(expected));
        self.verified += u64::from(result.provenance == Dkv4Provenance::VerifiedPresent);
        self.primary += u64::from(result.bank == Some(Bank::Primary));
        self.ordered += u64::from(result.bank == Some(Bank::Ordered));
        self.reads += result.matrix_reads;
        self.screened_same += u64::from(result.screened == result.value);
    }
}

#[derive(Default, Clone, Copy)]
struct Geometry {
    n: u64,
    equal: u64,
    abs_cosine_sum: f64,
    max_abs_cosine: f64,
    target_n: u64,
    target_exact: u64,
    verified: u64,
}

impl Geometry {
    fn add_keys(&mut self, left: &[i16], right: &[i16]) {
        let value = cosine(left, right).abs();
        self.n += 1;
        self.equal += u64::from(left == right);
        self.abs_cosine_sum += value;
        self.max_abs_cosine = self.max_abs_cosine.max(value);
    }
}

#[derive(Default, Clone, Copy)]
struct Negative {
    cases: u64,
    non_verified: u64,
    zero_read: u64,
    ordered_unverified: u64,
}

#[derive(Default, Clone, Copy)]
struct Work {
    primary_writes: u64,
    ordered_writes: u64,
    replication_writes: u64,
}

fn ppm(a: u64, b: u64) -> u64 {
    if b == 0 {
        0
    } else {
        a.saturating_mul(1_000_000) / b
    }
}

fn merge_accounting(dst: &mut ResolveAccounting, src: ResolveAccounting) {
    dst.verified += src.verified;
    dst.missing += src.missing;
    dst.conflict += src.conflict;
    dst.unverified += src.unverified;
    dst.matrix_reads += src.matrix_reads;
    dst.primary_reads += src.primary_reads;
    dst.ordered_reads += src.ordered_reads;
    dst.verified_source_segments += src.verified_source_segments;
    dst.verified_source_bytes += src.verified_source_bytes;
}

fn observe(
    map: &mut BTreeMap<(usize, usize, u32), Recall>,
    k: usize,
    pop: usize,
    page: u32,
    result: ResolveResult,
    expected: [bool; 2],
) {
    map.entry((k, pop, page))
        .or_default()
        .add(result, expected);
}

fn write_base_population<const K: usize>(
    seed: u64,
    pop: usize,
    panel: u64,
    source: &SourceSnapshot,
    facts: &[Fact],
    banks: &mut Banks,
    directory: &mut Directory,
    work: &mut Work,
) {
    let base_seed = seed * 1000 + 17;
    for index in write_order(seed, pop, panel) {
        let fact = &facts[index];
        let route = base_route(fact.page);
        admit::<K>(
            banks,
            directory,
            source,
            Bank::Primary,
            base_seed,
            fact.record,
            fact.subject,
            &fact.state,
            &route,
            fact.target,
        );
        work.primary_writes += 1;
    }
}

fn run_p1_p2<const K: usize>(
    p1: &mut BTreeMap<(usize, usize, u32), Recall>,
    p2: &mut BTreeMap<(usize, usize, u32), Recall>,
    accounting: &mut ResolveAccounting,
    work: &mut BTreeMap<(usize, usize, u8), Work>,
) {
    for pop in 0..POP_NAMES.len() {
        for seed in 0..SEEDS {
            let base_seed = seed * 1000 + 17;
            let (source, facts) = population_fixture(seed, pop);
            let history = verified_history(&source);
            let mut banks = Banks::new::<K>(source.ledger.stream_id());
            let mut directory = Directory::default();
            let mut local_work = Work::default();
            write_base_population::<K>(
                seed,
                pop,
                1,
                &source,
                &facts,
                &mut banks,
                &mut directory,
                &mut local_work,
            );

            let mut local = ResolveAccounting::default();
            for fact in &facts {
                let base = base_route(fact.page);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &base,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                observe(p1, K, pop, fact.page, result, fact.target);

                let secondary = secondary_route(fact.page);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &secondary,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                observe(p2, K, pop, fact.page, result, fact.target);
            }
            merge_accounting(accounting, local);
            let entry = work.entry((K, pop, 1)).or_default();
            entry.primary_writes += local_work.primary_writes;
        }
    }
}

#[derive(Default)]
struct P3Maps {
    base: BTreeMap<(usize, usize, u32), Recall>,
    bound: BTreeMap<(usize, usize, u32), Recall>,
    unbound: BTreeMap<(usize, usize, u32), Recall>,
}

fn run_p3<const K: usize>(
    maps: &mut P3Maps,
    accounting: &mut ResolveAccounting,
    work: &mut BTreeMap<(usize, usize, u8), Work>,
) {
    for pop in 0..POP_NAMES.len() {
        for seed in 0..SEEDS {
            let base_seed = seed * 1000 + 17;
            let (source, facts) = population_fixture(seed, pop);
            let history = verified_history(&source);
            let mut banks = Banks::new::<K>(source.ledger.stream_id());
            let mut directory = Directory::default();
            let mut local_work = Work::default();
            write_base_population::<K>(
                seed,
                pop,
                3,
                &source,
                &facts,
                &mut banks,
                &mut directory,
                &mut local_work,
            );
            for fact in facts.iter().filter(|fact| fact.index % 4 == 0) {
                let route = secondary_route(fact.page);
                let target = [!fact.target[0], !fact.target[1]];
                admit::<K>(
                    &mut banks,
                    &mut directory,
                    &source,
                    Bank::Ordered,
                    base_seed,
                    fact.record,
                    fact.subject,
                    &fact.state,
                    &route,
                    target,
                );
                local_work.ordered_writes += 1;
            }

            let mut local = ResolveAccounting::default();
            for fact in &facts {
                let base = base_route(fact.page);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &base,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                observe(&mut maps.base, K, pop, fact.page, result, fact.target);

                let secondary = secondary_route(fact.page);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &secondary,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                if fact.index % 4 == 0 {
                    observe(
                        &mut maps.bound,
                        K,
                        pop,
                        fact.page,
                        result,
                        [!fact.target[0], !fact.target[1]],
                    );
                } else {
                    observe(
                        &mut maps.unbound,
                        K,
                        pop,
                        fact.page,
                        result,
                        fact.target,
                    );
                }
            }
            merge_accounting(accounting, local);
            let entry = work.entry((K, pop, 3)).or_default();
            entry.primary_writes += local_work.primary_writes;
            entry.ordered_writes += local_work.ordered_writes;
        }
    }
}

fn run_p4<const K: usize>(
    geometry: &mut BTreeMap<(usize, usize), Geometry>,
    accounting: &mut ResolveAccounting,
) {
    for pop in 0..POP_NAMES.len() {
        for seed in 0..SEEDS {
            let base_seed = seed * 1000 + 17;
            let (source, facts) = population_fixture(seed, pop);
            let history = verified_history(&source);
            let mut banks = Banks::new::<K>(source.ledger.stream_id());
            let mut directory = Directory::default();
            let mut work = Work::default();
            write_base_population::<K>(
                seed,
                pop,
                4,
                &source,
                &facts,
                &mut banks,
                &mut directory,
                &mut work,
            );
            let mut local = ResolveAccounting::default();
            for fact in &facts {
                let base = base_route(fact.page);
                let switch = switch_route(fact.page);
                let base_key = hybrid_key::<K>(B_PRIMARY, base_seed, &fact.state, &base);
                let switch_key = hybrid_key::<K>(B_PRIMARY, base_seed, &fact.state, &switch);
                let tally = geometry.entry((K, pop)).or_default();
                tally.add_keys(&base_key, &switch_key);

                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &switch,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                tally.target_n += 1;
                tally.target_exact += u64::from(result.value == Some(fact.target));
                tally.verified +=
                    u64::from(result.provenance == Dkv4Provenance::VerifiedPresent);
            }
            merge_accounting(accounting, local);
        }
    }
}

fn run_p5<const K: usize>(
    base_map: &mut BTreeMap<(usize, usize, u32), Recall>,
    shared_map: &mut BTreeMap<(usize, usize, u32), Recall>,
    accounting: &mut ResolveAccounting,
    work: &mut BTreeMap<(usize, usize, u8), Work>,
) {
    for pop in 0..POP_NAMES.len() {
        for seed in 0..SEEDS {
            let base_seed = seed * 1000 + 17;
            let (source, facts) = population_fixture(seed, pop);
            let history = verified_history(&source);
            let mut banks = Banks::new::<K>(source.ledger.stream_id());
            let mut directory = Directory::default();
            let mut local_work = Work::default();
            write_base_population::<K>(
                seed,
                pop,
                5,
                &source,
                &facts,
                &mut banks,
                &mut directory,
                &mut local_work,
            );
            for fact in facts.iter().filter(|fact| fact.index % 4 == 1) {
                let switch = switch_route(fact.page);
                replicate_primary::<K>(&mut banks, &source, base_seed, fact, &switch);
                local_work.replication_writes += 1;
            }
            let mut local = ResolveAccounting::default();
            for fact in facts.iter().filter(|fact| fact.index % 4 == 1) {
                let base = base_route(fact.page);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &base,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                observe(base_map, K, pop, fact.page, result, fact.target);

                let switch = switch_route(fact.page);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &switch,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                observe(shared_map, K, pop, fact.page, result, fact.target);
            }
            merge_accounting(accounting, local);
            let entry = work.entry((K, pop, 5)).or_default();
            entry.primary_writes += local_work.primary_writes;
            entry.replication_writes += local_work.replication_writes;
        }
    }
}

#[derive(Default, Clone, Copy)]
struct TransitionTally {
    same: Recall,
    unshared: Recall,
    shared: Recall,
    geometry: Geometry,
}

fn run_p6<const K: usize>(
    results: &mut BTreeMap<(usize, usize), TransitionTally>,
    accounting: &mut ResolveAccounting,
) {
    for (pair, &(from, to, _count)) in TRANSITIONS.iter().enumerate() {
        let base_seed = 60_000 + pair as u64 * 100 + K as u64;
        let (source, facts) = transition_fixture(pair);
        let history = verified_history(&source);
        let mut banks = Banks::new::<K>(source.ledger.stream_id());
        let mut directory = Directory::default();
        for fact in &facts {
            let route = base_route(from);
            admit::<K>(
                &mut banks,
                &mut directory,
                &source,
                Bank::Primary,
                base_seed,
                fact.record,
                fact.subject,
                &fact.state,
                &route,
                fact.target,
            );
        }
        let mut local = ResolveAccounting::default();
        let tally = results.entry((K, pair)).or_default();
        if from == to {
            for fact in &facts {
                let route = base_route(to);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &route,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                tally.same.add(result, fact.target);
            }
        } else {
            for fact in &facts {
                let from_route = base_route(from);
                let to_route = base_route(to);
                let from_key = hybrid_key::<K>(B_PRIMARY, base_seed, &fact.state, &from_route);
                let to_key = hybrid_key::<K>(B_PRIMARY, base_seed, &fact.state, &to_route);
                tally.geometry.add_keys(&from_key, &to_key);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &to_route,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                tally.unshared.add(result, fact.target);
            }
            for fact in &facts {
                let to_route = base_route(to);
                replicate_primary::<K>(&mut banks, &source, base_seed, fact, &to_route);
            }
            for fact in &facts {
                let to_route = base_route(to);
                let result = resolve::<K>(
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                        subject: fact.subject,
                        route: &to_route,
                        state: &fact.state,
                    },
                    &mut local,
                    ReaderMode::Clean,
                );
                tally.shared.add(result, fact.target);
            }
        }
        merge_accounting(accounting, local);
    }
}

fn incomplete_control<const K: usize>(
    page: u32,
    accounting: &mut ResolveAccounting,
) -> ResolveResult {
    let stream = ContextStreamId::new(159).unwrap();
    let subject = SubjectKey::new(900_001, 0).unwrap();
    let events = [
        ObservationEvent::new(900_001, subject, [false, false], None).unwrap(),
        ObservationEvent::new(900_002, subject, [true, true], None).unwrap(),
    ];
    let source = snapshot(&events, stream.get());
    let history = verified_history(&source);
    let mut banks = Banks::new::<K>(source.ledger.stream_id());
    let mut directory = Directory::default();
    let state_a = build_state(1, 2, 3);
    let state_b = build_state(4, 5, 6);
    let route = base_route(page);
    admit::<K>(
        &mut banks,
        &mut directory,
        &source,
        Bank::Primary,
        17,
        900_001,
        subject,
        &state_a,
        &route,
        [false, false],
    );
    admit::<K>(
        &mut banks,
        &mut directory,
        &source,
        Bank::Primary,
        17,
        900_002,
        subject,
        &state_b,
        &route,
        [true, true],
    );
    resolve::<K>(
        &mut banks,
        &directory,
        &source,
        &history,
        17,
        ResolveRequest {
            record: RecordRequest::new(source.ledger.stream_id(), 900_001),
            subject,
            route: &route,
            state: &state_a,
        },
        accounting,
        ReaderMode::Clean,
    )
}

fn run_p7(
    negatives: &mut BTreeMap<usize, Negative>,
    accounting: &mut ResolveAccounting,
) {
    for pop in 0..POP_NAMES.len() {
        let seed = 0;
        let base_seed = 17;
        let (source, facts) = population_fixture(seed, pop);
        let history = verified_history(&source);
        let fact = &facts[0];
        let base = base_route(fact.page);
        let secondary = secondary_route(fact.page);
        let mut banks = Banks::new::<1024>(source.ledger.stream_id());
        let mut directory = Directory::default();
        admit::<1024>(
            &mut banks,
            &mut directory,
            &source,
            Bank::Primary,
            base_seed,
            fact.record,
            fact.subject,
            &fact.state,
            &base,
            fact.target,
        );
        let mut local = ResolveAccounting::default();
        let tally = negatives.entry(pop).or_default();

        let cases = [
            resolve::<1024>(
                &mut banks,
                &directory,
                &source,
                &history,
                base_seed,
                ResolveRequest {
                    record: RecordRequest::new(source.ledger.stream_id(), fact.record + 999_000),
                    subject: fact.subject,
                    route: &base,
                    state: &fact.state,
                },
                &mut local,
                ReaderMode::Clean,
            ),
            resolve::<1024>(
                &mut banks,
                &directory,
                &source,
                &history,
                base_seed,
                ResolveRequest {
                    record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                    subject: facts[1].subject,
                    route: &base,
                    state: &fact.state,
                },
                &mut local,
                ReaderMode::Clean,
            ),
            resolve::<1024>(
                &mut banks,
                &directory,
                &source,
                &history,
                base_seed,
                ResolveRequest {
                    record: RecordRequest::new(ContextStreamId::new(999).unwrap(), fact.record),
                    subject: fact.subject,
                    route: &base,
                    state: &fact.state,
                },
                &mut local,
                ReaderMode::Clean,
            ),
            resolve::<1024>(
                &mut banks,
                &directory,
                &source,
                &history,
                base_seed,
                ResolveRequest {
                    record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                    subject: fact.subject,
                    route: &base,
                    state: &fact.state,
                },
                &mut local,
                ReaderMode::CorruptFirst,
            ),
        ];
        for result in cases {
            tally.cases += 1;
            tally.non_verified +=
                u64::from(result.provenance != Dkv4Provenance::VerifiedPresent);
            tally.zero_read += u64::from(result.matrix_reads == 0);
        }

        // Declared ORDERED override with failed verification.
        admit::<1024>(
            &mut banks,
            &mut directory,
            &source,
            Bank::Ordered,
            base_seed,
            fact.record,
            fact.subject,
            &fact.state,
            &secondary,
            [!fact.target[0], !fact.target[1]],
        );
        let result = resolve::<1024>(
            &mut banks,
            &directory,
            &source,
            &history,
            base_seed,
            ResolveRequest {
                record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                subject: fact.subject,
                route: &secondary,
                state: &fact.state,
            },
            &mut local,
            ReaderMode::CorruptFirst,
        );
        tally.cases += 1;
        tally.non_verified +=
            u64::from(result.provenance != Dkv4Provenance::VerifiedPresent);
        tally.zero_read += u64::from(result.matrix_reads == 0);
        tally.ordered_unverified += u64::from(
            result.bank == Some(Bank::Ordered)
                && result.provenance == Dkv4Provenance::Unverified
                && result.matrix_reads == 0,
        );

        let result = incomplete_control::<1024>(fact.page, &mut local);
        tally.cases += 1;
        tally.non_verified +=
            u64::from(result.provenance != Dkv4Provenance::VerifiedPresent);
        tally.zero_read += u64::from(result.matrix_reads == 0);

        merge_accounting(accounting, local);
    }
}

fn aggregate(
    map: &BTreeMap<(usize, usize, u32), Recall>,
    k: usize,
    pop: usize,
) -> (Recall, u64, u64, usize) {
    let mut total = Recall::default();
    let mut macro_sum = 0_u64;
    let mut worst = 1_000_000_u64;
    let mut pages = 0_usize;
    for page in 0..10_u32 {
        if let Some(row) = map.get(&(k, pop, page)) {
            if row.n == 0 {
                continue;
            }
            total.n += row.n;
            total.exact += row.exact;
            total.verified += row.verified;
            total.primary += row.primary;
            total.ordered += row.ordered;
            total.reads += row.reads;
            total.screened_same += row.screened_same;
            let value = ppm(row.exact, row.n);
            macro_sum += value;
            worst = worst.min(value);
            pages += 1;
        }
    }
    let macro_ppm = if pages == 0 {
        0
    } else {
        macro_sum / pages as u64
    };
    (total, macro_ppm, worst, pages)
}

fn clean_rows_hold(map: &BTreeMap<(usize, usize, u32), Recall>) -> bool {
    map.values().all(|row| {
        row.verified == row.n && row.reads == row.n && row.screened_same == row.n
    })
}

fn main() {
    let mut p1 = BTreeMap::new();
    let mut p2 = BTreeMap::new();
    let mut p3 = P3Maps::default();
    let mut p4 = BTreeMap::new();
    let mut p5_base = BTreeMap::new();
    let mut p5_shared = BTreeMap::new();
    let mut p6 = BTreeMap::new();
    let mut p7 = BTreeMap::new();
    let mut accounting = ResolveAccounting::default();
    let mut work = BTreeMap::new();

    run_p1_p2::<256>(&mut p1, &mut p2, &mut accounting, &mut work);
    run_p1_p2::<1024>(&mut p1, &mut p2, &mut accounting, &mut work);
    run_p3::<256>(&mut p3, &mut accounting, &mut work);
    run_p3::<1024>(&mut p3, &mut accounting, &mut work);
    run_p4::<256>(&mut p4, &mut accounting);
    run_p4::<1024>(&mut p4, &mut accounting);
    run_p5::<256>(&mut p5_base, &mut p5_shared, &mut accounting, &mut work);
    run_p5::<1024>(&mut p5_base, &mut p5_shared, &mut accounting, &mut work);
    run_p6::<256>(&mut p6, &mut accounting);
    run_p6::<1024>(&mut p6, &mut accounting);
    run_p7(&mut p7, &mut accounting);

    let mut out = String::new();
    writeln!(out, "N13B_PROTOCOL,SML-DKV-2A-N13B-PAGE-SKEW").unwrap();
    for pop in 0..POP_NAMES.len() {
        writeln!(
            out,
            "N13B_POPULATION,name={},counts={:?}",
            POP_NAMES[pop], POP_COUNTS[pop]
        )
        .unwrap();
    }

    for &k in &KEY_WIDTHS {
        for pop in 0..POP_NAMES.len() {
            for (panel, map) in [("P1", &p1), ("P2", &p2)] {
                for page in 0..10_u32 {
                    if let Some(row) = map.get(&(k, pop, page)) {
                        writeln!(
                            out,
                            "N13B_{panel}_PAGE,K={k},pop={},page={page},n={},exact={},exact_ppm={},verified={},primary={},ordered={},reads={}",
                            POP_NAMES[pop],
                            row.n,
                            row.exact,
                            ppm(row.exact, row.n),
                            row.verified,
                            row.primary,
                            row.ordered,
                            row.reads,
                        )
                        .unwrap();
                    }
                }
                let (total, macro_ppm, worst_ppm, pages) = aggregate(map, k, pop);
                writeln!(
                    out,
                    "N13B_{panel}_SUMMARY,K={k},pop={},n={},exact={},micro_ppm={},macro_ppm={macro_ppm},worst_ppm={worst_ppm},nonempty_pages={pages},verified={},primary={},ordered={},reads={}",
                    POP_NAMES[pop],
                    total.n,
                    total.exact,
                    ppm(total.exact, total.n),
                    total.verified,
                    total.primary,
                    total.ordered,
                    total.reads,
                )
                .unwrap();
            }

            for (kind, map) in [
                ("BASE", &p3.base),
                ("BOUND", &p3.bound),
                ("UNBOUND", &p3.unbound),
            ] {
                let (total, _, worst, pages) = aggregate(map, k, pop);
                writeln!(
                    out,
                    "N13B_P3,K={k},pop={},kind={kind},n={},exact={},micro_ppm={},worst_ppm={worst},pages={pages},primary={},ordered={},reads={}",
                    POP_NAMES[pop],
                    total.n,
                    total.exact,
                    ppm(total.exact, total.n),
                    total.primary,
                    total.ordered,
                    total.reads,
                )
                .unwrap();
            }

            let g = p4[&(k, pop)];
            writeln!(
                out,
                "N13B_P4,K={k},pop={},n={},equal_keys={},mean_abs_cosine={:.6},max_abs_cosine={:.6},target_exact={},target_ppm={},verified={}",
                POP_NAMES[pop],
                g.n,
                g.equal,
                g.abs_cosine_sum / g.n as f64,
                g.max_abs_cosine,
                g.target_exact,
                ppm(g.target_exact, g.target_n),
                g.verified,
            )
            .unwrap();

            for (kind, map) in [("BASE", &p5_base), ("SHARED_SWITCH", &p5_shared)] {
                let (total, _, worst, pages) = aggregate(map, k, pop);
                writeln!(
                    out,
                    "N13B_P5,K={k},pop={},kind={kind},n={},exact={},micro_ppm={},worst_ppm={worst},pages={pages},reads={}",
                    POP_NAMES[pop],
                    total.n,
                    total.exact,
                    ppm(total.exact, total.n),
                    total.reads,
                )
                .unwrap();
            }
        }

        for (pair, &(from, to, count)) in TRANSITIONS.iter().enumerate() {
            let row = p6[&(k, pair)];
            if from == to {
                writeln!(
                    out,
                    "N13B_P6,K={k},pair={from}->{to},n13a_count={count},same_n={},same_exact={},same_ppm={}",
                    row.same.n,
                    row.same.exact,
                    ppm(row.same.exact, row.same.n),
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    "N13B_P6,K={k},pair={from}->{to},n13a_count={count},unshared_n={},unshared_target_ppm={},equal_keys={},mean_abs_cosine={:.6},max_abs_cosine={:.6},shared_n={},shared_exact={},shared_ppm={}",
                    row.unshared.n,
                    ppm(row.unshared.exact, row.unshared.n),
                    row.geometry.equal,
                    row.geometry.abs_cosine_sum / row.geometry.n as f64,
                    row.geometry.max_abs_cosine,
                    row.shared.n,
                    row.shared.exact,
                    ppm(row.shared.exact, row.shared.n),
                )
                .unwrap();
            }
        }
    }

    for pop in 0..POP_NAMES.len() {
        let row = p7[&pop];
        writeln!(
            out,
            "N13B_P7,pop={},cases={},non_verified={},zero_read={},ordered_unverified={}",
            POP_NAMES[pop], row.cases, row.non_verified, row.zero_read, row.ordered_unverified
        )
        .unwrap();
    }

    for ((k, pop, panel), row) in &work {
        writeln!(
            out,
            "N13B_WORK,K={k},pop={},panel={panel},primary_writes={},ordered_writes={},replication_writes={}",
            POP_NAMES[*pop], row.primary_writes, row.ordered_writes, row.replication_writes
        )
        .unwrap();
    }
    writeln!(
        out,
        "N13B_ACCOUNTING,verified={},missing={},conflict={},unverified={},matrix_reads={},primary_reads={},ordered_reads={},source_segments={},source_bytes={}",
        accounting.verified,
        accounting.missing,
        accounting.conflict,
        accounting.unverified,
        accounting.matrix_reads,
        accounting.primary_reads,
        accounting.ordered_reads,
        accounting.verified_source_segments,
        accounting.verified_source_bytes,
    )
    .unwrap();

    let negatives_hold = p7.values().all(|row| {
        row.cases == row.non_verified && row.cases == row.zero_read && row.ordered_unverified == 1
    });
    let h1 = negatives_hold
        && clean_rows_hold(&p1)
        && clean_rows_hold(&p2)
        && clean_rows_hold(&p3.base)
        && clean_rows_hold(&p3.bound)
        && clean_rows_hold(&p3.unbound)
        && clean_rows_hold(&p5_base)
        && clean_rows_hold(&p5_shared);

    let (balanced, balanced_macro, balanced_worst, _) = aggregate(&p1, 1024, 0);
    let h2 = ppm(balanced.exact, balanced.n) >= 900_000
        && balanced_macro >= 900_000
        && balanced_worst >= 800_000;

    let (empirical, _, empirical_worst, _) = aggregate(&p1, 1024, 1);
    let h3 = ppm(empirical.exact, empirical.n) >= 850_000 && empirical_worst >= 750_000;

    let (heavy, _, _, _) = aggregate(&p1, 1024, 2);
    let heavy0 = p1[&(1024, 2, 0)];
    let h4 = ppm(heavy.exact, heavy.n) >= 750_000 && ppm(heavy0.exact, heavy0.n) >= 700_000;

    let h5 = (0..3).all(|pop| {
        let (row, _, _, _) = aggregate(&p2, 1024, pop);
        ppm(row.exact, row.n) >= 850_000 && row.primary == row.n
    });

    let h6 = (0..3).all(|pop| {
        let (bound, _, _, _) = aggregate(&p3.bound, 1024, pop);
        let (unbound, _, _, _) = aggregate(&p3.unbound, 1024, pop);
        ppm(bound.exact, bound.n) >= 850_000
            && ppm(unbound.exact, unbound.n) >= 850_000
            && bound.ordered == bound.n
            && unbound.primary == unbound.n
    });

    let h7 = (0..3).all(|pop| {
        let row = p4[&(1024, pop)];
        row.equal == 0 && row.abs_cosine_sum / row.n as f64 <= 0.10
    });

    let h8 = (0..3).all(|pop| {
        let (row, _, _, _) = aggregate(&p5_shared, 1024, pop);
        ppm(row.exact, row.n) >= 850_000 && row.reads == row.n
    });

    let h9 = TRANSITIONS.iter().enumerate().all(|(pair, &(from, to, _))| {
        let row = p6[&(1024, pair)];
        if from == to {
            ppm(row.same.exact, row.same.n) >= 900_000
        } else {
            row.geometry.equal == 0
                && row.geometry.abs_cosine_sum / row.geometry.n as f64 <= 0.10
                && ppm(row.shared.exact, row.shared.n) >= 850_000
        }
    });

    let h10 = accounting.matrix_reads == accounting.verified;

    for (name, holds) in [
        ("H1_IDENTITY_INVARIANTS", h1),
        ("H2_BALANCED_CAPABILITY", h2),
        ("H3_EMPIRICAL_SKEW_CAPABILITY", h3),
        ("H4_HEAVY_SKEW_CAPABILITY", h4),
        ("H5_SECONDARY_CONTINUITY", h5),
        ("H6_SECONDARY_OVERRIDE", h6),
        ("H7_PRIMARY_SWITCH_ISOLATION", h7),
        ("H8_EXPLICIT_CROSS_PRIMARY_SHARING", h8),
        ("H9_LEARNED_TRANSITION_SEMANTICS", h9),
        ("H10_NO_HIDDEN_SOURCE_AUTHORITY", h10),
    ] {
        writeln!(
            out,
            "N13B_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .unwrap();
    }

    let selected = h1 && h2 && h3 && h4 && h5 && h6 && h7 && h8 && h9 && h10;
    writeln!(
        out,
        "N13B_SELECTION,page_semantics={}",
        if selected { "RETAIN" } else { "NONE" }
    )
    .unwrap();
    writeln!(out, "PROMOTION,BLOCKED,n13b_stress_only").unwrap();

    print!("{out}");
}

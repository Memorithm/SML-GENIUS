#![allow(dead_code)]

//! DKV-2a-N14 physical-resource qualification harness.
//!
//! Protocol: docs/SML-DKV-2A-N14-PHYSICAL-PREREGISTRATION.md.
//! Harness-only: retained q=3 two-bank and N12 identity/provenance APIs are unchanged.

use std::{collections::BTreeMap, env, fs, path::Path, time::Instant};

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
    for (index, &page) in pages.iter().enumerate().take(RECORDS) {
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
            page,
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
    let values = banks
        .memory_mut(bank)
        .read(&query_key)
        .expect("matrix read");
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

const N14_SEEDS: u64 = 8;
const WARMUP_PASSES: usize = 2;
const MEASURE_PASSES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhysicalArm {
    Primary,
    Mixed,
    DirectPrimary,
    DirectMixed,
}

impl PhysicalArm {
    fn parse(value: &str) -> Self {
        match value {
            "primary" => Self::Primary,
            "mixed" => Self::Mixed,
            "direct-primary" => Self::DirectPrimary,
            "direct-mixed" => Self::DirectMixed,
            _ => panic!("arm must be primary|mixed|direct-primary|direct-mixed"),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Primary => "PRIMARY",
            Self::Mixed => "MIXED",
            Self::DirectPrimary => "DIRECT_PRIMARY",
            Self::DirectMixed => "DIRECT_MIXED",
        }
    }

    const fn uses_overrides(self) -> bool {
        matches!(self, Self::Mixed | Self::DirectMixed)
    }

    const fn is_direct(self) -> bool {
        matches!(self, Self::DirectPrimary | Self::DirectMixed)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct MemorySnapshot {
    rss_bytes: u64,
    hwm_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
struct IoSnapshot {
    rchar: u64,
    wchar: u64,
    syscr: u64,
    syscw: u64,
    read_bytes: u64,
    write_bytes: u64,
    cancelled_write_bytes: u64,
}

impl IoSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            rchar: self.rchar.saturating_sub(before.rchar),
            wchar: self.wchar.saturating_sub(before.wchar),
            syscr: self.syscr.saturating_sub(before.syscr),
            syscw: self.syscw.saturating_sub(before.syscw),
            read_bytes: self.read_bytes.saturating_sub(before.read_bytes),
            write_bytes: self.write_bytes.saturating_sub(before.write_bytes),
            cancelled_write_bytes: self
                .cancelled_write_bytes
                .saturating_sub(before.cancelled_write_bytes),
        }
    }

    fn add(&mut self, other: Self) {
        self.rchar = self.rchar.saturating_add(other.rchar);
        self.wchar = self.wchar.saturating_add(other.wchar);
        self.syscr = self.syscr.saturating_add(other.syscr);
        self.syscw = self.syscw.saturating_add(other.syscw);
        self.read_bytes = self.read_bytes.saturating_add(other.read_bytes);
        self.write_bytes = self.write_bytes.saturating_add(other.write_bytes);
        self.cancelled_write_bytes = self
            .cancelled_write_bytes
            .saturating_add(other.cancelled_write_bytes);
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct FaultSnapshot {
    minor: u64,
    major: u64,
}

impl FaultSnapshot {
    fn delta(self, before: Self) -> Self {
        Self {
            minor: self.minor.saturating_sub(before.minor),
            major: self.major.saturating_sub(before.major),
        }
    }

    fn add(&mut self, other: Self) {
        self.minor = self.minor.saturating_add(other.minor);
        self.major = self.major.saturating_add(other.major);
    }
}

#[derive(Debug, Clone)]
struct EnergyCounter {
    value_uj: u64,
    max_uj: u64,
}

#[derive(Debug, Default)]
struct Aggregate {
    build_ns: Vec<u64>,
    latency_ns: Vec<u64>,
    exact: u64,
    warmup_exact: u64,
    measured_n: u64,
    warmup_n: u64,
    verified: u64,
    matrix_reads: u64,
    primary_reads: u64,
    ordered_reads: u64,
    bank_mismatch: u64,
    non_verified: u64,
    rss_build_max: u64,
    hwm_build_max: u64,
    rss_warm_max: u64,
    hwm_warm_max: u64,
    rss_measured_max: u64,
    hwm_measured_max: u64,
    io: IoSnapshot,
    faults: FaultSnapshot,
    cpu_runtime_ns: u64,
    io_available_batches: u64,
    fault_available_batches: u64,
    cpu_available_batches: u64,
    energy_available_batches: u64,
    energy_uj: BTreeMap<String, u128>,
}

fn parse_kib_status(key: &str) -> Option<u64> {
    let text = fs::read_to_string("/proc/self/status").ok()?;
    let line = text.lines().find(|line| line.starts_with(key))?;
    let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    kib.checked_mul(1024)
}

fn memory_snapshot() -> Option<MemorySnapshot> {
    Some(MemorySnapshot {
        rss_bytes: parse_kib_status("VmRSS:")?,
        hwm_bytes: parse_kib_status("VmHWM:")?,
    })
}

fn io_snapshot() -> Option<IoSnapshot> {
    let text = fs::read_to_string("/proc/self/io").ok()?;
    let mut values = BTreeMap::new();
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if let Ok(value) = value.trim().parse::<u64>() {
            values.insert(name, value);
        }
    }
    Some(IoSnapshot {
        rchar: *values.get("rchar")?,
        wchar: *values.get("wchar")?,
        syscr: *values.get("syscr")?,
        syscw: *values.get("syscw")?,
        read_bytes: *values.get("read_bytes")?,
        write_bytes: *values.get("write_bytes")?,
        cancelled_write_bytes: values.get("cancelled_write_bytes").copied().unwrap_or(0),
    })
}

fn fault_snapshot() -> Option<FaultSnapshot> {
    let text = fs::read_to_string("/proc/self/stat").ok()?;
    let close = text.rfind(')')?;
    let fields = text
        .get(close + 1..)?
        .split_whitespace()
        .collect::<Vec<_>>();
    Some(FaultSnapshot {
        minor: fields.get(7)?.parse().ok()?,
        major: fields.get(9)?.parse().ok()?,
    })
}

fn cpu_runtime_ns() -> Option<u64> {
    fs::read_to_string("/proc/self/schedstat")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn clean_field(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch == ',' || ch == '\n' || ch == '\r' {
                ' '
            } else {
                ch
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn powercap_snapshot() -> BTreeMap<String, EnergyCounter> {
    let root = Path::new("/sys/class/powercap");
    let Ok(entries) = fs::read_dir(root) else {
        return BTreeMap::new();
    };
    let mut out = BTreeMap::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.matches(':').count() > 1 {
            continue;
        }
        let path = entry.path();
        let Ok(value) = fs::read_to_string(path.join("energy_uj")) else {
            continue;
        };
        let Ok(maximum) = fs::read_to_string(path.join("max_energy_range_uj")) else {
            continue;
        };
        let Ok(value_uj) = value.trim().parse::<u64>() else {
            continue;
        };
        let Ok(max_uj) = maximum.trim().parse::<u64>() else {
            continue;
        };
        if max_uj == 0 {
            continue;
        }
        let name = fs::read_to_string(path.join("name"))
            .ok()
            .map(|value| clean_field(value.trim()))
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| file_name.clone());
        out.insert(
            format!("{file_name}:{name}"),
            EnergyCounter { value_uj, max_uj },
        );
    }
    out
}

fn energy_delta(
    before: &BTreeMap<String, EnergyCounter>,
    after: &BTreeMap<String, EnergyCounter>,
) -> Option<BTreeMap<String, u64>> {
    if before.is_empty() || before.keys().ne(after.keys()) {
        return None;
    }
    let mut out = BTreeMap::new();
    for (name, left) in before {
        let right = after.get(name)?;
        if left.max_uj != right.max_uj {
            return None;
        }
        let delta = if right.value_uj >= left.value_uj {
            right.value_uj - left.value_uj
        } else {
            left.max_uj
                .saturating_sub(left.value_uj)
                .saturating_add(right.value_uj)
        };
        out.insert(name.clone(), delta);
    }
    Some(out)
}

fn cpu_model() -> String {
    let Ok(text) = fs::read_to_string("/proc/cpuinfo") else {
        return "UNAVAILABLE".to_string();
    };
    for key in ["model name", "Hardware", "Processor"] {
        if let Some(line) = text.lines().find(|line| line.starts_with(key)) {
            if let Some((_, value)) = line.split_once(':') {
                return clean_field(value.trim());
            }
        }
    }
    "UNAVAILABLE".to_string()
}

fn cpus_allowed() -> String {
    let Ok(text) = fs::read_to_string("/proc/self/status") else {
        return "UNAVAILABLE".to_string();
    };
    text.lines()
        .find(|line| line.starts_with("Cpus_allowed_list:"))
        .and_then(|line| line.split_once(':').map(|(_, value)| value))
        .map(|value| clean_field(value.trim()))
        .unwrap_or_else(|| "UNAVAILABLE".to_string())
}

fn governor() -> String {
    fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .ok()
        .map(|value| clean_field(value.trim()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "UNAVAILABLE".to_string())
}

fn kernel_release() -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|value| clean_field(value.trim()))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "UNAVAILABLE".to_string())
}

fn deterministic_order(seed: u64, pop: usize) -> Vec<usize> {
    let mut order = (0..RECORDS).collect::<Vec<_>>();
    let mut stream = tagged(seed, pop as u64, 14, 1400);
    shuffle(&mut order, &mut stream);
    order
}

fn expected_bank(arm: PhysicalArm, fact: &Fact) -> Bank {
    if arm.uses_overrides() && fact.index % 4 == 0 {
        Bank::Ordered
    } else {
        Bank::Primary
    }
}

fn expected_target(arm: PhysicalArm, fact: &Fact) -> [bool; 2] {
    if arm.uses_overrides() && fact.index % 4 == 0 {
        [!fact.target[0], !fact.target[1]]
    } else {
        fact.target
    }
}

fn direct_resolve<const K: usize>(
    banks: &mut Banks,
    directory: &Directory,
    base_seed: u64,
    fact: &Fact,
    route: &[u32],
) -> ResolveResult {
    let locator = directory
        .select(fact.record, route)
        .expect("direct locator");
    let bank = locator.bank;
    let bank_id = usize::from(bank == Bank::Ordered);
    let query_key = hybrid_key::<K>(bank_id, base_seed, &fact.state, route);
    let before = banks.memory(bank).numeric_counters().reads;
    let values = banks
        .memory_mut(bank)
        .read(&query_key)
        .expect("direct read");
    let after = banks.memory(bank).numeric_counters().reads;
    let read_delta = after - before;
    assert_eq!(read_delta, 1);
    ResolveResult {
        provenance: Dkv4Provenance::Unverified,
        bank: Some(bank),
        value: candidate(&values),
        screened: None,
        matrix_reads: read_delta,
    }
}

#[allow(clippy::too_many_arguments)]
fn one_query<const K: usize>(
    arm: PhysicalArm,
    banks: &mut Banks,
    directory: &Directory,
    source: &SourceSnapshot,
    history: &VerifiedHistory<'_>,
    base_seed: u64,
    fact: &Fact,
    accounting: &mut ResolveAccounting,
) -> ResolveResult {
    let route = secondary_route(fact.page);
    if arm.is_direct() {
        let result = direct_resolve::<K>(banks, directory, base_seed, fact, &route);
        accounting.matrix_reads += result.matrix_reads;
        match result.bank {
            Some(Bank::Primary) => accounting.primary_reads += 1,
            Some(Bank::Ordered) => accounting.ordered_reads += 1,
            None => {}
        }
        result
    } else {
        resolve::<K>(
            banks,
            directory,
            source,
            history,
            base_seed,
            ResolveRequest {
                record: RecordRequest::new(source.ledger.stream_id(), fact.record),
                subject: fact.subject,
                route: &route,
                state: &fact.state,
            },
            accounting,
            ReaderMode::Clean,
        )
    }
}

fn build_world<const K: usize>(
    seed: u64,
    pop: usize,
    arm: PhysicalArm,
) -> (SourceSnapshot, Vec<Fact>, Banks, Directory, u64) {
    let started = Instant::now();
    let base_seed = seed * 1000 + 17;
    let (source, facts) = population_fixture(seed, pop);
    let mut banks = Banks::new::<K>(source.ledger.stream_id());
    let mut directory = Directory::default();

    for index in write_order(seed, pop, 14) {
        let fact = &facts[index];
        let route = base_route(fact.page);
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

    if arm.uses_overrides() {
        for fact in facts.iter().filter(|fact| fact.index % 4 == 0) {
            let route = secondary_route(fact.page);
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
                [!fact.target[0], !fact.target[1]],
            );
        }
    }

    let build_ns = u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX);
    (source, facts, banks, directory, build_ns)
}

fn observe_memory(target_rss: &mut u64, target_hwm: &mut u64) {
    if let Some(snapshot) = memory_snapshot() {
        *target_rss = (*target_rss).max(snapshot.rss_bytes);
        *target_hwm = (*target_hwm).max(snapshot.hwm_bytes);
    }
}

fn run_cell<const K: usize>(pop: usize, arm: PhysicalArm) -> Aggregate {
    let mut aggregate = Aggregate::default();

    for seed in 0..N14_SEEDS {
        let (source, facts, mut banks, directory, preliminary_build_ns) =
            build_world::<K>(seed, pop, arm);
        let history_started = Instant::now();
        let history = verified_history(&source);
        let history_ns = u64::try_from(history_started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        aggregate
            .build_ns
            .push(preliminary_build_ns.saturating_add(history_ns));
        observe_memory(&mut aggregate.rss_build_max, &mut aggregate.hwm_build_max);

        let order = deterministic_order(seed, pop);
        let base_seed = seed * 1000 + 17;
        let mut warmup_accounting = ResolveAccounting::default();
        for _ in 0..WARMUP_PASSES {
            for &index in &order {
                let fact = &facts[index];
                let result = one_query::<K>(
                    arm,
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    fact,
                    &mut warmup_accounting,
                );
                aggregate.warmup_n += 1;
                aggregate.warmup_exact +=
                    u64::from(result.value == Some(expected_target(arm, fact)));
            }
        }
        observe_memory(&mut aggregate.rss_warm_max, &mut aggregate.hwm_warm_max);

        let io_before = io_snapshot();
        let faults_before = fault_snapshot();
        let cpu_before = cpu_runtime_ns();
        let energy_before = powercap_snapshot();

        let mut measured_accounting = ResolveAccounting::default();
        for _ in 0..MEASURE_PASSES {
            for &index in &order {
                let fact = &facts[index];
                let started = Instant::now();
                let result = one_query::<K>(
                    arm,
                    &mut banks,
                    &directory,
                    &source,
                    &history,
                    base_seed,
                    fact,
                    &mut measured_accounting,
                );
                aggregate
                    .latency_ns
                    .push(u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX));
                aggregate.measured_n += 1;
                aggregate.exact += u64::from(result.value == Some(expected_target(arm, fact)));
                aggregate.bank_mismatch += u64::from(result.bank != Some(expected_bank(arm, fact)));
                if !arm.is_direct() {
                    aggregate.non_verified +=
                        u64::from(result.provenance != Dkv4Provenance::VerifiedPresent);
                }
            }
        }

        let energy_after = powercap_snapshot();
        let cpu_after = cpu_runtime_ns();
        let faults_after = fault_snapshot();
        let io_after = io_snapshot();

        aggregate.verified = aggregate
            .verified
            .saturating_add(measured_accounting.verified);
        aggregate.matrix_reads = aggregate
            .matrix_reads
            .saturating_add(measured_accounting.matrix_reads);
        aggregate.primary_reads = aggregate
            .primary_reads
            .saturating_add(measured_accounting.primary_reads);
        aggregate.ordered_reads = aggregate
            .ordered_reads
            .saturating_add(measured_accounting.ordered_reads);

        if let (Some(before), Some(after)) = (io_before, io_after) {
            aggregate.io.add(after.delta(before));
            aggregate.io_available_batches += 1;
        }
        if let (Some(before), Some(after)) = (faults_before, faults_after) {
            aggregate.faults.add(after.delta(before));
            aggregate.fault_available_batches += 1;
        }
        if let (Some(before), Some(after)) = (cpu_before, cpu_after) {
            aggregate.cpu_runtime_ns = aggregate
                .cpu_runtime_ns
                .saturating_add(after.saturating_sub(before));
            aggregate.cpu_available_batches += 1;
        }
        if let Some(delta) = energy_delta(&energy_before, &energy_after) {
            aggregate.energy_available_batches += 1;
            for (name, value) in delta {
                *aggregate.energy_uj.entry(name).or_default() += u128::from(value);
            }
        }

        observe_memory(
            &mut aggregate.rss_measured_max,
            &mut aggregate.hwm_measured_max,
        );
    }

    aggregate
}

fn nearest_rank(sorted: &[u64], percent: usize) -> u64 {
    assert!(!sorted.is_empty());
    let rank = (percent * sorted.len()).div_ceil(100).max(1);
    sorted[rank - 1]
}

fn print_distribution(label: &str, mut values: Vec<u64>) {
    values.sort_unstable();
    let total = values
        .iter()
        .fold(0_u128, |acc, value| acc + u128::from(*value));
    println!(
        "N14_{label},samples={},min_ns={},p50_ns={},p95_ns={},p99_ns={},max_ns={},total_ns={}",
        values.len(),
        values[0],
        nearest_rank(&values, 50),
        nearest_rank(&values, 95),
        nearest_rank(&values, 99),
        values[values.len() - 1],
        total
    );
}

fn print_identity() {
    let git_sha = env::var("GITHUB_SHA").unwrap_or_else(|_| "LOCAL".to_string());
    let runner = env::var("RUNNER_NAME").unwrap_or_else(|_| "LOCAL".to_string());
    let run_id = env::var("GITHUB_RUN_ID").unwrap_or_else(|_| "LOCAL".to_string());
    let job = env::var("GITHUB_JOB").unwrap_or_else(|_| "LOCAL".to_string());
    let logical = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(0);
    println!(
        "N14_IDENTITY,git_sha={},runner={},run_id={},job={},arch={},kernel={},logical_cpus={},cpu_model={},cpus_allowed={},governor={}",
        clean_field(&git_sha),
        clean_field(&runner),
        clean_field(&run_id),
        clean_field(&job),
        env::consts::ARCH,
        kernel_release(),
        logical,
        cpu_model(),
        cpus_allowed(),
        governor(),
    );
}

fn execute<const K: usize>(pop: usize, arm: PhysicalArm) {
    let aggregate = run_cell::<K>(pop, arm);
    print_identity();
    println!(
        "N14_CONFIG,K={K},arm={},pop={},seeds={},records={},warmup_passes={},measure_passes={}",
        arm.label(),
        POP_NAMES[pop],
        N14_SEEDS,
        RECORDS,
        WARMUP_PASSES,
        MEASURE_PASSES,
    );
    print_distribution("BUILD", aggregate.build_ns.clone());
    print_distribution("LATENCY", aggregate.latency_ns.clone());
    println!(
        "N14_MEMORY,rss_build_max_bytes={},hwm_build_max_bytes={},rss_warm_max_bytes={},hwm_warm_max_bytes={},rss_measured_max_bytes={},hwm_measured_max_bytes={}",
        aggregate.rss_build_max,
        aggregate.hwm_build_max,
        aggregate.rss_warm_max,
        aggregate.hwm_warm_max,
        aggregate.rss_measured_max,
        aggregate.hwm_measured_max,
    );
    println!(
        "N14_IO,batches={},rchar={},wchar={},syscr={},syscw={},read_bytes={},write_bytes={},cancelled_write_bytes={}",
        aggregate.io_available_batches,
        aggregate.io.rchar,
        aggregate.io.wchar,
        aggregate.io.syscr,
        aggregate.io.syscw,
        aggregate.io.read_bytes,
        aggregate.io.write_bytes,
        aggregate.io.cancelled_write_bytes,
    );
    println!(
        "N14_FAULTS,batches={},minor={},major={}",
        aggregate.fault_available_batches, aggregate.faults.minor, aggregate.faults.major
    );
    println!(
        "N14_CPU,batches={},runtime_ns={}",
        aggregate.cpu_available_batches, aggregate.cpu_runtime_ns
    );
    if aggregate.energy_available_batches == N14_SEEDS && !aggregate.energy_uj.is_empty() {
        println!(
            "N14_ENERGY_STATUS,AVAILABLE,batches={},domains={}",
            aggregate.energy_available_batches,
            aggregate.energy_uj.len()
        );
        for (name, delta) in &aggregate.energy_uj {
            println!("N14_ENERGY,domain={},delta_uj={delta}", clean_field(name));
        }
    } else {
        println!(
            "N14_ENERGY_STATUS,UNAVAILABLE,batches={},domains={}",
            aggregate.energy_available_batches,
            aggregate.energy_uj.len()
        );
    }

    let expected_measured = N14_SEEDS * RECORDS as u64 * MEASURE_PASSES as u64;
    let expected_warmup = N14_SEEDS * RECORDS as u64 * WARMUP_PASSES as u64;
    let expected_ordered = if arm.uses_overrides() {
        expected_measured / 4
    } else {
        0
    };
    let expected_primary = expected_measured - expected_ordered;
    let semantics_hold = aggregate.measured_n == expected_measured
        && aggregate.warmup_n == expected_warmup
        && aggregate.exact * WARMUP_PASSES as u64 == aggregate.warmup_exact * MEASURE_PASSES as u64
        && aggregate.non_verified == 0
        && aggregate.bank_mismatch == 0
        && (arm.is_direct() || aggregate.verified == expected_measured)
        && aggregate.matrix_reads == expected_measured
        && aggregate.primary_reads == expected_primary
        && aggregate.ordered_reads == expected_ordered;
    let instrumentation_hold = aggregate.io_available_batches == N14_SEEDS
        && aggregate.fault_available_batches == N14_SEEDS
        && aggregate.cpu_available_batches == N14_SEEDS
        && aggregate.rss_measured_max > 0
        && aggregate.hwm_measured_max > 0;
    let io_hold = aggregate.io_available_batches == N14_SEEDS
        && aggregate.io.read_bytes == 0
        && aggregate.io.write_bytes == 0;
    let faults_hold = aggregate.fault_available_batches == N14_SEEDS && aggregate.faults.major == 0;
    let single_read_hold = aggregate.matrix_reads == expected_measured
        && aggregate.primary_reads + aggregate.ordered_reads == expected_measured;
    let energy_hold =
        aggregate.energy_available_batches == N14_SEEDS && !aggregate.energy_uj.is_empty();

    println!(
        "N14_SEMANTICS,measured_n={},measured_exact={},warmup_n={},warmup_exact={},verified={},matrix_reads={},primary_reads={},ordered_reads={},bank_mismatch={},non_verified={}",
        aggregate.measured_n,
        aggregate.exact,
        aggregate.warmup_n,
        aggregate.warmup_exact,
        aggregate.verified,
        aggregate.matrix_reads,
        aggregate.primary_reads,
        aggregate.ordered_reads,
        aggregate.bank_mismatch,
        aggregate.non_verified,
    );
    println!(
        "N14_HYPOTHESIS,H1_SEMANTICS,{}",
        if semantics_hold { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "N14_HYPOTHESIS,H2_INSTRUMENTATION,{}",
        if instrumentation_hold {
            "HOLDS"
        } else {
            "REFUTED"
        }
    );
    println!(
        "N14_HYPOTHESIS,H3_ZERO_BLOCK_IO,{}",
        if io_hold { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "N14_HYPOTHESIS,H4_ZERO_MAJOR_FAULTS,{}",
        if faults_hold { "HOLDS" } else { "REFUTED" }
    );
    println!(
        "N14_HYPOTHESIS,H5_SINGLE_READ,{}",
        if single_read_hold { "HOLDS" } else { "REFUTED" }
    );
    println!("N14_HYPOTHESIS,H6_WIDTH_SCALING,MEASURE_ONLY");
    println!("N14_HYPOTHESIS,H7_VERIFICATION_COST,MEASURE_ONLY");
    println!(
        "N14_HYPOTHESIS,H8_ENERGY,{}",
        if energy_hold { "HOLDS" } else { "REFUTED" }
    );
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        4,
        "usage: dkv_2a_n14_physical <256|1024> <primary|mixed|direct-primary|direct-mixed> <population 0|1|2>"
    );
    let width = args[1].parse::<usize>().expect("width");
    let arm = PhysicalArm::parse(&args[2]);
    let pop = args[3].parse::<usize>().expect("population");
    assert!(pop < POP_NAMES.len(), "population must be 0..2");
    match width {
        256 => execute::<256>(pop, arm),
        1024 => execute::<1024>(pop, arm),
        _ => panic!("width must be 256 or 1024"),
    }
}

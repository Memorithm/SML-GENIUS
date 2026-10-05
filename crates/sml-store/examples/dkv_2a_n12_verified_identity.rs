//! DKV-2a-N12 verified identity/provenance requalification.
//!
//! Protocol: docs/SML-DKV-2A-N12-VERIFIED-IDENTITY-PREREGISTRATION.md.
//! Fixture freeze: docs/SML-DKV-2A-N12-FIXTURE-FREEZE.md.
//! Harness-only: current projector, context-memory and observed-history APIs are unchanged.

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
        SubjectBindingLimits, SubjectKey, MAX_EVENT_BYTES,
    },
    raw_context::{sha256_raw_bytes, RawContextStoreError},
};

type State = SemanticState<BITS>;

const BITS: usize = 64;
const SEEDS: u64 = 16;
const RECORDS: usize = 8;
const FULL_RATE: u16 = DELTA_KV_SCALE as u16;
const KEY_WIDTHS: [usize; 2] = [256, 1024];
const Q: usize = 3;

const PAGE_C: u64 = 0x9e37_79b9_7f4a_7c15;
const VALUE_C: u64 = 0xd6e8_feb8_6659_fd93;
const ROUTE_TAG: u64 = 0x6d75_6c74_695f_6d6f;
const ROUTE_C: u64 = 0xc2b2_ae3d_27d4_eb4f;
const RANK_C: u64 = 0x1656_67b1_9e37_79f9;
const LEN_C: u64 = 0x85eb_ca77_c2b2_ae63;

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

fn exact_clear(values: &[i32], bits: [bool; 2]) -> bool {
    candidate(values) == Some(bits)
}

fn subject(index: usize) -> SubjectKey {
    SubjectKey::new(index as u64 + 1, 0).expect("subject")
}

fn record_id(seed: u64, index: usize) -> u64 {
    seed * 100 + index as u64 + 1
}

fn context_limits() -> ContextLimits {
    ContextLimits {
        max_writes: 32,
        max_sources_per_write: 8,
        max_source_links: 128,
        max_ledger_segments: 32,
        max_ledger_semantic_links: 128,
        max_element_bytes: 8 << 20,
        max_staging_bytes: 8 << 20,
    }
}

fn verification_limits() -> VerificationLimits {
    VerificationLimits {
        max_sources: 32,
        max_raw_bytes: 64 << 10,
    }
}

fn record_limits() -> RecordBindingLimits {
    RecordBindingLimits {
        max_write_records: 32,
        max_sources: 32,
    }
}

fn history_limits() -> HistoryLimits {
    HistoryLimits {
        max_events: 32,
        max_keys: 32,
        max_element_bytes: 64 << 10,
    }
}

fn history_verification_limits() -> HistoryVerificationLimits {
    HistoryVerificationLimits {
        max_segments: 32,
        max_raw_bytes: 64 << 10,
        max_record_bytes: MAX_EVENT_BYTES,
    }
}

fn subject_limits() -> SubjectBindingLimits {
    SubjectBindingLimits {
        max_candidate_sources: 32,
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

fn clean_fixture(seed: u64) -> (SourceSnapshot, Vec<State>, Vec<[bool; 2]>) {
    let mut stream = tagged(seed, 12, 0, 1200);
    let mut events = Vec::new();
    let mut states = Vec::new();
    let mut bits = Vec::new();
    for index in 0..RECORDS {
        let id = record_id(seed, index);
        let state = build_state(stream.next(), stream.next(), stream.next());
        let target_word = stream.next();
        let target = [target_word & 1 != 0, target_word & 2 != 0];
        events.push(ObservationEvent::new(id, subject(index), target, None).expect("clean event"));
        states.push(state);
        bits.push(target);
    }
    (snapshot(&events, 1), states, bits)
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
    SubstituteFirst,
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
        ReaderMode::SubstituteFirst if index == 0 && raw.len() > 1 => {
            bytes = raw[1].clone();
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
    sources: u64,
    bytes: u64,
}

struct ResolveRequest<'a> {
    record: RecordRequest,
    subject: SubjectKey,
    route: &'a [u32],
    state: &'a State,
}

fn resolve<const K: usize>(
    banks: &mut Banks,
    directory: &Directory,
    source: &SourceSnapshot,
    base_seed: u64,
    request: ResolveRequest<'_>,
    accounting: &mut ResolveAccounting,
    reader_mode: ReaderMode,
    verify_limits: VerificationLimits,
    history_ledger: Option<&RawContextLedger>,
) -> ResolveResult {
    let Some(locator) = directory.select(request.record.record_id(), request.route) else {
        accounting.missing += 1;
        return ResolveResult {
            provenance: Dkv4Provenance::Missing,
            bank: None,
            value: None,
            screened: None,
            matrix_reads: 0,
            sources: 0,
            bytes: 0,
        };
    };
    let bank = locator.bank;
    let memory = banks.memory(bank);
    let history_ledger = history_ledger.unwrap_or(&source.ledger);

    let history = match verify_history(
        history_ledger,
        history_limits(),
        history_verification_limits(),
        |reference| read_source(reference, &source.raw, reader_mode),
    ) {
        Ok(history) => history,
        Err(_) => {
            accounting.unverified += 1;
            return ResolveResult {
                provenance: Dkv4Provenance::Unverified,
                bank: Some(bank),
                value: None,
                screened: None,
                matrix_reads: 0,
                sources: 0,
                bytes: 0,
            };
        }
    };

    let preflight = (|| {
        if locator.stream != request.record.stream_id()
            || locator.record_id != request.record.record_id()
            || locator.subject != request.subject
        {
            // Continue through the authoritative APIs rather than trusting locator fields.
        }
        let receipt = memory
            .query(&locator.write_key)
            .map_err(|_| Dkv4Provenance::Unverified)?
            .verify(&source.ledger, verify_limits, |reference| {
                read_source(reference, &source.raw, reader_mode)
            })
            .map_err(|_| Dkv4Provenance::Unverified)?;

        receipt
            .bind_record(request.record, record_limits())
            .map_err(|_| Dkv4Provenance::Unverified)?;

        let bound =
            match bind_projected_subject(&receipt, &history, request.subject, subject_limits()) {
                Ok(bound) => bound,
                Err(SubjectBindingError::MissingSubject) => return Err(Dkv4Provenance::Missing),
                Err(_) => return Err(Dkv4Provenance::Unverified),
            };
        let state = bound.observed_state();
        let segments = receipt.sources().len() as u64;
        let bytes = receipt.raw_bytes();
        match state {
            ObservedState::Agreement => Ok((segments, bytes)),
            ObservedState::Conflict => Err(Dkv4Provenance::Conflict),
            ObservedState::Missing => Err(Dkv4Provenance::Missing),
        }
    })();

    let (segments, bytes) = match preflight {
        Ok(counts) => counts,
        Err(Dkv4Provenance::Missing) => {
            accounting.missing += 1;
            return ResolveResult {
                provenance: Dkv4Provenance::Missing,
                bank: Some(bank),
                value: None,
                screened: None,
                matrix_reads: 0,
                sources: 0,
                bytes: 0,
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
                sources: 0,
                bytes: 0,
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
                sources: 0,
                bytes: 0,
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
    let screened = screen_candidate(&history, request.subject, latent);
    ResolveResult {
        provenance: Dkv4Provenance::VerifiedPresent,
        bank: Some(bank),
        value: latent,
        screened: screened.value(),
        matrix_reads: read_delta,
        sources: segments,
        bytes,
    }
}

fn perturb_one(state: &State, seed: u64, record: u64) -> State {
    let mut out = state.clone();
    let mut stream = tagged(seed, record, 1, 77);
    let bit = stream.below(BITS);
    let value = out.value.get(bit).expect("bit");
    out.value.set(bit, !value).expect("bit");
    out
}

#[derive(Default, Clone, Copy)]
struct Recall {
    n: u64,
    exact: u64,
    verified: u64,
    primary: u64,
    ordered: u64,
    screened_same: u64,
}

impl Recall {
    fn add(&mut self, result: ResolveResult, expected: [bool; 2]) {
        self.n += 1;
        self.exact += u64::from(result.value == Some(expected));
        self.verified += u64::from(result.provenance == Dkv4Provenance::VerifiedPresent);
        self.primary += u64::from(result.bank == Some(Bank::Primary));
        self.ordered += u64::from(result.bank == Some(Bank::Ordered));
        self.screened_same += u64::from(result.screened == result.value);
    }
}

fn ppm(a: u64, b: u64) -> u64 {
    if b == 0 {
        0
    } else {
        a.saturating_mul(1_000_000) / b
    }
}

fn clean_panels<const K: usize>(
    p1: &mut BTreeMap<(usize, usize), Recall>,
    p2: &mut BTreeMap<(usize, usize), Recall>,
    p3: &mut BTreeMap<(usize, usize), Recall>,
    p9: &mut BTreeMap<usize, (u64, u64, u64)>,
    accounting: &mut ResolveAccounting,
) {
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let (source, states, bits) = clean_fixture(seed);
        let mut banks = Banks::new::<K>(source.ledger.stream_id());
        let mut dir = Directory::default();

        for index in 0..RECORDS {
            let record = record_id(seed, index);
            admit::<K>(
                &mut banks,
                &mut dir,
                &source,
                Bank::Primary,
                base_seed,
                record,
                subject(index),
                &states[index],
                R2,
                bits[index],
            );
        }

        for index in 0..RECORDS {
            let record = record_id(seed, index);
            for (slot, route) in [(0_usize, R2), (1, R3), (2, R4)] {
                let result = resolve::<K>(
                    &mut banks,
                    &dir,
                    &source,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), record),
                        subject: subject(index),
                        route,
                        state: &states[index],
                    },
                    accounting,
                    ReaderMode::Clean,
                    verification_limits(),
                    None,
                );
                p1.entry((K, slot)).or_default().add(result, bits[index]);
            }
        }

        // P9: identity stays fixed while current latent state changes.
        for index in 0..RECORDS {
            let record = record_id(seed, index);
            let perturbed = perturb_one(&states[index], seed, record);
            let result = resolve::<K>(
                &mut banks,
                &dir,
                &source,
                base_seed,
                ResolveRequest {
                    record: RecordRequest::new(source.ledger.stream_id(), record),
                    subject: subject(index),
                    route: R2,
                    state: &perturbed,
                },
                accounting,
                ReaderMode::Clean,
                verification_limits(),
                None,
            );
            let entry = p9.entry(K).or_default();
            entry.0 += 1;
            entry.1 += u64::from(result.provenance == Dkv4Provenance::VerifiedPresent);
            entry.2 += u64::from(result.value != Some(bits[index]));
        }

        // P2: independent ORDERED override from same verified source identity.
        let mut override_banks = Banks::new::<K>(source.ledger.stream_id());
        let mut override_dir = Directory::default();
        for index in 0..RECORDS {
            let record = record_id(seed, index);
            let override_bits = [!bits[index][0], !bits[index][1]];
            admit::<K>(
                &mut override_banks,
                &mut override_dir,
                &source,
                Bank::Primary,
                base_seed,
                record,
                subject(index),
                &states[index],
                R2,
                bits[index],
            );
            admit::<K>(
                &mut override_banks,
                &mut override_dir,
                &source,
                Bank::Ordered,
                base_seed,
                record,
                subject(index),
                &states[index],
                R3,
                override_bits,
            );
        }
        for index in 0..RECORDS {
            let record = record_id(seed, index);
            let override_bits = [!bits[index][0], !bits[index][1]];
            for (slot, route, expected) in [
                (0_usize, R2, bits[index]),
                (1, R3, override_bits),
                (2, R4, bits[index]),
            ] {
                let result = resolve::<K>(
                    &mut override_banks,
                    &override_dir,
                    &source,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), record),
                        subject: subject(index),
                        route,
                        state: &states[index],
                    },
                    accounting,
                    ReaderMode::Clean,
                    verification_limits(),
                    None,
                );
                p2.entry((K, slot)).or_default().add(result, expected);
            }
        }

        // P3 primary-switch override.
        let mut switch_banks = Banks::new::<K>(source.ledger.stream_id());
        let mut switch_dir = Directory::default();
        for index in 0..RECORDS {
            let record = record_id(seed, index);
            let override_bits = [!bits[index][0], !bits[index][1]];
            admit::<K>(
                &mut switch_banks,
                &mut switch_dir,
                &source,
                Bank::Primary,
                base_seed,
                record,
                subject(index),
                &states[index],
                R2,
                bits[index],
            );
            admit::<K>(
                &mut switch_banks,
                &mut switch_dir,
                &source,
                Bank::Ordered,
                base_seed,
                record,
                subject(index),
                &states[index],
                R6,
                override_bits,
            );
        }
        for index in 0..RECORDS {
            let record = record_id(seed, index);
            let override_bits = [!bits[index][0], !bits[index][1]];
            for (slot, route, expected) in [(0_usize, R2, bits[index]), (1, R6, override_bits)] {
                let result = resolve::<K>(
                    &mut switch_banks,
                    &switch_dir,
                    &source,
                    base_seed,
                    ResolveRequest {
                        record: RecordRequest::new(source.ledger.stream_id(), record),
                        subject: subject(index),
                        route,
                        state: &states[index],
                    },
                    accounting,
                    ReaderMode::Clean,
                    verification_limits(),
                    None,
                );
                p3.entry((K, slot)).or_default().add(result, expected);
            }
        }
    }
}

fn identity_negative<const K: usize>(
    accounting: &mut ResolveAccounting,
) -> BTreeMap<&'static str, u64> {
    let seed = 0;
    let base_seed = 17;
    let (source, states, bits) = clean_fixture(seed);
    let mut banks = Banks::new::<K>(source.ledger.stream_id());
    let mut dir = Directory::default();
    let record = record_id(seed, 0);
    admit::<K>(
        &mut banks,
        &mut dir,
        &source,
        Bank::Primary,
        base_seed,
        record,
        subject(0),
        &states[0],
        R2,
        bits[0],
    );
    let mut result = BTreeMap::new();

    let missing = resolve::<K>(
        &mut banks,
        &dir,
        &source,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(source.ledger.stream_id(), record + 10_000),
            subject: subject(0),
            route: R2,
            state: &states[0],
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    );
    result.insert(
        "nonexistent",
        u64::from(missing.provenance != Dkv4Provenance::VerifiedPresent),
    );

    let foreign = resolve::<K>(
        &mut banks,
        &dir,
        &source,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(ContextStreamId::new(2).unwrap(), record),
            subject: subject(0),
            route: R2,
            state: &states[0],
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    );
    result.insert(
        "foreign_stream",
        u64::from(foreign.provenance != Dkv4Provenance::VerifiedPresent),
    );

    let wrong_subject = resolve::<K>(
        &mut banks,
        &dir,
        &source,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(source.ledger.stream_id(), record),
            subject: SubjectKey::new(1001, 0).unwrap(),
            route: R2,
            state: &states[0],
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    );
    result.insert(
        "wrong_subject",
        u64::from(wrong_subject.provenance != Dkv4Provenance::VerifiedPresent),
    );

    // Incomplete subject: two versions, only first behind selected write key.
    let key = subject(0);
    let events = [
        ObservationEvent::new(1, key, [false, false], None).unwrap(),
        ObservationEvent::new(2, key, [true, true], None).unwrap(),
    ];
    let incomplete = snapshot(&events, 1);
    let state = build_state(1, 2, 3);
    let other_state = build_state(4, 5, 6);
    let mut ibanks = Banks::new::<K>(incomplete.ledger.stream_id());
    let mut idir = Directory::default();
    admit::<K>(
        &mut ibanks,
        &mut idir,
        &incomplete,
        Bank::Primary,
        base_seed,
        1,
        key,
        &state,
        R2,
        [false, false],
    );
    // Admit record2 under a different exact key so the selected receipt omits it.
    admit::<K>(
        &mut ibanks,
        &mut idir,
        &incomplete,
        Bank::Primary,
        base_seed,
        2,
        key,
        &other_state,
        R2,
        [true, true],
    );
    let inc = resolve::<K>(
        &mut ibanks,
        &idir,
        &incomplete,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(incomplete.ledger.stream_id(), 1),
            subject: key,
            route: R2,
            state: &state,
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    );
    result.insert(
        "incomplete_subject",
        u64::from(inc.provenance != Dkv4Provenance::VerifiedPresent),
    );
    result
}

fn conflict_and_supersession<const K: usize>(
    accounting: &mut ResolveAccounting,
) -> (ResolveResult, ResolveResult, bool) {
    let base_seed = 17;
    let key = subject(0);

    // P5 conflict.
    let conflict_events = [
        ObservationEvent::new(1, key, [false, false], None).unwrap(),
        ObservationEvent::new(2, key, [true, true], None).unwrap(),
    ];
    let conflict_source = snapshot(&conflict_events, 1);
    let same_state = build_state(11, 22, 33);
    let mut cbanks = Banks::new::<K>(conflict_source.ledger.stream_id());
    let mut cdir = Directory::default();
    admit::<K>(
        &mut cbanks,
        &mut cdir,
        &conflict_source,
        Bank::Primary,
        base_seed,
        1,
        key,
        &same_state,
        R2,
        [false, false],
    );
    // Same selected write key, separate record/source.
    let write_key = hybrid_key::<K>(B_PRIMARY, base_seed, &same_state, R2);
    let source2 = source_for_record(&conflict_source.ledger, 2);
    cbanks
        .primary
        .write(
            &conflict_source.ledger,
            2,
            &write_key,
            &target_q12([true, true]),
            FULL_RATE,
            &[source2],
        )
        .unwrap();
    let conflict = resolve::<K>(
        &mut cbanks,
        &cdir,
        &conflict_source,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(conflict_source.ledger.stream_id(), 1),
            subject: key,
            route: R2,
            state: &same_state,
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    );

    // P6 explicit supersession.
    let supersede_events = [
        ObservationEvent::new(1, key, [false, false], None).unwrap(),
        ObservationEvent::new(2, key, [true, true], Some(1)).unwrap(),
    ];
    let supersede_source = snapshot(&supersede_events, 1);
    let mut sbanks = Banks::new::<K>(supersede_source.ledger.stream_id());
    let mut sdir = Directory::default();
    admit::<K>(
        &mut sbanks,
        &mut sdir,
        &supersede_source,
        Bank::Primary,
        base_seed,
        1,
        key,
        &same_state,
        R2,
        [false, false],
    );
    let source2 = source_for_record(&supersede_source.ledger, 2);
    sbanks
        .primary
        .write(
            &supersede_source.ledger,
            2,
            &write_key,
            &target_q12([true, true]),
            FULL_RATE,
            &[source2],
        )
        .unwrap();
    let supersede = resolve::<K>(
        &mut sbanks,
        &sdir,
        &supersede_source,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(supersede_source.ledger.stream_id(), 1),
            subject: key,
            route: R2,
            state: &same_state,
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    );
    // Agreement must preserve, not repair, an intentionally wrong latent candidate.
    let wrong_preserved = supersede.screened == supersede.value;
    (conflict, supersede, wrong_preserved)
}

fn unverified_controls<const K: usize>(
    accounting: &mut ResolveAccounting,
) -> BTreeMap<&'static str, ResolveResult> {
    let seed = 0;
    let base_seed = 17;
    let (source, states, bits) = clean_fixture(seed);
    let record = record_id(seed, 0);
    let mut banks = Banks::new::<K>(source.ledger.stream_id());
    let mut dir = Directory::default();
    admit::<K>(
        &mut banks,
        &mut dir,
        &source,
        Bank::Primary,
        base_seed,
        record,
        subject(0),
        &states[0],
        R2,
        bits[0],
    );
    admit::<K>(
        &mut banks,
        &mut dir,
        &source,
        Bank::Ordered,
        base_seed,
        record,
        subject(0),
        &states[0],
        R3,
        [!bits[0][0], !bits[0][1]],
    );

    let request = |state: &State| ResolveRequest {
        record: RecordRequest::new(source.ledger.stream_id(), record),
        subject: subject(0),
        route: R3,
        state,
    };
    let mut rows = BTreeMap::new();

    for (name, mode) in [
        ("corruption", ReaderMode::CorruptFirst),
        ("substitution", ReaderMode::SubstituteFirst),
    ] {
        let result = resolve::<K>(
            &mut banks,
            &dir,
            &source,
            base_seed,
            request(&states[0]),
            accounting,
            mode,
            verification_limits(),
            None,
        );
        rows.insert(name, result);
    }

    // Stale ledger: omit final segment.
    let stale_events: Vec<_> = (0..RECORDS - 1)
        .map(|index| {
            ObservationEvent::new(record_id(seed, index), subject(index), bits[index], None)
                .unwrap()
        })
        .collect();
    let stale = snapshot(&stale_events, 1);
    let result = resolve::<K>(
        &mut banks,
        &dir,
        &source,
        base_seed,
        request(&states[0]),
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        Some(&stale.ledger),
    );
    rows.insert("stale", result);

    // Same-count fork: change first event bits, rebuilding descriptor/hash.
    let mut fork_events: Vec<_> = (0..RECORDS)
        .map(|index| {
            ObservationEvent::new(record_id(seed, index), subject(index), bits[index], None)
                .unwrap()
        })
        .collect();
    fork_events[0] =
        ObservationEvent::new(record, subject(0), [!bits[0][0], bits[0][1]], None).unwrap();
    let fork = snapshot(&fork_events, 1);
    let result = resolve::<K>(
        &mut banks,
        &dir,
        &source,
        base_seed,
        request(&states[0]),
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        Some(&fork.ledger),
    );
    rows.insert("fork", result);

    let result = resolve::<K>(
        &mut banks,
        &dir,
        &source,
        base_seed,
        request(&states[0]),
        accounting,
        ReaderMode::Clean,
        VerificationLimits {
            max_sources: 0,
            ..verification_limits()
        },
        None,
    );
    rows.insert("budget", result);
    rows
}

fn alias_controls<const K: usize>(
    accounting: &mut ResolveAccounting,
) -> (Dkv4Provenance, Dkv4Provenance, Dkv4Provenance) {
    let base_seed = 17;
    let s1 = SubjectKey::new(1, 0).unwrap();
    let s2 = SubjectKey::new(2, 0).unwrap();
    let events = [
        ObservationEvent::new(1, s1, [true, false], None).unwrap(),
        ObservationEvent::new(2, s2, [false, true], None).unwrap(),
    ];
    let source = snapshot(&events, 1);
    let state = build_state(7, 8, 9);
    let mut banks = Banks::new::<K>(source.ledger.stream_id());
    let mut dir = Directory::default();
    admit::<K>(
        &mut banks,
        &mut dir,
        &source,
        Bank::Primary,
        base_seed,
        1,
        s1,
        &state,
        R2,
        [true, false],
    );
    let same_key = hybrid_key::<K>(B_PRIMARY, base_seed, &state, R2);
    let source2 = source_for_record(&source.ledger, 2);
    banks
        .primary
        .write(
            &source.ledger,
            2,
            &same_key,
            &target_q12([false, true]),
            FULL_RATE,
            &[source2],
        )
        .unwrap();

    let unwritten = resolve::<K>(
        &mut banks,
        &dir,
        &source,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(source.ledger.stream_id(), 1),
            subject: SubjectKey::new(3, 0).unwrap(),
            route: R2,
            state: &state,
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    )
    .provenance;

    // Conflict for requested subject behind same exact key.
    let conflict_events = [
        ObservationEvent::new(1, s1, [true, false], None).unwrap(),
        ObservationEvent::new(2, s1, [false, true], None).unwrap(),
    ];
    let csource = snapshot(&conflict_events, 1);
    let mut cbanks = Banks::new::<K>(csource.ledger.stream_id());
    let mut cdir = Directory::default();
    admit::<K>(
        &mut cbanks,
        &mut cdir,
        &csource,
        Bank::Primary,
        base_seed,
        1,
        s1,
        &state,
        R2,
        [true, false],
    );
    let source2 = source_for_record(&csource.ledger, 2);
    cbanks
        .primary
        .write(
            &csource.ledger,
            2,
            &same_key,
            &target_q12([false, true]),
            FULL_RATE,
            &[source2],
        )
        .unwrap();
    let conflict = resolve::<K>(
        &mut cbanks,
        &cdir,
        &csource,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(csource.ledger.stream_id(), 1),
            subject: s1,
            route: R2,
            state: &state,
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    )
    .provenance;

    // Incomplete requested subject: second version behind different key.
    let isource = snapshot(&conflict_events, 1);
    let mut ibanks = Banks::new::<K>(isource.ledger.stream_id());
    let mut idir = Directory::default();
    admit::<K>(
        &mut ibanks,
        &mut idir,
        &isource,
        Bank::Primary,
        base_seed,
        1,
        s1,
        &state,
        R2,
        [true, false],
    );
    let other_state = build_state(70, 80, 90);
    admit::<K>(
        &mut ibanks,
        &mut idir,
        &isource,
        Bank::Primary,
        base_seed,
        2,
        s1,
        &other_state,
        R2,
        [false, true],
    );
    let incomplete = resolve::<K>(
        &mut ibanks,
        &idir,
        &isource,
        base_seed,
        ResolveRequest {
            record: RecordRequest::new(isource.ledger.stream_id(), 1),
            subject: s1,
            route: R2,
            state: &state,
        },
        accounting,
        ReaderMode::Clean,
        verification_limits(),
        None,
    )
    .provenance;

    (unwritten, conflict, incomplete)
}

fn compatibility<const K: usize>() -> u64 {
    let mut checks = 0;
    for seed in 0..SEEDS {
        let base_seed = seed * 1000 + 17;
        let mut stream = tagged(seed, K as u64, 12, 1212);
        for route in [R2, R3, R4, R6] {
            let state = build_state(stream.next(), stream.next(), stream.next());
            for bank in [B_PRIMARY, B_ORDERED] {
                let first = hybrid_key::<K>(bank, base_seed, &state, route);
                let second = hybrid_key::<K>(bank, base_seed, &state, route);
                assert_eq!(first, second);
                checks += 1;
            }
        }
        let duplicate = decision(&[5, 5]);
        let state = build_state(stream.next(), stream.next(), stream.next());
        let projector = DeltaKvSemanticProjector::<K>::new_with_ambiguity_projection(
            DeltaKvSemanticProjectionPolicy::default(),
            base_seed,
            DeltaKvAmbiguityProjection::ActiveOnly,
        )
        .unwrap();
        assert!(projector.project(&state, &duplicate, None).is_err());
        checks += 1;
    }
    checks
}

fn provenance_name(value: Dkv4Provenance) -> &'static str {
    match value {
        Dkv4Provenance::VerifiedPresent => "VerifiedPresent",
        Dkv4Provenance::Missing => "Missing",
        Dkv4Provenance::Conflict => "Conflict",
        Dkv4Provenance::Unverified => "Unverified",
    }
}

fn main() {
    let compatibility_checks = compatibility::<256>() + compatibility::<1024>();
    let mut p1 = BTreeMap::new();
    let mut p2 = BTreeMap::new();
    let mut p3 = BTreeMap::new();
    let mut p9 = BTreeMap::new();
    let mut accounting = ResolveAccounting::default();

    clean_panels::<256>(&mut p1, &mut p2, &mut p3, &mut p9, &mut accounting);
    clean_panels::<1024>(&mut p1, &mut p2, &mut p3, &mut p9, &mut accounting);

    let p4_256 = identity_negative::<256>(&mut accounting);
    let p4_1024 = identity_negative::<1024>(&mut accounting);
    let (p5_256, p6_256, wrong_256) = conflict_and_supersession::<256>(&mut accounting);
    let (p5_1024, p6_1024, wrong_1024) = conflict_and_supersession::<1024>(&mut accounting);
    let p7_256 = unverified_controls::<256>(&mut accounting);
    let p7_1024 = unverified_controls::<1024>(&mut accounting);
    let p8_256 = alias_controls::<256>(&mut accounting);
    let p8_1024 = alias_controls::<1024>(&mut accounting);

    let mut out = String::new();
    writeln!(
        out,
        "N12_CONTROL,compatibility_assertions={compatibility_checks}"
    )
    .unwrap();

    for &k in &KEY_WIDTHS {
        for (panel, cells, routes) in [
            ("P1", &p1, &["R2", "R3", "R4"][..]),
            ("P2", &p2, &["R2", "R3", "R4"][..]),
            ("P3", &p3, &["R2", "R6"][..]),
        ] {
            for (slot, route) in routes.iter().enumerate() {
                let tally = cells[&(k, slot)];
                writeln!(
                    out,
                    "N12_{panel},K={k},route={route},n={},exact={},exact_ppm={},verified={},primary={},ordered={},screened_same={}",
                    tally.n,
                    tally.exact,
                    ppm(tally.exact, tally.n),
                    tally.verified,
                    tally.primary,
                    tally.ordered,
                    tally.screened_same,
                )
                .unwrap();
            }
        }
        let p9row = p9[&k];
        writeln!(
            out,
            "N12_P9,K={k},n={},verified={},latent_changed={}",
            p9row.0, p9row.1, p9row.2
        )
        .unwrap();
    }

    for (k, rows) in [(256, &p4_256), (1024, &p4_1024)] {
        for (name, blocked) in rows {
            writeln!(out, "N12_P4,K={k},case={name},not_verified={blocked}").unwrap();
        }
    }

    for (k, conflict, supersede, wrong_preserved) in [
        (256, p5_256, p6_256, wrong_256),
        (1024, p5_1024, p6_1024, wrong_1024),
    ] {
        writeln!(
            out,
            "N12_P5,K={k},provenance={},matrix_reads={},screened={:?}",
            provenance_name(conflict.provenance),
            conflict.matrix_reads,
            conflict.screened,
        )
        .unwrap();
        writeln!(
            out,
            "N12_P6,K={k},provenance={},matrix_reads={},screened={:?},wrong_candidate_preserved={wrong_preserved}",
            provenance_name(supersede.provenance),
            supersede.matrix_reads,
            supersede.screened,
        )
        .unwrap();
    }

    for (k, rows) in [(256, &p7_256), (1024, &p7_1024)] {
        for (name, result) in rows {
            writeln!(
                out,
                "N12_P7,K={k},case={name},provenance={},bank={:?},matrix_reads={}",
                provenance_name(result.provenance),
                result.bank,
                result.matrix_reads,
            )
            .unwrap();
        }
    }

    for (k, rows) in [(256, p8_256), (1024, p8_1024)] {
        writeln!(
            out,
            "N12_P8,K={k},unwritten={},conflict={},incomplete={}",
            provenance_name(rows.0),
            provenance_name(rows.1),
            provenance_name(rows.2),
        )
        .unwrap();
    }

    writeln!(
        out,
        "N12_ACCOUNTING,verified={},missing={},conflict={},unverified={},matrix_reads={},primary_reads={},ordered_reads={},source_segments={},source_bytes={}",
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

    let h1 = (0..3).all(|slot| {
        let row = p1[&(1024, slot)];
        ppm(row.exact, row.n) >= 950_000 && row.verified == row.n && row.ordered == 0
    });
    let h2 = (0..3).all(|slot| {
        let row = p2[&(1024, slot)];
        ppm(row.exact, row.n) >= 950_000 && row.verified == row.n
    }) && p2[&(1024, 0)].primary == p2[&(1024, 0)].n
        && p2[&(1024, 1)].ordered == p2[&(1024, 1)].n
        && p2[&(1024, 2)].primary == p2[&(1024, 2)].n;
    let h3 = (0..2).all(|slot| {
        let row = p3[&(1024, slot)];
        ppm(row.exact, row.n) >= 950_000 && row.verified == row.n
    }) && p3[&(1024, 0)].primary == p3[&(1024, 0)].n
        && p3[&(1024, 1)].ordered == p3[&(1024, 1)].n;
    let h4 = p4_1024.values().all(|value| *value == 1);
    let h5 = p5_1024.provenance == Dkv4Provenance::Conflict
        && p5_1024.matrix_reads == 0
        && p5_1024.screened.is_none();
    let h6 = p6_1024.provenance == Dkv4Provenance::VerifiedPresent
        && p6_1024.screened == p6_1024.value
        && wrong_1024;
    let h7 = p7_1024
        .values()
        .all(|result| result.provenance == Dkv4Provenance::Unverified && result.matrix_reads == 0);
    let h8 = p8_1024.0 != Dkv4Provenance::VerifiedPresent
        && p8_1024.1 == Dkv4Provenance::Conflict
        && p8_1024.2 != Dkv4Provenance::VerifiedPresent;
    let p9row = p9[&1024];
    let h9 = p9row.0 == p9row.1 && p9row.0 > 0;
    let h10 = accounting.matrix_reads == accounting.verified;

    for (name, holds) in [
        ("H1_CLEAN_VERIFIED_CONTINUITY", h1),
        ("H2_EXPLICIT_OVERRIDE", h2),
        ("H3_PRIMARY_SWITCH_OVERRIDE", h3),
        ("H4_IDENTITY_NEGATIVES", h4),
        ("H5_CONFLICT_FAIL_CLOSED", h5),
        ("H6_SUPERSESSION_BOUNDARY", h6),
        ("H7_UNVERIFIED_FAIL_CLOSED", h7),
        ("H8_ALIAS_BOUNDARY", h8),
        ("H9_PROVENANCE_CORRECTNESS_SEPARATION", h9),
        ("H10_ONE_BANK_VERIFIED_READ", h10),
    ] {
        writeln!(
            out,
            "N12_HYPOTHESIS,{name},{}",
            if holds { "HOLDS" } else { "REFUTED" }
        )
        .unwrap();
    }

    let selected = h1 && h2 && h3 && h4 && h5 && h6 && h7 && h8 && h9 && h10;
    writeln!(
        out,
        "N12_SELECTION,verified_two_bank={}",
        if selected { "RETAIN" } else { "NONE" }
    )
    .unwrap();
    writeln!(out, "PROMOTION,BLOCKED,n12_development_identity_only").unwrap();

    print!("{out}");
}

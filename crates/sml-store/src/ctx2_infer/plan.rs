//! Arms, CONTEXT-2 prompt assembly, plan hashing and the enforced call ledger
//! (prereg sections 4, 8.1, 11). Model-free: histories are bytes; token IDs are
//! supplied later by the pinned runtime and audited in [`super::audit`].

use std::collections::BTreeMap;
use std::sync::Arc;

use sml_core::context_ledger::{
    ContextContentHash, ContextSegmentId, ContextSegmentRef, ContextStreamId,
};
use sml_core::elastic_ngram_paged_sha::sha256;

use super::compute::ComputeEnvelope;
use super::generator::{generate_seed, Session, Split, Turn};
use super::restart::{context2_restart_history, RestartControl};
use super::{dev_seeds, first_u64_le, hex, Ctx2InferError, PreregFreeze};
use crate::raw_context::{
    build_raw_segment_artifact, parse_raw_segment_artifact, sha256_raw_bytes, RawContextStoreLimits,
};
use crate::working_context::{WorkingContext, WorkingPolicy, WorkingSegment};

/// Segment budgets `k` (prereg section 4).
pub const K_VALUES: [u32; 3] = [4, 8, 16];
/// Raw-store admission limits for one turn artifact.
pub const RAW_LIMITS: RawContextStoreLimits = RawContextStoreLimits {
    max_file_bytes: 4096,
    max_raw_bytes: 1024,
    max_semantic_links: 0,
};

/// Arms of the prereg (section 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Arm {
    /// No history.
    A0,
    /// Strong no-CONTEXT-2 baseline: last `B(s, k)` token IDs of the full history.
    A1,
    /// Strong no-CONTEXT-2 baseline: last `k` turns.
    A2,
    /// CONTEXT-2 `CapacityBounded{k}` through the raw store.
    A3,
    /// A3 with seal / open / anchored restore at turns 12, 24, 36.
    A3r,
    /// Full 48-turn reference (not budget-matched).
    R,
}

impl Arm {
    pub const ALL: [Self; 6] = [Self::A0, Self::A1, Self::A2, Self::A3, Self::A3r, Self::R];
    /// Arms that depend on `k`.
    pub const BUDGETED: [Self; 4] = [Self::A1, Self::A2, Self::A3, Self::A3r];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::A0 => "A0",
            Self::A1 => "A1",
            Self::A2 => "A2",
            Self::A3 => "A3",
            Self::A3r => "A3r",
            Self::R => "R",
        }
    }

    /// Whether the arm uses CONTEXT-2.
    #[must_use]
    pub const fn uses_context2(self) -> bool {
        matches!(self, Self::A3 | Self::A3r)
    }
}

/// Execution phase of a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Phase {
    /// Determinism precheck: R on dev seed 0, run 0 or 1.
    Precheck { run: u8 },
    /// Preregistered test grid.
    Test,
    /// CI-only smoke (GitHub-hosted, not the test host). Run 1 repeats R.
    Smoke { run: u8 },
}

impl Phase {
    fn tag(self) -> String {
        match self {
            Self::Precheck { run } => format!("precheck{run}"),
            Self::Test => "test".to_owned(),
            Self::Smoke { run } => format!("smoke{run}"),
        }
    }
}

/// Identity of one planned model call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallKey {
    pub phase: Phase,
    pub seed: u64,
    pub session: u32,
    pub query: u8,
    pub arm: Arm,
    /// `None` for A0 and R (deduplicated over `k`).
    pub k: Option<u32>,
}

impl CallKey {
    /// Group shared by every arm of the same query (template invariant).
    #[must_use]
    pub const fn query_group(&self) -> (Phase, u64, u32, u8) {
        (self.phase, self.seed, self.session, self.query)
    }
}

/// One planned prompt (history bytes only; tokens come later).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptRecord {
    pub key: CallKey,
    pub gold: String,
    pub distance: u32,
    pub query_text: String,
    /// Segment IDs whose bytes form `history` (A1: all 48; truncation is token-level).
    pub segment_ids: Vec<u64>,
    pub history: Arc<[u8]>,
    pub history_sha256: [u8; 32],
    pub query_sha256: [u8; 32],
}

/// In-memory CONTEXT-1a raw store: SMLCTX1A artifacts keyed by segment identity.
#[derive(Debug, Clone, Default)]
pub struct InMemoryRawStore {
    artifacts: BTreeMap<(u64, u64), Vec<u8>>,
}

impl InMemoryRawStore {
    /// Publish every turn of `session` as an immutable raw artifact.
    pub fn publish_session(session: &Session) -> Result<Self, Ctx2InferError> {
        let stream = stream_id(session)?;
        let mut store = Self::default();
        let mut byte_start = 0_u64;
        for turn in &session.turns {
            let id = segment_id(stream, turn)?;
            let hash = content_hash(&turn.bytes)?;
            let len = turn.bytes.len() as u64;
            let reference = ContextSegmentRef::new(
                id,
                byte_start,
                len,
                0,
                0,
                u64::from(turn.number),
                hash,
                Vec::new(),
            )
            .map_err(|e| Ctx2InferError::Context(format!("{e:?}")))?;
            byte_start += len;
            let artifact = build_raw_segment_artifact(&reference, &turn.bytes).map_err(|e| {
                Ctx2InferError::RawStore {
                    stream_id: stream.get(),
                    segment_id: id.segment_id,
                    reason: format!("{e:?}"),
                }
            })?;
            store
                .artifacts
                .insert((stream.get(), id.segment_id), artifact);
        }
        Ok(store)
    }

    /// Load, parse and hash-verify one segment against `expected`. Fail closed.
    pub fn load_verified(
        &self,
        id: ContextSegmentId,
        expected: &ContextContentHash,
    ) -> Result<Vec<u8>, Ctx2InferError> {
        let stream_id = id.stream_id.get();
        let segment_id = id.segment_id;
        let bytes =
            self.artifacts
                .get(&(stream_id, segment_id))
                .ok_or(Ctx2InferError::MissingSegment {
                    stream_id,
                    segment_id,
                })?;
        let fail = |reason: String| Ctx2InferError::RawStore {
            stream_id,
            segment_id,
            reason,
        };
        let seg =
            parse_raw_segment_artifact(bytes, RAW_LIMITS).map_err(|e| fail(format!("{e:?}")))?;
        if seg.reference.identity != id {
            return Err(fail("identity mismatch".to_owned()));
        }
        let actual = content_hash(&seg.raw_bytes)?;
        if actual != seg.reference.content_hash || actual != *expected {
            return Err(fail("content hash mismatch".to_owned()));
        }
        Ok(seg.raw_bytes)
    }

    /// Negative control only: XOR one byte of a stored artifact.
    pub fn corrupt_for_negative_control(&mut self, stream_id: u64, segment_id: u64, pos: usize) {
        if let Some(a) = self.artifacts.get_mut(&(stream_id, segment_id)) {
            if let Some(b) = a.get_mut(pos) {
                *b ^= 0x01;
            }
        }
    }

    /// Negative control only: remove a stored artifact.
    pub fn remove_for_negative_control(&mut self, stream_id: u64, segment_id: u64) {
        self.artifacts.remove(&(stream_id, segment_id));
    }

    /// Negative control only: replace an artifact with another segment's bytes.
    pub fn swap_for_negative_control(&mut self, stream_id: u64, a: u64, b: u64) {
        let (Some(x), Some(y)) = (
            self.artifacts.get(&(stream_id, a)).cloned(),
            self.artifacts.get(&(stream_id, b)).cloned(),
        ) else {
            return;
        };
        self.artifacts.insert((stream_id, a), y);
        self.artifacts.insert((stream_id, b), x);
    }
}

/// Stream ID of a session: `index + 1` (zero is reserved).
pub fn stream_id(session: &Session) -> Result<ContextStreamId, Ctx2InferError> {
    ContextStreamId::new(u64::from(session.index) + 1)
        .map_err(|e| Ctx2InferError::Context(format!("{e:?}")))
}

/// Segment ID of a turn: its one-based number.
pub fn segment_id(
    stream: ContextStreamId,
    turn: &Turn,
) -> Result<ContextSegmentId, Ctx2InferError> {
    ContextSegmentId::new(stream, u64::from(turn.number))
        .map_err(|e| Ctx2InferError::Context(format!("{e:?}")))
}

fn content_hash(bytes: &[u8]) -> Result<ContextContentHash, Ctx2InferError> {
    sha256_raw_bytes(bytes).map_err(|e| Ctx2InferError::Context(format!("{e:?}")))
}

/// Fingerprint = first 8 bytes LE of the content hash, forced non-zero.
#[must_use]
pub fn fingerprint(hash: &ContextContentHash) -> u64 {
    match first_u64_le(hash.bytes()) {
        0 => 1,
        v => v,
    }
}

/// Admit one turn into a working context (prereg section 11).
pub fn admit_turn(
    ctx: &mut WorkingContext,
    stream: ContextStreamId,
    turn: &Turn,
) -> Result<(), Ctx2InferError> {
    let hash = content_hash(&turn.bytes)?;
    let seg = WorkingSegment::new(
        segment_id(stream, turn)?,
        u64::from(turn.number),
        hash,
        fingerprint(&hash),
    )
    .map_err(|e| Ctx2InferError::Context(format!("{e:?}")))?;
    ctx.admit(seg)
        .map_err(|e| Ctx2InferError::Context(format!("{e:?}")))?;
    Ok(())
}

/// `retained_identities()` → load → verify → concatenate in admission order.
pub fn assemble_retained(
    ctx: &WorkingContext,
    store: &InMemoryRawStore,
) -> Result<(Vec<u64>, Vec<u8>), Ctx2InferError> {
    let hashes: BTreeMap<u64, ContextContentHash> = ctx
        .accounting()
        .into_iter()
        .map(|a| (a.identity.segment_id, a.content_hash))
        .collect();
    let mut ids = Vec::new();
    let mut bytes = Vec::new();
    for id in ctx.retained_identities() {
        let expected = hashes
            .get(&id.segment_id)
            .ok_or(Ctx2InferError::MissingSegment {
                stream_id: id.stream_id.get(),
                segment_id: id.segment_id,
            })?;
        bytes.extend_from_slice(&store.load_verified(id, expected)?);
        ids.push(id.segment_id);
    }
    Ok((ids, bytes))
}

/// A3: CONTEXT-2 `CapacityBounded{k}`, every turn admitted.
pub fn context2_history(
    session: &Session,
    k: u32,
    store: &InMemoryRawStore,
) -> Result<(Vec<u64>, Vec<u8>), Ctx2InferError> {
    let stream = stream_id(session)?;
    let mut ctx = WorkingContext::new(
        stream,
        WorkingPolicy::CapacityBounded {
            capacity: k as usize,
        },
    )
    .map_err(|e| Ctx2InferError::Context(format!("{e:?}")))?;
    for turn in &session.turns {
        admit_turn(&mut ctx, stream, turn)?;
    }
    assemble_retained(&ctx, store)
}

/// A2: last `k` turns concatenated in order; no CONTEXT-2.
#[must_use]
pub fn sliding_window_history(session: &Session, k: u32) -> (Vec<u64>, Vec<u8>) {
    let start = session.turns.len().saturating_sub(k as usize);
    let turns = &session.turns[start..];
    (
        turns.iter().map(|t| u64::from(t.number)).collect(),
        turns.iter().flat_map(|t| t.bytes.iter().copied()).collect(),
    )
}

/// R and A1 source: all turns.
#[must_use]
pub fn full_history(session: &Session) -> (Vec<u64>, Vec<u8>) {
    sliding_window_history(session, session.turns.len() as u32)
}

/// H3a: the gold value of an evicted query (`d > k`) must be absent from history bytes.
#[must_use]
pub fn leaks_gold(history: &[u8], gold: &str) -> bool {
    let g = gold.as_bytes();
    !g.is_empty() && history.windows(g.len()).any(|w| w == g)
}

/// Segment IDs and shared history bytes of one arm.
pub type History = (Vec<u64>, Arc<[u8]>);

/// Histories of one session for every arm and `k`.
#[derive(Debug, Clone)]
pub struct SessionHistories {
    pub full: History,
    pub by_arm_k: BTreeMap<(Arm, u32), History>,
}

/// Compute all histories of one session. A3r uses `control` (normally `None`).
pub fn session_histories(
    session: &Session,
    store: &InMemoryRawStore,
    control: &RestartControl,
) -> Result<SessionHistories, Ctx2InferError> {
    let (fids, fbytes) = full_history(session);
    let mut by_arm_k = BTreeMap::new();
    for k in K_VALUES {
        let (ids, b) = sliding_window_history(session, k);
        by_arm_k.insert((Arm::A2, k), (ids, Arc::from(b)));
        let (ids, b) = context2_history(session, k, store)?;
        by_arm_k.insert((Arm::A3, k), (ids, Arc::from(b)));
        let (ids, b) = context2_restart_history(session, k, store, control)?.history;
        by_arm_k.insert((Arm::A3r, k), (ids, Arc::from(b)));
    }
    Ok(SessionHistories {
        full: (fids, Arc::from(fbytes)),
        by_arm_k,
    })
}

fn record(
    key: CallKey,
    session: &Session,
    query: usize,
    ids: &[u64],
    history: &Arc<[u8]>,
) -> PromptRecord {
    let q = &session.queries[query];
    PromptRecord {
        key,
        gold: q.gold.clone(),
        distance: q.distance,
        query_text: q.text.clone(),
        segment_ids: ids.to_vec(),
        history: Arc::clone(history),
        history_sha256: sha256(history),
        query_sha256: sha256(q.text.as_bytes()),
    }
}

/// The full call plan with its content digest.
#[derive(Debug, Clone)]
pub struct Plan {
    pub records: Vec<PromptRecord>,
    pub digest: [u8; 32],
}

/// Canonical one-line serialization of a record (digest input).
#[must_use]
pub fn canonical_line(r: &PromptRecord) -> String {
    let k = r.key.k.map_or_else(|| "-".to_owned(), |k| k.to_string());
    let ids: Vec<String> = r.segment_ids.iter().map(u64::to_string).collect();
    format!(
        "{}|{:#018x}|{}|{}|{}|{}|{}|{}|{}|{}|{}\n",
        r.key.phase.tag(),
        r.key.seed,
        r.key.session,
        r.key.query,
        r.key.arm.name(),
        k,
        hex(&r.history_sha256),
        hex(&r.query_sha256),
        r.gold,
        r.distance,
        ids.join(",")
    )
}

/// SHA-256 over `"CTX2-INFER-plan-v1\n"` and every canonical line in order.
#[must_use]
pub fn plan_digest(records: &[PromptRecord]) -> [u8; 32] {
    let mut buf = b"CTX2-INFER-plan-v1\n".to_vec();
    for r in records {
        buf.extend_from_slice(canonical_line(r).as_bytes());
    }
    sha256(&buf)
}

impl Plan {
    fn new(records: Vec<PromptRecord>) -> Self {
        let digest = plan_digest(&records);
        Self { records, digest }
    }

    /// Recompute the digest and compare with a pinned value. Fail closed.
    pub fn verify_digest(&self, expected: &[u8; 32]) -> Result<(), Ctx2InferError> {
        if plan_digest(&self.records) == *expected && self.digest == *expected {
            Ok(())
        } else {
            Err(Ctx2InferError::PlanDigestMismatch)
        }
    }

    /// Model-free structural summary (H1, H2p, H3a on prompt bytes).
    #[must_use]
    pub fn summary(&self) -> PlanSummary {
        let mut by_key: BTreeMap<CallKey, &PromptRecord> = BTreeMap::new();
        for r in &self.records {
            by_key.insert(r.key, r);
        }
        let mut s = PlanSummary::default();
        for r in &self.records {
            let Some(k) = r.key.k else { continue };
            let twin = |arm| {
                by_key
                    .get(&CallKey { arm, ..r.key })
                    .map(|t| t.history_sha256)
            };
            match r.key.arm {
                Arm::A3 => {
                    s.h1_total += 1;
                    if twin(Arm::A2) == Some(r.history_sha256) {
                        s.h1_identical += 1;
                    }
                }
                Arm::A3r => {
                    s.h2p_total += 1;
                    if twin(Arm::A3) == Some(r.history_sha256) {
                        s.h2p_identical += 1;
                    }
                }
                _ => {}
            }
            if matches!(r.key.arm, Arm::A2 | Arm::A3 | Arm::A3r) && r.distance > k {
                s.h3a_checked += 1;
                if leaks_gold(&r.history, &r.gold) {
                    s.h3a_leaks += 1;
                }
            }
        }
        s
    }
}

/// Counts of the structural (prompt-level) invariants. No model output involved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlanSummary {
    pub h1_identical: u64,
    pub h1_total: u64,
    pub h2p_identical: u64,
    pub h2p_total: u64,
    pub h3a_leaks: u64,
    pub h3a_checked: u64,
}

fn push_session_test_calls(
    out: &mut Vec<PromptRecord>,
    phase: Phase,
    session: &Session,
    h: &SessionHistories,
    k_values: &[u32],
) {
    let (fids, fbytes) = &h.full;
    let base = |query: u8, arm, k| CallKey {
        phase,
        seed: session.seed,
        session: session.index,
        query,
        arm,
        k,
    };
    for q in 0..session.queries.len() {
        let qi = q as u8;
        out.push(record(
            base(qi, Arm::A0, None),
            session,
            q,
            &[],
            &Arc::from(Vec::new()),
        ));
        out.push(record(base(qi, Arm::R, None), session, q, fids, fbytes));
        for &k in k_values {
            out.push(record(base(qi, Arm::A1, Some(k)), session, q, fids, fbytes));
            for arm in [Arm::A2, Arm::A3, Arm::A3r] {
                let (ids, b) = &h.by_arm_k[&(arm, k)];
                out.push(record(base(qi, arm, Some(k)), session, q, ids, b));
            }
        }
    }
}

/// Precheck + test grid for explicit seeds. Use [`build_frozen_plan`] for the
/// preregistered run; this entry point exists for tests and dev harnessing.
pub fn build_grid(test_seeds: &[u64], precheck_seed: u64) -> Result<Plan, Ctx2InferError> {
    let mut records = Vec::new();
    let dev = generate_seed(precheck_seed, Split::Dev)?;
    for run in 0..2 {
        for s in &dev {
            let (fids, fbytes) = full_history(s);
            let fbytes: Arc<[u8]> = Arc::from(fbytes);
            for q in 0..s.queries.len() {
                let key = CallKey {
                    phase: Phase::Precheck { run },
                    seed: s.seed,
                    session: s.index,
                    query: q as u8,
                    arm: Arm::R,
                    k: None,
                };
                records.push(record(key, s, q, &fids, &fbytes));
            }
        }
    }
    for &seed in test_seeds {
        for s in generate_seed(seed, Split::Test)? {
            let store = InMemoryRawStore::publish_session(&s)?;
            let h = session_histories(&s, &store, &RestartControl::Clean)?;
            push_session_test_calls(&mut records, Phase::Test, &s, &h, &K_VALUES);
        }
    }
    Ok(Plan::new(records))
}

/// The preregistered plan: needs an attested merge SHA; call count enforced.
pub fn build_frozen_plan(freeze: &PreregFreeze) -> Result<Plan, Ctx2InferError> {
    let seeds = freeze.test_seeds()?;
    let plan = build_grid(&seeds, dev_seeds()[0])?;
    let expected = ComputeEnvelope::FROZEN.admitted_host_calls as usize;
    if plan.records.len() != expected {
        return Err(Ctx2InferError::PlanCallCount {
            expected,
            actual: plan.records.len(),
        });
    }
    Ok(plan)
}

/// CI-only smoke plan: dev seed, 2 sessions, every arm at `k = 4`, plus one
/// repeat of R (prereg section 11; 84 calls). Not a measurement.
pub fn build_smoke_plan(dev_seed: u64) -> Result<Plan, Ctx2InferError> {
    let mut records = Vec::new();
    let sessions: Vec<Session> = generate_seed(dev_seed, Split::Dev)?
        .into_iter()
        .take(2)
        .collect();
    for s in &sessions {
        let store = InMemoryRawStore::publish_session(s)?;
        let h = session_histories(s, &store, &RestartControl::Clean)?;
        push_session_test_calls(&mut records, Phase::Smoke { run: 0 }, s, &h, &[4]);
    }
    for s in &sessions {
        let (fids, fbytes) = full_history(s);
        let fbytes: Arc<[u8]> = Arc::from(fbytes);
        for q in 0..s.queries.len() {
            let key = CallKey {
                phase: Phase::Smoke { run: 1 },
                seed: s.seed,
                session: s.index,
                query: q as u8,
                arm: Arm::R,
                k: None,
            };
            records.push(record(key, s, q, &fids, &fbytes));
        }
    }
    Ok(Plan::new(records))
}

/// State of one planned call in the ledger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallState {
    Pending,
    InFlight { attempt: u8 },
    RetryAllowed,
    Done { attempts: u8 },
}

/// Enforces the plan: no unplanned call, no duplicate, at most one retry after
/// a transport failure (identical token array), and a final count equal to the
/// plan (prereg section 8.1).
#[derive(Debug, Clone)]
pub struct CallLedger {
    states: BTreeMap<CallKey, CallState>,
    retries: u64,
}

impl CallLedger {
    #[must_use]
    pub fn new(plan: &Plan) -> Self {
        Self {
            states: plan
                .records
                .iter()
                .map(|r| (r.key, CallState::Pending))
                .collect(),
            retries: 0,
        }
    }

    fn err(msg: &str, key: &CallKey) -> Ctx2InferError {
        Ctx2InferError::Ledger(format!("{msg}: {key:?}"))
    }

    /// Authorize issuing `key`.
    pub fn issue(&mut self, key: &CallKey) -> Result<u8, Ctx2InferError> {
        let state = self
            .states
            .get_mut(key)
            .ok_or_else(|| Self::err("unplanned call", key))?;
        match *state {
            CallState::Pending => {
                *state = CallState::InFlight { attempt: 1 };
                Ok(1)
            }
            CallState::RetryAllowed => {
                *state = CallState::InFlight { attempt: 2 };
                self.retries += 1;
                Ok(2)
            }
            CallState::InFlight { .. } => Err(Self::err("call already in flight", key)),
            CallState::Done { .. } => Err(Self::err("duplicate call", key)),
        }
    }

    /// Record a transport-level failure. The second one is unrecoverable.
    pub fn transport_failure(&mut self, key: &CallKey) -> Result<(), Ctx2InferError> {
        let state = self
            .states
            .get_mut(key)
            .ok_or_else(|| Self::err("unplanned call", key))?;
        match *state {
            CallState::InFlight { attempt: 1 } => {
                *state = CallState::RetryAllowed;
                Ok(())
            }
            CallState::InFlight { .. } => Err(Self::err("unrecoverable runner failure", key)),
            _ => Err(Self::err("no call in flight", key)),
        }
    }

    /// Record a completed call.
    pub fn complete(&mut self, key: &CallKey) -> Result<(), Ctx2InferError> {
        let state = self
            .states
            .get_mut(key)
            .ok_or_else(|| Self::err("unplanned call", key))?;
        match *state {
            CallState::InFlight { attempt } => {
                *state = CallState::Done { attempts: attempt };
                Ok(())
            }
            _ => Err(Self::err("no call in flight", key)),
        }
    }

    /// Final check: every planned call done exactly once. Returns `(calls, retries)`.
    pub fn finish(&self) -> Result<(u64, u64), Ctx2InferError> {
        let done = self
            .states
            .values()
            .filter(|s| matches!(s, CallState::Done { .. }))
            .count();
        if done != self.states.len() {
            return Err(Ctx2InferError::PlanCallCount {
                expected: self.states.len(),
                actual: done,
            });
        }
        Ok((done as u64, self.retries))
    }
}

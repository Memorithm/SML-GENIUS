//! The continual-learning loop:
//! OBSERVE → IDENTIFY GAP → TEACH → LEARN → EXAMINE → VALIDATE → CONSOLIDATE →
//! REVISE → REPEAT.
//!
//! This first engine is wired to the native ANF induction learner
//! ([`crate::anf_lesson`]) because it has an exact, independent oracle. The
//! lesson domain tags (French, English, Mathematics, Rust) are used **only** to
//! exercise per-discipline lanes, fairness and telemetry with an identical
//! load; every lesson in [`anf_curriculum`] is a Mathematics/M2 Boolean-algebra
//! item and must not be reported as linguistic or Rust knowledge.
//!
//! Roles stay separate: the teacher proposes labelled rows, an independent
//! oracle filters them, the learner fits an ANF, the examiner
//! ([`crate::mastery::certify`]) applies the fixed 100 % rule, and the store
//! publishes transactionally. Learning runs in parallel; publication is a
//! single transaction per wave.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::anf_lesson::{
    decode_checkpoint, encode_checkpoint, exam, learn, monomials, partition, random_task, AnfTask,
    LearnOutcome, Partitions,
};
use crate::domain::{Domain, KnowledgeState, Lane};
use crate::executor::{run_parallel, CancellationToken, JobOutcome};
use crate::hash::{Canonical, Digest};
use crate::journal::{Journal, JournalError};
use crate::lesson::{ErrorStage, LessonId, LessonSpec, SourceRef};
use crate::mastery::{certify, check_partitions, Cohort, CohortResult, ExamReport};
use crate::registry::{LessonRegistry, RegistryError};
use crate::resources::{Lease, Machine, MachineRole, ResourceManager, Resources};
use crate::scheduler::{priority, AimdLimit, Scheduler, SubmitError};
use crate::store::{KnowledgeStore, StagedEntry, StoreError};
use crate::teacher::{
    admit_batch, LessonRequest, Oracle, Proposal, Residency, Teacher, TeacherKind, TeacherLedger,
    Verdict,
};
use crate::telemetry::Telemetry;

/// Parameters of one ANF lesson. Everything is derived deterministically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnfLessonParams {
    pub inputs: u8,
    pub degree: u8,
    pub seed: u64,
    /// TRAIN rows requested on the first attempt; doubled on each retry.
    pub base_train: usize,
    pub max_train: usize,
    pub dev: usize,
    pub test: usize,
    pub testtest: usize,
}

impl AnfLessonParams {
    fn encode(&self) -> Vec<u8> {
        let mut c = Vec::new();
        c.extend_from_slice(b"SML_TUTOR_ANF_PARAMS_V1");
        c.push(self.inputs);
        c.push(self.degree);
        for v in [
            self.seed,
            self.base_train as u64,
            self.max_train as u64,
            self.dev as u64,
            self.test as u64,
            self.testtest as u64,
        ] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        c
    }

    pub fn task(&self) -> AnfTask {
        random_task(self.inputs, self.degree, self.seed)
    }

    pub fn partitions(&self) -> Result<Partitions, EngineError> {
        partition(
            self.inputs,
            self.max_train,
            self.dev,
            self.test,
            self.testtest,
            self.seed ^ 0x5bd1_e995,
        )
        .map_err(|e| EngineError::Curriculum(e.to_string()))
    }

    pub fn unknowns(&self) -> usize {
        monomials(self.inputs, self.degree).len()
    }
}

pub struct AnfCurriculum {
    pub specs: Vec<LessonSpec>,
    pub params: BTreeMap<LessonId, AnfLessonParams>,
}

/// Knowledge key under which a lesson's checkpoint is published.
pub fn knowledge_key(lesson: &LessonId) -> Digest {
    let mut c = Canonical::new("SML_TUTOR_ANF_KNOWLEDGE_KEY_V1");
    c.digest(lesson);
    c.finish()
}

/// Builds an engine-qualification curriculum: `per_domain` lessons tagged with
/// each of the four domains (identical load), with intra-domain chains and
/// cross-domain prerequisite edges. Every 5th lesson starts deliberately
/// under-determined (TRAIN smaller than the number of unknowns) so that the
/// abstain → gap → re-teach loop is exercised.
pub fn anf_curriculum(per_domain: usize, inputs: u8, degree: u8, seed: u64) -> AnfCurriculum {
    let unknowns = monomials(inputs, degree).len();
    let space = 1usize << inputs;
    let held = (space / 8).max(1);
    let max_train = space.saturating_sub(3 * held);
    let mut specs = Vec::new();
    let mut params = BTreeMap::new();
    let mut last: BTreeMap<Domain, LessonId> = BTreeMap::new();
    let source = SourceRef {
        name: "sml-tutor-anf-generator".into(),
        version: "1".into(),
        license: "PolyForm-Noncommercial-1.0.0".into(),
        authors: "Memorithm".into(),
        sha256: Digest::of(b"sml_tutor::anf_lesson::random_task/splitmix64"),
    };
    for i in 0..per_domain {
        for domain in Domain::ALL {
            let p = AnfLessonParams {
                inputs,
                degree,
                seed: seed
                    .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                    .wrapping_add((i as u64) << 8 | domain as u64),
                base_train: if i % 5 == 3 {
                    unknowns / 2
                } else {
                    (unknowns * 3 / 2).min(max_train)
                },
                max_train,
                dev: held,
                test: held,
                testtest: held,
            };
            let mut prerequisites = Vec::new();
            if i % 4 != 0 {
                if let Some(prev) = last.get(&domain) {
                    prerequisites.push(*prev);
                }
            }
            if i % 8 == 5 && domain != Domain::Mathematics {
                if let Some(m) = last.get(&Domain::Mathematics) {
                    prerequisites.push(*m);
                }
            }
            prerequisites.dedup();
            let spec = LessonSpec {
                domain,
                level: "ENGINE-QUAL-M2-ANF".into(),
                skill: format!("anf-n{inputs}-d{degree}-{}-{i}", domain.name()),
                prerequisites,
                source: source.clone(),
                payload: Digest::of(&p.encode()),
                difficulty: (p.base_train * 1000 / unknowns.max(1)).min(1000) as u16,
                scientific_value: 100,
                expected_gain: 100,
                user_priority: 0,
            };
            let id = spec.id();
            last.insert(domain, id);
            params.insert(id, p);
            specs.push(spec);
        }
    }
    AnfCurriculum { specs, params }
}

/// Deterministic teacher: labels TRAIN rows from the true function, but lies on
/// one row out of `lie_every` (0 = never). It is never trusted.
pub struct AnfTeacher {
    params: Arc<BTreeMap<LessonId, AnfLessonParams>>,
    lie_every: u64,
}

const ALL_DOMAINS: [Domain; 4] = Domain::ALL;

impl Teacher for AnfTeacher {
    fn name(&self) -> &str {
        "anf-generator-teacher"
    }
    fn kind(&self) -> TeacherKind {
        TeacherKind::MathSolver
    }
    fn residency(&self) -> Residency {
        Residency::Resident
    }
    fn domains(&self) -> &[Domain] {
        &ALL_DOMAINS
    }
    fn propose_batch(&self, requests: &[LessonRequest]) -> Vec<Proposal> {
        let mut out = Vec::new();
        for r in requests {
            let Some(p) = self.params.get(&r.lesson) else {
                continue;
            };
            let Ok(parts) = p.partitions() else {
                continue;
            };
            let task = p.task();
            let monos = monomials(p.inputs, p.degree);
            for &x in parts.train.iter().take(r.count as usize) {
                let mut bit = crate::anf_lesson::evaluate(&monos, &task.target, x);
                let item_key = Digest::of(&[r.lesson.0.as_slice(), &x.to_le_bytes()].concat());
                if self.lie_every > 0
                    && u64::from_le_bytes(item_key.0[..8].try_into().unwrap_or([0; 8]))
                        % self.lie_every
                        == 0
                {
                    bit = !bit;
                }
                out.push(Proposal {
                    lesson: r.lesson,
                    teacher: self.name().to_string(),
                    item_key,
                    payload: x.to_le_bytes().to_vec(),
                    claimed_answer: vec![u8::from(bit)],
                });
            }
        }
        out
    }
}

/// Independent exact oracle: re-evaluates the generator's function.
pub struct AnfOracle {
    params: Arc<BTreeMap<LessonId, AnfLessonParams>>,
}

impl Oracle for AnfOracle {
    fn name(&self) -> &str {
        "anf-exact-oracle"
    }
    fn verify(&self, proposal: &Proposal) -> Verdict {
        let Some(p) = self.params.get(&proposal.lesson) else {
            return Verdict::Reject("unknown lesson".into());
        };
        let Ok(bytes) = <[u8; 4]>::try_from(proposal.payload.as_slice()) else {
            return Verdict::Reject("malformed payload".into());
        };
        let x = u32::from_le_bytes(bytes);
        let task = p.task();
        let truth = crate::anf_lesson::evaluate(&monomials(p.inputs, p.degree), &task.target, x);
        match proposal.claimed_answer.as_slice() {
            [b] if (*b == 1) == truth => Verdict::Accept,
            [_] => Verdict::Reject("wrong label".into()),
            _ => Verdict::Reject("malformed answer".into()),
        }
    }
}

#[derive(Debug)]
pub enum EngineError {
    Registry(RegistryError),
    Journal(JournalError),
    Store(StoreError),
    Curriculum(String),
    ForeignJournal(LessonId),
}

impl core::fmt::Display for EngineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EngineError::Registry(e) => write!(f, "registry: {e}"),
            EngineError::Journal(e) => write!(f, "journal: {e}"),
            EngineError::Store(e) => write!(f, "store: {e}"),
            EngineError::Curriculum(e) => write!(f, "curriculum: {e}"),
            EngineError::ForeignJournal(l) => {
                write!(f, "journal mentions lesson {} not in curriculum", l.short())
            }
        }
    }
}

impl std::error::Error for EngineError {}

impl From<RegistryError> for EngineError {
    fn from(e: RegistryError) -> Self {
        EngineError::Registry(e)
    }
}
impl From<JournalError> for EngineError {
    fn from(e: JournalError) -> Self {
        EngineError::Journal(e)
    }
}
impl From<StoreError> for EngineError {
    fn from(e: StoreError) -> Self {
        EngineError::Store(e)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// Worker threads for the learning phase (1 = sequential baseline).
    pub workers: usize,
    /// Lessons dispatched per wave (kept identical across worker counts so
    /// sequential and parallel runs do the same work and the same commits).
    pub wave: usize,
    pub max_attempts: u32,
    /// Teacher lies on one TRAIN row out of this many (0 = honest).
    pub teacher_lie_every: u64,
    /// Test hook: stop abruptly after this many waves, leaving in-flight
    /// lessons in the journal exactly as a crash would.
    pub crash_after_waves: Option<usize>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            workers: 1,
            wave: 64,
            max_attempts: 6,
            teacher_lie_every: 0,
            crash_after_waves: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResumeReport {
    pub replayed_records: u64,
    pub torn_tail: bool,
    pub interrupted_reset: usize,
    pub consolidated_missing_from_store: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RunSummary {
    pub waves: usize,
    pub wall_nanos: u128,
    pub learn_phase_nanos: u128,
    pub stolen: u64,
    pub crashed: bool,
    pub store_version: u64,
    pub states: BTreeMap<KnowledgeState, usize>,
}

struct Job {
    lane: Lane,
    id: LessonId,
    attempt: u32,
    lease: Lease,
    prereqs: Vec<(Digest, Vec<u8>, AnfLessonParams)>,
}

struct AttemptOutput {
    report: ExamReport,
    checkpoint: Option<Vec<u8>>,
    verdicts: Vec<Verdict>,
    teach_nanos: u128,
    learn_nanos: u128,
    validate_nanos: u128,
    abstained: bool,
    detail: String,
}

pub struct Engine {
    registry: LessonRegistry,
    scheduler: Scheduler,
    store: KnowledgeStore,
    journal: Journal,
    resources: ResourceManager,
    params: Arc<BTreeMap<LessonId, AnfLessonParams>>,
    teacher: AnfTeacher,
    oracle: AnfOracle,
    pub ledger: TeacherLedger,
    pub telemetry: Telemetry,
    cfg: EngineConfig,
    root: PathBuf,
}

fn journal_transition(
    registry: &mut LessonRegistry,
    journal: &mut Journal,
    id: &LessonId,
    to: KnowledgeState,
) -> Result<(), EngineError> {
    let from = registry.transition(id, to)?;
    journal.append(id, from, to)?;
    Ok(())
}

impl Engine {
    /// Opens (or resumes) an engine rooted at `root`.
    pub fn open(
        root: &Path,
        curriculum: AnfCurriculum,
        cfg: EngineConfig,
    ) -> Result<(Self, ResumeReport), EngineError> {
        std::fs::create_dir_all(root).map_err(|e| EngineError::Store(StoreError::Io(e)))?;
        let store = KnowledgeStore::open(root.join("store"))?;
        let (mut journal, replay) = Journal::open(&root.join("journal.log"))?;
        let mut registry = LessonRegistry::new();
        for spec in curriculum.specs {
            registry.register(spec)?;
        }
        for id in replay.states.keys() {
            if registry.get(id).is_none() {
                return Err(EngineError::ForeignJournal(*id));
            }
        }
        let mut report = ResumeReport {
            replayed_records: replay.records,
            torn_tail: replay.torn_tail,
            ..ResumeReport::default()
        };
        let ids: Vec<LessonId> = registry.iter().map(|l| l.id).collect();
        let snapshot = store.snapshot()?;
        for id in ids {
            match replay.states.get(&id) {
                None => journal
                    .append(&id, KnowledgeState::Unknown, KnowledgeState::Identified)
                    .map(|_| ())?,
                Some(s) => registry.restore_state(&id, *s)?,
            }
            // Work lost in a crash is redone; acquisition is idempotent and
            // republication of an identical entry is a no-op.
            let state = registry.state(&id).unwrap_or(KnowledgeState::Unknown);
            let path: &[KnowledgeState] = match state {
                KnowledgeState::Acquiring | KnowledgeState::Validating => {
                    &[KnowledgeState::Identified]
                }
                KnowledgeState::Learned => {
                    &[KnowledgeState::Validating, KnowledgeState::Identified]
                }
                KnowledgeState::Certified => &[KnowledgeState::Identified],
                _ => &[],
            };
            if !path.is_empty() {
                report.interrupted_reset += 1;
            }
            for to in path {
                journal_transition(&mut registry, &mut journal, &id, *to)?;
            }
            if state == KnowledgeState::Consolidated
                && !snapshot.entries.contains_key(&knowledge_key(&id))
            {
                report.consolidated_missing_from_store += 1;
                journal_transition(
                    &mut registry,
                    &mut journal,
                    &id,
                    KnowledgeState::NeedsRevision,
                )?;
            }
        }
        journal.sync()?;
        let params = Arc::new(curriculum.params);
        let wave = cfg.wave.max(1);
        let resources = ResourceManager::new(vec![Machine {
            name: "local".into(),
            role: MachineRole::AiCompute,
            capacity: Resources {
                cpu_milli: 1000 * wave as u64,
                gpu_slots: 0,
                ram_mib: 64 * wave as u64,
                vram_mib: 0,
                storage_mib: 1024,
            },
            measured: true,
            online: true,
        }])
        .map_err(|e| EngineError::Curriculum(e.to_string()))?;
        Ok((
            Self {
                registry,
                scheduler: Scheduler::new(AimdLimit::new(wave, wave, wave)),
                store,
                journal,
                resources,
                teacher: AnfTeacher {
                    params: Arc::clone(&params),
                    lie_every: cfg.teacher_lie_every,
                },
                oracle: AnfOracle {
                    params: Arc::clone(&params),
                },
                params,
                ledger: TeacherLedger::default(),
                telemetry: Telemetry::new(),
                cfg,
                root: root.to_path_buf(),
            },
            report,
        ))
    }

    pub fn registry(&self) -> &LessonRegistry {
        &self.registry
    }

    pub fn store(&self) -> &KnowledgeStore {
        &self.store
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn enqueue_ready(&mut self) {
        let mut ready = self.registry.ready();
        ready.sort_by_key(|id| {
            std::cmp::Reverse(self.registry.get(id).map(priority).unwrap_or(i64::MIN))
        });
        for id in ready {
            let Some(l) = self.registry.get(&id) else {
                continue;
            };
            let lane = if l.state == KnowledgeState::NeedsRevision {
                Lane::Revision
            } else {
                l.spec.domain.acquisition_lane()
            };
            match self.scheduler.submit(lane, id, priority(l)) {
                Ok(()) | Err(SubmitError::Duplicate) | Err(SubmitError::Backpressure(_)) => {}
            }
        }
    }

    fn prereq_checkpoints(
        &self,
        id: &LessonId,
    ) -> Result<Vec<(Digest, Vec<u8>, AnfLessonParams)>, EngineError> {
        let mut out = Vec::new();
        for p in self.registry.graph().prerequisites(id) {
            let key = knowledge_key(p);
            let bytes = self.store.get(&key)?.ok_or_else(|| {
                EngineError::Curriculum(format!("prerequisite {} not in store", p.short()))
            })?;
            let params = *self
                .params
                .get(p)
                .ok_or_else(|| EngineError::Curriculum(format!("no params for {}", p.short())))?;
            out.push((key, bytes, params));
        }
        Ok(out)
    }

    /// Runs waves until no lesson is dispatchable (or the crash hook fires).
    pub fn run(&mut self, cancel: &CancellationToken) -> Result<RunSummary, EngineError> {
        let started = Instant::now();
        let mut summary = RunSummary::default();
        loop {
            if cancel.is_cancelled() {
                self.scheduler.cancel_queued();
                break;
            }
            self.enqueue_ready();
            let wave = self.scheduler.next_wave();
            if wave.is_empty() {
                break;
            }
            let mut jobs = Vec::with_capacity(wave.len());
            for (lane, id) in wave {
                let lease = match self.resources.acquire(Resources {
                    cpu_milli: 1000,
                    ram_mib: 64,
                    ..Resources::default()
                }) {
                    Ok(l) => l,
                    Err(_) => {
                        self.scheduler.complete(lane, &id, true);
                        continue;
                    }
                };
                // Prerequisites are re-examined against the published store.
                let prereqs = self.prereq_checkpoints(&id)?;
                journal_transition(
                    &mut self.registry,
                    &mut self.journal,
                    &id,
                    KnowledgeState::Acquiring,
                )?;
                let attempt = match self.registry.get_mut(&id) {
                    Some(l) => {
                        l.attempts += 1;
                        l.attempts
                    }
                    None => 1,
                };
                jobs.push(Job {
                    lane,
                    id,
                    attempt,
                    lease,
                    prereqs,
                });
            }
            self.journal.sync()?;
            let keys: Vec<(Lane, LessonId, Lease, u32)> = jobs
                .iter()
                .map(|j| (j.lane, j.id, j.lease.clone(), j.attempt))
                .collect();
            let params = Arc::clone(&self.params);
            let teacher = &self.teacher;
            let oracle = &self.oracle;
            let learn_start = Instant::now();
            let (outcomes, stats) = run_parallel(jobs, self.cfg.workers, cancel, |job| {
                attempt_lesson(&params, teacher, oracle, &job)
            });
            summary.learn_phase_nanos += learn_start.elapsed().as_nanos();
            summary.stolen += stats.stolen;
            summary.waves += 1;
            if self.cfg.crash_after_waves == Some(summary.waves) {
                summary.crashed = true;
                break;
            }
            let mut staged = Vec::new();
            for ((lane, id, lease, attempt), outcome) in keys.into_iter().zip(outcomes) {
                let _ = self.resources.release(&lease);
                let domain = self
                    .registry
                    .get(&id)
                    .map(|l| l.spec.domain)
                    .unwrap_or(Domain::Mathematics);
                let mut overload = false;
                match outcome {
                    JobOutcome::Completed(out) => {
                        if let Some(e) = self.process_attempt(&id, domain, attempt, out)? {
                            staged.push(e);
                        }
                    }
                    JobOutcome::Panicked(msg) => {
                        overload = true;
                        self.registry.record_error(
                            &id,
                            ErrorStage::Learner,
                            format!("panic: {msg}"),
                        )?;
                        self.retry_or_block(&id, KnowledgeState::Identified)?;
                    }
                    JobOutcome::Cancelled => {
                        journal_transition(
                            &mut self.registry,
                            &mut self.journal,
                            &id,
                            KnowledgeState::Identified,
                        )?;
                    }
                }
                self.scheduler.complete(lane, &id, overload);
            }
            self.consolidate(staged)?;
            self.journal.sync()?;
        }
        summary.wall_nanos = started.elapsed().as_nanos();
        summary.store_version = self.store.version()?;
        let mut states = BTreeMap::new();
        for l in self.registry.iter() {
            *states.entry(l.state).or_insert(0) += 1;
        }
        summary.states = states;
        Ok(summary)
    }

    fn retry_or_block(&mut self, id: &LessonId, via: KnowledgeState) -> Result<(), EngineError> {
        if self.registry.state(id) != Some(via) {
            journal_transition(&mut self.registry, &mut self.journal, id, via)?;
        }
        let attempts = self.registry.get(id).map_or(0, |l| l.attempts);
        if attempts >= self.cfg.max_attempts {
            journal_transition(
                &mut self.registry,
                &mut self.journal,
                id,
                KnowledgeState::Blocked,
            )?;
        }
        Ok(())
    }

    fn process_attempt(
        &mut self,
        id: &LessonId,
        domain: Domain,
        attempt: u32,
        out: AttemptOutput,
    ) -> Result<Option<(LessonId, StagedEntry)>, EngineError> {
        for v in &out.verdicts {
            self.ledger.record(
                self.teacher.name(),
                v,
                out.teach_nanos / out.verdicts.len().max(1) as u128,
            );
        }
        let rejected = out
            .verdicts
            .iter()
            .filter(|v| matches!(v, Verdict::Reject(_)))
            .count() as u64;
        {
            let c = self.telemetry.domain_mut(domain);
            c.processed += 1;
            c.attempts += 1;
            c.teacher_proposals += out.verdicts.len() as u64;
            c.teacher_rejections += rejected;
            c.learn_nanos += out.learn_nanos;
            c.validate_nanos += out.validate_nanos;
        }
        {
            let m = self.telemetry.machine_mut("local");
            m.processed += 1;
            m.learn_nanos += out.learn_nanos;
        }
        if rejected > 0 {
            self.registry.record_error(
                id,
                ErrorStage::Teacher,
                format!("{rejected} teacher labels rejected by independent oracle"),
            )?;
        }
        if let Some(l) = self.registry.get_mut(id) {
            l.measured_cost_nanos = out.learn_nanos + out.validate_nanos;
            l.teacher = Some(self.teacher.name().to_string());
            l.test_corpus = Some(out.report.corpus.digest());
            l.train_digest = Some(out.report.corpus.train);
            l.model_version = Some("sml-tutor-anf-gf2-v1".into());
            l.exams.push(out.report.clone());
        }
        let Some(checkpoint) = out.checkpoint else {
            // Learner abstained or found inconsistent data: a gap to re-teach.
            let c = self.telemetry.domain_mut(domain);
            c.abstentions += u64::from(out.abstained);
            c.exam_failures += 1;
            // A gap is counted once per lesson; later failures are retries.
            if attempt == 1 {
                c.gaps_detected += 1;
            }
            self.registry
                .record_error(id, ErrorStage::Learner, out.detail)?;
            self.retry_or_block(id, KnowledgeState::Identified)?;
            return Ok(None);
        };
        journal_transition(
            &mut self.registry,
            &mut self.journal,
            id,
            KnowledgeState::Learned,
        )?;
        self.telemetry.domain_mut(domain).learned += 1;
        journal_transition(
            &mut self.registry,
            &mut self.journal,
            id,
            KnowledgeState::Validating,
        )?;
        let retained: Vec<Digest> = self
            .registry
            .graph()
            .prerequisites(id)
            .map(knowledge_key)
            .collect();
        match certify(*id, &out.report, &retained) {
            Ok(cert) => {
                journal_transition(
                    &mut self.registry,
                    &mut self.journal,
                    id,
                    KnowledgeState::Certified,
                )?;
                let c = self.telemetry.domain_mut(domain);
                c.certified += 1;
                if attempt > 1 {
                    c.gaps_resolved += 1;
                }
                Ok(Some((
                    *id,
                    StagedEntry::new(&cert, knowledge_key(id), checkpoint),
                )))
            }
            Err(e) => {
                let c = self.telemetry.domain_mut(domain);
                c.exam_failures += 1;
                if attempt == 1 {
                    c.gaps_detected += 1;
                }
                self.registry
                    .record_error(id, ErrorStage::Exam, e.to_string())?;
                self.retry_or_block(id, KnowledgeState::Identified)?;
                Ok(None)
            }
        }
    }

    fn consolidate(&mut self, staged: Vec<(LessonId, StagedEntry)>) -> Result<(), EngineError> {
        if staged.is_empty() {
            return Ok(());
        }
        let start = Instant::now();
        let entries: Vec<StagedEntry> = staged.iter().map(|(_, e)| e.clone()).collect();
        let ok: BTreeSet<LessonId> = match self.store.commit_rebasing(entries, 10_000) {
            Ok(_) => staged.iter().map(|(id, _)| *id).collect(),
            Err(StoreError::Contradiction { .. }) => {
                // Isolate the contradictory entries; publish the rest.
                let mut ok = BTreeSet::new();
                for (id, e) in &staged {
                    match self.store.commit_rebasing(vec![e.clone()], 10_000) {
                        Ok(_) => {
                            ok.insert(*id);
                        }
                        Err(StoreError::Contradiction { .. }) => {
                            journal_transition(
                                &mut self.registry,
                                &mut self.journal,
                                id,
                                KnowledgeState::Contradictory,
                            )?;
                            if let Some(d) = self.registry.get(id).map(|l| l.spec.domain) {
                                self.telemetry.domain_mut(d).contradictions += 1;
                            }
                        }
                        Err(other) => return Err(other.into()),
                    }
                }
                ok
            }
            Err(other) => return Err(other.into()),
        };
        let nanos = start.elapsed().as_nanos();
        let per = nanos / ok.len().max(1) as u128;
        for id in ok {
            journal_transition(
                &mut self.registry,
                &mut self.journal,
                &id,
                KnowledgeState::Consolidated,
            )?;
            if let Some(d) = self.registry.get(&id).map(|l| l.spec.domain) {
                let c = self.telemetry.domain_mut(d);
                c.consolidated += 1;
                c.consolidate_nanos += per;
            }
        }
        Ok(())
    }
}

fn cohort(cohort: Cohort, correct: u64, total: u64) -> CohortResult {
    CohortResult {
        cohort,
        correct,
        abstained: 0,
        total,
    }
}

fn case_digests(lesson: &LessonId, xs: &[u32]) -> Vec<Digest> {
    xs.iter()
        .map(|x| Digest::of(&[lesson.0.as_slice(), &x.to_le_bytes()].concat()))
        .collect()
}

/// One isolated attempt: TEACH → (independent oracle) → LEARN → EXAMINE.
/// Pure with respect to engine state, so attempts run in parallel safely.
fn attempt_lesson(
    params: &BTreeMap<LessonId, AnfLessonParams>,
    teacher: &AnfTeacher,
    oracle: &AnfOracle,
    job: &Job,
) -> AttemptOutput {
    let id = job.id;
    let fail = |detail: String| AttemptOutput {
        report: ExamReport {
            lesson: id,
            corpus: check_partitions(&[], &[], &[], &[]).unwrap_or(
                crate::mastery::CorpusIdentity {
                    train: Digest::ZERO,
                    dev: Digest::ZERO,
                    test: Digest::ZERO,
                    testtest: Digest::ZERO,
                },
            ),
            cohorts: vec![],
        },
        checkpoint: None,
        verdicts: vec![],
        teach_nanos: 0,
        learn_nanos: 0,
        validate_nanos: 0,
        abstained: false,
        detail,
    };
    let Some(p) = params.get(&id).copied() else {
        return fail("no lesson parameters".into());
    };
    let parts = match p.partitions() {
        Ok(x) => x,
        Err(e) => return fail(e.to_string()),
    };
    let task = p.task();
    let shift = (job.attempt.saturating_sub(1)).min(16);
    let count = (p.base_train << shift).min(p.max_train);

    // TEACH: the teacher proposes; the oracle independently filters.
    let t0 = Instant::now();
    let proposals = teacher.propose_batch(&[LessonRequest {
        lesson: id,
        domain: Domain::Mathematics,
        skill: "anf".into(),
        count: count as u32,
    }]);
    let mut local_ledger = TeacherLedger::default();
    let mut seen = BTreeSet::new();
    let admitted = admit_batch(oracle, &mut local_ledger, &mut seen, proposals);
    let rejected = local_ledger
        .stats(teacher.name())
        .map_or(0, |s| s.rejected as usize);
    let mut verdicts = vec![Verdict::Accept; admitted.len()];
    verdicts.extend((0..rejected).map(|_| Verdict::Reject("oracle rejected label".into())));
    let teach_nanos = t0.elapsed().as_nanos();
    let examples: Vec<(u32, bool)> = admitted
        .iter()
        .filter_map(|pr| {
            let x = u32::from_le_bytes(<[u8; 4]>::try_from(pr.payload.as_slice()).ok()?);
            Some((x, pr.claimed_answer.first() == Some(&1)))
        })
        .collect();
    let train_used: Vec<u32> = examples.iter().map(|(x, _)| *x).collect();

    // LEARN
    let t1 = Instant::now();
    let outcome = learn(p.inputs, p.degree, &examples);
    let learn_nanos = t1.elapsed().as_nanos();

    // EXAMINE (independent cohorts, disjoint partitions)
    let t2 = Instant::now();
    let corpus = match check_partitions(
        &case_digests(&id, &train_used),
        &case_digests(&id, &parts.dev),
        &case_digests(&id, &parts.test),
        &case_digests(&id, &parts.testtest),
    ) {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };
    let (checkpoint, cohorts, abstained, detail) = match outcome {
        LearnOutcome::Learned { coeffs } => {
            let mut cohorts = Vec::new();
            let (c, t) = exam(p.inputs, p.degree, &coeffs, &task, &train_used);
            cohorts.push(cohort(Cohort::TrainRecall, c, t));
            let (c, t) = exam(p.inputs, p.degree, &coeffs, &task, &parts.dev);
            cohorts.push(cohort(Cohort::Dev, c, t));
            let (c, t) = exam(p.inputs, p.degree, &coeffs, &task, &parts.test);
            cohorts.push(cohort(Cohort::Test, c, t));
            let (c, t) = exam(p.inputs, p.degree, &coeffs, &task, &parts.testtest);
            cohorts.push(cohort(Cohort::TestTest, c, t));
            let ckpt = encode_checkpoint(p.inputs, p.degree, &coeffs);
            let reload = match decode_checkpoint(&ckpt) {
                Ok((i, d, reloaded)) => {
                    let all: Vec<u32> = parts.test.iter().chain(&parts.testtest).copied().collect();
                    let (c, t) = exam(i, d, &reloaded, &task, &all);
                    cohort(Cohort::Reload, c, t)
                }
                Err(_) => cohort(Cohort::Reload, 0, 1),
            };
            cohorts.push(reload);
            for (key, bytes, pp) in &job.prereqs {
                let r = match (decode_checkpoint(bytes), pp.partitions()) {
                    (Ok((i, d, coeffs)), Ok(pparts)) => {
                        let all: Vec<u32> = pparts
                            .test
                            .iter()
                            .chain(&pparts.testtest)
                            .copied()
                            .collect();
                        let (c, t) = exam(i, d, &coeffs, &pp.task(), &all);
                        cohort(Cohort::Retention(*key), c, t)
                    }
                    _ => cohort(Cohort::Retention(*key), 0, 1),
                };
                cohorts.push(r);
            }
            (Some(ckpt), cohorts, false, String::new())
        }
        LearnOutcome::Underdetermined { rank, unknowns } => {
            let n = parts.test.len() as u64;
            (
                None,
                vec![CohortResult {
                    cohort: Cohort::Test,
                    correct: 0,
                    abstained: n,
                    total: n,
                }],
                true,
                format!(
                    "abstained: rank {rank} < {unknowns} unknowns with {} admitted rows",
                    examples.len()
                ),
            )
        }
        LearnOutcome::Inconsistent => (
            None,
            vec![cohort(Cohort::Test, 0, parts.test.len() as u64)],
            false,
            "inconsistent admitted labels".into(),
        ),
    };
    let validate_nanos = t2.elapsed().as_nanos();
    AttemptOutput {
        report: ExamReport {
            lesson: id,
            corpus,
            cohorts,
        },
        checkpoint,
        verdicts,
        teach_nanos,
        learn_nanos,
        validate_nanos,
        abstained,
        detail,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("sml-tutor-engine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    fn run_fresh(name: &str, workers: usize, lie: u64) -> (Engine, RunSummary) {
        let dir = tmp(name);
        let cfg = EngineConfig {
            workers,
            wave: 16,
            teacher_lie_every: lie,
            ..EngineConfig::default()
        };
        let (mut e, _) = Engine::open(&dir, anf_curriculum(6, 7, 2, 11), cfg).unwrap();
        let s = e.run(&CancellationToken::new()).unwrap();
        (e, s)
    }

    #[test]
    fn every_lesson_is_consolidated_including_gap_retries() {
        let (e, s) = run_fresh("all", 4, 0);
        assert_eq!(
            s.states.get(&KnowledgeState::Consolidated),
            Some(&24),
            "{:?}",
            s.states
        );
        assert_eq!(e.store().audit().unwrap(), 24);
        let total = e.telemetry.total();
        assert!(
            total.gaps_detected > 0,
            "under-determined lessons must abstain first"
        );
        assert_eq!(total.gaps_detected, total.gaps_resolved);
        assert_eq!(total.certified, 24);
        let _ = std::fs::remove_dir_all(e.root());
    }

    #[test]
    fn sequential_and_parallel_publish_identical_knowledge() {
        let (a, _) = run_fresh("seq", 1, 0);
        let (b, _) = run_fresh("par", 4, 0);
        assert_eq!(
            a.store().snapshot().unwrap().entries,
            b.store().snapshot().unwrap().entries
        );
        let _ = std::fs::remove_dir_all(a.root());
        let _ = std::fs::remove_dir_all(b.root());
    }

    #[test]
    fn lying_teacher_is_filtered_by_independent_oracle() {
        let (e, s) = run_fresh("lie", 2, 7);
        assert_eq!(s.states.get(&KnowledgeState::Consolidated), Some(&24));
        let total = e.telemetry.total();
        assert!(total.teacher_rejections > 0);
        let _ = std::fs::remove_dir_all(e.root());
    }

    #[test]
    fn crash_and_resume_reaches_the_same_store() {
        let (reference, _) = run_fresh("ref", 2, 0);
        let dir = tmp("crash");
        let cfg = EngineConfig {
            workers: 2,
            wave: 16,
            crash_after_waves: Some(2),
            ..EngineConfig::default()
        };
        let (mut e, _) = Engine::open(&dir, anf_curriculum(6, 7, 2, 11), cfg).unwrap();
        let s = e.run(&CancellationToken::new()).unwrap();
        assert!(s.crashed);
        drop(e);
        let cfg = EngineConfig {
            crash_after_waves: None,
            ..cfg
        };
        let (mut e, resume) = Engine::open(&dir, anf_curriculum(6, 7, 2, 11), cfg).unwrap();
        assert!(resume.interrupted_reset > 0, "{resume:?}");
        let s = e.run(&CancellationToken::new()).unwrap();
        assert_eq!(s.states.get(&KnowledgeState::Consolidated), Some(&24));
        assert_eq!(
            e.store().snapshot().unwrap().entries,
            reference.store().snapshot().unwrap().entries
        );
        // A third open is a no-op: nothing left to learn.
        drop(e);
        let (mut e, resume) = Engine::open(&dir, anf_curriculum(6, 7, 2, 11), cfg).unwrap();
        assert_eq!(resume.interrupted_reset, 0);
        let s = e.run(&CancellationToken::new()).unwrap();
        assert_eq!(s.waves, 0);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(reference.root());
    }

    #[test]
    fn prerequisites_are_reexamined_as_retention_cohorts() {
        let (e, _) = run_fresh("ret", 2, 0);
        let with_prereq = e
            .registry()
            .iter()
            .find(|l| !l.spec.prerequisites.is_empty())
            .unwrap();
        let last = with_prereq.exams.last().unwrap();
        let retention = last
            .cohorts
            .iter()
            .filter(|c| matches!(c.cohort, Cohort::Retention(_)))
            .count();
        assert_eq!(retention, with_prereq.spec.prerequisites.len());
        assert!(e.registry().graph().cross_domain_edges() > 0);
        let _ = std::fs::remove_dir_all(e.root());
    }
}

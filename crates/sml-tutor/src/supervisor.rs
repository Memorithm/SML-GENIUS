//! `sml-tutord` supervisor: the permanent control loop above the P1 engine.
//!
//! The engine ([`crate::engine`]) runs one curriculum to completion. The
//! supervisor decides *which* curriculum cycle runs next, *whether* the host
//! may run anything at all, and keeps an auditable record of every decision so
//! that a restart resumes without losing or duplicating a cycle.
//!
//! Design, deliberately small and explicit:
//!
//! * **State machine** ([`SupervisorState`]): transitions are whitelisted; an
//!   emergency stop is terminal for the process.
//! * **Durable, hash-chained event log** (`supervisor.log`): every job
//!   submission, start, success, failure, quarantine and state change is one
//!   line, chained by SHA-256. The persistent queue is *derived* by replaying
//!   the log (event sourcing), so there is exactly one source of truth. A torn
//!   final line is truncated; any other irregularity fails closed.
//! * **Idempotent identities** ([`JobId`]): a job is identified by the digest
//!   of its kind, parameters and dependencies. Submitting the same job twice
//!   is a no-op, so replanning after a restart never duplicates work.
//! * **Acyclic dependencies**: a job may only depend on jobs already known, so
//!   the dependency graph is acyclic by construction.
//! * **Priorities and backpressure**: highest priority ready job first; the
//!   queue is bounded and submissions beyond the bound are refused.
//! * **Bounded retries, exponential backoff, quarantine**: a job that fails
//!   `max_attempts` times is quarantined and never retried automatically.
//! * **Resource gate** ([`Gate`]): nothing starts while the host is over
//!   budget (fail closed on unknown readings).
//! * **Modes**: `Simulation` and `Production` are recorded in every start
//!   event; the runner decides what each means (the daemon refuses production
//!   without an explicit admission file).
//! * **Control**: pause, resume, maintenance, drain (finish nothing new, then
//!   stop), stop and emergency stop, delivered through [`Command`].
//! * **Health**: [`Supervisor::health_json`] is written atomically by the
//!   daemon after every tick.
//! * **Rollback**: the runner's [`CycleRunner::verify`] runs after each
//!   successful cycle; on failure [`CycleRunner::rollback`] restores the
//!   previous published version and the cycle counts as a failed attempt.
//!
//! What this module does not claim: it is an orchestration substrate. It
//! makes no statement about what is learned; the runner and the examiner do.

use std::collections::BTreeMap;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::executor::CancellationToken;
use crate::hash::{Canonical, Digest};

const LOG_HEADER: &str = "SML_TUTORD_LOG_V1";

/// Supervisor lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SupervisorState {
    Starting,
    Running,
    Paused,
    Maintenance,
    Draining,
    Stopped,
    EmergencyStopped,
}

impl SupervisorState {
    pub const ALL: [SupervisorState; 7] = [
        SupervisorState::Starting,
        SupervisorState::Running,
        SupervisorState::Paused,
        SupervisorState::Maintenance,
        SupervisorState::Draining,
        SupervisorState::Stopped,
        SupervisorState::EmergencyStopped,
    ];

    pub fn name(self) -> &'static str {
        match self {
            SupervisorState::Starting => "starting",
            SupervisorState::Running => "running",
            SupervisorState::Paused => "paused",
            SupervisorState::Maintenance => "maintenance",
            SupervisorState::Draining => "draining",
            SupervisorState::Stopped => "stopped",
            SupervisorState::EmergencyStopped => "emergency_stopped",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == s)
    }

    /// Whitelisted transitions. Anything else is refused.
    pub fn can_go(self, to: SupervisorState) -> bool {
        use SupervisorState::*;
        if to == EmergencyStopped {
            return self != EmergencyStopped;
        }
        matches!(
            (self, to),
            (Starting, Running)
                | (Starting, Stopped)
                | (Running, Paused)
                | (Running, Maintenance)
                | (Running, Draining)
                | (Running, Stopped)
                | (Paused, Running)
                | (Paused, Maintenance)
                | (Paused, Draining)
                | (Paused, Stopped)
                | (Maintenance, Running)
                | (Maintenance, Paused)
                | (Maintenance, Stopped)
                | (Draining, Stopped)
                // A restarted process re-enters through Starting.
                | (Stopped, Starting)
        )
    }

    /// Whether new cycles may start in this state.
    pub fn dispatches(self) -> bool {
        self == SupervisorState::Running
    }
}

/// Operating mode, recorded in every start event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Simulation,
    Production,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Simulation => "simulation",
            Mode::Production => "production",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "simulation" => Some(Mode::Simulation),
            "production" => Some(Mode::Production),
            _ => None,
        }
    }
}

/// Operator commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Pause,
    Resume,
    Maintenance,
    Drain,
    Stop,
    Emergency,
}

impl Command {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "pause" => Some(Command::Pause),
            "resume" => Some(Command::Resume),
            "maintenance" => Some(Command::Maintenance),
            "drain" => Some(Command::Drain),
            "stop" => Some(Command::Stop),
            "emergency" => Some(Command::Emergency),
            _ => None,
        }
    }

    fn target(self) -> SupervisorState {
        match self {
            Command::Pause => SupervisorState::Paused,
            Command::Resume => SupervisorState::Running,
            Command::Maintenance => SupervisorState::Maintenance,
            Command::Drain => SupervisorState::Draining,
            Command::Stop => SupervisorState::Stopped,
            Command::Emergency => SupervisorState::EmergencyStopped,
        }
    }
}

/// Content identity of a job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(pub Digest);

impl JobId {
    pub fn short(&self) -> String {
        self.0.short()
    }
}

/// What to run. Priority and retry budget are scheduling metadata and are not
/// part of the identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobSpec {
    pub kind: String,
    pub params: BTreeMap<String, String>,
    pub deps: Vec<JobId>,
    pub priority: i64,
    pub max_attempts: u32,
}

impl JobSpec {
    pub fn new(kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            params: BTreeMap::new(),
            deps: Vec::new(),
            priority: 0,
            max_attempts: 3,
        }
    }

    pub fn param(mut self, k: &str, v: impl ToString) -> Self {
        self.params.insert(k.to_string(), v.to_string());
        self
    }

    pub fn id(&self) -> JobId {
        let mut c = Canonical::new("sml-tutord-job-v1");
        c.str(&self.kind);
        c.u64(self.params.len() as u64);
        for (k, v) in &self.params {
            c.str(k).str(v);
        }
        let mut deps = self.deps.clone();
        deps.sort();
        deps.dedup();
        c.u64(deps.len() as u64);
        for d in &deps {
            c.bytes(&d.0 .0);
        }
        JobId(c.finish())
    }
}

/// Result of one successful cycle, as reported by the runner.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CycleReport {
    /// Knowledge entries newly certified and consolidated by this cycle.
    pub new_certified: u64,
    /// Re-examinations of earlier knowledge (never counted as new).
    pub reexamined: u64,
    pub rejected: u64,
    pub store_version: u64,
    pub detail: String,
}

/// Executes one job. Implementations own the learning; the supervisor only
/// orchestrates.
pub trait CycleRunner {
    fn run(
        &mut self,
        id: JobId,
        spec: &JobSpec,
        mode: Mode,
        cancel: &CancellationToken,
    ) -> Result<CycleReport, String>;

    /// Post-cycle health check of published knowledge (e.g. store audit and
    /// retention re-examination). `Err` triggers [`CycleRunner::rollback`].
    fn verify(&mut self, _id: JobId, _spec: &JobSpec) -> Result<(), String> {
        Ok(())
    }

    /// Restores the state published before the failed cycle.
    fn rollback(&mut self, _id: JobId, _spec: &JobSpec) -> Result<(), String> {
        Ok(())
    }
}

/// Host admission gate (CPU, RAM, disk, GPU...). `Err` lists the reasons.
pub trait Gate {
    fn admit(&mut self) -> Result<(), Vec<String>>;
}

/// Always admits; for tests and simulation only.
pub struct OpenGate;

impl Gate for OpenGate {
    fn admit(&mut self) -> Result<(), Vec<String>> {
        Ok(())
    }
}

/// Lifecycle of a job, derived from the event log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobStatus {
    Queued,
    Running,
    Backoff { until_ms: u64 },
    Succeeded,
    Quarantined { reason: String },
}

#[derive(Clone, Debug)]
pub struct JobRecord {
    pub spec: JobSpec,
    pub status: JobStatus,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub report: Option<CycleReport>,
}

#[derive(Clone, Copy, Debug)]
pub struct SupervisorConfig {
    pub max_queued: usize,
    pub backoff_base_ms: u64,
    pub backoff_max_ms: u64,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            max_queued: 256,
            backoff_base_ms: 30_000,
            backoff_max_ms: 3_600_000,
        }
    }
}

#[derive(Debug)]
pub enum SupervisorError {
    Io(io::Error),
    Corrupt {
        line: usize,
        reason: String,
    },
    IllegalTransition(SupervisorState, SupervisorState),
    Backpressure(usize),
    UnknownDependency(JobId),
    EmergencyStopped,
    /// Another supervisor process holds `<root>/supervisor.lock`.
    Locked,
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SupervisorError::Io(e) => write!(f, "supervisor I/O: {e}"),
            SupervisorError::Corrupt { line, reason } => {
                write!(f, "supervisor log corrupt at line {line}: {reason}")
            }
            SupervisorError::IllegalTransition(a, b) => {
                write!(
                    f,
                    "illegal supervisor transition {} -> {}",
                    a.name(),
                    b.name()
                )
            }
            SupervisorError::Backpressure(n) => write!(f, "queue full ({n} jobs pending)"),
            SupervisorError::UnknownDependency(d) => {
                write!(f, "dependency {} is not a known job", d.short())
            }
            SupervisorError::EmergencyStopped => write!(f, "supervisor is emergency-stopped"),
            SupervisorError::Locked => write!(f, "another supervisor holds this root"),
        }
    }
}

impl std::error::Error for SupervisorError {}

impl From<io::Error> for SupervisorError {
    fn from(e: io::Error) -> Self {
        SupervisorError::Io(e)
    }
}

/// What a tick did; useful for the daemon loop and for tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tick {
    /// Dispatching is disabled in the current state.
    Idle(SupervisorState),
    /// The gate refused; nothing started.
    Gated(Vec<String>),
    /// No ready job (empty queue or every job waiting for backoff/deps).
    NothingReady,
    Succeeded(JobId),
    Failed(JobId),
    Quarantined(JobId),
    /// Draining finished; the supervisor is now stopped.
    Drained,
}

// ---------------------------------------------------------------------------
// Event log

#[derive(Clone, Debug, PartialEq)]
enum Event {
    Submit(JobId, JobSpec),
    Start(JobId, u32, Mode),
    Succeed(JobId, CycleReport),
    Fail(JobId, u32, String, u64),
    Quarantine(JobId, String),
    State(SupervisorState, SupervisorState),
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '%' => o.push_str("%25"),
            '\t' => o.push_str("%09"),
            '\n' => o.push_str("%0A"),
            '\r' => o.push_str("%0D"),
            ',' => o.push_str("%2C"),
            '=' => o.push_str("%3D"),
            c => o.push(c),
        }
    }
    o
}

fn unesc(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let h = s.get(i + 1..i + 3).ok_or("bad escape")?;
            out.push(u8::from_str_radix(h, 16).map_err(|_| "bad escape")?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "invalid UTF-8".to_string())
}

fn hex_id(id: &JobId) -> String {
    id.0.to_hex()
}

fn parse_id(s: &str) -> Result<JobId, String> {
    Digest::from_hex(s)
        .map(JobId)
        .ok_or_else(|| "bad job id".into())
}

impl Event {
    fn fields(&self) -> Vec<String> {
        match self {
            Event::Submit(id, s) => {
                let params: Vec<String> = s
                    .params
                    .iter()
                    .map(|(k, v)| format!("{}={}", esc(k), esc(v)))
                    .collect();
                let deps: Vec<String> = s.deps.iter().map(hex_id).collect();
                vec![
                    "submit".into(),
                    hex_id(id),
                    esc(&s.kind),
                    params.join(","),
                    deps.join(","),
                    s.priority.to_string(),
                    s.max_attempts.to_string(),
                ]
            }
            Event::Start(id, a, m) => {
                vec!["start".into(), hex_id(id), a.to_string(), m.name().into()]
            }
            Event::Succeed(id, r) => vec![
                "succeed".into(),
                hex_id(id),
                r.new_certified.to_string(),
                r.reexamined.to_string(),
                r.rejected.to_string(),
                r.store_version.to_string(),
                esc(&r.detail),
            ],
            Event::Fail(id, a, e, until) => vec![
                "fail".into(),
                hex_id(id),
                a.to_string(),
                esc(e),
                until.to_string(),
            ],
            Event::Quarantine(id, r) => vec!["quarantine".into(), hex_id(id), esc(r)],
            Event::State(a, b) => vec!["state".into(), a.name().into(), b.name().into()],
        }
    }

    fn parse(f: &[&str]) -> Result<Event, String> {
        let num = |s: &str| s.parse::<u64>().map_err(|_| format!("bad number {s:?}"));
        let n32 = |s: &str| s.parse::<u32>().map_err(|_| format!("bad number {s:?}"));
        match f {
            ["submit", id, kind, params, deps, prio, max] => {
                let mut spec = JobSpec::new(&unesc(kind)?);
                for kv in params.split(',').filter(|p| !p.is_empty()) {
                    let (k, v) = kv.split_once('=').ok_or("bad param")?;
                    spec.params.insert(unesc(k)?, unesc(v)?);
                }
                for d in deps.split(',').filter(|p| !p.is_empty()) {
                    spec.deps.push(parse_id(d)?);
                }
                spec.priority = prio.parse().map_err(|_| "bad priority")?;
                spec.max_attempts = n32(max)?;
                let id = parse_id(id)?;
                if spec.id() != id {
                    return Err("job id does not match its spec".into());
                }
                Ok(Event::Submit(id, spec))
            }
            ["start", id, a, m] => Ok(Event::Start(
                parse_id(id)?,
                n32(a)?,
                Mode::parse(m).ok_or("bad mode")?,
            )),
            ["succeed", id, n, re, rej, v, d] => Ok(Event::Succeed(
                parse_id(id)?,
                CycleReport {
                    new_certified: num(n)?,
                    reexamined: num(re)?,
                    rejected: num(rej)?,
                    store_version: num(v)?,
                    detail: unesc(d)?,
                },
            )),
            ["fail", id, a, e, until] => {
                Ok(Event::Fail(parse_id(id)?, n32(a)?, unesc(e)?, num(until)?))
            }
            ["quarantine", id, r] => Ok(Event::Quarantine(parse_id(id)?, unesc(r)?)),
            ["state", a, b] => Ok(Event::State(
                SupervisorState::parse(a).ok_or("bad state")?,
                SupervisorState::parse(b).ok_or("bad state")?,
            )),
            _ => Err("unknown record".into()),
        }
    }
}

fn chain(seq: u64, prev: &Digest, fields: &[String]) -> Digest {
    let mut c = Canonical::new("sml-tutord-log-v1");
    c.u64(seq).bytes(&prev.0).u64(fields.len() as u64);
    for f in fields {
        c.str(f);
    }
    c.finish()
}

// ---------------------------------------------------------------------------
// Supervisor

pub struct Supervisor {
    root: PathBuf,
    log: File,
    /// Held for the supervisor's lifetime: one writer per root.
    _lock: File,
    seq: u64,
    head: Digest,
    state: SupervisorState,
    jobs: BTreeMap<JobId, JobRecord>,
    order: Vec<JobId>,
    cfg: SupervisorConfig,
    mode: Mode,
    started_ms: u64,
    /// Number of `Running` jobs found at open: they were interrupted by a
    /// crash and are put back in the queue (the attempt still counts).
    pub interrupted_at_open: usize,
    pub torn_tail_at_open: bool,
    last_gate: Vec<String>,
}

fn corrupt(line: usize, reason: impl Into<String>) -> SupervisorError {
    SupervisorError::Corrupt {
        line,
        reason: reason.into(),
    }
}

impl Supervisor {
    /// Opens (or creates) the supervisor rooted at `root`, replays the log,
    /// requeues interrupted jobs and enters `Starting`.
    pub fn open(
        root: &Path,
        mode: Mode,
        cfg: SupervisorConfig,
        now_ms: u64,
    ) -> Result<Self, SupervisorError> {
        std::fs::create_dir_all(root)?;
        // Exclusive advisory lock before reading anything: a second instance
        // on the same root would fork the hash chain and re-run live jobs.
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join("supervisor.lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(SupervisorError::Locked),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        let path = root.join("supervisor.log");
        let mut log = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)?;
        let mut bytes = Vec::new();
        log.seek(SeekFrom::Start(0))?;
        log.read_to_end(&mut bytes)?;
        let mut sup = Supervisor {
            root: root.to_path_buf(),
            log,
            _lock: lock,
            seq: 0,
            head: Digest::ZERO,
            state: SupervisorState::Stopped,
            jobs: BTreeMap::new(),
            order: Vec::new(),
            cfg,
            mode,
            started_ms: now_ms,
            interrupted_at_open: 0,
            torn_tail_at_open: false,
            last_gate: Vec::new(),
        };
        if bytes.is_empty() {
            writeln!(sup.log, "{LOG_HEADER}")?;
            sup.log.sync_data()?;
        } else {
            let end = match bytes.iter().rposition(|&b| b == b'\n') {
                Some(p) => p + 1,
                None => 0,
            };
            if end < bytes.len() {
                sup.torn_tail_at_open = true;
            }
            if end == 0 {
                if !LOG_HEADER.as_bytes().starts_with(&bytes) {
                    return Err(corrupt(1, "wrong header"));
                }
                sup.log.set_len(0)?;
                writeln!(sup.log, "{LOG_HEADER}")?;
                sup.log.sync_data()?;
            } else {
                sup.replay(&bytes[..end])?;
                if sup.torn_tail_at_open {
                    sup.log.set_len(end as u64)?;
                    sup.log.sync_data()?;
                }
            }
        }
        // Crash recovery: a job still `Running` never finished.
        let running: Vec<JobId> = sup
            .jobs
            .iter()
            .filter(|(_, j)| j.status == JobStatus::Running)
            .map(|(id, _)| *id)
            .collect();
        sup.interrupted_at_open = running.len();
        for id in running {
            let attempts = sup.jobs[&id].attempts;
            sup.record_failure(id, attempts, "interrupted by restart".into(), now_ms, false)?;
        }
        // A crash between a final `Fail` and its `Quarantine` leaves an
        // exhausted job in backoff: complete the quarantine before dispatch.
        let exhausted: Vec<(JobId, u32)> = sup
            .jobs
            .iter()
            .filter(|(_, j)| {
                matches!(j.status, JobStatus::Queued | JobStatus::Backoff { .. })
                    && j.attempts >= j.spec.max_attempts
            })
            .map(|(id, j)| (*id, j.attempts))
            .collect();
        for (id, attempts) in exhausted {
            sup.append(Event::Quarantine(
                id,
                format!("{attempts} failed attempts (recovered at open)"),
            ))?;
        }
        match sup.state {
            SupervisorState::EmergencyStopped => return Err(SupervisorError::EmergencyStopped),
            SupervisorState::Stopped => sup.set_state(SupervisorState::Starting)?,
            // A crash left a live state behind: record the implicit stop.
            _ => {
                sup.append(Event::State(sup.state, SupervisorState::Stopped))?;
                sup.state = SupervisorState::Stopped;
                sup.set_state(SupervisorState::Starting)?;
            }
        }
        Ok(sup)
    }

    fn replay(&mut self, body: &[u8]) -> Result<(), SupervisorError> {
        let text = std::str::from_utf8(body).map_err(|_| corrupt(0, "invalid UTF-8"))?;
        for (i, line) in text.lines().enumerate() {
            let n = i + 1;
            if n == 1 {
                if line != LOG_HEADER {
                    return Err(corrupt(1, "wrong header"));
                }
                continue;
            }
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 4 || parts[0] != "E" {
                return Err(corrupt(n, "malformed line"));
            }
            let seq: u64 = parts[1].parse().map_err(|_| corrupt(n, "bad seq"))?;
            if seq != self.seq + 1 {
                return Err(corrupt(n, "sequence gap"));
            }
            let prev = Digest::from_hex(parts[2]).ok_or_else(|| corrupt(n, "bad prev"))?;
            if prev != self.head {
                return Err(corrupt(n, "broken hash chain"));
            }
            let fields = &parts[3..parts.len() - 1];
            let hash =
                Digest::from_hex(parts[parts.len() - 1]).ok_or_else(|| corrupt(n, "bad hash"))?;
            let owned: Vec<String> = fields.iter().map(|s| s.to_string()).collect();
            if chain(seq, &prev, &owned) != hash {
                return Err(corrupt(n, "hash mismatch"));
            }
            let ev = Event::parse(fields).map_err(|r| corrupt(n, r))?;
            self.apply(&ev).map_err(|r| corrupt(n, r))?;
            self.seq = seq;
            self.head = hash;
        }
        Ok(())
    }

    fn apply(&mut self, ev: &Event) -> Result<(), String> {
        match ev {
            Event::Submit(id, spec) => {
                if self.jobs.contains_key(id) {
                    return Err("duplicate submit".into());
                }
                for d in &spec.deps {
                    if !self.jobs.contains_key(d) {
                        return Err("unknown dependency".into());
                    }
                }
                self.jobs.insert(
                    *id,
                    JobRecord {
                        spec: spec.clone(),
                        status: JobStatus::Queued,
                        attempts: 0,
                        last_error: None,
                        report: None,
                    },
                );
                self.order.push(*id);
            }
            Event::Start(id, a, _) => {
                let j = self.jobs.get_mut(id).ok_or("start of unknown job")?;
                if !matches!(j.status, JobStatus::Queued | JobStatus::Backoff { .. })
                    || *a != j.attempts + 1
                {
                    return Err("illegal start".into());
                }
                j.attempts = *a;
                j.status = JobStatus::Running;
            }
            Event::Succeed(id, r) => {
                let j = self.jobs.get_mut(id).ok_or("success of unknown job")?;
                if j.status != JobStatus::Running {
                    return Err("success of a job that is not running".into());
                }
                j.status = JobStatus::Succeeded;
                j.report = Some(r.clone());
                j.last_error = None;
            }
            Event::Fail(id, a, e, until) => {
                let j = self.jobs.get_mut(id).ok_or("failure of unknown job")?;
                if j.status != JobStatus::Running || *a != j.attempts {
                    return Err("illegal failure".into());
                }
                j.status = JobStatus::Backoff { until_ms: *until };
                j.last_error = Some(e.clone());
            }
            Event::Quarantine(id, r) => {
                let j = self.jobs.get_mut(id).ok_or("quarantine of unknown job")?;
                if matches!(
                    j.status,
                    JobStatus::Succeeded | JobStatus::Quarantined { .. }
                ) {
                    return Err("illegal quarantine".into());
                }
                j.status = JobStatus::Quarantined { reason: r.clone() };
            }
            Event::State(a, b) => {
                if *a != self.state || !self.state.can_go(*b) {
                    return Err(format!("illegal state {} -> {}", a.name(), b.name()));
                }
                self.state = *b;
            }
        }
        Ok(())
    }

    fn append(&mut self, ev: Event) -> Result<(), SupervisorError> {
        let fields = ev.fields();
        let seq = self.seq + 1;
        let hash = chain(seq, &self.head, &fields);
        let line = format!(
            "E\t{seq}\t{}\t{}\t{}\n",
            self.head.to_hex(),
            fields.join("\t"),
            hash.to_hex()
        );
        // Validate against the in-memory model before making it durable.
        self.apply(&ev).map_err(|r| corrupt(seq as usize, r))?;
        self.log.write_all(line.as_bytes())?;
        self.log.sync_data()?;
        self.seq = seq;
        self.head = hash;
        Ok(())
    }

    fn set_state(&mut self, to: SupervisorState) -> Result<(), SupervisorError> {
        if self.state == to {
            return Ok(());
        }
        if !self.state.can_go(to) {
            return Err(SupervisorError::IllegalTransition(self.state, to));
        }
        self.append(Event::State(self.state, to))
    }

    fn record_failure(
        &mut self,
        id: JobId,
        attempt: u32,
        error: String,
        now_ms: u64,
        backoff: bool,
    ) -> Result<bool, SupervisorError> {
        let shift = attempt.saturating_sub(1).min(20);
        // A restart is not the job's fault: retry at once (the attempt still
        // counts against the bound, so a job that crashes the host is
        // eventually quarantined).
        let delay = if !backoff {
            0
        } else {
            self.cfg
                .backoff_base_ms
                .saturating_mul(1u64 << shift)
                .min(self.cfg.backoff_max_ms)
        };
        self.append(Event::Fail(
            id,
            attempt,
            error.clone(),
            now_ms.saturating_add(delay),
        ))?;
        if attempt >= self.jobs[&id].spec.max_attempts {
            self.append(Event::Quarantine(
                id,
                format!("{attempt} failed attempts; last: {error}"),
            ))?;
            return Ok(true);
        }
        Ok(false)
    }

    // -- public API ---------------------------------------------------------

    pub fn state(&self) -> SupervisorState {
        self.state
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn log_seq(&self) -> u64 {
        self.seq
    }

    pub fn log_head(&self) -> Digest {
        self.head
    }

    pub fn job(&self, id: &JobId) -> Option<&JobRecord> {
        self.jobs.get(id)
    }

    /// Jobs in submission order.
    pub fn jobs(&self) -> impl Iterator<Item = (&JobId, &JobRecord)> {
        self.order.iter().map(move |id| (id, &self.jobs[id]))
    }

    /// Jobs not yet finished (queued, running or in backoff).
    pub fn pending(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| {
                matches!(
                    j.status,
                    JobStatus::Queued | JobStatus::Running | JobStatus::Backoff { .. }
                )
            })
            .count()
    }

    /// Idempotent submission. Returns `Ok(false)` if the job already exists.
    pub fn submit(&mut self, spec: JobSpec) -> Result<(JobId, bool), SupervisorError> {
        let id = spec.id();
        if self.jobs.contains_key(&id) {
            return Ok((id, false));
        }
        if self.state == SupervisorState::EmergencyStopped {
            return Err(SupervisorError::EmergencyStopped);
        }
        for d in &spec.deps {
            if !self.jobs.contains_key(d) {
                return Err(SupervisorError::UnknownDependency(*d));
            }
        }
        let pending = self.pending();
        if pending >= self.cfg.max_queued {
            return Err(SupervisorError::Backpressure(pending));
        }
        self.append(Event::Submit(id, spec))?;
        Ok((id, true))
    }

    pub fn command(&mut self, cmd: Command) -> Result<(), SupervisorError> {
        if self.state == SupervisorState::Starting && cmd == Command::Resume {
            return self.set_state(SupervisorState::Running);
        }
        self.set_state(cmd.target())
    }

    /// Leaves `Starting` for `Running`.
    pub fn start(&mut self) -> Result<(), SupervisorError> {
        if self.state == SupervisorState::Starting {
            self.set_state(SupervisorState::Running)?;
        }
        Ok(())
    }

    fn deps_done(&self, spec: &JobSpec) -> bool {
        spec.deps
            .iter()
            .all(|d| self.jobs.get(d).map(|j| j.status == JobStatus::Succeeded) == Some(true))
    }

    /// Highest-priority job that may start now; ties broken by submission order.
    pub fn next_ready(&self, now_ms: u64) -> Option<JobId> {
        let mut best: Option<(i64, usize, JobId)> = None;
        for (pos, id) in self.order.iter().enumerate() {
            let j = &self.jobs[id];
            let ready = match j.status {
                JobStatus::Queued => true,
                JobStatus::Backoff { until_ms } => now_ms >= until_ms,
                _ => false,
            };
            if !ready || !self.deps_done(&j.spec) {
                continue;
            }
            let key = (j.spec.priority, usize::MAX - pos, *id);
            if best.as_ref().is_none_or(|b| (key.0, key.1) > (b.0, b.1)) {
                best = Some(key);
            }
        }
        best.map(|b| b.2)
    }

    /// One scheduling step: at most one cycle runs.
    pub fn tick(
        &mut self,
        runner: &mut dyn CycleRunner,
        gate: &mut dyn Gate,
        cancel: &CancellationToken,
        now_ms: u64,
    ) -> Result<Tick, SupervisorError> {
        if self.state == SupervisorState::Draining {
            // Cycles are synchronous, so nothing is in flight here.
            self.set_state(SupervisorState::Stopped)?;
            return Ok(Tick::Drained);
        }
        if !self.state.dispatches() {
            return Ok(Tick::Idle(self.state));
        }
        let Some(id) = self.next_ready(now_ms) else {
            return Ok(Tick::NothingReady);
        };
        if let Err(reasons) = gate.admit() {
            self.last_gate = reasons.clone();
            return Ok(Tick::Gated(reasons));
        }
        self.last_gate.clear();
        let attempt = self.jobs[&id].attempts + 1;
        let spec = self.jobs[&id].spec.clone();
        self.append(Event::Start(id, attempt, self.mode))?;
        let cycle_started = std::time::Instant::now();
        let outcome = runner.run(id, &spec, self.mode, cancel);
        let failure = match outcome {
            Ok(report) => match runner.verify(id, &spec) {
                Ok(()) => {
                    self.append(Event::Succeed(id, report))?;
                    return Ok(Tick::Succeeded(id));
                }
                Err(e) => match runner.rollback(id, &spec) {
                    Ok(()) => format!("verification failed, rolled back: {e}"),
                    Err(r) => {
                        // Published state is uncertain: stop everything.
                        self.append(Event::Fail(id, attempt, format!("{e}; rollback: {r}"), 0))?;
                        self.set_state(SupervisorState::EmergencyStopped)?;
                        return Ok(Tick::Failed(id));
                    }
                },
            },
            Err(e) => e,
        };
        // Backoff counts from the end of the failed cycle, not its start.
        let failed_at = now_ms.saturating_add(cycle_started.elapsed().as_millis() as u64);
        if self.record_failure(id, attempt, failure, failed_at, true)? {
            Ok(Tick::Quarantined(id))
        } else {
            Ok(Tick::Failed(id))
        }
    }

    /// Machine-readable health snapshot (`SML_TUTORD_HEALTH_V1`).
    pub fn health_json(&self, now_ms: u64) -> String {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut new_certified = 0u64;
        let mut reexamined = 0u64;
        let mut last_success: Option<(&JobId, &CycleReport)> = None;
        let mut last_error: Option<&str> = None;
        for (id, j) in self.jobs() {
            let k = match j.status {
                JobStatus::Queued => "queued",
                JobStatus::Running => "running",
                JobStatus::Backoff { .. } => "backoff",
                JobStatus::Succeeded => "succeeded",
                JobStatus::Quarantined { .. } => "quarantined",
            };
            *counts.entry(k).or_default() += 1;
            if let Some(r) = &j.report {
                new_certified += r.new_certified;
                reexamined += r.reexamined;
                last_success = Some((id, r));
            }
            if let Some(e) = &j.last_error {
                last_error = Some(e);
            }
        }
        let jstr = |s: &str| {
            let mut o = String::from("\"");
            for ch in s.chars() {
                match ch {
                    '"' => o.push_str("\\\""),
                    '\\' => o.push_str("\\\\"),
                    c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
                    c => o.push(c),
                }
            }
            o.push('"');
            o
        };
        let count = |k: &str| counts.get(k).copied().unwrap_or(0);
        let (ls_id, ls_ver) = match last_success {
            Some((id, r)) => (jstr(&id.0.to_hex()), r.store_version.to_string()),
            None => ("null".into(), "null".into()),
        };
        let gate: Vec<String> = self.last_gate.iter().map(|g| jstr(g)).collect();
        format!(
            concat!(
                "{{\"schema\":\"SML_TUTORD_HEALTH_V1\",\"state\":{},\"mode\":{},",
                "\"uptime_ms\":{},\"log_seq\":{},\"log_head\":{},",
                "\"jobs\":{{\"queued\":{},\"running\":{},\"backoff\":{},\"succeeded\":{},\"quarantined\":{}}},",
                "\"new_certified_total\":{},\"reexamined_total\":{},",
                "\"last_success_job\":{},\"last_success_store_version\":{},",
                "\"last_error\":{},\"gate_refusals\":[{}],",
                "\"interrupted_at_open\":{},\"torn_tail_at_open\":{}}}"
            ),
            jstr(self.state.name()),
            jstr(self.mode.name()),
            now_ms.saturating_sub(self.started_ms),
            self.seq,
            jstr(&self.head.to_hex()),
            count("queued"),
            count("running"),
            count("backoff"),
            count("succeeded"),
            count("quarantined"),
            new_certified,
            reexamined,
            ls_id,
            ls_ver,
            last_error.map(jstr).unwrap_or_else(|| "null".into()),
            gate.join(","),
            self.interrupted_at_open,
            self.torn_tail_at_open,
        )
    }

    /// Writes `health.json` atomically (temp file + rename).
    pub fn write_health(&self, now_ms: u64) -> io::Result<()> {
        let tmp = self.root.join("health.json.tmp");
        {
            let mut f = File::create(&tmp)?;
            f.write_all(self.health_json(now_ms).as_bytes())?;
            f.write_all(b"\n")?;
            f.sync_all()?;
        }
        std::fs::rename(tmp, self.root.join("health.json"))
    }

    /// Reads and consumes an operator command from `<root>/control`.
    pub fn poll_control(&mut self) -> Result<Option<Command>, SupervisorError> {
        let path = self.root.join("control");
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        std::fs::remove_file(&path)?;
        match Command::parse(&text) {
            Some(c) => {
                self.command(c)?;
                Ok(Some(c))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "sml-tutord-test-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    /// Fails the first `fail_first` runs of each job, records every call.
    #[derive(Default)]
    struct Scripted {
        fail_first: u32,
        calls: Vec<(JobId, u32)>,
        verify_fails: bool,
        rollbacks: usize,
    }

    impl CycleRunner for Scripted {
        fn run(
            &mut self,
            id: JobId,
            _spec: &JobSpec,
            _mode: Mode,
            _c: &CancellationToken,
        ) -> Result<CycleReport, String> {
            let n = self.calls.iter().filter(|(j, _)| *j == id).count() as u32 + 1;
            self.calls.push((id, n));
            if n <= self.fail_first {
                return Err(format!("scripted failure {n}"));
            }
            Ok(CycleReport {
                new_certified: 4,
                store_version: n as u64,
                detail: "ok".into(),
                ..CycleReport::default()
            })
        }
        fn verify(&mut self, _: JobId, _: &JobSpec) -> Result<(), String> {
            if self.verify_fails {
                Err("audit mismatch".into())
            } else {
                Ok(())
            }
        }
        fn rollback(&mut self, _: JobId, _: &JobSpec) -> Result<(), String> {
            self.rollbacks += 1;
            Ok(())
        }
    }

    struct Closed;
    impl Gate for Closed {
        fn admit(&mut self) -> Result<(), Vec<String>> {
            Err(vec!["mem_available unknown".into()])
        }
    }

    fn cfg() -> SupervisorConfig {
        SupervisorConfig {
            max_queued: 4,
            backoff_base_ms: 100,
            backoff_max_ms: 1_000,
        }
    }

    fn open(root: &Path) -> Supervisor {
        let mut s = Supervisor::open(root, Mode::Simulation, cfg(), 0).unwrap();
        s.start().unwrap();
        s
    }

    #[test]
    fn identity_ignores_priority_and_dep_order() {
        let a = JobSpec::new("x").param("seed", 1);
        let b = JobSpec::new("x").param("seed", 2);
        let mut c = JobSpec::new("y");
        c.deps = vec![a.id(), b.id()];
        let mut d = c.clone();
        d.deps.reverse();
        d.priority = 9;
        assert_eq!(c.id(), d.id());
        assert_ne!(a.id(), b.id());
    }

    #[test]
    fn submit_is_idempotent_and_bounded() {
        let root = tmp("submit");
        let mut s = open(&root);
        let (id, fresh) = s.submit(JobSpec::new("x").param("seed", 0)).unwrap();
        assert!(fresh);
        assert_eq!(
            s.submit(JobSpec::new("x").param("seed", 0)).unwrap(),
            (id, false)
        );
        for i in 1..4 {
            s.submit(JobSpec::new("x").param("seed", i)).unwrap();
        }
        assert!(matches!(
            s.submit(JobSpec::new("x").param("seed", 99)),
            Err(SupervisorError::Backpressure(4))
        ));
        let mut orphan = JobSpec::new("x");
        orphan.deps.push(JobId(Digest::of(b"nope")));
        assert!(matches!(
            s.submit(orphan),
            Err(SupervisorError::UnknownDependency(_))
        ));
    }

    #[test]
    fn priority_then_fifo_and_dependencies_gate_dispatch() {
        let root = tmp("prio");
        let mut s = open(&root);
        let (a, _) = s.submit(JobSpec::new("a")).unwrap();
        let mut hi = JobSpec::new("hi");
        hi.priority = 5;
        hi.deps = vec![a];
        let (hi_id, _) = s.submit(hi).unwrap();
        let (b, _) = s.submit(JobSpec::new("b")).unwrap();
        let mut r = Scripted::default();
        let c = CancellationToken::new();
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::Succeeded(a)
        );
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::Succeeded(hi_id)
        );
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::Succeeded(b)
        );
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::NothingReady
        );
    }

    #[test]
    fn bounded_retries_backoff_then_quarantine() {
        let root = tmp("retry");
        let mut s = open(&root);
        let mut spec = JobSpec::new("flaky");
        spec.max_attempts = 3;
        let (id, _) = s.submit(spec).unwrap();
        let mut r = Scripted {
            fail_first: 10,
            ..Scripted::default()
        };
        let c = CancellationToken::new();
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::Failed(id)
        );
        // Backoff 100 ms after attempt 1.
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 99).unwrap(),
            Tick::NothingReady
        );
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 100).unwrap(),
            Tick::Failed(id)
        );
        // 200 ms after attempt 2.
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 299).unwrap(),
            Tick::NothingReady
        );
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 300).unwrap(),
            Tick::Quarantined(id)
        );
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 1_000_000).unwrap(),
            Tick::NothingReady
        );
        assert_eq!(r.calls.len(), 3);
        drop(s);
        let s = open(&root);
        assert!(matches!(
            s.job(&id).unwrap().status,
            JobStatus::Quarantined { .. }
        ));
    }

    #[test]
    fn gate_and_states_block_dispatch() {
        let root = tmp("gate");
        let mut s = open(&root);
        s.submit(JobSpec::new("x")).unwrap();
        let mut r = Scripted::default();
        let c = CancellationToken::new();
        assert!(matches!(
            s.tick(&mut r, &mut Closed, &c, 0).unwrap(),
            Tick::Gated(_)
        ));
        assert!(s.health_json(0).contains("mem_available unknown"));
        s.command(Command::Pause).unwrap();
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::Idle(SupervisorState::Paused)
        );
        s.command(Command::Maintenance).unwrap();
        s.command(Command::Resume).unwrap();
        s.command(Command::Drain).unwrap();
        assert_eq!(s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(), Tick::Drained);
        assert_eq!(s.state(), SupervisorState::Stopped);
        assert!(r.calls.is_empty());
        assert!(matches!(
            s.command(Command::Pause),
            Err(SupervisorError::IllegalTransition(..))
        ));
    }

    #[test]
    fn restart_resumes_without_loss_or_duplication() {
        let root = tmp("restart");
        let c = CancellationToken::new();
        let (a, b);
        {
            let mut s = open(&root);
            a = s.submit(JobSpec::new("a")).unwrap().0;
            b = s.submit(JobSpec::new("b")).unwrap().0;
            let mut r = Scripted::default();
            assert_eq!(
                s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
                Tick::Succeeded(a)
            );
            // Simulate a crash in the middle of `b`: start recorded, no outcome.
            s.append(Event::Start(b, 1, Mode::Simulation)).unwrap();
        }
        let mut s = open(&root);
        assert_eq!(s.interrupted_at_open, 1);
        assert_eq!(s.job(&a).unwrap().status, JobStatus::Succeeded);
        assert!(!s.submit(JobSpec::new("a")).unwrap().1);
        let mut r = Scripted::default();
        // Interrupted job is retried immediately, exactly once.
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::Succeeded(b)
        );
        assert_eq!(r.calls, vec![(b, 1)]);
        assert_eq!(s.job(&b).unwrap().attempts, 2);
    }

    #[test]
    fn failed_verification_rolls_back_and_counts_as_failure() {
        let root = tmp("rollback");
        let mut s = open(&root);
        let (id, _) = s.submit(JobSpec::new("x")).unwrap();
        let mut r = Scripted {
            verify_fails: true,
            ..Scripted::default()
        };
        let c = CancellationToken::new();
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 0).unwrap(),
            Tick::Failed(id)
        );
        assert_eq!(r.rollbacks, 1);
        assert!(s
            .job(&id)
            .unwrap()
            .last_error
            .as_deref()
            .unwrap()
            .contains("rolled back"));
    }

    #[test]
    fn second_instance_on_same_root_is_refused() {
        let root = tmp("lock");
        let s = open(&root);
        assert!(matches!(
            Supervisor::open(&root, Mode::Simulation, cfg(), 0),
            Err(SupervisorError::Locked)
        ));
        drop(s);
        let _again = open(&root);
    }

    #[test]
    fn exhausted_job_left_in_backoff_by_a_crash_is_quarantined_at_open() {
        let root = tmp("exhausted");
        let mut spec = JobSpec::new("x");
        spec.max_attempts = 1;
        let id = spec.id();
        {
            let mut s = open(&root);
            s.submit(spec).unwrap();
            // Crash after the final Fail, before its Quarantine.
            s.append(Event::Start(id, 1, Mode::Simulation)).unwrap();
            s.append(Event::Fail(id, 1, "boom".into(), 0)).unwrap();
        }
        let mut s = open(&root);
        assert!(matches!(
            s.job(&id).unwrap().status,
            JobStatus::Quarantined { .. }
        ));
        let mut r = Scripted::default();
        let c = CancellationToken::new();
        assert_eq!(
            s.tick(&mut r, &mut OpenGate, &c, 10_000).unwrap(),
            Tick::NothingReady
        );
        assert!(r.calls.is_empty());
    }

    #[test]
    fn emergency_stop_is_sticky() {
        let root = tmp("emergency");
        {
            let mut s = open(&root);
            s.command(Command::Emergency).unwrap();
            assert!(matches!(
                s.submit(JobSpec::new("x")),
                Err(SupervisorError::EmergencyStopped)
            ));
        }
        assert!(matches!(
            Supervisor::open(&root, Mode::Simulation, cfg(), 0),
            Err(SupervisorError::EmergencyStopped)
        ));
    }

    #[test]
    fn torn_tail_is_truncated_and_tampering_fails_closed() {
        let root = tmp("tamper");
        {
            let mut s = open(&root);
            s.submit(JobSpec::new("x").param("k", "a,b=c%\t")).unwrap();
        }
        let log = root.join("supervisor.log");
        let good = std::fs::read(&log).unwrap();
        let mut torn = good.clone();
        torn.extend_from_slice(b"E\t99\tdead");
        std::fs::write(&log, &torn).unwrap();
        let s = open(&root);
        assert!(s.torn_tail_at_open);
        assert_eq!(
            s.jobs().next().unwrap().1.spec.params["k"],
            "a,b=c%\t".to_string()
        );
        drop(s);
        let text = String::from_utf8(std::fs::read(&log).unwrap()).unwrap();
        let tampered = text.replacen("submit\t", "submit\t0", 1);
        std::fs::write(&log, tampered).unwrap();
        assert!(matches!(
            Supervisor::open(&root, Mode::Simulation, cfg(), 0),
            Err(SupervisorError::Corrupt { .. })
        ));
    }

    #[test]
    fn control_file_and_health_are_consistent() {
        let root = tmp("control");
        let mut s = open(&root);
        std::fs::write(root.join("control"), "pause\n").unwrap();
        assert_eq!(s.poll_control().unwrap(), Some(Command::Pause));
        assert!(!root.join("control").exists());
        assert_eq!(s.poll_control().unwrap(), None);
        s.write_health(5).unwrap();
        let h = std::fs::read_to_string(root.join("health.json")).unwrap();
        assert!(h.starts_with("{\"schema\":\"SML_TUTORD_HEALTH_V1\",\"state\":\"paused\""));
        assert!(h.contains("\"mode\":\"simulation\""));
    }
}

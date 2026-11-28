//! `sml-tutord`: permanent SML-TUTOR supervisor daemon.
//!
//! Usage:
//!
//! ```text
//! sml-tutord root=DIR [mode=simulation|production] [workers=N] [per_domain=16]
//!            [inputs=10] [degree=3] [max_cycles=0] [idle_ms=5000] [cycle_gap_ms=0] [archive_cycles=0] [plan_ahead=2]
//!            [min_mem_gib=4] [max_load_per_cpu=0.85] [min_disk_gib=2]
//!            [l1_native=EXE l1_fr=DIR l1_en=DIR l1_fr_sha=HEX l1_en_sha=HEX
//!             l1_every=4] [resource_lock=PATH]
//! ```
//!
//! With the `l1_*` arguments, every `l1_every`-th planned job is an
//! `l1-retention` job: the two already-certified native L1 banks (French,
//! English) are re-admitted through the P1 engine into a fresh cycle store and
//! must pass every cohort at 100 % again, byte-identical to the parent bank.
//! These count as **re-examinations**, never as new knowledge. The native
//! learner executable and the frozen corpus directories must come from a
//! trusted, pinned build (RemoteOps), exactly as for `sml_tutor_bilingual_l1`.
//!
//! Each cycle is one ANF GF(2) curriculum (Mathematics/M2, native induction)
//! with a fresh seed, run through the P1 engine into its own transactional
//! store under `DIR/cycles/<job>`. The four discipline labels inside an ANF
//! curriculum only exercise lane fairness: they are **not** French, English or
//! Rust lessons. Real linguistic and Rust backends plug in as further job
//! kinds.
//!
//! `resource_lock=PATH` makes every cycle take an exclusive `flock` on PATH
//! (for example the shared Dell lock) just for that cycle; if another job
//! holds it, the tick is gated and retried after `idle_ms`.
//!
//! `archive_cycles=1` packs each successful cycle directory into one
//! verified archive `DIR/archive/<job>.smlpack` (every byte of every file,
//! see `sml_tutor::archive`) and removes the directory only after the archive
//! has been fsynced and re-read byte for byte against it. If anything fails
//! the directory is kept. `sml-tutord unpack ARCHIVE DEST` restores it.
//!
//! `cycle_gap_ms=N` paces a continuous run: after each successful job the
//! daemon waits N ms (cut short as soon as `DIR/control` holds a command), so
//! the store growth and the host share stay bounded.
//!
//! `mode=production` is refused unless `DIR/PRODUCTION_ADMITTED` exists: the
//! switch to production is an explicit operator decision, never automatic.
//! Operator commands are written to `DIR/control` (pause, resume, maintenance,
//! drain, stop, emergency). `DIR/health.json` is rewritten after every tick.
//! `max_cycles=0` runs until stopped in legacy mode.
//! `cumulative_math=1` is a bounded simulation on a separate root, with one
//! math cycle per store version and exact replay after an interruption. With
//! frozen L1 arguments, the first cycle also re-admits both language banks.
//! Later cycles re-examine them directly without teacher or relearning.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sml_tutor::domain::Domain;
use sml_tutor::domain::KnowledgeState;
use sml_tutor::engine::{
    anf_curriculum, linguistic_l1_identity, linguistic_l1_key, math_curriculum, reexamine_l1,
    reexamine_math, Engine, EngineConfig, L1Input,
};
use sml_tutor::executor::{available_workers, CancellationToken};
use sml_tutor::hash::Digest;
use sml_tutor::host_probe::{sample, ResourceBudget};
use sml_tutor::mastery::Cohort;
use sml_tutor::supervisor::{
    CycleReport, CycleRunner, Gate, JobId, JobSpec, Mode, Supervisor, SupervisorConfig,
    SupervisorState, Tick,
};

const ANF_KIND: &str = "anf-m2";
const L1_KIND: &str = "l1-retention";
const CUMULATIVE_KIND: &str = "anf-cumulative-v1";
const CUMULATIVE_MAX_CYCLES: u64 = 64;

/// Frozen inputs of the L1 retention job (paths validated at every run).
#[derive(Clone, Debug)]
struct L1Config {
    native: PathBuf,
    fr: PathBuf,
    en: PathBuf,
    fr_sha: Digest,
    en_sha: Digest,
    every: u64,
}

impl L1Config {
    fn inputs(&self) -> Result<[L1Input; 2], String> {
        Ok([
            l1_input(Domain::French, &self.fr, self.fr_sha)?,
            l1_input(Domain::English, &self.en, self.en_sha)?,
        ])
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

struct Args {
    root: PathBuf,
    mode: Mode,
    workers: usize,
    per_domain: usize,
    inputs: u8,
    degree: u8,
    max_cycles: u64,
    idle_ms: u64,
    /// Pause after each successful job (pacing for continuous runs).
    cycle_gap_ms: u64,
    /// Pack successful cycle directories into verified archives.
    archive_cycles: bool,
    cumulative_math: bool,
    cumulative_l1_identity: Option<Digest>,
    plan_ahead: usize,
    budget: ResourceBudget,
    l1: Option<L1Config>,
    resource_lock: Option<PathBuf>,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        root: PathBuf::new(),
        mode: Mode::Simulation,
        workers: available_workers().min(8),
        per_domain: 16,
        inputs: 10,
        degree: 3,
        max_cycles: 0,
        idle_ms: 5_000,
        cycle_gap_ms: 0,
        archive_cycles: false,
        cumulative_math: false,
        cumulative_l1_identity: None,
        plan_ahead: 2,
        budget: ResourceBudget::default(),
        l1: None,
        resource_lock: None,
    };
    let (mut l1_native, mut l1_fr, mut l1_en, mut l1_fr_sha, mut l1_en_sha) =
        (None, None, None, None, None);
    let mut l1_every = 4u64;
    const GIB_KIB: f64 = 1024.0 * 1024.0;
    for arg in std::env::args().skip(1) {
        let (k, v) = arg
            .split_once('=')
            .ok_or_else(|| format!("expected key=value, got {arg:?}"))?;
        let bad = || format!("bad value for {k}: {v:?}");
        match k {
            "root" => a.root = PathBuf::from(v),
            "mode" => a.mode = Mode::parse(v).ok_or_else(bad)?,
            "workers" => a.workers = v.parse().map_err(|_| bad())?,
            "per_domain" => a.per_domain = v.parse().map_err(|_| bad())?,
            "inputs" => a.inputs = v.parse().map_err(|_| bad())?,
            "degree" => a.degree = v.parse().map_err(|_| bad())?,
            "max_cycles" => a.max_cycles = v.parse().map_err(|_| bad())?,
            "idle_ms" => a.idle_ms = v.parse().map_err(|_| bad())?,
            "cycle_gap_ms" => a.cycle_gap_ms = v.parse().map_err(|_| bad())?,
            "archive_cycles" => {
                a.archive_cycles = match v {
                    "0" => false,
                    "1" => true,
                    _ => return Err(bad()),
                }
            }
            "cumulative_math" => {
                a.cumulative_math = match v {
                    "0" => false,
                    "1" => true,
                    _ => return Err(bad()),
                }
            }
            "plan_ahead" => a.plan_ahead = v.parse().map_err(|_| bad())?,
            "min_mem_gib" => {
                a.budget.min_mem_available_kib =
                    (v.parse::<f64>().map_err(|_| bad())? * GIB_KIB) as u64
            }
            "min_disk_gib" => {
                a.budget.min_disk_free_kib = (v.parse::<f64>().map_err(|_| bad())? * GIB_KIB) as u64
            }
            "max_load_per_cpu" => a.budget.max_load_per_cpu = v.parse().map_err(|_| bad())?,
            "resource_lock" => a.resource_lock = Some(PathBuf::from(v)),
            "l1_native" => l1_native = Some(PathBuf::from(v)),
            "l1_fr" => l1_fr = Some(PathBuf::from(v)),
            "l1_en" => l1_en = Some(PathBuf::from(v)),
            "l1_fr_sha" => l1_fr_sha = Some(Digest::from_hex(v).ok_or_else(bad)?),
            "l1_en_sha" => l1_en_sha = Some(Digest::from_hex(v).ok_or_else(bad)?),
            "l1_every" => l1_every = v.parse().map_err(|_| bad())?,
            _ => return Err(format!("unknown argument {k}")),
        }
    }
    a.l1 = match (l1_native, l1_fr, l1_en, l1_fr_sha, l1_en_sha) {
        (None, None, None, None, None) => None,
        (Some(native), Some(fr), Some(en), Some(fr_sha), Some(en_sha)) if l1_every >= 1 => {
            Some(L1Config {
                native,
                fr,
                en,
                fr_sha,
                en_sha,
                every: l1_every,
            })
        }
        _ => {
            return Err(
                "l1 retention needs all of l1_native, l1_fr, l1_en, l1_fr_sha, l1_en_sha and l1_every>=1"
                    .into(),
            )
        }
    };
    if a.root.as_os_str().is_empty() {
        return Err("root=DIR is required".into());
    }
    if a.workers == 0
        || a.per_domain == 0
        || !(4..=sml_tutor::anf_lesson::MAX_INPUTS).contains(&a.inputs)
        || a.degree == 0
    {
        return Err(format!(
            "workers, per_domain, inputs (4..={}) or degree out of range",
            sml_tutor::anf_lesson::MAX_INPUTS
        ));
    }
    if a.cumulative_math
        && (a.mode != Mode::Simulation
            || a.plan_ahead != 1
            || !(1..=CUMULATIVE_MAX_CYCLES).contains(&a.max_cycles)
            || a.per_domain > 64
            || a.inputs > 10
            || a.degree > 3)
    {
        return Err("cumulative math requires bounded simulation and plan_ahead=1".into());
    }
    if a.cumulative_math {
        if let Some(l1) = &a.l1 {
            a.cumulative_l1_identity = Some(linguistic_l1_identity(&l1.native, l1.inputs()?)?);
        }
    }
    Ok(a)
}

struct HostGate {
    budget: ResourceBudget,
    disk: PathBuf,
    /// Shared host lock (`flock`) taken for the duration of one cycle only,
    /// so other jobs on the same host wait at most one cycle.
    lock_path: Option<PathBuf>,
    held: Option<std::fs::File>,
}

impl HostGate {
    /// Releases the per-cycle host lock (closing the descriptor drops it).
    fn release(&mut self) {
        self.held = None;
    }
}

impl Gate for HostGate {
    fn admit(&mut self) -> Result<(), Vec<String>> {
        self.budget.admit(&sample(&self.disk))?;
        if let Some(path) = &self.lock_path {
            let f = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(path)
                .map_err(|e| vec![format!("resource lock {}: {e}", path.display())])?;
            match f.try_lock() {
                Ok(()) => self.held = Some(f),
                Err(std::fs::TryLockError::WouldBlock) => {
                    return Err(vec![format!("resource lock {} busy", path.display())])
                }
                Err(std::fs::TryLockError::Error(e)) => {
                    return Err(vec![format!("resource lock {}: {e}", path.display())])
                }
            }
        }
        Ok(())
    }
}

struct AnfRunner {
    root: PathBuf,
    workers: usize,
    l1: Option<L1Config>,
}

/// Reads one frozen corpus file: regular file, not a symlink, at most 64 KiB.
fn read_frozen(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let path = dir.join(name);
    let meta = std::fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !meta.is_file() || meta.len() > 65_536 {
        return Err(format!(
            "{}: not a regular file of at most 64 KiB",
            path.display()
        ));
    }
    std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
}

fn l1_input(domain: Domain, dir: &Path, provenance: Digest) -> Result<L1Input, String> {
    let text = |name: &str| {
        String::from_utf8(read_frozen(dir, name)?).map_err(|_| format!("{name}: not UTF-8"))
    };
    Ok(L1Input {
        domain,
        lexicon: text("lexicon.txt")?,
        proposed_train: text("train.txt")?,
        oracle_train: text("oracle-train.txt")?,
        held_out: text("test.txt")?,
        parent_bank: read_frozen(dir, "linguistic.bank")?,
        provenance,
    })
}

impl AnfRunner {
    fn dir(&self, id: JobId) -> PathBuf {
        self.root.join("cycles").join(id.0.to_hex())
    }

    fn params(spec: &JobSpec) -> Result<(usize, u8, u8, u64), String> {
        let get = |k: &str| {
            spec.params
                .get(k)
                .ok_or_else(|| format!("missing parameter {k}"))
        };
        Ok((
            get("per_domain")?.parse().map_err(|_| "bad per_domain")?,
            get("inputs")?.parse().map_err(|_| "bad inputs")?,
            get("degree")?.parse().map_err(|_| "bad degree")?,
            get("seed")?.parse().map_err(|_| "bad seed")?,
        ))
        .and_then(|p: (usize, u8, u8, u64)| {
            if (4..=sml_tutor::anf_lesson::MAX_INPUTS).contains(&p.1) {
                Ok(p)
            } else {
                Err(format!(
                    "inputs {} outside 4..={}",
                    p.1,
                    sml_tutor::anf_lesson::MAX_INPUTS
                ))
            }
        })
    }
}

impl AnfRunner {
    /// Re-admits the two certified native L1 banks into a fresh cycle store.
    /// Success means every cohort passed at 100 % again and the published
    /// checkpoint is byte-identical to the parent bank: a re-examination.
    fn run_l1(
        &mut self,
        id: JobId,
        spec: &JobSpec,
        cancel: &CancellationToken,
    ) -> Result<CycleReport, String> {
        let l1 = self
            .l1
            .clone()
            .ok_or("l1-retention job but no l1_* configuration")?;
        // The job's identity pins the provenance it was planned with.
        for (k, want) in [("fr", l1.fr_sha), ("en", l1.en_sha)] {
            if spec.params.get(k).map(String::as_str) != Some(want.to_hex().as_str()) {
                return Err(format!("job provenance {k} differs from configuration"));
            }
        }
        let inputs = [
            l1_input(Domain::French, &l1.fr, l1.fr_sha)?,
            l1_input(Domain::English, &l1.en, l1.en_sha)?,
        ];
        let cfg = EngineConfig {
            workers: self.workers.min(2),
            wave: 2,
            max_attempts: 1,
            ..EngineConfig::default()
        };
        // A retention job is a pure re-examination and its exam reports live
        // only in memory: a resumed engine would skip lessons consolidated by
        // an interrupted attempt and leave no report to verify. Any previous
        // attempt's store is therefore moved aside (never deleted) and the
        // job re-examines both banks from scratch.
        self.rollback(id, spec)?;
        let (mut engine, resume) =
            Engine::open_bilingual_l1(&self.dir(id), &l1.native, inputs.clone(), cfg)
                .map_err(|e| e.to_string())?;
        let summary = engine.run(cancel).map_err(|e| e.to_string())?;
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        let consolidated = summary
            .states
            .get(&KnowledgeState::Consolidated)
            .copied()
            .unwrap_or(0);
        if consolidated != 2 || engine.store().audit().map_err(|e| e.to_string())? != 2 {
            return Err(format!("only {consolidated}/2 L1 banks re-admitted"));
        }
        let mut held_out = 0u64;
        for lesson in engine.registry().iter() {
            let report = lesson.exams.last().ok_or("missing exam report")?;
            let parent = inputs
                .iter()
                .find(|i| i.domain == lesson.spec.domain)
                .ok_or("unexpected language")?;
            let bytes = engine
                .store()
                .get(&linguistic_l1_key(&lesson.id))
                .map_err(|e| e.to_string())?
                .ok_or("missing re-admitted checkpoint")?;
            if bytes != parent.parent_bank || !report.passed() || report.cohorts.len() != 6 {
                return Err(format!(
                    "{}: checkpoint differs from parent or a cohort failed",
                    lesson.spec.domain
                ));
            }
            held_out += report
                .cohorts
                .iter()
                .filter(|c| matches!(c.cohort, Cohort::Dev | Cohort::Test | Cohort::TestTest))
                .map(|c| c.total)
                .sum::<u64>();
        }
        Ok(CycleReport {
            new_certified: 0,
            reexamined: 2,
            rejected: 0,
            store_version: summary.store_version,
            detail: format!(
                "l1-retention seq={} banks=2 held_out={held_out} waves={} wall_ms={} resumed_records={}",
                spec.params.get("seq").map(String::as_str).unwrap_or("?"),
                summary.waves,
                summary.wall_nanos / 1_000_000,
                resume.replayed_records
            ),
        })
    }
}

impl AnfRunner {
    fn cumulative_store(&self) -> Result<sml_tutor::store::KnowledgeStore, String> {
        sml_tutor::store::KnowledgeStore::open(self.root.join("cumulative-math-v1"))
            .map_err(|e| e.to_string())
    }

    /// The job seed is also the expected previous manifest version. The
    /// supervisor runs one job at a time, and the next job is only planned
    /// after a success. The only allowed ahead-of-journal state is the
    /// exact current cycle committed before its Succeed event was synced.
    fn run_cumulative(
        &mut self,
        id: JobId,
        spec: &JobSpec,
        cancel: &CancellationToken,
    ) -> Result<CycleReport, String> {
        let (lessons, inputs, degree, seed) = Self::params(spec)?;
        if seed >= CUMULATIVE_MAX_CYCLES
            || !(1..=64).contains(&lessons)
            || inputs > 10
            || degree > 3
        {
            return Err("cumulative curriculum outside qualified bounds".into());
        }
        let linguistic = match &self.l1 {
            Some(l1) => {
                let inputs = l1.inputs()?;
                let identity = linguistic_l1_identity(&l1.native, inputs.clone())?;
                if spec.params.get("l1").map(String::as_str) != Some(identity.to_hex().as_str()) {
                    return Err("cumulative L1 identity mismatch".into());
                }
                Some((l1.native.clone(), inputs))
            }
            None if spec.params.contains_key("l1") => return Err("missing cumulative L1".into()),
            None => None,
        };
        let banks = if linguistic.is_some() { 2 } else { 0 };
        let cardinality =
            |version: u64| version as usize * lessons + if version > 0 { banks } else { 0 };
        let target = self.cumulative_store()?;
        let before = target.snapshot().map_err(|e| e.to_string())?;
        if !(before.version == seed || before.version == seed + 1)
            || before.entries.len() != cardinality(before.version)
        {
            return Err("cumulative version/cardinality mismatch".into());
        }
        let (retained, retention_cases) = if seed == 0 {
            (0, 0)
        } else {
            reexamine_math(&target, &math_curriculum(lessons, inputs, degree, seed - 1))
                .map_err(|e| e.to_string())?
        };
        let (l1_retained, l1_cases) = match &linguistic {
            Some((binary, inputs)) if seed != 0 => reexamine_l1(&target, binary, inputs.clone())?,
            _ => (0, 0),
        };
        // Live exam reports are required to reconstruct certificates. An
        // interrupted local cycle is kept as evidence and independently
        // re-examined from scratch, even if its cumulative commit survived.
        self.move_cycle_aside(id)?;
        let cfg = EngineConfig {
            workers: self.workers,
            ..EngineConfig::default()
        };
        let curriculum = math_curriculum(lessons, inputs, degree, seed);
        let (mut engine, _) = match &linguistic {
            Some((binary, input)) if seed == 0 => Engine::open_mixed_math(
                &self.dir(id),
                binary,
                input.clone(),
                curriculum,
                EngineConfig {
                    workers: self.workers.min(2),
                    wave: 2,
                    ..cfg
                },
            ),
            _ => Engine::open(&self.dir(id), curriculum, cfg),
        }
        .map_err(|e| e.to_string())?;
        let published = lessons + if seed == 0 { banks } else { 0 };
        let summary = engine.run(cancel).map_err(|e| e.to_string())?;
        if cancel.is_cancelled()
            || summary.states.get(&KnowledgeState::Consolidated) != Some(&published)
        {
            return Err("cumulative cycle incomplete".into());
        }
        reexamine_math(
            engine.store(),
            &math_curriculum(lessons, inputs, degree, seed),
        )
        .map_err(|e| e.to_string())?;
        if target.snapshot().map_err(|e| e.to_string())? != before {
            return Err("cumulative target changed during cycle".into());
        }
        let outcome = engine
            .publish_certified_cycle(&target)
            .map_err(|e| e.to_string())?;
        let replay = before.version == seed + 1;
        if outcome.version != seed + 1
            || (replay && (outcome.added != 0 || outcome.idempotent != published))
            || (!replay && (outcome.added != published || outcome.idempotent != 0))
        {
            return Err("unexpected cumulative publication receipt".into());
        }
        let total = engine.telemetry.total();
        // This is the unique job's acquisition count, not the number of
        // writes attempted in this process. The journal counts Succeed once.
        Ok(CycleReport {
            new_certified: lessons as u64,
            reexamined: (retained + l1_retained + if seed == 0 { banks } else { 0 }) as u64,
            rejected: total.teacher_rejections + total.exam_failures,
            store_version: outcome.version,
            detail: format!(
                "cumulative-math seed={seed} lessons={lessons} retained={retained} retention_cases={retention_cases} l1_banks={banks} l1_retained={l1_retained} l1_cases={l1_cases} exact_replay={replay}"
            ),
        })
    }

    fn move_cycle_aside(&self, id: JobId) -> Result<(), String> {
        let from = self.dir(id);
        if !from.exists() {
            return Ok(());
        }
        let to = self
            .root
            .join("rolled-back")
            .join(format!("{}-{}", id.0.to_hex(), now_ms()));
        std::fs::create_dir_all(to.parent().unwrap_or(Path::new(".")))
            .and_then(|_| std::fs::rename(&from, &to))
            .map_err(|e| e.to_string())
    }
}

impl CycleRunner for AnfRunner {
    fn run(
        &mut self,
        id: JobId,
        spec: &JobSpec,
        mode: Mode,
        cancel: &CancellationToken,
    ) -> Result<CycleReport, String> {
        match spec.kind.as_str() {
            ANF_KIND => {}
            CUMULATIVE_KIND if mode == Mode::Simulation => {
                return self.run_cumulative(id, spec, cancel);
            }
            L1_KIND => return self.run_l1(id, spec, cancel),
            k => return Err(format!("unsupported job kind {k}")),
        }
        let (per_domain, inputs, degree, seed) = Self::params(spec)?;
        let cfg = EngineConfig {
            workers: self.workers,
            ..EngineConfig::default()
        };
        let curriculum = anf_curriculum(per_domain, inputs, degree, seed);
        let lessons = curriculum.specs.len() as u64;
        let (mut engine, resume) =
            Engine::open(&self.dir(id), curriculum, cfg).map_err(|e| e.to_string())?;
        let summary = engine.run(cancel).map_err(|e| e.to_string())?;
        if cancel.is_cancelled() {
            return Err("cancelled".into());
        }
        let consolidated = summary
            .states
            .get(&KnowledgeState::Consolidated)
            .copied()
            .unwrap_or(0) as u64;
        if consolidated != lessons {
            return Err(format!(
                "only {consolidated}/{lessons} lessons consolidated"
            ));
        }
        let total = engine.telemetry.total();
        Ok(CycleReport {
            new_certified: consolidated,
            reexamined: 0,
            rejected: total.teacher_rejections + total.exam_failures,
            store_version: summary.store_version,
            detail: format!(
                "anf seed={seed} lessons={lessons} waves={} wall_ms={} resumed_records={}",
                summary.waves,
                summary.wall_nanos / 1_000_000,
                resume.replayed_records
            ),
        })
    }

    fn verify(&mut self, id: JobId, spec: &JobSpec) -> Result<(), String> {
        if spec.kind == CUMULATIVE_KIND {
            let (lessons, inputs, degree, seed) = Self::params(spec)?;
            let target = self.cumulative_store()?;
            let snapshot = target.snapshot().map_err(|e| e.to_string())?;
            let banks = if spec.params.contains_key("l1") { 2 } else { 0 };
            if snapshot.version != seed + 1
                || snapshot.entries.len() != (seed + 1) as usize * lessons + banks
            {
                return Err("cumulative post-commit version/cardinality mismatch".into());
            }
            reexamine_math(&target, &math_curriculum(lessons, inputs, degree, seed))
                .map_err(|e| e.to_string())?;
            if let Some(l1) = &self.l1 {
                let inputs = l1.inputs()?;
                let identity = linguistic_l1_identity(&l1.native, inputs.clone())?;
                if spec.params.get("l1").map(String::as_str) != Some(identity.to_hex().as_str()) {
                    return Err("post-commit L1 identity mismatch".into());
                }
                reexamine_l1(&target, &l1.native, inputs)?;
            }
            return Ok(());
        }
        let store = sml_tutor::store::KnowledgeStore::open(self.dir(id).join("store"))
            .map_err(|e| e.to_string())?;
        store.audit().map(|_| ()).map_err(|e| e.to_string())
    }

    /// Non-destructive rollback: the cycle's store is moved aside, never deleted.
    fn rollback(&mut self, id: JobId, spec: &JobSpec) -> Result<(), String> {
        if spec.kind == CUMULATIVE_KIND {
            // A published shared manifest must not be silently undone or
            // removed. An uncertain post-commit audit forces emergency stop.
            return Err("cumulative publication requires operator recovery".into());
        }
        self.move_cycle_aside(id)
    }
}

/// Keeps `plan_ahead` jobs pending. With L1 configured, the job planned at
/// position `k` (counting every planned job) is an L1 retention job when
/// `k % l1_every == l1_every - 1`, an ANF cycle otherwise.
fn cumulative_spec(a: &Args, seed: u64) -> JobSpec {
    let spec = JobSpec::new(CUMULATIVE_KIND)
        .param("per_domain", a.per_domain)
        .param("inputs", a.inputs)
        .param("degree", a.degree)
        .param("seed", seed);
    match a.cumulative_l1_identity {
        Some(id) => spec.param("l1", id.to_hex()),
        None => spec,
    }
}

/// Reconcile the durable job ledger with the cumulative store before
/// dispatch. An interrupted publication may be exactly one version ahead.
fn check_cumulative_start(sup: &Supervisor, a: &Args) -> Result<(), String> {
    let dir = a.root.join("cumulative-math-v1");
    if !a.cumulative_math {
        if dir.exists() || sup.count_kind(CUMULATIVE_KIND) != 0 {
            return Err("cumulative root requires cumulative_math=1".into());
        }
        return Ok(());
    }
    if sup.count_kind(ANF_KIND) != 0
        || sup.count_kind(L1_KIND) != 0
        || sup.count_kind(CUMULATIVE_KIND) != sup.jobs().count() as u64
    {
        return Err("legacy or compacted ledger cannot enter bounded cumulative mode".into());
    }
    let mut succeeded = 0u64;
    let mut pending = false;
    for (i, (_, job)) in sup.jobs().enumerate() {
        let seed = i as u64;
        if job.spec != cumulative_spec(a, seed) || seed >= CUMULATIVE_MAX_CYCLES || pending {
            return Err("incompatible cumulative job ledger".into());
        }
        match &job.status {
            JobStatus::Succeeded => {
                let report = job.report.as_ref().ok_or("missing cumulative receipt")?;
                if report.store_version != seed + 1 || report.new_certified != a.per_domain as u64 {
                    return Err("cumulative receipt mismatch".into());
                }
                succeeded += 1;
            }
            JobStatus::Quarantined { .. } => return Err("cumulative job quarantined".into()),
            _ => pending = true,
        }
    }
    if succeeded + a.max_cycles > CUMULATIVE_MAX_CYCLES {
        return Err("cumulative root reached simulation cycle budget".into());
    }
    // A first queued job can precede creation of the target directory.
    if !dir.exists() && succeeded != 0 {
        return Err("cumulative store missing".into());
    }
    let target = sml_tutor::store::KnowledgeStore::open(dir).map_err(|e| e.to_string())?;
    let snapshot = target.snapshot().map_err(|e| e.to_string())?;
    if snapshot.version < succeeded
        || snapshot.version > succeeded + u64::from(pending)
        || target.audit().map_err(|e| e.to_string())?
            != snapshot.version as usize * a.per_domain
                + if snapshot.version != 0 && a.cumulative_l1_identity.is_some() {
                    2
                } else {
                    0
                }
    {
        return Err("cumulative store and job ledger disagree".into());
    }
    Ok(())
}

fn plan(sup: &mut Supervisor, a: &Args) -> Result<(), String> {
    if a.cumulative_math {
        if sup
            .jobs()
            .any(|(_, j)| matches!(j.status, JobStatus::Quarantined { .. }))
        {
            return Err("cumulative job quarantined; refusing next seed".into());
        }
        if sup.pending() == 0 {
            let seed = sup.count_kind(CUMULATIVE_KIND);
            if seed >= CUMULATIVE_MAX_CYCLES {
                return Err("cumulative cycle budget exhausted".into());
            }
            sup.submit(cumulative_spec(a, seed))
                .map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    // Counts include succeeded jobs compacted out of the record table.
    let mut anf = sup.count_kind(ANF_KIND);
    let mut l1 = sup.count_kind(L1_KIND);
    while sup.pending() < a.plan_ahead {
        let spec = match &a.l1 {
            Some(c) if (anf + l1) % c.every == c.every - 1 => JobSpec::new(L1_KIND)
                .param("seq", l1)
                .param("fr", c.fr_sha.to_hex())
                .param("en", c.en_sha.to_hex()),
            _ => JobSpec::new(ANF_KIND)
                .param("per_domain", a.per_domain)
                .param("inputs", a.inputs)
                .param("degree", a.degree)
                .param("seed", anf),
        };
        let is_l1 = spec.kind == L1_KIND;
        sup.submit(spec).map_err(|e| e.to_string())?;
        if is_l1 {
            l1 += 1;
        } else {
            anf += 1;
        }
    }
    Ok(())
}

/// Packs `root/cycles/<id>` into `root/archive/<id>.smlpack`, verifies the
/// archive byte for byte against the directory, then removes the directory.
/// Returns (files, archive bytes, digest). On any error the directory stays.
fn archive_cycle(root: &Path, id: JobId) -> Result<Option<(usize, u64, String)>, String> {
    use sml_tutor::archive;
    let hex = id.0.to_hex();
    let dir = root.join("cycles").join(&hex);
    if !dir.is_dir() {
        return Ok(None);
    }
    let adir = root.join("archive");
    if !adir.is_dir() {
        std::fs::create_dir_all(&adir).map_err(|e| e.to_string())?;
        // Persist the new directory entry before any cycle data can be removed.
        std::fs::File::open(root)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
    }
    let fin = adir.join(format!("{hex}.smlpack"));
    let digest = if fin.exists() {
        // A crash after the rename: the archive is complete, re-check it.
        archive::read(&fin).map_err(|e| e.to_string())?.0
    } else {
        let tmp = adir.join(format!("{hex}.smlpack.partial"));
        // Only ever an unfinished archive written by this daemon.
        let _ = std::fs::remove_file(&tmp);
        let d = archive::pack(&dir, &tmp).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &fin).map_err(|e| e.to_string())?;
        std::fs::File::open(&adir)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        d
    };
    let files = archive::verify_against(&fin, digest, &dir).map_err(|e| e.to_string())?;
    let bytes = std::fs::metadata(&fin).map_err(|e| e.to_string())?.len();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(Some((files, bytes, digest.to_hex())))
}

/// Archives one cycle and prints the outcome line.
fn archive_and_report(root: &Path, id: JobId) {
    match archive_cycle(root, id) {
        Ok(Some((files, bytes, digest))) => println!(
            "SML_TUTORD_ARCHIVE=PASS job={} files={files} bytes={bytes} digest={digest}",
            id.short()
        ),
        Ok(None) => {}
        Err(e) => println!(
            "SML_TUTORD_ARCHIVE=FAIL job={} kept_dir=true reason={e}",
            id.short()
        ),
    }
}

/// Sleeps up to `ms`, returning early once an operator command is waiting
/// in `root/control`, so pacing never delays a drain or an emergency stop.
fn pause(root: &Path, ms: u64) {
    let end = std::time::Instant::now() + Duration::from_millis(ms);
    while std::time::Instant::now() < end && !root.join("control").exists() {
        std::thread::sleep(Duration::from_millis(100).min(end - std::time::Instant::now()));
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    if argv.get(1).map(String::as_str) == Some("unpack") {
        let code = match (argv.get(2), argv.get(3), argv.len()) {
            (Some(src), Some(dest), 4) => {
                match sml_tutor::archive::unpack(Path::new(src), Path::new(dest)) {
                    Ok(n) => {
                        println!("SML_TUTORD_UNPACK=PASS files={n} dest={dest}");
                        0
                    }
                    Err(e) => {
                        eprintln!("SML_TUTORD_UNPACK=FAIL reason={e}");
                        1
                    }
                }
            }
            _ => {
                eprintln!("usage: sml-tutord unpack ARCHIVE DEST");
                2
            }
        };
        std::process::exit(code);
    }
    let a = match parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("SML_TUTORD=FAIL reason={e}");
            std::process::exit(2);
        }
    };
    if a.mode == Mode::Production && !a.root.join("PRODUCTION_ADMITTED").is_file() {
        eprintln!("SML_TUTORD=FAIL reason=production mode requires PRODUCTION_ADMITTED");
        std::process::exit(2);
    }
    let mut sup = match Supervisor::open(&a.root, a.mode, SupervisorConfig::default(), now_ms()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SML_TUTORD=FAIL reason={e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = check_cumulative_start(&sup, &a) {
        eprintln!("SML_TUTORD=FAIL reason={e}");
        std::process::exit(1);
    }
    println!(
        "SML_TUTORD_START mode={} root={} workers={} log_seq={} interrupted={} torn_tail={}",
        a.mode.name(),
        a.root.display(),
        a.workers,
        sup.log_seq(),
        sup.interrupted_at_open,
        sup.torn_tail_at_open
    );
    if a.archive_cycles {
        // Finish archiving left unfinished by a crash or an earlier failure:
        // succeeded jobs are never run again, so this is their only retry.
        // The cycle directories are scanned (not the record table) so that
        // succeeded jobs compacted out of memory are covered too.
        let mut done: Vec<JobId> = std::fs::read_dir(a.root.join("cycles"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().to_str().and_then(Digest::from_hex))
            .map(JobId)
            .filter(|id| sup.succeeded(id))
            .collect();
        done.sort();
        for id in done {
            archive_and_report(&a.root, id);
        }
    }
    let mut runner = AnfRunner {
        root: a.root.clone(),
        workers: a.workers,
        l1: a.l1.clone(),
    };
    let mut gate = HostGate {
        budget: a.budget,
        disk: a.root.clone(),
        lock_path: a.resource_lock.clone(),
        held: None,
    };
    let cancel = CancellationToken::new();
    let mut cycles = 0u64;
    let code = loop {
        let step = (|| -> Result<Option<i32>, String> {
            sup.start().map_err(|e| e.to_string())?;
            if let Some(c) = sup.poll_control().map_err(|e| e.to_string())? {
                println!(
                    "SML_TUTORD_CONTROL command={c:?} state={}",
                    sup.state().name()
                );
            }
            match sup.state() {
                SupervisorState::Stopped => return Ok(Some(0)),
                SupervisorState::EmergencyStopped => return Ok(Some(3)),
                _ => {}
            }
            if sup.state().dispatches() {
                plan(&mut sup, &a)?;
            }
            let t = now_ms();
            let tick = sup
                .tick(&mut runner, &mut gate, &cancel, t)
                .map_err(|e| e.to_string());
            gate.release();
            let tick = tick?;
            sup.write_health(now_ms()).map_err(|e| e.to_string())?;
            match &tick {
                Tick::Succeeded(id) => {
                    cycles += 1;
                    let r = sup
                        .job(id)
                        .and_then(|j| j.report.clone())
                        .unwrap_or_default();
                    println!(
                        "SML_TUTORD_CYCLE=PASS job={} new_certified={} store_version={} {}",
                        id.short(),
                        r.new_certified,
                        r.store_version,
                        r.detail
                    );
                    if a.archive_cycles {
                        archive_and_report(&a.root, *id);
                    }
                    if a.max_cycles != 0 && cycles >= a.max_cycles {
                        sup.command(sml_tutor::supervisor::Command::Drain)
                            .map_err(|e| e.to_string())?;
                    }
                }
                Tick::Failed(id) | Tick::Quarantined(id) => {
                    let err = sup.job(id).and_then(|j| j.last_error.clone());
                    println!(
                        "SML_TUTORD_CYCLE=FAIL job={} outcome={tick:?} error={err:?}",
                        id.short()
                    );
                    if sup.breaker_tripped() {
                        println!(
                            "SML_TUTORD_BREAKER consecutive_quarantines={} state={}",
                            sup.consecutive_quarantines(),
                            sup.state().name()
                        );
                    }
                }
                Tick::Gated(r) => println!("SML_TUTORD_GATED reasons={r:?}"),
                Tick::Drained => println!("SML_TUTORD_DRAINED"),
                Tick::Idle(_) | Tick::NothingReady => {}
            }
            if matches!(
                tick,
                Tick::Gated(_) | Tick::Idle(_) | Tick::NothingReady | Tick::Failed(_)
            ) {
                std::thread::sleep(Duration::from_millis(a.idle_ms));
            } else if matches!(tick, Tick::Succeeded(_))
                && a.cycle_gap_ms > 0
                && sup.state().dispatches()
            {
                pause(&a.root, a.cycle_gap_ms);
            }
            Ok(None)
        })();
        match step {
            Ok(Some(code)) => break code,
            Ok(None) => {}
            Err(e) => {
                eprintln!("SML_TUTORD=FAIL reason={e}");
                break 1;
            }
        }
    };
    let _ = sup.write_health(now_ms());
    println!(
        "SML_TUTORD_EXIT code={code} state={} cycles_this_run={cycles} log_seq={} log_head={}",
        sup.state().name(),
        sup.log_seq(),
        sup.log_head().to_hex()
    );
    std::process::exit(code);
}

#[cfg(test)]
mod cumulative_tests {
    use super::*;

    // Simulate process loss in the exact window between committing HEAD and
    // appending the supervisor's Succeed event, using the real job ledger.
    struct InterruptedRunner<'a>(&'a mut AnfRunner);

    impl CycleRunner for InterruptedRunner<'_> {
        fn run(
            &mut self,
            id: JobId,
            spec: &JobSpec,
            mode: Mode,
            cancel: &CancellationToken,
        ) -> Result<CycleReport, String> {
            self.0.run(id, spec, mode, cancel)
        }

        fn verify(&mut self, _: JobId, _: &JobSpec) -> Result<(), String> {
            panic!("simulated process loss after cumulative commit");
        }
    }

    #[test]
    fn ledger_recovers_publication_without_double_counting() {
        use sml_tutor::supervisor::OpenGate;

        let root = std::env::temp_dir().join(format!(
            "sml-tutord-cumulative-ledger-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut runner = AnfRunner {
            root: root.clone(),
            workers: 2,
            l1: None,
        };
        let spec = JobSpec::new(CUMULATIVE_KIND)
            .param("per_domain", 4)
            .param("inputs", 7)
            .param("degree", 2)
            .param("seed", 0);
        let id = spec.id();
        let cancel = CancellationToken::new();
        let cfg = SupervisorConfig::default();
        {
            let mut supervisor = Supervisor::open(&root, Mode::Simulation, cfg, 0).unwrap();
            supervisor.start().unwrap();
            supervisor.submit(spec).unwrap();
            let crash = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                supervisor.tick(
                    &mut InterruptedRunner(&mut runner),
                    &mut OpenGate,
                    &cancel,
                    0,
                )
            }));
            assert!(crash.is_err());
            assert_eq!(supervisor.job(&id).unwrap().status, JobStatus::Running);
        }
        let target = runner.cumulative_store().unwrap();
        let before = target.snapshot().unwrap();
        assert_eq!(before.version, 1);
        assert_eq!(before.entries.len(), 4);
        let mut supervisor = Supervisor::open(&root, Mode::Simulation, cfg, 1).unwrap();
        assert_eq!(supervisor.interrupted_at_open, 1);
        supervisor.start().unwrap();
        assert!(matches!(
            supervisor
                .tick(&mut runner, &mut OpenGate, &cancel, 1)
                .unwrap(),
            Tick::Succeeded(job) if job == id
        ));
        assert_eq!(target.snapshot().unwrap(), before);
        assert!(supervisor
            .health_json(1)
            .contains("\"new_certified_total\":4,"));
        assert_eq!(
            supervisor
                .job(&id)
                .unwrap()
                .report
                .as_ref()
                .unwrap()
                .new_certified,
            4
        );
        drop(supervisor);
        let supervisor = Supervisor::open(&root, Mode::Simulation, cfg, 2).unwrap();
        assert!(supervisor
            .health_json(2)
            .contains("\"new_certified_total\":4,"));
        assert_eq!(supervisor.pending(), 0);
        drop(supervisor);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn exact_replay_after_commit_before_success_preserves_head_and_retains_math() {
        let root =
            std::env::temp_dir().join(format!("sml-tutord-cumulative-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut runner = AnfRunner {
            root: root.clone(),
            workers: 2,
            l1: None,
        };
        let spec = |seed| {
            JobSpec::new(CUMULATIVE_KIND)
                .param("per_domain", 4)
                .param("inputs", 7)
                .param("degree", 2)
                .param("seed", seed)
        };
        let cancel = CancellationToken::new();
        let first = spec(0);
        let report = runner
            .run(first.id(), &first, Mode::Simulation, &cancel)
            .unwrap();
        assert_eq!((report.new_certified, report.store_version), (4, 1));
        let target = runner.cumulative_store().unwrap();
        let before = target.snapshot().unwrap();
        drop(runner);
        let mut resumed = AnfRunner {
            root: root.clone(),
            workers: 1,
            l1: None,
        };
        let replay = resumed
            .run(first.id(), &first, Mode::Simulation, &cancel)
            .unwrap();
        assert_eq!((replay.new_certified, replay.store_version), (4, 1));
        assert!(replay.detail.contains("exact_replay=true"));
        assert_eq!(target.snapshot().unwrap(), before);
        resumed.verify(first.id(), &first).unwrap();
        let second = spec(1);
        let next = resumed
            .run(second.id(), &second, Mode::Simulation, &cancel)
            .unwrap();
        assert_eq!(
            (next.new_certified, next.reexamined, next.store_version),
            (4, 4, 2)
        );
        resumed.verify(second.id(), &second).unwrap();
        assert_eq!(target.audit().unwrap(), 8);
        for (key, record) in before.entries {
            assert_eq!(target.snapshot().unwrap().entries.get(&key), Some(&record));
        }
        assert!(resumed.rollback(second.id(), &second).is_err());
        assert_eq!(target.version().unwrap(), 2);
        let _ = std::fs::remove_dir_all(root);
    }
}

//! `sml-tutord`: permanent SML-TUTOR supervisor daemon.
//!
//! Usage:
//!
//! ```text
//! sml-tutord root=DIR [mode=simulation|production] [workers=N] [per_domain=16]
//!            [inputs=10] [degree=3] [max_cycles=0] [idle_ms=5000] [plan_ahead=2]
//!            [min_mem_gib=4] [max_load_per_cpu=0.85] [min_disk_gib=2]
//! ```
//!
//! Each cycle is one ANF GF(2) curriculum (Mathematics/M2, native induction)
//! with a fresh seed, run through the P1 engine into its own transactional
//! store under `DIR/cycles/<job>`. The four discipline labels inside an ANF
//! curriculum only exercise lane fairness: they are **not** French, English or
//! Rust lessons. Real linguistic and Rust backends plug in as further job
//! kinds.
//!
//! `mode=production` is refused unless `DIR/PRODUCTION_ADMITTED` exists: the
//! switch to production is an explicit operator decision, never automatic.
//! Operator commands are written to `DIR/control` (pause, resume, maintenance,
//! drain, stop, emergency). `DIR/health.json` is rewritten after every tick.
//! `max_cycles=0` runs until stopped.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sml_tutor::domain::KnowledgeState;
use sml_tutor::engine::{anf_curriculum, Engine, EngineConfig};
use sml_tutor::executor::{available_workers, CancellationToken};
use sml_tutor::host_probe::{sample, ResourceBudget};
use sml_tutor::supervisor::{
    CycleReport, CycleRunner, Gate, JobId, JobSpec, Mode, Supervisor, SupervisorConfig,
    SupervisorState, Tick,
};

const ANF_KIND: &str = "anf-m2";

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
    plan_ahead: usize,
    budget: ResourceBudget,
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
        plan_ahead: 2,
        budget: ResourceBudget::default(),
    };
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
            "plan_ahead" => a.plan_ahead = v.parse().map_err(|_| bad())?,
            "min_mem_gib" => {
                a.budget.min_mem_available_kib =
                    (v.parse::<f64>().map_err(|_| bad())? * GIB_KIB) as u64
            }
            "min_disk_gib" => {
                a.budget.min_disk_free_kib = (v.parse::<f64>().map_err(|_| bad())? * GIB_KIB) as u64
            }
            "max_load_per_cpu" => a.budget.max_load_per_cpu = v.parse().map_err(|_| bad())?,
            _ => return Err(format!("unknown argument {k}")),
        }
    }
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
    Ok(a)
}

struct HostGate {
    budget: ResourceBudget,
    disk: PathBuf,
}

impl Gate for HostGate {
    fn admit(&mut self) -> Result<(), Vec<String>> {
        self.budget.admit(&sample(&self.disk))
    }
}

struct AnfRunner {
    root: PathBuf,
    workers: usize,
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

impl CycleRunner for AnfRunner {
    fn run(
        &mut self,
        id: JobId,
        spec: &JobSpec,
        _mode: Mode,
        cancel: &CancellationToken,
    ) -> Result<CycleReport, String> {
        if spec.kind != ANF_KIND {
            return Err(format!("unsupported job kind {}", spec.kind));
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

    fn verify(&mut self, id: JobId, _spec: &JobSpec) -> Result<(), String> {
        let store = sml_tutor::store::KnowledgeStore::open(self.dir(id).join("store"))
            .map_err(|e| e.to_string())?;
        store.audit().map(|_| ()).map_err(|e| e.to_string())
    }

    /// Non-destructive rollback: the cycle's store is moved aside, never deleted.
    fn rollback(&mut self, id: JobId, _spec: &JobSpec) -> Result<(), String> {
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

fn plan(sup: &mut Supervisor, a: &Args) -> Result<(), String> {
    let mut submitted = sup.jobs().filter(|(_, j)| j.spec.kind == ANF_KIND).count() as u64;
    while sup.pending() < a.plan_ahead {
        let spec = JobSpec::new(ANF_KIND)
            .param("per_domain", a.per_domain)
            .param("inputs", a.inputs)
            .param("degree", a.degree)
            .param("seed", submitted);
        match sup.submit(spec) {
            Ok(_) => submitted += 1,
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

fn main() {
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
    println!(
        "SML_TUTORD_START mode={} root={} workers={} log_seq={} interrupted={} torn_tail={}",
        a.mode.name(),
        a.root.display(),
        a.workers,
        sup.log_seq(),
        sup.interrupted_at_open,
        sup.torn_tail_at_open
    );
    let mut runner = AnfRunner {
        root: a.root.clone(),
        workers: a.workers,
    };
    let mut gate = HostGate {
        budget: a.budget,
        disk: a.root.clone(),
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
                .map_err(|e| e.to_string())?;
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

//! SML-TUTOR engine qualification bench.
//!
//! Runs the identical ANF curriculum (Mathematics/M2 Boolean algebra lessons,
//! tagged across the four discipline lanes only to exercise fairness) with
//! several worker counts, checks that every configuration publishes exactly
//! the same knowledge, then simulates a crash and verifies automatic resume.
//!
//! Usage: `cargo run --release -p sml-tutor --example sml_tutor_engine_bench --
//!         [per_domain=64] [inputs=10] [degree=3] [workers=1,2,4] [lie_every=0]`
//!
//! Output lines prefixed `SML_TUTOR_ENGINE_` are machine-readable. This is an
//! engine benchmark, not a claim of linguistic, Rust or general mathematical
//! competence.

use std::path::PathBuf;

use sml_tutor::domain::{Domain, KnowledgeState};
use sml_tutor::engine::{anf_curriculum, Engine, EngineConfig};
use sml_tutor::executor::{available_workers, CancellationToken};
use sml_tutor::telemetry::rate;

struct Args {
    per_domain: usize,
    inputs: u8,
    degree: u8,
    workers: Vec<usize>,
    lie_every: u64,
}

fn parse() -> Args {
    let mut a = Args {
        per_domain: 64,
        inputs: 10,
        degree: 3,
        workers: vec![1, 2, 4],
        lie_every: 0,
    };
    for arg in std::env::args().skip(1) {
        let Some((k, v)) = arg.split_once('=') else {
            continue;
        };
        match k {
            "per_domain" => a.per_domain = v.parse().unwrap_or(a.per_domain),
            "inputs" => a.inputs = v.parse().unwrap_or(a.inputs),
            "degree" => a.degree = v.parse().unwrap_or(a.degree),
            "lie_every" => a.lie_every = v.parse().unwrap_or(a.lie_every),
            "workers" => {
                a.workers = v.split(',').filter_map(|w| w.parse().ok()).collect();
            }
            _ => {}
        }
    }
    a
}

fn tmp(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("sml-tutor-bench-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a = parse();
    let lessons = a.per_domain * 4;
    println!(
        "SML_TUTOR_ENGINE_HOST available_parallelism={} lessons={lessons} inputs={} degree={} lie_every={}",
        available_workers(),
        a.inputs,
        a.degree,
        a.lie_every
    );
    let mut reference = None;
    let mut baseline_nanos = None;
    for &w in &a.workers {
        let dir = tmp(&format!("w{w}"));
        let cfg = EngineConfig {
            workers: w,
            wave: 64,
            teacher_lie_every: a.lie_every,
            ..EngineConfig::default()
        };
        let (mut e, _) = Engine::open(
            &dir,
            anf_curriculum(a.per_domain, a.inputs, a.degree, 2026),
            cfg,
        )?;
        let s = e.run(&CancellationToken::new())?;
        let audited = e.store().audit()?;
        let entries = e.store().snapshot()?.entries;
        let identical = match &reference {
            None => {
                reference = Some(entries);
                true
            }
            Some(r) => *r == entries,
        };
        let t = e.telemetry.total();
        let base = *baseline_nanos.get_or_insert(s.learn_phase_nanos);
        let consolidated = s
            .states
            .get(&KnowledgeState::Consolidated)
            .copied()
            .unwrap_or(0);
        println!(
            "SML_TUTOR_ENGINE_RUN workers={w} waves={} consolidated={consolidated}/{lessons} blocked={} attempts={} gaps_detected={} gaps_resolved={} teacher_rejections={} audited_objects={audited} store_identical_to_first={identical} wall_ms={:.3} learn_phase_ms={:.3} learn_speedup={:.3} certified_per_sec={:.1} steals={}",
            s.waves,
            s.states.get(&KnowledgeState::Blocked).copied().unwrap_or(0),
            t.attempts,
            t.gaps_detected,
            t.gaps_resolved,
            t.teacher_rejections,
            s.wall_nanos as f64 / 1e6,
            s.learn_phase_nanos as f64 / 1e6,
            base as f64 / s.learn_phase_nanos.max(1) as f64,
            rate(t.certified, s.wall_nanos),
            s.stolen,
        );
        for d in Domain::ALL {
            let c = e.telemetry.domain(d);
            println!(
                "SML_TUTOR_ENGINE_LANE workers={w} lane={} certified={} consolidated={} exam_failures={} abstentions={}",
                d.name(),
                c.certified,
                c.consolidated,
                c.exam_failures,
                c.abstentions
            );
        }
        if !identical || consolidated != lessons {
            return Err("parallel run diverged from sequential reference".into());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Crash after two waves, then resume.
    let dir = tmp("resume");
    let w = a.workers.iter().copied().max().unwrap_or(1);
    let cfg = EngineConfig {
        workers: w,
        wave: 64,
        teacher_lie_every: a.lie_every,
        crash_after_waves: Some(2),
        ..EngineConfig::default()
    };
    let (mut e, _) = Engine::open(
        &dir,
        anf_curriculum(a.per_domain, a.inputs, a.degree, 2026),
        cfg,
    )?;
    let crashed = e.run(&CancellationToken::new())?;
    drop(e);
    let cfg = EngineConfig {
        crash_after_waves: None,
        ..cfg
    };
    let (mut e, resume) = Engine::open(
        &dir,
        anf_curriculum(a.per_domain, a.inputs, a.degree, 2026),
        cfg,
    )?;
    let s = e.run(&CancellationToken::new())?;
    let identical = reference.as_ref() == Some(&e.store().snapshot()?.entries);
    println!(
        "SML_TUTOR_ENGINE_RESUME crashed={} replayed_records={} interrupted_reset={} consolidated={}/{lessons} store_identical_to_uninterrupted={identical}",
        crashed.crashed,
        resume.replayed_records,
        resume.interrupted_reset,
        s.states.get(&KnowledgeState::Consolidated).copied().unwrap_or(0),
    );
    let _ = std::fs::remove_dir_all(&dir);
    if !identical {
        return Err("resume diverged".into());
    }
    println!("SML_TUTOR_ENGINE=PASS");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn small_bench_passes() {
        // Exercises the same code path as `main` on a reduced curriculum.
        use super::*;
        let dir = tmp("unit");
        let (mut e, _) =
            Engine::open(&dir, anf_curriculum(3, 6, 2, 1), EngineConfig::default()).unwrap();
        let s = e.run(&CancellationToken::new()).unwrap();
        assert_eq!(s.states.get(&KnowledgeState::Consolidated), Some(&12));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

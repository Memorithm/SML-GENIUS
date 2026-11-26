//! Re-admit two native L1 banks through the actual P1 engine. Use only a
//! trusted, independently built native executable and frozen corpus inputs.
use sml_tutor::domain::{Domain, KnowledgeState};
use sml_tutor::engine::{linguistic_l1_key, Engine, EngineConfig, L1Input};
use sml_tutor::executor::CancellationToken;
use sml_tutor::hash::Digest;
use sml_tutor::mastery::Cohort;
use std::error::Error;
use std::fs;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn read(dir: &Path, name: &str) -> Result<Vec<u8>> {
    let path = dir.join(name);
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_file() || meta.len() > 65_536 {
        return Err("invalid input file".into());
    }
    Ok(fs::read(path)?)
}

fn input(domain: Domain, dir: &str, provenance: &str) -> Result<L1Input> {
    let dir = Path::new(dir);
    Ok(L1Input {
        domain,
        lexicon: String::from_utf8(read(dir, "lexicon.txt")?)?,
        proposed_train: String::from_utf8(read(dir, "train.txt")?)?,
        oracle_train: String::from_utf8(read(dir, "oracle-train.txt")?)?,
        held_out: String::from_utf8(read(dir, "test.txt")?)?,
        parent_bank: read(dir, "linguistic.bank")?,
        provenance: Digest::from_hex(provenance).ok_or("invalid provenance digest")?,
    })
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [native, root, fr, en, fr_sha, en_sha, workers] = args.as_slice() else {
        return Err("usage: NATIVE ROOT FR_DIR EN_DIR FR_PROVENANCE EN_PROVENANCE WORKERS".into());
    };
    let root = Path::new(root);
    if root.exists() {
        return Err("qualification requires fresh store".into());
    }
    let inputs = [
        input(Domain::French, fr, fr_sha)?,
        input(Domain::English, en, en_sha)?,
    ];
    let cfg = EngineConfig {
        workers: workers.parse()?,
        wave: 2,
        max_attempts: 1,
        ..EngineConfig::default()
    };
    let (mut engine, _) = Engine::open_bilingual_l1(root, Path::new(native), inputs.clone(), cfg)?;
    let result = engine.run(&CancellationToken::new())?;
    if result.states.get(&KnowledgeState::Consolidated) != Some(&2) || engine.store().audit()? != 2
    {
        for lesson in engine.registry().iter() {
            eprintln!(
                "{}: {:?} {:?}",
                lesson.spec.domain, lesson.state, lesson.errors
            );
        }
        return Err("bilingual engine admission failed".into());
    }
    for lesson in engine.registry().iter() {
        let report = lesson.exams.last().ok_or("missing report")?;
        let expected = inputs
            .iter()
            .find(|i| i.domain == lesson.spec.domain)
            .ok_or("missing language")?;
        let bytes = engine
            .store()
            .get(&linguistic_l1_key(&lesson.id))?
            .ok_or("missing native checkpoint")?;
        if bytes != expected.parent_bank
            || !report.passed()
            || report.cohorts.len() != 6
            || report
                .cohorts
                .iter()
                .filter(|c| matches!(c.cohort, Cohort::Dev | Cohort::Test | Cohort::TestTest))
                .map(|c| c.total)
                .sum::<u64>()
                != 896
        {
            return Err("wrong certified checkpoint or cohort".into());
        }
        for cohort in &report.cohorts {
            println!(
                "SML_TUTOR_L1_ENGINE_COHORT lang={} cohort={} correct={} total={} abstained={}",
                lesson.spec.domain,
                cohort.cohort.label(),
                cohort.correct,
                cohort.total,
                cohort.abstained
            );
        }
    }
    let first = engine.store().snapshot()?.entries;
    drop(engine);
    let (mut reopened, _) = Engine::open_bilingual_l1(root, Path::new(native), inputs, cfg)?;
    let again = reopened.run(&CancellationToken::new())?;
    if again.waves != 0 || reopened.store().snapshot()?.entries != first {
        return Err("reopen changed consolidated memory".into());
    }
    println!(
        "SML_TUTOR_L1_ENGINE=PASS languages=2 workers={} learned_annotations=96 held_out=1792 store_entries=2 reopen_unchanged=true",
        cfg.workers
    );
    Ok(())
}

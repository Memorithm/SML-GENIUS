//! Qualify a real mixed curriculum and its restart, not four ANF lane tags.
use sml_tutor::domain::{Domain, KnowledgeState};
use sml_tutor::engine::{knowledge_key, linguistic_l1_key, Engine, EngineConfig, L1Input};
use sml_tutor::executor::CancellationToken;
use sml_tutor::hash::Digest;
use std::collections::BTreeMap;
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
        max_attempts: 6,
        ..EngineConfig::default()
    };
    let (mut engine, _) = Engine::open_mixed_l1_anf(root, Path::new(native), inputs.clone(), cfg)?;
    let result = engine.run(&CancellationToken::new())?;
    if result.states.get(&KnowledgeState::Consolidated) != Some(&8) || engine.store().audit()? != 8
    {
        return Err("mixed curriculum incomplete".into());
    }
    let mut counts = BTreeMap::new();
    for lesson in engine.registry().iter() {
        let domain = lesson.spec.domain;
        *counts.entry(domain).or_insert(0) += 1;
        let report = lesson.exams.last().ok_or("missing exam")?;
        if !report.passed() {
            return Err("uncertified mixed lesson".into());
        }
        let (key, teacher, model) = match domain {
            Domain::French | Domain::English => (
                linguistic_l1_key(&lesson.id),
                "frozen-l1-annotation-corpus",
                "sml-tutor-native-l1-bridge-v1",
            ),
            Domain::Mathematics => (
                knowledge_key(&lesson.id),
                "anf-generator-teacher",
                "sml-tutor-anf-gf2-v1",
            ),
            Domain::Rust => return Err("fake Rust curriculum".into()),
        };
        if lesson.teacher.as_deref() != Some(teacher)
            || lesson.model_version.as_deref() != Some(model)
        {
            return Err("incorrect backend provenance".into());
        }
        let bank = engine.store().get(&key)?.ok_or("missing mixed bank")?;
        if let Some(expected) = inputs.iter().find(|i| i.domain == domain) {
            if bank != expected.parent_bank {
                return Err("linguistic bank regression".into());
            }
        }
    }
    if counts
        != BTreeMap::from([
            (Domain::French, 1),
            (Domain::English, 1),
            (Domain::Mathematics, 6),
        ])
    {
        return Err("incorrect discipline counts".into());
    }
    let snapshot = engine.store().snapshot()?.entries;
    drop(engine);
    let (mut reopened, _) = Engine::open_mixed_l1_anf(root, Path::new(native), inputs, cfg)?;
    let again = reopened.run(&CancellationToken::new())?;
    if again.waves != 0 || reopened.store().snapshot()?.entries != snapshot {
        return Err("mixed restart changed store".into());
    }
    println!(
        "SML_TUTOR_MIXED=PASS fr=1 en=1 math=6 rust=0 workers={} store_entries=8 reopen_unchanged=true",
        cfg.workers
    );
    Ok(())
}

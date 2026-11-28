//! Physical R2/R3 integration gate: replay externally admitted labels into
//! a fresh experimental shared store and compare against native bank bytes.
use sml_tutor::domain::KnowledgeState;
use sml_tutor::engine::{
    math_curriculum, reexamine_rust_composition, rust_composition_key, Engine, EngineConfig,
    RustCompositionInput,
};
use sml_tutor::executor::CancellationToken;
use sml_tutor::hash::Digest;
use sml_tutor::store::KnowledgeStore;
use std::fs;
use std::io::Read;
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn read(path: &Path) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    fs::File::open(path)?.take(8193).read_to_end(&mut data)?;
    if data.is_empty() || data.len() > 8192 {
        return Err("invalid input size".into());
    }
    Ok(data)
}

fn qualify(root: &Path, input: RustCompositionInput, r2_bank: &[u8], r3_bank: &[u8]) -> Result<()> {
    // Refuse existing roots: never point this experimental gate at production.
    fs::create_dir(root)?;
    let cfg = EngineConfig {
        workers: 1,
        wave: 1,
        ..EngineConfig::default()
    };
    let cancel = CancellationToken::new();
    let target = KnowledgeStore::open(root.join("shared"))?;
    let (mut math, _) = Engine::open(&root.join("math"), math_curriculum(4, 7, 2, 11), cfg)?;
    math.run(&cancel)?;
    if math.publish_certified_cycle(&target)?.added != 4 {
        return Err("math seed publication mismatch".into());
    }
    let before = target.snapshot()?;
    let (mut engine, _) = Engine::open_rust_composition(&root.join("rust"), input.clone(), cfg)?;
    let summary = engine.run(&cancel)?;
    if summary.states.get(&KnowledgeState::Consolidated) != Some(&2) {
        return Err("composition engine did not consolidate both banks".into());
    }
    for lesson in engine.registry().iter() {
        let expected = match lesson.spec.level.as_str() {
            "R2" => r2_bank,
            "R3" => r3_bank,
            _ => return Err("unexpected lesson".into()),
        };
        let key = rust_composition_key(&lesson.id);
        if engine.store().get(&key)?.as_deref() != Some(expected) {
            return Err("engine bank differs from qualified native bank".into());
        }
    }
    if engine.publish_certified_cycle(&target)?.added != 2 {
        return Err("atomic publication mismatch".into());
    }
    let after = target.snapshot()?;
    if after.entries.len() != 6 || after.version != before.version + 1 {
        return Err("unexpected shared store cardinality or version".into());
    }
    for (key, record) in before.entries {
        if after.entries.get(&key) != Some(&record) {
            return Err("previous math record changed".into());
        }
    }
    if engine.publish_certified_cycle(&target)?.idempotent != 2 || target.snapshot()? != after {
        return Err("non-idempotent replay".into());
    }
    drop(target);
    let reopened = KnowledgeStore::open(root.join("shared"))?;
    if reopened.snapshot()? != after || reopened.audit()? != 6 {
        return Err("shared store reload failed".into());
    }
    if reexamine_rust_composition(&reopened, input)? != (458_752, 458_752) {
        return Err("independent retained examination failed".into());
    }
    println!("SML_TUTOR_RUST_STORE=PASS banks=2 programs=14 entries=6 prior_math=4 exact_bank_match=true r2_cases=458752 r3_cases=458752 replay=true reload=true");
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 7 {
        return Err(
            "usage: root r2.train r3.train r2.bank r3.bank r2_archive_sha r3_archive_sha".into(),
        );
    }
    let provenance = |i: usize| -> Result<Digest> {
        Digest::from_hex(args[i].to_str().ok_or("invalid digest")?)
            .filter(|d| *d != Digest::ZERO)
            .ok_or_else(|| "invalid archive digest".into())
    };
    let input = RustCompositionInput {
        r2_train: String::from_utf8(read(Path::new(&args[1]))?)?,
        r3_train: String::from_utf8(read(Path::new(&args[2]))?)?,
        r2_provenance: provenance(5)?,
        r3_provenance: provenance(6)?,
    };
    qualify(
        Path::new(&args[0]),
        input,
        &read(Path::new(&args[3]))?,
        &read(Path::new(&args[4]))?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sml_core::{tutor_rust_r2 as r2, tutor_rust_r3 as r3};

    #[test]
    fn exact_native_bank_and_reload_contract() {
        let root = std::env::temp_dir().join(format!(
            "sml-tutor-rust-store-example-{}",
            std::process::id()
        ));
        let input = RustCompositionInput {
            r2_train: r2::fixture(),
            r3_train: r3::fixture(),
            r2_provenance: Digest::of(b"synthetic-r2-fixture"),
            r3_provenance: Digest::of(b"synthetic-r3-fixture"),
        };
        let parent = r2::induce(&input.r2_train).unwrap();
        let child = r3::induce(&parent, &input.r3_train).unwrap();
        qualify(&root, input.clone(), parent.as_bytes(), child.as_bytes()).unwrap();
        assert!(qualify(&root, input, parent.as_bytes(), child.as_bytes()).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

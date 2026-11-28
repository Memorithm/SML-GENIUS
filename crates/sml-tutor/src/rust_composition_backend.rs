//! Native R2/R3 composition via the certified prerequisite graph.
use super::{AttemptOutput, EngineBackend, Job};
use crate::domain::Domain;
use crate::hash::{Canonical, Digest};
use crate::lesson::{LessonId, LessonSpec, SourceRef};
use crate::mastery::{check_partitions, Cohort, CohortResult, CorpusIdentity, ExamReport};
use crate::store::KnowledgeStore;
use crate::teacher::Verdict;
use sml_core::{tutor_rust_r2 as r2, tutor_rust_r3 as r3};
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;

/// Independently admitted canonical labels, with controller-attested lineage.
#[derive(Clone)]
pub struct RustCompositionInput {
    pub r2_train: String,
    pub r2_provenance: Digest,
    pub r3_train: String,
    pub r3_provenance: Digest,
}

pub fn rust_composition_key(id: &LessonId) -> Digest {
    let mut c = Canonical::new("SML_TUTOR_RUST_COMPOSITION_KEY_V1");
    c.digest(id);
    c.finish()
}

fn corpus(train: &[(u8, u8)], dev: &[(u8, u8)]) -> Result<CorpusIdentity> {
    let mut parts = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for a in 0u8..=255 {
        for b in 0u8..=255 {
            let i = usize::from(a) * 256 + usize::from(b);
            let part = if train.contains(&(a, b)) {
                0
            } else if dev.contains(&(a, b)) {
                1
            } else {
                2 + i % 2
            };
            let mut c = Canonical::new("SML_TUTOR_RUST_COMPOSITION_PAIR_V1");
            c.bytes(&[a, b]);
            parts[part].push(c.finish());
        }
    }
    let [train, dev, test, testtest] = parts;
    check_partitions(&train, &dev, &test, &testtest).map_err(|e| e.to_string())
}

fn spec(
    level: &str,
    train: &str,
    provenance: Digest,
    corpus: CorpusIdentity,
    prerequisites: Vec<LessonId>,
) -> Result<LessonSpec> {
    if provenance == Digest::ZERO || train.is_empty() || train.len() > 8192 {
        return Err("invalid Rust composition lineage or corpus size".into());
    }
    let mut payload = Canonical::new("SML_TUTOR_RUST_COMPOSITION_INPUT_V1");
    payload.str(train).digest(&corpus.digest());
    Ok(LessonSpec {
        domain: Domain::Rust,
        level: level.into(),
        skill: format!("{level}-closed-grammar-seven-programs"),
        prerequisites,
        source: SourceRef {
            name: format!("{level} canonical labels and independent oracle"),
            version: format!("{level}-U8-V1"),
            license: "PolyForm-Noncommercial-1.0.0".into(),
            authors: "Memorithm".into(),
            sha256: provenance,
        },
        payload: payload.finish(),
        difficulty: 100,
        scientific_value: 100,
        expected_gain: 100,
        user_priority: 0,
    })
}

pub(super) struct CompositionBackend {
    input: RustCompositionInput,
    r2_id: LessonId,
    r3_id: LessonId,
    r2_corpus: CorpusIdentity,
    r3_corpus: CorpusIdentity,
}

impl CompositionBackend {
    pub(super) fn new(input: RustCompositionInput) -> Result<(Vec<LessonSpec>, Self)> {
        let r2_corpus = corpus(&r2::TRAIN, &r2::DEV)?;
        let r3_corpus = corpus(&r3::TRAIN, &r3::DEV)?;
        let parent = spec(
            "R2",
            &input.r2_train,
            input.r2_provenance,
            r2_corpus,
            Vec::new(),
        )?;
        let r2_id = parent.id();
        let child = spec(
            "R3",
            &input.r3_train,
            input.r3_provenance,
            r3_corpus,
            vec![r2_id],
        )?;
        let r3_id = child.id();
        Ok((
            vec![parent, child],
            Self {
                input,
                r2_id,
                r3_id,
                r2_corpus,
                r3_corpus,
            },
        ))
    }
}

fn add_exam(out: &mut AttemptOutput, totals: [u64; 4]) {
    for (cohort, total) in [
        Cohort::TrainRecall,
        Cohort::Dev,
        Cohort::Test,
        Cohort::TestTest,
    ]
    .into_iter()
    .zip(totals)
    {
        out.report.cohorts.push(CohortResult {
            cohort,
            correct: total,
            total,
            abstained: 0,
        });
    }
    let total = totals.iter().sum();
    out.report.cohorts.push(CohortResult {
        cohort: Cohort::Reload,
        correct: total,
        total,
        abstained: 0,
    });
}

impl EngineBackend for CompositionBackend {
    fn key(&self, id: &LessonId) -> Digest {
        rust_composition_key(id)
    }

    fn teacher_name(&self, id: &LessonId) -> &str {
        if *id == self.r2_id {
            "r2-canonical-train"
        } else {
            "r3-canonical-train"
        }
    }

    fn model_version(&self, id: &LessonId) -> &str {
        if *id == self.r2_id {
            "sml-core-r2-u8-v1"
        } else {
            "sml-core-r3-u8-v1"
        }
    }

    fn attempt(&self, job: &Job) -> AttemptOutput {
        let parent = job.id == self.r2_id;
        let mut out = AttemptOutput {
            report: ExamReport {
                lesson: job.id,
                corpus: if parent {
                    self.r2_corpus
                } else {
                    self.r3_corpus
                },
                cohorts: Vec::new(),
            },
            checkpoint: None,
            verdicts: Vec::new(),
            teach_nanos: 0,
            learn_nanos: 0,
            validate_nanos: 0,
            abstained: false,
            detail: String::new(),
        };
        let memory = match job.prereqs.as_slice() {
            [] if parent => None,
            [(id, key, bytes)]
                if job.id == self.r3_id
                    && *id == self.r2_id
                    && *key == rust_composition_key(id) =>
            {
                match std::str::from_utf8(bytes) {
                    Ok(raw) => Some(raw),
                    Err(e) => {
                        out.detail = e.to_string();
                        return out;
                    }
                }
            }
            _ => {
                out.detail = "missing or wrong certified R2 prerequisite".into();
                return out;
            }
        };
        let start = Instant::now();
        let bank = match memory {
            Some(raw) => r3::induce(raw, &self.input.r3_train),
            None => r2::induce(&self.input.r2_train),
        };
        let bank = match bank {
            Ok(bank) => bank,
            Err(e) => {
                out.detail = e.to_string();
                out.verdicts.push(Verdict::Reject(out.detail.clone()));
                return out;
            }
        };
        out.learn_nanos = start.elapsed().as_nanos();
        out.verdicts = vec![Verdict::Accept; if parent { 112 } else { 168 }];
        let start = Instant::now();
        let checked = (|| -> Result<()> {
            let totals = match memory {
                Some(raw) => {
                    let retained = r2::examine(raw).map_err(|e| e.to_string())?;
                    let total = retained.iter().sum();
                    out.report.cohorts.push(CohortResult {
                        cohort: Cohort::Retention(rust_composition_key(&self.r2_id)),
                        correct: total,
                        total,
                        abstained: 0,
                    });
                    r3::examine(raw, &bank)
                }
                None => r2::examine(&bank),
            }
            .map_err(|e| e.to_string())?;
            add_exam(&mut out, totals);
            Ok(())
        })();
        match checked {
            Ok(()) => out.checkpoint = Some(bank.into_bytes()),
            Err(e) => out.detail = e,
        }
        out.validate_nanos = start.elapsed().as_nanos();
        out
    }
}

/// Independently re-examine both stored banks, including the actual parent.
pub fn reexamine_rust_composition(
    store: &KnowledgeStore,
    input: RustCompositionInput,
) -> Result<(u64, u64)> {
    let (specs, _) = CompositionBackend::new(input)?;
    let snapshot = store.snapshot().map_err(|e| e.to_string())?;
    let mut banks = Vec::new();
    for spec in specs {
        let id = spec.id();
        let key = rust_composition_key(&id);
        let rec = snapshot
            .entries
            .get(&key)
            .ok_or("missing composition record")?;
        if rec.lesson != id {
            return Err("composition record identity mismatch".into());
        }
        let bytes = store
            .get(&key)
            .map_err(|e| e.to_string())?
            .ok_or("missing composition bank")?;
        banks.push(String::from_utf8(bytes).map_err(|e| e.to_string())?);
    }
    let parent = r2::examine(&banks[0]).map_err(|e| e.to_string())?;
    let child = r3::examine(&banks[0], &banks[1]).map_err(|e| e.to_string())?;
    if store.snapshot().map_err(|e| e.to_string())? != snapshot {
        return Err("composition store changed during exam".into());
    }
    Ok((parent.iter().sum(), child.iter().sum()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::KnowledgeState;
    use crate::engine::{math_curriculum, Engine, EngineConfig};
    use crate::executor::CancellationToken;

    #[test]
    fn r3_depends_on_consolidated_r2_and_shared_memory_retains_both() {
        let root = std::env::temp_dir().join(format!(
            "sml-tutor-composition-engine-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let input = RustCompositionInput {
            r2_train: r2::fixture(),
            r2_provenance: Digest::of(b"oracle-fixture-r2"),
            r3_train: r3::fixture(),
            r3_provenance: Digest::of(b"oracle-fixture-r3"),
        };
        let cfg = EngineConfig {
            workers: 1,
            wave: 1,
            max_attempts: 6,
            ..EngineConfig::default()
        };
        let target = KnowledgeStore::open(root.join("shared")).unwrap();
        let cancel = CancellationToken::new();
        let (mut math, _) =
            Engine::open(&root.join("math"), math_curriculum(4, 7, 2, 11), cfg).unwrap();
        math.run(&cancel).unwrap();
        math.publish_certified_cycle(&target).unwrap();
        let before = target.snapshot().unwrap();
        let (mut engine, _) =
            Engine::open_rust_composition(&root.join("rust"), input.clone(), cfg).unwrap();
        let summary = engine.run(&cancel).unwrap();
        assert_eq!(summary.states.get(&KnowledgeState::Consolidated), Some(&2));
        assert_eq!(engine.publish_certified_cycle(&target).unwrap().added, 2);
        assert_eq!(
            engine.publish_certified_cycle(&target).unwrap().idempotent,
            2
        );
        let after = target.snapshot().unwrap();
        assert_eq!(after.entries.len(), 6);
        for (key, record) in before.entries {
            assert_eq!(after.entries.get(&key), Some(&record));
        }
        assert_eq!(
            reexamine_rust_composition(&target, input.clone()).unwrap(),
            (458_752, 458_752)
        );
        let bad = RustCompositionInput {
            r2_train: input.r2_train.replacen("0|0|0|0", "0|0|0|1", 1),
            ..input
        };
        let (mut engine, _) = Engine::open_rust_composition(&root.join("bad"), bad, cfg).unwrap();
        let summary = engine.run(&cancel).unwrap();
        assert_eq!(summary.states.get(&KnowledgeState::Consolidated), None);
        assert!(engine.publish_certified_cycle(&target).is_err());
        assert_eq!(target.snapshot().unwrap(), after);
        let _ = std::fs::remove_dir_all(root);
    }
}

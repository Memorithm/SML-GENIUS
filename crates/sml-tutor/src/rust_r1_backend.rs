//! Rust R1 admission through the existing independent mastery and store.
use super::{AttemptOutput, EngineBackend, Job};
use crate::domain::Domain;
use crate::hash::{Canonical, Digest};
use crate::lesson::{LessonId, LessonSpec, SourceRef};
use crate::mastery::{check_partitions, Cohort, CohortResult, CorpusIdentity, ExamReport};
use crate::store::KnowledgeStore;
use crate::teacher::Verdict;
use sml_core::tutor_rust_r1::{examine, induce, DEV, TRAIN};
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;

/// The controller must verify the external teacher's source lineage. The
/// learner separately verifies every canonical TRAIN label with its oracle.
#[derive(Clone)]
pub struct R1Input {
    pub teacher_train: String,
    pub provenance: Digest,
}

pub fn rust_r1_key(id: &LessonId) -> Digest {
    let mut c = Canonical::new("SML_TUTOR_RUST_R1_KEY_V1");
    c.digest(id);
    c.finish()
}

fn corpus() -> Result<CorpusIdentity> {
    // All nine tasks share each pair's partition; a pair is the leakage unit.
    let mut parts = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for a in 0u8..=255 {
        for b in 0u8..=255 {
            let i = usize::from(a) * 256 + usize::from(b);
            let part = if TRAIN.contains(&(a, b)) {
                0
            } else if DEV.contains(&(a, b)) {
                1
            } else {
                2 + i % 2
            };
            let mut c = Canonical::new("SML_TUTOR_RUST_R1_PAIR_V1");
            c.bytes(&[a, b]);
            parts[part].push(c.finish());
        }
    }
    let [train, dev, test, testtest] = parts;
    check_partitions(&train, &dev, &test, &testtest).map_err(|e| e.to_string())
}

pub(super) struct R1Backend {
    input: R1Input,
    id: LessonId,
    corpus: CorpusIdentity,
}

impl R1Backend {
    pub(super) fn new(input: R1Input) -> Result<(LessonSpec, Self)> {
        if input.provenance == Digest::ZERO
            || input.teacher_train.is_empty()
            || input.teacher_train.len() > 4096
        {
            return Err("invalid R1 provenance or TRAIN length".into());
        }
        let corpus = corpus()?;
        let mut payload = Canonical::new("SML_TUTOR_RUST_R1_INPUT_V1");
        payload.str(&input.teacher_train).digest(&corpus.digest());
        let spec = LessonSpec {
            domain: Domain::Rust,
            level: "R1".into(),
            skill: "closed-grammar-u8-nine-programs".into(),
            prerequisites: Vec::new(),
            source: SourceRef {
                name: "R1 canonical TRAIN labels with independent oracle".into(),
                version: "R1-U8-V1".into(),
                license: "PolyForm-Noncommercial-1.0.0".into(),
                authors: "Memorithm".into(),
                sha256: input.provenance,
            },
            payload: payload.finish(),
            difficulty: 100,
            scientific_value: 100,
            expected_gain: 100,
            user_priority: 0,
        };
        let id = spec.id();
        Ok((spec, Self { input, id, corpus }))
    }
}

/// Read the actual shared checkpoint and compile/examine it without training.
/// Returns the number of compiled and restored predictions, separately.
pub fn reexamine_r1(store: &KnowledgeStore, input: R1Input) -> Result<(u64, u64)> {
    let (spec, _) = R1Backend::new(input)?;
    let key = rust_r1_key(&spec.id());
    let snapshot = store.snapshot().map_err(|e| e.to_string())?;
    let record = snapshot.entries.get(&key).ok_or("missing R1 record")?;
    if record.lesson != spec.id() {
        return Err("R1 lesson identity mismatch".into());
    }
    let raw = store
        .get(&key)
        .map_err(|e| e.to_string())?
        .ok_or("missing R1 checkpoint")?;
    let report = examine(std::str::from_utf8(&raw).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if store.snapshot().map_err(|e| e.to_string())? != snapshot {
        return Err("store changed during R1 retention".into());
    }
    Ok((report.total.iter().sum(), report.reload))
}

impl EngineBackend for R1Backend {
    fn key(&self, id: &LessonId) -> Digest {
        rust_r1_key(id)
    }

    fn teacher_name(&self, _: &LessonId) -> &str {
        "r1-canonical-train"
    }

    fn model_version(&self, _: &LessonId) -> &str {
        "sml-core-r1-u8-v1"
    }

    fn attempt(&self, job: &Job) -> AttemptOutput {
        let mut out = AttemptOutput {
            report: ExamReport {
                lesson: job.id,
                corpus: self.corpus,
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
        if job.id != self.id || !job.prereqs.is_empty() {
            out.detail = "unexpected R1 identity or dependency".into();
            return out;
        }
        let start = Instant::now();
        let bank = match induce(Some(&self.input.teacher_train)) {
            Ok(bank) => bank,
            Err(e) => {
                out.detail = e.to_string();
                out.verdicts.push(Verdict::Reject(out.detail.clone()));
                return out;
            }
        };
        out.learn_nanos = start.elapsed().as_nanos();
        out.verdicts = vec![Verdict::Accept; 72];
        let start = Instant::now();
        match examine(&bank) {
            Ok(exam) => {
                for (i, cohort) in [
                    Cohort::TrainRecall,
                    Cohort::Dev,
                    Cohort::Test,
                    Cohort::TestTest,
                ]
                .into_iter()
                .enumerate()
                {
                    out.report.cohorts.push(CohortResult {
                        cohort,
                        correct: exam.correct[i],
                        total: exam.total[i],
                        abstained: 0,
                    });
                }
                out.report.cohorts.push(CohortResult {
                    cohort: Cohort::Reload,
                    correct: exam.reload,
                    total: exam.total.iter().sum(),
                    abstained: 0,
                });
                out.checkpoint = Some(bank.into_bytes());
            }
            Err(e) => out.detail = e.to_string(),
        }
        out.validate_nanos = start.elapsed().as_nanos();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::KnowledgeState;
    use crate::engine::{math_curriculum, reexamine_math, Engine, EngineConfig};
    use crate::executor::CancellationToken;

    #[test]
    fn rust_bank_coexists_with_math_and_reexamines_without_learning() {
        let root = std::env::temp_dir().join(format!("sml-tutor-r1-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let target = KnowledgeStore::open(root.join("shared")).unwrap();
        let cfg = EngineConfig {
            workers: 1,
            wave: 1,
            max_attempts: 6,
            ..EngineConfig::default()
        };
        let cancel = CancellationToken::new();
        let (mut math, _) =
            Engine::open(&root.join("math"), math_curriculum(4, 7, 2, 11), cfg).unwrap();
        math.run(&cancel).unwrap();
        assert_eq!(math.publish_certified_cycle(&target).unwrap().added, 4);
        let input = R1Input {
            teacher_train: sml_core::tutor_rust_r1::oracle_fixture(),
            provenance: Digest::of(b"test-only-oracle-fixture"),
        };
        let before = target.snapshot().unwrap();
        let (mut rust, _) = Engine::open_rust_r1(&root.join("rust"), input.clone(), cfg).unwrap();
        let summary = rust.run(&cancel).unwrap();
        assert_eq!(summary.states.get(&KnowledgeState::Consolidated), Some(&1));
        assert_eq!(rust.publish_certified_cycle(&target).unwrap().added, 1);
        assert_eq!(rust.publish_certified_cycle(&target).unwrap().idempotent, 1);
        let after = target.snapshot().unwrap();
        assert_eq!(after.entries.len(), 5);
        for (key, record) in before.entries {
            assert_eq!(after.entries.get(&key), Some(&record));
        }
        assert_eq!(
            reexamine_r1(&target, input.clone()).unwrap(),
            (589_824, 589_824)
        );
        assert_eq!(
            reexamine_math(&target, &math_curriculum(4, 7, 2, 11)).unwrap(),
            (4, 512)
        );
        let bad = R1Input {
            teacher_train: input.teacher_train.replacen("0|0|0|0", "0|0|0|1", 1),
            ..input
        };
        let (mut bad, _) = Engine::open_rust_r1(&root.join("bad"), bad, cfg).unwrap();
        bad.run(&cancel).unwrap();
        assert!(bad.publish_certified_cycle(&target).is_err());
        assert_eq!(target.snapshot().unwrap(), after);
        let _ = std::fs::remove_dir_all(root);
    }
}

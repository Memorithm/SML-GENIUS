//! Structured lesson representation with stable, content-addressed identity.

use crate::domain::{Domain, KnowledgeState};
use crate::hash::{Canonical, Digest};
use crate::mastery::ExamReport;

/// Stable lesson identity: SHA-256 of the canonical [`LessonSpec`].
pub type LessonId = Digest;

/// Provenance of the material a lesson is built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRef {
    pub name: String,
    pub version: String,
    pub license: String,
    pub authors: String,
    pub sha256: Digest,
}

/// Immutable part of a lesson. Two specs with identical fields have the same
/// [`LessonId`], which is what makes registration idempotent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LessonSpec {
    pub domain: Domain,
    /// Curriculum level such as `L3`, `M2`, `R1`.
    pub level: String,
    pub skill: String,
    pub prerequisites: Vec<LessonId>,
    pub source: SourceRef,
    /// Digest of the lesson payload (the training material description).
    pub payload: Digest,
    /// 0 (trivial) ..= 1000 (hardest known).
    pub difficulty: u16,
    /// Scientific value of the acquisition, 0..=1000.
    pub scientific_value: u16,
    /// Expected capability gain (e.g. newly reachable functions), 0..=1000.
    pub expected_gain: u16,
    /// User-defined priority, -1000..=1000.
    pub user_priority: i16,
}

impl LessonSpec {
    pub fn id(&self) -> LessonId {
        let mut c = Canonical::new("SML_TUTOR_LESSON_SPEC_V1");
        c.str(self.domain.name())
            .str(&self.level)
            .str(&self.skill)
            .u64(self.prerequisites.len() as u64);
        let mut prereqs = self.prerequisites.clone();
        prereqs.sort();
        for p in &prereqs {
            c.digest(p);
        }
        c.str(&self.source.name)
            .str(&self.source.version)
            .str(&self.source.license)
            .str(&self.source.authors)
            .digest(&self.source.sha256)
            .digest(&self.payload)
            .u64(u64::from(self.difficulty))
            .u64(u64::from(self.scientific_value))
            .u64(u64::from(self.expected_gain))
            .u64(self.user_priority as i64 as u64);
        c.finish()
    }
}

/// One recorded learner or teacher error. Errors are never discarded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorRecord {
    pub attempt: u32,
    pub stage: ErrorStage,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorStage {
    Teacher,
    Learner,
    Exam,
    Consolidation,
}

/// Mutable lesson record held by the registry.
#[derive(Clone, Debug)]
pub struct Lesson {
    pub id: LessonId,
    pub spec: LessonSpec,
    pub state: KnowledgeState,
    pub teacher: Option<String>,
    pub train_digest: Option<Digest>,
    pub test_corpus: Option<Digest>,
    pub model_version: Option<String>,
    pub exams: Vec<ExamReport>,
    pub checkpoint: Option<Digest>,
    pub errors: Vec<ErrorRecord>,
    pub attempts: u32,
    /// Measured cost of the last attempt in nanoseconds (0 if never run).
    pub measured_cost_nanos: u128,
}

impl Lesson {
    pub fn new(spec: LessonSpec) -> Self {
        Self {
            id: spec.id(),
            spec,
            state: KnowledgeState::Unknown,
            teacher: None,
            train_digest: None,
            test_corpus: None,
            model_version: None,
            exams: Vec::new(),
            checkpoint: None,
            errors: Vec::new(),
            attempts: 0,
            measured_cost_nanos: 0,
        }
    }

    pub fn failed_exams(&self) -> usize {
        self.exams.iter().filter(|e| !e.passed()).count()
    }
}

#[cfg(test)]
pub(crate) fn test_spec(domain: Domain, skill: &str, prerequisites: Vec<LessonId>) -> LessonSpec {
    LessonSpec {
        domain,
        level: "T0".into(),
        skill: skill.into(),
        prerequisites,
        source: SourceRef {
            name: "unit-test".into(),
            version: "1".into(),
            license: "PolyForm-Noncommercial-1.0.0".into(),
            authors: "test".into(),
            sha256: Digest::of(b"unit-test"),
        },
        payload: Digest::of(skill.as_bytes()),
        difficulty: 100,
        scientific_value: 100,
        expected_gain: 100,
        user_priority: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_stable_and_prereq_order_independent() {
        let a = Digest::of(b"a");
        let b = Digest::of(b"b");
        let s1 = test_spec(Domain::Mathematics, "x", vec![a, b]);
        let s2 = test_spec(Domain::Mathematics, "x", vec![b, a]);
        assert_eq!(s1.id(), s2.id());
        let s3 = test_spec(Domain::Rust, "x", vec![a, b]);
        assert_ne!(s1.id(), s3.id());
    }
}

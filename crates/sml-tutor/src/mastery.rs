//! Mastery examiner: the only code allowed to mint a [`Certificate`].
//!
//! The admission rule is fixed and has no configurable threshold:
//!
//! * every cohort must have `total > 0` and `correct == total`;
//! * an abstention is a failure (it is counted in `total` but not in `correct`);
//! * the cohorts `TrainRecall`, `Test` and `Reload` are mandatory;
//! * every retained prior skill that the caller asks to re-examine must be
//!   present as a `Retention` cohort and pass at 100 %;
//! * TRAIN, DEV, TEST and TESTTEST case identities must be pairwise disjoint.
//!
//! Pass rate and coverage are reported separately: certifying a cohort says
//! nothing about the part of the domain that was never examined.

use std::collections::BTreeSet;

use crate::hash::{Canonical, Digest};
use crate::lesson::LessonId;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cohort {
    TrainRecall,
    Dev,
    Test,
    TestTest,
    Reload,
    /// Re-examination of an earlier certified skill (by its knowledge key).
    Retention(Digest),
}

impl Cohort {
    pub fn label(&self) -> String {
        match self {
            Cohort::TrainRecall => "train_recall".into(),
            Cohort::Dev => "dev".into(),
            Cohort::Test => "test".into(),
            Cohort::TestTest => "testtest".into(),
            Cohort::Reload => "reload".into(),
            Cohort::Retention(k) => format!("retention:{}", k.short()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CohortResult {
    pub cohort: Cohort,
    pub correct: u64,
    pub abstained: u64,
    pub total: u64,
}

impl CohortResult {
    pub fn passed(&self) -> bool {
        self.total > 0 && self.correct == self.total && self.abstained == 0
    }

    pub fn failed(&self) -> u64 {
        self.total.saturating_sub(self.correct)
    }
}

/// Identity of the four evaluation partitions, as digests of their case sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CorpusIdentity {
    pub train: Digest,
    pub dev: Digest,
    pub test: Digest,
    pub testtest: Digest,
}

impl CorpusIdentity {
    pub fn digest(&self) -> Digest {
        let mut c = Canonical::new("SML_TUTOR_CORPUS_IDENTITY_V1");
        c.digest(&self.train)
            .digest(&self.dev)
            .digest(&self.test)
            .digest(&self.testtest);
        c.finish()
    }
}

/// Digest of a case set, independent of order.
pub fn case_set_digest(cases: &[Digest]) -> Digest {
    let set: BTreeSet<&Digest> = cases.iter().collect();
    let mut c = Canonical::new("SML_TUTOR_CASE_SET_V1");
    c.u64(set.len() as u64);
    for d in set {
        c.digest(d);
    }
    c.finish()
}

/// Verifies that the four partitions share no case. Returns the corpus identity.
pub fn check_partitions(
    train: &[Digest],
    dev: &[Digest],
    test: &[Digest],
    testtest: &[Digest],
) -> Result<CorpusIdentity, MasteryFailure> {
    let parts = [train, dev, test, testtest];
    let mut seen = BTreeSet::new();
    for part in parts {
        let mut local = BTreeSet::new();
        for case in part {
            if !local.insert(*case) {
                continue;
            }
            if !seen.insert(*case) {
                return Err(MasteryFailure::PartitionLeak(*case));
            }
        }
    }
    Ok(CorpusIdentity {
        train: case_set_digest(train),
        dev: case_set_digest(dev),
        test: case_set_digest(test),
        testtest: case_set_digest(testtest),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExamReport {
    pub lesson: LessonId,
    pub corpus: CorpusIdentity,
    pub cohorts: Vec<CohortResult>,
}

impl ExamReport {
    pub fn passed(&self) -> bool {
        !self.cohorts.is_empty() && self.cohorts.iter().all(CohortResult::passed)
    }

    pub fn failed_cases(&self) -> u64 {
        self.cohorts.iter().map(CohortResult::failed).sum()
    }

    pub fn digest(&self) -> Digest {
        let mut c = Canonical::new("SML_TUTOR_EXAM_REPORT_V1");
        c.digest(&self.lesson).digest(&self.corpus.digest());
        let mut cohorts: Vec<&CohortResult> = self.cohorts.iter().collect();
        cohorts.sort_by(|a, b| a.cohort.cmp(&b.cohort));
        c.u64(cohorts.len() as u64);
        for r in cohorts {
            c.str(&r.cohort.label())
                .u64(r.correct)
                .u64(r.abstained)
                .u64(r.total);
        }
        c.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MasteryFailure {
    MissingCohort(String),
    DuplicateCohort(String),
    EmptyCohort(String),
    Imperfect {
        cohort: String,
        correct: u64,
        abstained: u64,
        total: u64,
    },
    PartitionLeak(Digest),
    WrongLesson,
}

impl core::fmt::Display for MasteryFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MasteryFailure::MissingCohort(c) => write!(f, "missing mandatory cohort {c}"),
            MasteryFailure::DuplicateCohort(c) => write!(f, "duplicate cohort {c}"),
            MasteryFailure::EmptyCohort(c) => write!(f, "empty cohort {c}"),
            MasteryFailure::Imperfect {
                cohort,
                correct,
                abstained,
                total,
            } => write!(
                f,
                "cohort {cohort} scored {correct}/{total} with {abstained} abstentions (100% required)"
            ),
            MasteryFailure::PartitionLeak(d) => {
                write!(f, "case {} appears in two partitions", d.short())
            }
            MasteryFailure::WrongLesson => f.write_str("exam report belongs to another lesson"),
        }
    }
}

impl std::error::Error for MasteryFailure {}

/// Proof that a lesson passed its exam. Fields are private and the only
/// constructor is [`certify`], so neither the scheduler nor a teacher can
/// forge one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    lesson: LessonId,
    report: Digest,
    corpus: Digest,
    examined_cases: u64,
}

impl Certificate {
    pub fn lesson(&self) -> LessonId {
        self.lesson
    }
    pub fn report_digest(&self) -> Digest {
        self.report
    }
    pub fn corpus_digest(&self) -> Digest {
        self.corpus
    }
    pub fn examined_cases(&self) -> u64 {
        self.examined_cases
    }
    pub fn digest(&self) -> Digest {
        let mut c = Canonical::new("SML_TUTOR_CERTIFICATE_V1");
        c.digest(&self.lesson)
            .digest(&self.report)
            .digest(&self.corpus)
            .u64(self.examined_cases);
        c.finish()
    }
}

/// Applies the fixed 100 % admission rule.
pub fn certify(
    lesson: LessonId,
    report: &ExamReport,
    retained_skills: &[Digest],
) -> Result<Certificate, MasteryFailure> {
    if report.lesson != lesson {
        return Err(MasteryFailure::WrongLesson);
    }
    let mut labels = BTreeSet::new();
    for r in &report.cohorts {
        if !labels.insert(r.cohort.clone()) {
            return Err(MasteryFailure::DuplicateCohort(r.cohort.label()));
        }
    }
    let mut required = vec![Cohort::TrainRecall, Cohort::Test, Cohort::Reload];
    required.extend(retained_skills.iter().map(|k| Cohort::Retention(*k)));
    for c in &required {
        if !labels.contains(c) {
            return Err(MasteryFailure::MissingCohort(c.label()));
        }
    }
    for r in &report.cohorts {
        if r.total == 0 {
            return Err(MasteryFailure::EmptyCohort(r.cohort.label()));
        }
        if !r.passed() {
            return Err(MasteryFailure::Imperfect {
                cohort: r.cohort.label(),
                correct: r.correct,
                abstained: r.abstained,
                total: r.total,
            });
        }
    }
    Ok(Certificate {
        lesson,
        report: report.digest(),
        corpus: report.corpus.digest(),
        examined_cases: report.cohorts.iter().map(|r| r.total).sum(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(i: u64) -> Digest {
        Digest::of(&i.to_le_bytes())
    }

    fn corpus() -> CorpusIdentity {
        check_partitions(&[d(1)], &[d(2)], &[d(3)], &[d(4)]).unwrap()
    }

    fn ok(cohort: Cohort, n: u64) -> CohortResult {
        CohortResult {
            cohort,
            correct: n,
            abstained: 0,
            total: n,
        }
    }

    fn report(lesson: LessonId, cohorts: Vec<CohortResult>) -> ExamReport {
        ExamReport {
            lesson,
            corpus: corpus(),
            cohorts,
        }
    }

    #[test]
    fn perfect_report_is_certified() {
        let l = d(9);
        let r = report(
            l,
            vec![
                ok(Cohort::TrainRecall, 10),
                ok(Cohort::Test, 5),
                ok(Cohort::Reload, 5),
            ],
        );
        let c = certify(l, &r, &[]).unwrap();
        assert_eq!(c.examined_cases(), 20);
        assert_eq!(c.lesson(), l);
    }

    #[test]
    fn one_error_in_a_million_is_rejected_no_rounding() {
        let l = d(9);
        let mut test = ok(Cohort::Test, 1_000_000);
        test.correct -= 1;
        let r = report(
            l,
            vec![ok(Cohort::TrainRecall, 10), test, ok(Cohort::Reload, 5)],
        );
        assert!(matches!(
            certify(l, &r, &[]),
            Err(MasteryFailure::Imperfect {
                correct: 999_999,
                ..
            })
        ));
    }

    #[test]
    fn abstention_is_not_a_correct_answer() {
        let l = d(9);
        let test = CohortResult {
            cohort: Cohort::Test,
            correct: 4,
            abstained: 1,
            total: 5,
        };
        let r = report(
            l,
            vec![ok(Cohort::TrainRecall, 10), test, ok(Cohort::Reload, 5)],
        );
        assert!(certify(l, &r, &[]).is_err());
    }

    #[test]
    fn missing_reload_or_retention_is_rejected() {
        let l = d(9);
        let r = report(l, vec![ok(Cohort::TrainRecall, 10), ok(Cohort::Test, 5)]);
        assert_eq!(
            certify(l, &r, &[]),
            Err(MasteryFailure::MissingCohort("reload".into()))
        );
        let prior = d(77);
        let r = report(
            l,
            vec![
                ok(Cohort::TrainRecall, 10),
                ok(Cohort::Test, 5),
                ok(Cohort::Reload, 5),
            ],
        );
        assert!(matches!(
            certify(l, &r, &[prior]),
            Err(MasteryFailure::MissingCohort(_))
        ));
    }

    #[test]
    fn empty_cohort_cannot_pass_vacuously() {
        let l = d(9);
        let r = report(
            l,
            vec![
                ok(Cohort::TrainRecall, 10),
                ok(Cohort::Test, 0),
                ok(Cohort::Reload, 5),
            ],
        );
        assert!(matches!(
            certify(l, &r, &[]),
            Err(MasteryFailure::EmptyCohort(_))
        ));
    }

    #[test]
    fn partition_leak_is_detected() {
        assert_eq!(
            check_partitions(&[d(1), d(2)], &[], &[d(2)], &[]),
            Err(MasteryFailure::PartitionLeak(d(2)))
        );
    }

    #[test]
    fn duplicate_cohort_and_wrong_lesson_rejected() {
        let l = d(9);
        let r = report(
            l,
            vec![
                ok(Cohort::TrainRecall, 10),
                ok(Cohort::Test, 5),
                ok(Cohort::Test, 5),
                ok(Cohort::Reload, 5),
            ],
        );
        assert!(matches!(
            certify(l, &r, &[]),
            Err(MasteryFailure::DuplicateCohort(_))
        ));
        assert_eq!(certify(d(8), &r, &[]), Err(MasteryFailure::WrongLesson));
    }
}

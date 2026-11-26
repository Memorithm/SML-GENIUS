//! Native L1 bridge. A frozen annotation corpus is the teacher, the separate
//! original labeled fixture is the admission oracle, and the existing native
//! L1 executable is the learner. A second evaluator compares actual answers
//! with labeled cases. The engine's fixed mastery gate remains authoritative.
//! This is re-admission of earlier competencies, not new CUDA teaching.
use super::{AttemptOutput, EngineBackend, Job};
use crate::domain::Domain;
use crate::hash::{Canonical, Digest};
use crate::lesson::{LessonId, LessonSpec, SourceRef};
use crate::mastery::{check_partitions, Cohort, CohortResult, CorpusIdentity, ExamReport};
use crate::teacher::{
    admit_batch, LessonRequest, Oracle, Proposal, Residency, Teacher, TeacherKind, TeacherLedger,
    Verdict,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;
const MAX_TEXT: usize = 65_536;
const DOMAINS: [Domain; 2] = [Domain::French, Domain::English];
const TEACHER: &str = "frozen-l1-annotation-corpus";

/// Trusted inputs, bound into the lesson identity. The physical controller
/// must verify source provenance before supplying them.
#[derive(Clone)]
pub struct L1Input {
    pub domain: Domain,
    pub lexicon: String,
    pub proposed_train: String,
    pub oracle_train: String,
    pub held_out: String,
    pub parent_bank: Vec<u8>,
    pub provenance: Digest,
}

#[derive(Clone, Debug)]
struct Row {
    sentence: String,
    label: String,
    triple: [u8; 3],
}

fn rows(raw: &str) -> Result<Vec<Row>> {
    if raw.is_empty() || raw.len() > MAX_TEXT || !raw.ends_with('\n') {
        return Err("invalid L1 corpus length".into());
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for line in raw.lines() {
        let (sentence, label) = line.split_once('|').ok_or("missing label")?;
        let fields: Vec<_> = label.split('|').collect();
        let [s, v, o, neg] = fields.as_slice() else {
            return Err("invalid L1 label".into());
        };
        let idx = |x: &str| -> Result<u8> {
            let n = x.parse::<u8>().map_err(|_| "invalid role coordinate")?;
            if n >= 16 || n.to_string() != x {
                return Err("invalid role coordinate".into());
            }
            Ok(n)
        };
        if !matches!(*neg, "0" | "1")
            || sentence.is_empty()
            || sentence.len() > 512
            || sentence.chars().any(char::is_control)
            || !seen.insert(sentence)
        {
            return Err("invalid or duplicate L1 sentence".into());
        }
        out.push(Row {
            sentence: sentence.to_owned(),
            label: label.to_owned(),
            triple: [idx(s)?, idx(v)?, idx(o)?],
        });
    }
    if out.len() > 1024 {
        return Err("oversized L1 row count".into());
    }
    Ok(out)
}

fn row_text(rows: &[Row]) -> String {
    rows.iter()
        .map(|r| format!("{}|{}\n", r.sentence, r.label))
        .collect()
}

fn keys(domain: Domain, rows: &[Row]) -> Vec<Digest> {
    let mut set = BTreeSet::new();
    for row in rows {
        let mut c = Canonical::new("SML_TUTOR_L1_TRIPLE_CASE_V1");
        c.str(domain.name()).bytes(&row.triple);
        set.insert(c.finish());
    }
    set.into_iter().collect()
}

struct Prepared {
    input: L1Input,
    proposed: Vec<Row>,
    oracle: BTreeMap<String, String>,
    dev: Vec<Row>,
    test: Vec<Row>,
    testtest: Vec<Row>,
    corpus: CorpusIdentity,
    parent_digest: Digest,
}

impl Prepared {
    fn new(input: L1Input) -> Result<Self> {
        if !DOMAINS.contains(&input.domain)
            || input.provenance == Digest::ZERO
            || input.lexicon.is_empty()
            || input.lexicon.len() > MAX_TEXT
            || input.parent_bank.is_empty()
            || input.parent_bank.len() > MAX_TEXT
        {
            return Err("invalid L1 input".into());
        }
        let proposed = rows(&input.proposed_train)?;
        let reference = rows(&input.oracle_train)?;
        let held = rows(&input.held_out)?;
        if proposed.len() != 48 || reference.len() != 192 || held.len() != 896 {
            return Err("unexpected L1 cohort cardinality".into());
        }
        let reference_sentences: BTreeSet<_> = reference.iter().map(|r| &r.sentence).collect();
        let reference_triples: BTreeSet<_> = reference.iter().map(|r| r.triple).collect();
        if held.iter().any(|r| {
            reference_sentences.contains(&r.sentence) || reference_triples.contains(&r.triple)
        }) || held
            .iter()
            .any(|r| proposed.iter().any(|p| p.sentence == r.sentence))
        {
            return Err("L1 oracle/held-out leakage".into());
        }
        // Keep all paraphrases of the same role triple in a single partition.
        let triples: BTreeSet<_> = held.iter().map(|r| r.triple).collect();
        let split: BTreeMap<_, _> = triples
            .into_iter()
            .enumerate()
            .map(|(i, t)| (t, i % 3))
            .collect();
        let mut partitions = [Vec::new(), Vec::new(), Vec::new()];
        for row in held {
            partitions[split[&row.triple]].push(row);
        }
        let [dev, test, testtest] = partitions;
        if [&dev, &test, &testtest].iter().any(|p| p.is_empty()) {
            return Err("empty L1 evaluation partition".into());
        }
        let corpus = check_partitions(
            &keys(input.domain, &proposed),
            &keys(input.domain, &dev),
            &keys(input.domain, &test),
            &keys(input.domain, &testtest),
        )
        .map_err(|e| e.to_string())?;
        let oracle = reference
            .into_iter()
            .map(|r| (r.sentence, r.label))
            .collect();
        let parent_digest = Digest::of(&input.parent_bank);
        Ok(Self {
            input,
            proposed,
            oracle,
            dev,
            test,
            testtest,
            corpus,
            parent_digest,
        })
    }

    fn spec(&self, binary_digest: Digest) -> LessonSpec {
        let input = &self.input;
        let mut payload = Canonical::new("SML_TUTOR_L1_BRIDGE_INPUT_V1");
        payload
            .digest(&binary_digest)
            .str(&input.lexicon)
            .str(&input.proposed_train)
            .str(&input.oracle_train)
            .str(&input.held_out)
            .digest(&self.parent_digest);
        LessonSpec {
            domain: input.domain,
            level: "L1".into(),
            skill: "native-role-polarity-readmission".into(),
            prerequisites: Vec::new(),
            source: SourceRef {
                name: "Memorithm L1 frozen annotations and controlled oracle".into(),
                version: "L1-physical-admission-v1".into(),
                license: "PolyForm-Noncommercial-1.0.0".into(),
                authors: "Memorithm".into(),
                sha256: input.provenance,
            },
            payload: payload.finish(),
            difficulty: 100,
            scientific_value: 100,
            expected_gain: 0,
            user_priority: 0,
        }
    }
}

struct CorpusTeacher<'a> {
    prepared: &'a Prepared,
}

impl Teacher for CorpusTeacher<'_> {
    fn name(&self) -> &str {
        TEACHER
    }

    fn kind(&self) -> TeacherKind {
        TeacherKind::Corpus
    }

    fn residency(&self) -> Residency {
        Residency::NonResident
    }

    fn domains(&self) -> &[Domain] {
        &DOMAINS
    }

    fn propose_batch(&self, requests: &[LessonRequest]) -> Vec<Proposal> {
        let mut out = Vec::new();
        for r in requests {
            if r.domain != self.prepared.input.domain {
                continue;
            }
            for row in self.prepared.proposed.iter().take(r.count as usize) {
                out.push(Proposal {
                    lesson: r.lesson,
                    teacher: TEACHER.into(),
                    item_key: Digest::of(row.sentence.as_bytes()),
                    payload: row.sentence.as_bytes().to_vec(),
                    claimed_answer: row.label.as_bytes().to_vec(),
                });
            }
        }
        out
    }
}

struct CorpusOracle<'a> {
    id: LessonId,
    expected: &'a BTreeMap<String, String>,
}

impl Oracle for CorpusOracle<'_> {
    fn name(&self) -> &str {
        "independent-frozen-l1-labels"
    }

    fn verify(&self, p: &Proposal) -> Verdict {
        if p.lesson == self.id
            && p.item_key == Digest::of(&p.payload)
            && std::str::from_utf8(&p.payload)
                .ok()
                .and_then(|s| self.expected.get(s))
                .is_some_and(|label| label.as_bytes() == p.claimed_answer)
        {
            Verdict::Accept
        } else {
            Verdict::Reject("L1 label differs from independent oracle".into())
        }
    }
}

struct Temp(PathBuf);
static NEXT: AtomicU64 = AtomicU64::new(0);

impl Temp {
    fn new() -> Result<Self> {
        let p = std::env::temp_dir().join(format!(
            "sml-tutor-l1-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).map_err(|e| e.to_string())?;
        Ok(Self(p))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn binary_digest(path: &Path) -> Result<Digest> {
    let m = fs::metadata(path).map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() == 0 || m.len() > 64 * 1024 * 1024 {
        return Err("invalid native learner".into());
    }
    let raw = fs::read(path).map_err(|e| e.to_string())?;
    Ok(Digest::of(&raw))
}

pub fn linguistic_l1_key(id: &LessonId) -> Digest {
    let mut c = Canonical::new("SML_TUTOR_L1_KNOWLEDGE_KEY_V1");
    c.digest(id);
    c.finish()
}

pub(super) struct L1Backend {
    binary: PathBuf,
    binary_digest: Digest,
    lessons: BTreeMap<LessonId, Prepared>,
}

impl L1Backend {
    pub(super) fn new(binary: &Path, inputs: [L1Input; 2]) -> Result<(Vec<LessonSpec>, Self)> {
        let binary = binary.canonicalize().map_err(|e| e.to_string())?;
        let digest = binary_digest(&binary)?;
        let mut specs = Vec::new();
        let mut lessons = BTreeMap::new();
        let mut domains = BTreeSet::new();
        for input in inputs {
            if !domains.insert(input.domain) {
                return Err("duplicate language".into());
            }
            let p = Prepared::new(input)?;
            let spec = p.spec(digest);
            lessons.insert(spec.id(), p);
            specs.push(spec);
        }
        Ok((
            specs,
            Self {
                binary,
                binary_digest: digest,
                lessons,
            },
        ))
    }

    fn command(&self, dir: &Path, args: &[&OsStr]) -> Result<String> {
        // Only the pinned native SML executable is permitted by the trusted
        // physical controller. No generated program or teacher command runs.
        let output = Command::new(&self.binary)
            .args(args)
            .current_dir(dir)
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() || output.stdout.len() > MAX_TEXT {
            return Err("native L1 command failed".into());
        }
        String::from_utf8(output.stdout).map_err(|e| e.to_string())
    }

    fn assess(
        &self,
        dir: &Path,
        bank: &Path,
        rows: &[Row],
        cohort: Cohort,
    ) -> Result<CohortResult> {
        let mut correct = 0;
        let mut abstained = 0;
        for row in rows {
            let actual = self.command(
                dir,
                &[
                    OsStr::new("--predict"),
                    bank.as_os_str(),
                    OsStr::new(&row.sentence),
                ],
            )?;
            correct += u64::from(actual == format!("{}\n", row.label));
            abstained += u64::from(actual == "ABSTAIN\n");
        }
        Ok(CohortResult {
            cohort,
            correct,
            abstained,
            total: rows.len() as u64,
        })
    }

    fn replay(
        &self,
        dir: &Path,
        bank: &Path,
        train: &Path,
        held: &Path,
        cohort: Cohort,
    ) -> Result<CohortResult> {
        let actual = self.command(
            dir,
            &[
                OsStr::new("--verify"),
                bank.as_os_str(),
                train.as_os_str(),
                held.as_os_str(),
            ],
        )?;
        let correct = if actual == "SML_TUTOR_LINGUISTIC_RELOAD=PASS test=896 errors=0\n" {
            896
        } else {
            0
        };
        Ok(CohortResult {
            cohort,
            correct,
            abstained: 0,
            total: 896,
        })
    }

    fn learn(&self, job: &Job, p: &Prepared, out: &mut AttemptOutput) -> Result<()> {
        if !job.prereqs.is_empty() || binary_digest(&self.binary)? != self.binary_digest {
            return Err("unexpected dependency or changed native binary".into());
        }
        let t = Instant::now();
        let teacher = CorpusTeacher { prepared: p };
        let oracle = CorpusOracle {
            id: job.id,
            expected: &p.oracle,
        };
        let proposals = teacher.propose_batch(&[LessonRequest {
            lesson: job.id,
            domain: p.input.domain,
            skill: "L1".into(),
            count: 48,
        }]);
        let mut ledger = TeacherLedger::default();
        let mut seen = BTreeSet::new();
        let admitted = admit_batch(&oracle, &mut ledger, &mut seen, proposals);
        let rejected = ledger.stats(TEACHER).map_or(0, |s| s.rejected);
        out.verdicts = vec![Verdict::Accept; admitted.len()];
        out.verdicts.extend(
            (0..rejected).map(|_| Verdict::Reject("independent L1 oracle rejected label".into())),
        );
        out.teach_nanos = t.elapsed().as_nanos();
        if admitted.len() != 48 || rejected != 0 {
            return Err("incomplete teacher admission".into());
        }
        let mut admitted_text = String::new();
        for proposal in &admitted {
            let sentence = std::str::from_utf8(&proposal.payload).map_err(|e| e.to_string())?;
            let label = std::str::from_utf8(&proposal.claimed_answer).map_err(|e| e.to_string())?;
            admitted_text.push_str(&format!("{sentence}|{label}\n"));
        }
        let stage = Temp::new()?;
        let lex = stage.0.join("lexicon.txt");
        let train = stage.0.join("train.txt");
        let dev = stage.0.join("dev.txt");
        let held = stage.0.join("held.txt");
        let parent = stage.0.join("parent.bank");
        let bank = stage.0.join("new.bank");
        fs::write(&lex, &p.input.lexicon).map_err(|e| e.to_string())?;
        fs::write(&train, admitted_text).map_err(|e| e.to_string())?;
        fs::write(&dev, row_text(&p.dev)).map_err(|e| e.to_string())?;
        fs::write(&held, &p.input.held_out).map_err(|e| e.to_string())?;
        fs::write(&parent, &p.input.parent_bank).map_err(|e| e.to_string())?;
        let t = Instant::now();
        let receipt = self.command(
            &stage.0,
            &[
                lex.as_os_str(),
                train.as_os_str(),
                dev.as_os_str(),
                bank.as_os_str(),
            ],
        )?;
        out.learn_nanos = t.elapsed().as_nanos();
        if receipt
            != format!(
                "SML_TUTOR_LINGUISTIC train=48 test={} templates=8 errors=0\n",
                p.dev.len()
            )
        {
            return Err("wrong learner receipt".into());
        }
        let bytes = fs::read(&bank).map_err(|e| e.to_string())?;
        if bytes != p.input.parent_bank {
            return Err("relearned native bank differs from admitted parent".into());
        }
        let t = Instant::now();
        for (cases, cohort) in [
            (&p.proposed, Cohort::TrainRecall),
            (&p.dev, Cohort::Dev),
            (&p.test, Cohort::Test),
            (&p.testtest, Cohort::TestTest),
        ] {
            out.report
                .cohorts
                .push(self.assess(&stage.0, &bank, cases, cohort)?);
        }
        out.report
            .cohorts
            .push(self.replay(&stage.0, &bank, &train, &held, Cohort::Reload)?);
        out.report.cohorts.push(self.replay(
            &stage.0,
            &parent,
            &train,
            &held,
            Cohort::Retention(p.parent_digest),
        )?);
        out.validate_nanos = t.elapsed().as_nanos();
        out.abstained = out.report.cohorts.iter().any(|c| c.abstained > 0);
        out.checkpoint = Some(bytes);
        Ok(())
    }
}

impl EngineBackend for L1Backend {
    fn key(&self, id: &LessonId) -> Digest {
        linguistic_l1_key(id)
    }

    fn teacher_name(&self) -> &str {
        TEACHER
    }

    fn model_version(&self) -> &str {
        "sml-tutor-native-l1-bridge-v1"
    }

    fn external_retention(&self, id: &LessonId) -> Vec<Digest> {
        self.lessons
            .get(id)
            .map_or_else(Vec::new, |p| vec![p.parent_digest])
    }

    fn attempt(&self, job: &Job) -> AttemptOutput {
        let prepared = self.lessons.get(&job.id);
        let mut out = AttemptOutput {
            report: ExamReport {
                lesson: job.id,
                corpus: prepared.map_or(
                    CorpusIdentity {
                        train: Digest::ZERO,
                        dev: Digest::ZERO,
                        test: Digest::ZERO,
                        testtest: Digest::ZERO,
                    },
                    |p| p.corpus,
                ),
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
        out.detail = match prepared {
            Some(p) => self.learn(job, p, &mut out).err().unwrap_or_default(),
            None => "unknown L1 lesson".into(),
        };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_rejects_mislabeled_teacher_and_wrong_lesson() {
        let id = Digest::of(b"lesson");
        let expected = BTreeMap::from([("alice sees beth".into(), "0|0|1|0".into())]);
        let oracle = CorpusOracle {
            id,
            expected: &expected,
        };
        let mut p = Proposal {
            lesson: id,
            teacher: TEACHER.into(),
            item_key: Digest::of(b"alice sees beth"),
            payload: b"alice sees beth".to_vec(),
            claimed_answer: b"0|0|1|0".to_vec(),
        };
        assert_eq!(oracle.verify(&p), Verdict::Accept);
        p.claimed_answer = b"1|0|0|0".to_vec();
        assert!(matches!(oracle.verify(&p), Verdict::Reject(_)));
        p.claimed_answer = b"0|0|1|0".to_vec();
        p.lesson = Digest::ZERO;
        assert!(matches!(oracle.verify(&p), Verdict::Reject(_)));
    }

    #[test]
    fn triple_case_identity_groups_paraphrases_but_not_languages() {
        let data = rows("alice sees beth|0|0|1|0\nbeth is seen by alice|0|0|1|0\n").unwrap();
        let en = keys(Domain::English, &data);
        assert_eq!(en.len(), 1);
        assert_ne!(en, keys(Domain::French, &data));
        assert!(rows("alice sees beth|0|0|1|0\nalice sees beth|1|0|0|0\n").is_err());
    }
}

//! Generic teacher interface and proposal admission.
//!
//! Teachers are never authorities. A [`Teacher`] only proposes material; every
//! [`Proposal`] must pass an independent [`Oracle`] before it is admitted by
//! [`admit_batch`]. The ledger records what each teacher proposed, what was
//! admitted, what was rejected and why, and what the verification cost, so
//! that the scheduler can prefer teachers whose proposals survive checking.
//!
//! This is an engine component: it counts and checks. It does not understand
//! language or mathematics, and an admitted proposal is only as good as the
//! oracle that accepted it.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

use crate::domain::Domain;
use crate::hash::{Canonical, Digest};

/// Whether a teacher's model must be loaded per batch.
///
/// A [`Residency::NonResident`] teacher is loaded for one batch and then
/// released. The engine must release GPU memory after each batch for such
/// teachers, so that two heavy teachers never share the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Residency {
    NonResident,
    Resident,
}

/// The kind of teacher, used for reporting and routing only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeacherKind {
    LanguageModel,
    MathSolver,
    ProofEngine,
    RustCompiler,
    TestBench,
    Dictionary,
    Corpus,
    KnowledgeBase,
    Ensemble,
}

/// A request for `count` items of `skill` in `domain`, for the lesson `lesson`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LessonRequest {
    pub lesson: Digest,
    pub domain: Domain,
    pub skill: String,
    pub count: u32,
}

/// Material proposed by a teacher, with the answer the teacher claims for it.
///
/// `claimed_answer` is a claim, not a fact: only the oracle decides whether it
/// is correct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub lesson: Digest,
    pub teacher: String,
    pub item_key: Digest,
    pub payload: Vec<u8>,
    pub claimed_answer: Vec<u8>,
}

impl Proposal {
    /// Content identity of the proposal, used for deduplication.
    pub fn digest(&self) -> Digest {
        let mut c = Canonical::new("SML_TUTOR_PROPOSAL_V1");
        c.digest(&self.lesson)
            .str(&self.teacher)
            .digest(&self.item_key)
            .bytes(&self.payload)
            .bytes(&self.claimed_answer);
        c.finish()
    }
}

/// A source of proposed material. Implementations must not be trusted to be
/// correct; their output is verified by an [`Oracle`].
pub trait Teacher: Send + Sync {
    fn name(&self) -> &str;
    fn kind(&self) -> TeacherKind;
    fn residency(&self) -> Residency;
    fn domains(&self) -> &[Domain];
    fn propose_batch(&self, requests: &[LessonRequest]) -> Vec<Proposal>;
}

/// The outcome of verifying one proposal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Accept,
    Reject(String),
}

/// An independent checker. It sees only the proposal and must not consult
/// the teacher that produced it.
pub trait Oracle: Send + Sync {
    fn name(&self) -> &str;
    fn verify(&self, proposal: &Proposal) -> Verdict;
}

/// Number of rejection reasons retained per teacher.
const REASON_RING: usize = 64;

/// Counters for one teacher. `proposed` counts verified proposals only;
/// duplicates are counted separately and never verified.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeacherStats {
    pub proposed: u64,
    pub admitted: u64,
    pub rejected: u64,
    pub duplicates: u64,
    pub cost_nanos: u128,
}

impl TeacherStats {
    /// Fraction of verified proposals that were admitted; 0.0 before any proposal.
    pub fn admitted_rate(&self) -> f64 {
        if self.proposed == 0 {
            0.0
        } else {
            self.admitted as f64 / self.proposed as f64
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Entry {
    stats: TeacherStats,
    reasons: VecDeque<String>,
}

/// Per-teacher accounting of verification outcomes and cost.
#[derive(Debug, Default)]
pub struct TeacherLedger {
    entries: BTreeMap<String, Entry>,
}

impl TeacherLedger {
    /// Records the verdict for one verified proposal of `teacher`.
    pub fn record(&mut self, teacher: &str, verdict: &Verdict, cost_nanos: u128) {
        let entry = self.entries.entry(teacher.to_string()).or_default();
        entry.stats.proposed += 1;
        entry.stats.cost_nanos += cost_nanos;
        match verdict {
            Verdict::Accept => entry.stats.admitted += 1,
            Verdict::Reject(reason) => {
                entry.stats.rejected += 1;
                if entry.reasons.len() == REASON_RING {
                    entry.reasons.pop_front();
                }
                entry.reasons.push_back(reason.clone());
            }
        }
    }

    /// Records that `teacher` offered a proposal already seen; it was not verified.
    pub fn record_duplicate(&mut self, teacher: &str) {
        self.entries
            .entry(teacher.to_string())
            .or_default()
            .stats
            .duplicates += 1;
    }

    /// Current counters for `teacher`, if it has ever been recorded.
    pub fn stats(&self, teacher: &str) -> Option<TeacherStats> {
        self.entries.get(teacher).map(|e| e.stats.clone())
    }

    /// The most recent rejection reasons for `teacher`, oldest first, at most 64.
    pub fn rejection_reasons(&self, teacher: &str) -> Vec<String> {
        self.entries
            .get(teacher)
            .map(|e| e.reasons.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Chooses the teacher to ask for `domain` among `candidates`.
    ///
    /// Teachers that have never proposed are preferred (optimistic exploration).
    /// Otherwise the teacher with the highest admitted-per-second rate wins.
    /// Ties are broken by name. Teachers that do not serve `domain` are never
    /// returned.
    pub fn select<'a>(
        &self,
        candidates: &'a [&'a dyn Teacher],
        domain: Domain,
    ) -> Option<&'a dyn Teacher> {
        let mut best: Option<&'a dyn Teacher> = None;
        for &t in candidates {
            if !t.domains().contains(&domain) {
                continue;
            }
            best = match best {
                None => Some(t),
                Some(b) if self.is_better(t.name(), b.name()) => Some(t),
                keep => keep,
            };
        }
        best
    }

    /// `(untried, admitted_per_second)` for a teacher name.
    fn preference(&self, teacher: &str) -> (bool, f64) {
        match self.entries.get(teacher) {
            None => (true, 0.0),
            Some(e) if e.stats.proposed == 0 => (true, 0.0),
            Some(e) => {
                let secs = e.stats.cost_nanos as f64 / 1e9;
                let rate = if secs > 0.0 {
                    e.stats.admitted as f64 / secs
                } else if e.stats.admitted > 0 {
                    f64::INFINITY
                } else {
                    0.0
                };
                (false, rate)
            }
        }
    }

    fn is_better(&self, a: &str, b: &str) -> bool {
        let (untried_a, rate_a) = self.preference(a);
        let (untried_b, rate_b) = self.preference(b);
        if untried_a != untried_b {
            return untried_a;
        }
        match rate_a.total_cmp(&rate_b) {
            Ordering::Greater => true,
            Ordering::Less => false,
            Ordering::Equal => a < b,
        }
    }
}

/// Verifies each proposal with `oracle` and returns only the admitted ones.
///
/// A proposal whose digest was already in `seen` is counted as a duplicate
/// and is neither verified nor returned. `seen` records every digest presented,
/// admitted or not, so a rejected proposal offered again is not re-verified.
pub fn admit_batch(
    oracle: &dyn Oracle,
    ledger: &mut TeacherLedger,
    seen: &mut BTreeSet<Digest>,
    proposals: Vec<Proposal>,
) -> Vec<Proposal> {
    let mut admitted = Vec::new();
    for proposal in proposals {
        if !seen.insert(proposal.digest()) {
            ledger.record_duplicate(&proposal.teacher);
            continue;
        }
        let started = Instant::now();
        let verdict = oracle.verify(&proposal);
        let cost = started.elapsed().as_nanos();
        ledger.record(&proposal.teacher, &verdict, cost);
        if verdict == Verdict::Accept {
            admitted.push(proposal);
        }
    }
    admitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    /// Proposes `count` items with payload `k` and claimed answer `2k`, lying
    /// (claiming `2k+1`) on every odd `k`.
    struct LyingTeacher {
        name: String,
        domains: Vec<Domain>,
        issued: AtomicU64,
    }

    impl Teacher for LyingTeacher {
        fn name(&self) -> &str {
            &self.name
        }
        fn kind(&self) -> TeacherKind {
            TeacherKind::LanguageModel
        }
        fn residency(&self) -> Residency {
            Residency::Resident
        }
        fn domains(&self) -> &[Domain] {
            &self.domains
        }
        fn propose_batch(&self, requests: &[LessonRequest]) -> Vec<Proposal> {
            let mut out = Vec::new();
            for req in requests {
                for _ in 0..req.count {
                    let k = self.issued.fetch_add(1, AtomicOrdering::SeqCst);
                    let truth = 2 * k;
                    let claim = if k % 2 == 1 { truth + 1 } else { truth };
                    out.push(Proposal {
                        lesson: req.lesson,
                        teacher: self.name.clone(),
                        item_key: Digest::of(format!("{}:{k}", req.skill).as_bytes()),
                        payload: k.to_string().into_bytes(),
                        claimed_answer: claim.to_string().into_bytes(),
                    });
                }
            }
            out
        }
    }

    /// Independent reference: recomputes the answer from the payload alone.
    struct DoublingOracle {
        calls: AtomicU64,
    }

    impl Oracle for DoublingOracle {
        fn name(&self) -> &str {
            "doubling"
        }
        fn verify(&self, p: &Proposal) -> Verdict {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            let Some(k) = std::str::from_utf8(&p.payload)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
            else {
                return Verdict::Reject("payload is not a number".into());
            };
            let want = (2 * k).to_string();
            if p.claimed_answer == want.as_bytes() {
                Verdict::Accept
            } else {
                Verdict::Reject(format!("claimed wrong answer for {k}"))
            }
        }
    }

    /// Stub teacher used only for selection tests.
    struct Stub {
        name: String,
        domains: Vec<Domain>,
    }

    impl Teacher for Stub {
        fn name(&self) -> &str {
            &self.name
        }
        fn kind(&self) -> TeacherKind {
            TeacherKind::Dictionary
        }
        fn residency(&self) -> Residency {
            Residency::NonResident
        }
        fn domains(&self) -> &[Domain] {
            &self.domains
        }
        fn propose_batch(&self, _requests: &[LessonRequest]) -> Vec<Proposal> {
            Vec::new()
        }
    }

    fn stub(name: &str, domains: &[Domain]) -> Stub {
        Stub {
            name: name.to_string(),
            domains: domains.to_vec(),
        }
    }

    fn lying(name: &str) -> LyingTeacher {
        LyingTeacher {
            name: name.to_string(),
            domains: vec![Domain::Mathematics],
            issued: AtomicU64::new(0),
        }
    }

    fn request(count: u32) -> LessonRequest {
        LessonRequest {
            lesson: Digest::of(b"lesson-doubling"),
            domain: Domain::Mathematics,
            skill: "double".into(),
            count,
        }
    }

    #[test]
    fn lying_teacher_is_rejected_and_ledger_counts_are_exact() {
        let teacher = lying("liar");
        let oracle = DoublingOracle {
            calls: AtomicU64::new(0),
        };
        let mut ledger = TeacherLedger::default();
        let mut seen = BTreeSet::new();

        let proposals = teacher.propose_batch(&[request(10)]);
        let admitted = admit_batch(&oracle, &mut ledger, &mut seen, proposals);

        assert_eq!(admitted.len(), 5);
        for p in &admitted {
            let k: u64 = std::str::from_utf8(&p.payload).unwrap().parse().unwrap();
            assert_eq!(k % 2, 0, "only truthful (even) items may be admitted");
        }
        assert_eq!(oracle.calls.load(AtomicOrdering::SeqCst), 10);

        let stats = ledger.stats("liar").expect("liar recorded");
        assert_eq!(stats.proposed, 10);
        assert_eq!(stats.admitted, 5);
        assert_eq!(stats.rejected, 5);
        assert_eq!(stats.duplicates, 0);
        assert!((stats.admitted_rate() - 0.5).abs() < 1e-12);
        assert_eq!(ledger.rejection_reasons("liar").len(), 5);
        assert!(ledger.stats("nobody").is_none());
    }

    #[test]
    fn duplicates_are_skipped_without_calling_the_oracle() {
        let teacher = lying("dup");
        let oracle = DoublingOracle {
            calls: AtomicU64::new(0),
        };
        let mut ledger = TeacherLedger::default();
        let mut seen = BTreeSet::new();

        // Truthful items only, to isolate deduplication from rejection.
        let base = teacher.propose_batch(&[request(2)]);
        let first = base[0].clone();
        let second = base[1].clone();
        let batch = vec![first.clone(), first.clone(), second.clone(), first.clone()];
        let admitted = admit_batch(&oracle, &mut ledger, &mut seen, batch);

        // Item 0 (truthful) and item 1 (a lie, rejected) are verified once each.
        assert_eq!(oracle.calls.load(AtomicOrdering::SeqCst), 2);
        assert_eq!(admitted, vec![first.clone()]);
        let stats = ledger.stats("dup").unwrap();
        assert_eq!(stats.proposed, 2);
        assert_eq!(stats.duplicates, 2);

        // A later batch repeating an already-seen digest is also a duplicate.
        let again = admit_batch(&oracle, &mut ledger, &mut seen, vec![first]);
        assert!(again.is_empty());
        assert_eq!(oracle.calls.load(AtomicOrdering::SeqCst), 2);
        assert_eq!(ledger.stats("dup").unwrap().duplicates, 3);
    }

    #[test]
    fn selection_prefers_untried_then_higher_admitted_rate() {
        let slow = stub("slow", &[Domain::French]);
        let fast = stub("fast", &[Domain::French]);
        let fresh = stub("fresh", &[Domain::French]);
        let mut ledger = TeacherLedger::default();
        for _ in 0..2 {
            ledger.record("slow", &Verdict::Accept, 1_000_000_000);
        }
        for _ in 0..4 {
            ledger.record("fast", &Verdict::Accept, 1_000_000_000);
        }

        let candidates: Vec<&dyn Teacher> = vec![&slow, &fast];
        let chosen = ledger.select(&candidates, Domain::French).unwrap();
        assert_eq!(chosen.name(), "fast");

        let with_fresh: Vec<&dyn Teacher> = vec![&slow, &fast, &fresh];
        let chosen = ledger.select(&with_fresh, Domain::French).unwrap();
        assert_eq!(chosen.name(), "fresh", "untried teacher is explored first");
    }

    #[test]
    fn selection_never_returns_a_teacher_outside_the_domain() {
        let rust_only = stub("rust_only", &[Domain::Rust]);
        let french = stub("french", &[Domain::French]);
        let mut ledger = TeacherLedger::default();
        // rust_only has an enormous observed rate; it must still be ignored.
        ledger.record("rust_only", &Verdict::Accept, 1);
        ledger.record("french", &Verdict::Accept, 1_000_000_000);

        let candidates: Vec<&dyn Teacher> = vec![&rust_only, &french];
        let chosen = ledger.select(&candidates, Domain::French).unwrap();
        assert_eq!(chosen.name(), "french");

        let only_rust: Vec<&dyn Teacher> = vec![&rust_only];
        assert!(ledger.select(&only_rust, Domain::French).is_none());
    }

    #[test]
    fn selection_breaks_ties_by_name() {
        let b = stub("b", &[Domain::Rust]);
        let a = stub("a", &[Domain::Rust]);
        let ledger = TeacherLedger::default();
        let candidates: Vec<&dyn Teacher> = vec![&b, &a];
        let chosen = ledger.select(&candidates, Domain::Rust).unwrap();
        assert_eq!(chosen.name(), "a");
    }

    #[test]
    fn rejection_reason_ring_is_bounded_to_64() {
        let mut ledger = TeacherLedger::default();
        for i in 0..100 {
            ledger.record("t", &Verdict::Reject(format!("r{i}")), 0);
        }
        let reasons = ledger.rejection_reasons("t");
        assert_eq!(reasons.len(), 64);
        assert_eq!(reasons.first().map(String::as_str), Some("r36"));
        assert_eq!(reasons.last().map(String::as_str), Some("r99"));
        let stats = ledger.stats("t").unwrap();
        assert_eq!(stats.rejected, 100);
        assert_eq!(stats.proposed, 100);
    }

    #[test]
    fn proposal_digest_changes_with_any_field() {
        let base = Proposal {
            lesson: Digest::of(b"l"),
            teacher: "t".into(),
            item_key: Digest::of(b"k"),
            payload: b"p".to_vec(),
            claimed_answer: b"a".to_vec(),
        };
        let mut other = base.clone();
        other.claimed_answer = b"b".to_vec();
        assert_ne!(base.digest(), other.digest());
        assert_eq!(base.digest(), base.clone().digest());
    }
}

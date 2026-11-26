//! Parallel learning planner: priority lanes, fair share across lanes,
//! backpressure, deduplication and adaptive concurrency.
//!
//! The scheduler only decides *ordering* and *admission*. It holds no
//! reference to the examiner or the store and cannot certify anything.

use std::collections::{BTreeSet, BinaryHeap};

use crate::domain::Lane;
use crate::lesson::{Lesson, LessonId};

/// Priority of a lesson. Higher runs first. Deterministic and documented so
/// that it can be audited:
///
/// `value + gain + 2*user_priority + 50*min(failed_exams,10) - difficulty/2
///  - cost_penalty` where `cost_penalty = min(measured_cost_ms, 500)`.
///
/// Lessons that already failed are boosted (gap closure) but bounded so a
/// permanently failing lesson cannot starve the others.
pub fn priority(lesson: &Lesson) -> i64 {
    let s = &lesson.spec;
    let failed = lesson.failed_exams().min(10) as i64;
    let cost_ms = (lesson.measured_cost_nanos / 1_000_000).min(500) as i64;
    i64::from(s.scientific_value)
        + i64::from(s.expected_gain)
        + 2 * i64::from(s.user_priority)
        + 50 * failed
        - i64::from(s.difficulty) / 2
        - cost_ms
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    /// The lane queue is full: the producer must slow down.
    Backpressure(Lane),
    /// Already queued or running: deduplicated.
    Duplicate,
}

/// Additive-increase / multiplicative-decrease concurrency limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AimdLimit {
    pub min: usize,
    pub max: usize,
    current: usize,
    successes: usize,
}

impl AimdLimit {
    pub fn new(min: usize, max: usize, initial: usize) -> Self {
        let min = min.max(1);
        let max = max.max(min);
        Self {
            min,
            max,
            current: initial.clamp(min, max),
            successes: 0,
        }
    }

    pub fn current(&self) -> usize {
        self.current
    }

    /// One full window of successes raises the limit by one.
    pub fn on_success(&mut self) {
        self.successes += 1;
        if self.successes >= self.current {
            self.successes = 0;
            self.current = (self.current + 1).min(self.max);
        }
    }

    /// A resource fault (OOM, timeout, machine loss) halves the limit.
    pub fn on_overload(&mut self) {
        self.successes = 0;
        self.current = (self.current / 2).max(self.min);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LaneConfig {
    pub capacity: usize,
    /// Relative share in deficit round robin (>= 1).
    pub weight: u32,
    pub max_in_flight: usize,
}

impl Default for LaneConfig {
    fn default() -> Self {
        Self {
            capacity: 10_000,
            weight: 1,
            max_in_flight: usize::MAX,
        }
    }
}

struct LaneState {
    cfg: LaneConfig,
    heap: BinaryHeap<(i64, std::cmp::Reverse<u64>, LessonId)>,
    deficit: u64,
    in_flight: usize,
    dispatched: u64,
}

pub struct Scheduler {
    lanes: Vec<LaneState>,
    known: BTreeSet<LessonId>,
    seq: u64,
    cursor: usize,
    in_flight: usize,
    pub limit: AimdLimit,
}

impl Scheduler {
    pub fn new(limit: AimdLimit) -> Self {
        Self::with_lanes(limit, |_| LaneConfig::default())
    }

    pub fn with_lanes(limit: AimdLimit, cfg: impl Fn(Lane) -> LaneConfig) -> Self {
        Self {
            lanes: Lane::ALL
                .iter()
                .map(|l| {
                    let mut c = cfg(*l);
                    c.weight = c.weight.max(1);
                    LaneState {
                        cfg: c,
                        heap: BinaryHeap::new(),
                        deficit: 0,
                        in_flight: 0,
                        dispatched: 0,
                    }
                })
                .collect(),
            known: BTreeSet::new(),
            seq: 0,
            cursor: 0,
            in_flight: 0,
            limit,
        }
    }

    pub fn submit(&mut self, lane: Lane, id: LessonId, priority: i64) -> Result<(), SubmitError> {
        if self.known.contains(&id) {
            return Err(SubmitError::Duplicate);
        }
        let st = &mut self.lanes[lane.index()];
        if st.heap.len() >= st.cfg.capacity {
            return Err(SubmitError::Backpressure(lane));
        }
        self.seq += 1;
        st.heap.push((priority, std::cmp::Reverse(self.seq), id));
        self.known.insert(id);
        Ok(())
    }

    pub fn queued(&self, lane: Lane) -> usize {
        self.lanes[lane.index()].heap.len()
    }

    pub fn total_queued(&self) -> usize {
        self.lanes.iter().map(|l| l.heap.len()).sum()
    }

    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    pub fn dispatched(&self, lane: Lane) -> u64 {
        self.lanes[lane.index()].dispatched
    }

    /// Next task by weighted deficit round robin. Every non-empty lane that is
    /// below its in-flight cap is served at least once per round, which rules
    /// out starvation between disciplines. Returns `None` when the global
    /// concurrency limit is reached or nothing is dispatchable.
    pub fn dispatch(&mut self) -> Option<(Lane, LessonId)> {
        if self.in_flight >= self.limit.current() {
            return None;
        }
        let n = self.lanes.len();
        // At most two passes: one to top up deficits, one to dispatch.
        for _ in 0..2 * n {
            let i = self.cursor;
            let st = &mut self.lanes[i];
            let eligible = !st.heap.is_empty() && st.in_flight < st.cfg.max_in_flight;
            if eligible {
                if st.deficit == 0 {
                    st.deficit = u64::from(st.cfg.weight);
                }
                if let Some((_, _, id)) = st.heap.pop() {
                    st.deficit -= 1;
                    st.in_flight += 1;
                    st.dispatched += 1;
                    if st.deficit == 0 {
                        self.cursor = (i + 1) % n;
                    }
                    self.in_flight += 1;
                    return Some((Lane::ALL[i], id));
                }
            } else {
                st.deficit = 0;
            }
            self.cursor = (i + 1) % n;
        }
        None
    }

    /// Drains up to the current concurrency limit.
    pub fn next_wave(&mut self) -> Vec<(Lane, LessonId)> {
        let mut out = Vec::new();
        while let Some(t) = self.dispatch() {
            out.push(t);
        }
        out
    }

    /// Marks a dispatched task finished. `overload` signals a resource fault.
    /// The id is forgotten so that it can be resubmitted (retry / revision).
    pub fn complete(&mut self, lane: Lane, id: &LessonId, overload: bool) {
        let st = &mut self.lanes[lane.index()];
        st.in_flight = st.in_flight.saturating_sub(1);
        self.in_flight = self.in_flight.saturating_sub(1);
        self.known.remove(id);
        if overload {
            self.limit.on_overload();
        } else {
            self.limit.on_success();
        }
    }

    /// Controlled cancellation: removes every queued (not running) task.
    pub fn cancel_queued(&mut self) -> Vec<LessonId> {
        let mut out = Vec::new();
        for st in &mut self.lanes {
            for (_, _, id) in st.heap.drain() {
                out.push(id);
            }
        }
        for id in &out {
            self.known.remove(id);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Domain;
    use crate::hash::Digest;
    use crate::lesson::test_spec;

    fn id(i: u64) -> LessonId {
        Digest::of(&i.to_le_bytes())
    }

    #[test]
    fn dedup_and_backpressure() {
        let mut s = Scheduler::with_lanes(AimdLimit::new(1, 8, 8), |_| LaneConfig {
            capacity: 2,
            ..LaneConfig::default()
        });
        s.submit(Lane::French, id(1), 0).unwrap();
        assert_eq!(
            s.submit(Lane::English, id(1), 0),
            Err(SubmitError::Duplicate)
        );
        s.submit(Lane::French, id(2), 0).unwrap();
        assert_eq!(
            s.submit(Lane::French, id(3), 0),
            Err(SubmitError::Backpressure(Lane::French))
        );
        // Other lanes are unaffected by French backpressure.
        s.submit(Lane::Rust, id(3), 0).unwrap();
    }

    #[test]
    fn priority_order_within_lane() {
        let mut s = Scheduler::new(AimdLimit::new(1, 1, 1));
        s.submit(Lane::Mathematics, id(1), 5).unwrap();
        s.submit(Lane::Mathematics, id(2), 50).unwrap();
        s.submit(Lane::Mathematics, id(3), 50).unwrap();
        let (l, a) = s.dispatch().unwrap();
        assert_eq!(a, id(2), "highest priority, FIFO among equals");
        assert_eq!(s.dispatch(), None, "concurrency limit 1 reached");
        s.complete(l, &a, false);
        assert_eq!(s.dispatch().unwrap().1, id(3));
    }

    #[test]
    fn no_starvation_between_disciplines() {
        let mut s = Scheduler::new(AimdLimit::new(1000, 1000, 1000));
        for i in 0..400 {
            s.submit(Lane::Mathematics, id(i), 1000).unwrap();
        }
        for i in 0..4 {
            s.submit(Lane::French, id(10_000 + i), -1000).unwrap();
            s.submit(Lane::Rust, id(20_000 + i), -1000).unwrap();
        }
        let first: Vec<Lane> = (0..12).map(|_| s.dispatch().unwrap().0).collect();
        assert_eq!(first.iter().filter(|l| **l == Lane::French).count(), 4);
        assert_eq!(first.iter().filter(|l| **l == Lane::Rust).count(), 4);
    }

    #[test]
    fn weights_give_proportional_share() {
        let mut s = Scheduler::with_lanes(AimdLimit::new(1000, 1000, 1000), |l| LaneConfig {
            weight: if l == Lane::Mathematics { 3 } else { 1 },
            ..LaneConfig::default()
        });
        for i in 0..100 {
            s.submit(Lane::Mathematics, id(i), 0).unwrap();
            s.submit(Lane::English, id(1000 + i), 0).unwrap();
        }
        for _ in 0..80 {
            s.dispatch().unwrap();
        }
        assert_eq!(s.dispatched(Lane::Mathematics), 60);
        assert_eq!(s.dispatched(Lane::English), 20);
    }

    #[test]
    fn aimd_adapts_and_cancel_drains() {
        let mut a = AimdLimit::new(1, 16, 8);
        a.on_overload();
        assert_eq!(a.current(), 4);
        for _ in 0..4 {
            a.on_success();
        }
        assert_eq!(a.current(), 5);
        let mut s = Scheduler::new(a);
        s.submit(Lane::Revision, id(1), 0).unwrap();
        s.submit(Lane::Transfer, id(2), 0).unwrap();
        assert_eq!(s.cancel_queued().len(), 2);
        assert_eq!(s.total_queued(), 0);
        s.submit(Lane::Revision, id(1), 0).unwrap();
    }

    #[test]
    fn failed_lessons_are_boosted_but_bounded() {
        use crate::mastery::{check_partitions, Cohort, CohortResult, ExamReport};
        let mut l = Lesson::new(test_spec(Domain::English, "x", vec![]));
        let base = priority(&l);
        let corpus = check_partitions(&[], &[], &[], &[]).unwrap();
        for _ in 0..30 {
            l.exams.push(ExamReport {
                lesson: l.id,
                corpus,
                cohorts: vec![CohortResult {
                    cohort: Cohort::Test,
                    correct: 0,
                    abstained: 0,
                    total: 1,
                }],
            });
        }
        assert_eq!(priority(&l), base + 500);
    }
}

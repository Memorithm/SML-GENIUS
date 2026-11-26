//! A bounded, std-only parallel job executor.
//!
//! This is an engine component: it runs caller-supplied closures over a
//! vector of jobs on a fixed number of worker threads. It has no knowledge
//! of what the jobs mean.
//!
//! Properties:
//! - Results are index-aligned with the input jobs, whatever worker ran them.
//! - Work is distributed round-robin into one deque per worker. A worker pops
//!   from the front of its own deque and, when that is empty, steals from the
//!   back of the other deques.
//! - Each job runs under `catch_unwind`; a panic becomes
//!   [`JobOutcome::Panicked`] and does not affect other jobs.
//! - A [`CancellationToken`] is checked before each job starts. Jobs that
//!   have already started are never interrupted.
//! - `workers == 1` runs everything sequentially on the calling thread.

use std::any::Any;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

/// Cooperative cancellation flag shared between the caller and running jobs.
#[derive(Clone, Default, Debug)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Creates a token that is not cancelled.
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    /// Requests cancellation. Jobs not yet started will be marked `Cancelled`.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    /// Returns `true` once [`CancellationToken::cancel`] has been called on any clone.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// The result of one job.
#[derive(Debug)]
pub enum JobOutcome<R> {
    /// The job ran to completion and produced a value.
    Completed(R),
    /// The job panicked; the payload is rendered as a message.
    Panicked(String),
    /// The job was never started because cancellation had been requested.
    Cancelled,
}

impl<R> JobOutcome<R> {
    /// Returns `true` for [`JobOutcome::Completed`].
    pub fn is_completed(&self) -> bool {
        matches!(self, JobOutcome::Completed(_))
    }
}

/// Counters describing one `run_parallel` call.
#[derive(Debug, Clone)]
pub struct ExecStats {
    /// Effective worker count after clamping.
    pub workers: usize,
    /// Jobs whose closure was invoked (including ones that panicked).
    pub executed: u64,
    /// Successful steals from another worker's deque.
    pub stolen: u64,
    /// Jobs whose closure panicked.
    pub panicked: u64,
    /// Jobs marked `Cancelled` without being started.
    pub cancelled: u64,
    /// Wall-clock time of the whole call in nanoseconds.
    pub wall_nanos: u128,
}

/// Number of workers the platform suggests, or 1 if it cannot tell.
pub fn available_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[derive(Default)]
struct Counters {
    executed: AtomicU64,
    stolen: AtomicU64,
    panicked: AtomicU64,
    cancelled: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn panic_message(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic".to_string()
    }
}

/// Runs one job, honouring cancellation and isolating panics.
fn run_job<J, R, F>(job: J, cancel: &CancellationToken, f: &F, c: &Counters) -> JobOutcome<R>
where
    F: Fn(J) -> R,
{
    if cancel.is_cancelled() {
        c.cancelled.fetch_add(1, Ordering::Relaxed);
        return JobOutcome::Cancelled;
    }
    c.executed.fetch_add(1, Ordering::Relaxed);
    match catch_unwind(AssertUnwindSafe(|| f(job))) {
        Ok(value) => JobOutcome::Completed(value),
        Err(payload) => {
            c.panicked.fetch_add(1, Ordering::Relaxed);
            JobOutcome::Panicked(panic_message(payload.as_ref()))
        }
    }
}

/// State shared by the worker threads of one parallel run.
struct Shared<'a, J, R, F> {
    deques: Vec<Mutex<VecDeque<(usize, J)>>>,
    results: Vec<Mutex<Option<JobOutcome<R>>>>,
    cancel: &'a CancellationToken,
    f: &'a F,
    counters: Counters,
}

impl<J, R, F> Shared<'_, J, R, F>
where
    F: Fn(J) -> R,
{
    /// Tries to steal one job from the back of another worker's deque.
    fn steal(&self, me: usize) -> Option<(usize, J)> {
        let n = self.deques.len();
        for offset in 1..n {
            let victim = (me + offset) % n;
            // The guard is a temporary dropped at the end of this statement,
            // so no two deque locks are ever held at the same time.
            let got = lock(&self.deques[victim]).pop_back();
            if let Some(item) = got {
                self.counters.stolen.fetch_add(1, Ordering::Relaxed);
                return Some(item);
            }
        }
        None
    }

    /// Body of one worker thread. Exits when every deque is empty; no jobs
    /// are added during a run, so an empty scan means the work is finished.
    fn work(&self, me: usize) {
        loop {
            let own = lock(&self.deques[me]).pop_front();
            let Some((idx, job)) = own.or_else(|| self.steal(me)) else {
                break;
            };
            let outcome = run_job(job, self.cancel, self.f, &self.counters);
            *lock(&self.results[idx]) = Some(outcome);
        }
    }
}

/// Runs `f` over `jobs` on up to `workers` threads.
///
/// The returned outcomes are in the same order as `jobs`. `workers` is
/// clamped to `[1, jobs.len().max(1)]`. With one worker the jobs run on the
/// calling thread in input order.
pub fn run_parallel<J, R, F>(
    jobs: Vec<J>,
    workers: usize,
    cancel: &CancellationToken,
    f: F,
) -> (Vec<JobOutcome<R>>, ExecStats)
where
    J: Send,
    R: Send,
    F: Fn(J) -> R + Sync,
{
    let start = Instant::now();
    let n = jobs.len();
    let workers = workers.clamp(1, n.max(1));

    let (outcomes, counters) = if workers == 1 {
        let counters = Counters::default();
        let outcomes: Vec<JobOutcome<R>> = jobs
            .into_iter()
            .map(|job| run_job(job, cancel, &f, &counters))
            .collect();
        (outcomes, counters)
    } else {
        let mut deques: Vec<VecDeque<(usize, J)>> = (0..workers).map(|_| VecDeque::new()).collect();
        for (i, job) in jobs.into_iter().enumerate() {
            deques[i % workers].push_back((i, job));
        }
        let shared = Shared {
            deques: deques.into_iter().map(Mutex::new).collect(),
            results: (0..n).map(|_| Mutex::new(None)).collect(),
            cancel,
            f: &f,
            counters: Counters::default(),
        };
        std::thread::scope(|s| {
            for me in 0..workers {
                let sh = &shared;
                s.spawn(move || sh.work(me));
            }
        });
        let Shared {
            results, counters, ..
        } = shared;
        let outcomes = results
            .into_iter()
            .map(|slot| {
                slot.into_inner()
                    .unwrap_or_else(PoisonError::into_inner)
                    .unwrap_or(JobOutcome::Cancelled)
            })
            .collect();
        (outcomes, counters)
    };

    let stats = ExecStats {
        workers,
        executed: counters.executed.load(Ordering::Relaxed),
        stolen: counters.stolen.load(Ordering::Relaxed),
        panicked: counters.panicked.load(Ordering::Relaxed),
        cancelled: counters.cancelled.load(Ordering::Relaxed),
        wall_nanos: start.elapsed().as_nanos(),
    };
    (outcomes, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Deterministic, non-trivial value for job `x`.
    fn mix(x: usize) -> u64 {
        let mut h = (x as u64) ^ 0x9E37_79B9_7F4A_7C15;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 31;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^ (h >> 29)
    }

    fn values(outcomes: &[JobOutcome<u64>]) -> Vec<u64> {
        outcomes
            .iter()
            .map(|o| match o {
                JobOutcome::Completed(v) => *v,
                other => panic!("expected Completed, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn token_starts_clear_and_cancel_is_shared_across_clones() {
        let t = CancellationToken::new();
        assert!(!t.is_cancelled());
        let clone = t.clone();
        clone.cancel();
        assert!(t.is_cancelled());
        assert!(!CancellationToken::default().is_cancelled());
    }

    #[test]
    fn order_preserved_with_1_2_4_8_workers() {
        let expected: Vec<u64> = (0..1000).map(mix).collect();
        for workers in [1, 2, 4, 8] {
            let jobs: Vec<usize> = (0..1000).collect();
            let token = CancellationToken::new();
            let (out, stats) = run_parallel(jobs, workers, &token, mix);
            assert_eq!(out.len(), 1000, "workers={workers}");
            assert_eq!(values(&out), expected, "workers={workers}");
            assert_eq!(stats.executed, 1000);
            assert_eq!(stats.panicked, 0);
            assert_eq!(stats.cancelled, 0);
            assert_eq!(stats.workers, workers);
        }
    }

    #[test]
    fn sequential_and_parallel_results_are_identical() {
        let token = CancellationToken::new();
        let (seq, _) = run_parallel((0..1000usize).collect(), 1, &token, mix);
        let (par, _) = run_parallel((0..1000usize).collect(), 8, &token, mix);
        assert_eq!(values(&seq), values(&par));
    }

    #[test]
    fn panicking_job_is_isolated() {
        let token = CancellationToken::new();
        let (out, stats) = run_parallel((0..100usize).collect(), 4, &token, |x| {
            if x == 37 {
                panic!("boom at {x}");
            }
            mix(x)
        });
        assert_eq!(out.len(), 100);
        for (i, o) in out.iter().enumerate() {
            if i == 37 {
                match o {
                    JobOutcome::Panicked(msg) => assert!(msg.contains("boom at 37"), "{msg}"),
                    other => panic!("expected Panicked, got {other:?}"),
                }
            } else {
                assert!(o.is_completed(), "job {i} should complete");
            }
        }
        assert_eq!(stats.panicked, 1);
        assert_eq!(stats.executed, 100);
    }

    #[test]
    fn cancel_before_run_marks_everything_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        for workers in [1, 4] {
            let (out, stats) = run_parallel((0..200usize).collect(), workers, &token, mix);
            assert_eq!(out.len(), 200);
            assert!(out.iter().all(|o| matches!(o, JobOutcome::Cancelled)));
            assert_eq!(stats.executed, 0, "workers={workers}");
            assert_eq!(stats.cancelled, 200, "workers={workers}");
        }
    }

    #[test]
    fn cancel_from_inside_a_job_stops_the_rest() {
        // Sequential: exact boundary is known.
        let token = CancellationToken::new();
        let inner = token.clone();
        let (out, stats) = run_parallel((0..1000usize).collect(), 1, &token, move |x| {
            if x == 10 {
                inner.cancel();
            }
            mix(x)
        });
        for (i, o) in out.iter().enumerate().take(11) {
            assert!(o.is_completed(), "job {i} ran before cancellation");
        }
        assert!(out[11..].iter().all(|o| matches!(o, JobOutcome::Cancelled)));
        assert_eq!(stats.executed, 11);
        assert_eq!(stats.cancelled, 989);

        // Parallel: job 0 cancels; some jobs are cancelled, every completed
        // value is still correct.
        let token = CancellationToken::new();
        let inner = token.clone();
        // Non-zero jobs sleep so the cancel from job 0 reliably lands before
        // the bulk of the queue has been started.
        let (out, stats) = run_parallel((0..1000usize).collect(), 4, &token, move |x| {
            if x == 0 {
                inner.cancel();
            } else {
                std::thread::sleep(Duration::from_millis(1));
            }
            mix(x)
        });
        assert!(stats.cancelled > 0);
        assert_eq!(stats.executed + stats.cancelled, 1000);
        assert!(out[0].is_completed());
        for (i, o) in out.iter().enumerate() {
            match o {
                JobOutcome::Completed(v) => assert_eq!(*v, mix(i)),
                JobOutcome::Cancelled => {}
                JobOutcome::Panicked(m) => panic!("unexpected panic: {m}"),
            }
        }
    }

    #[test]
    fn stealing_happens_on_imbalanced_load() {
        // Round-robin over 4 workers: worker 0 owns every job with index % 4 == 0,
        // and those are slow. The other workers finish early and must steal.
        let token = CancellationToken::new();
        let jobs: Vec<usize> = (0..64).collect();
        let (out, stats) = run_parallel(jobs, 4, &token, |x| {
            if x % 4 == 0 {
                std::thread::sleep(Duration::from_millis(2));
            }
            mix(x)
        });
        assert!(stats.stolen > 0, "expected steals, got {stats:?}");
        assert_eq!(values(&out), (0..64).map(mix).collect::<Vec<_>>());
    }

    #[test]
    fn worker_count_is_clamped_and_empty_input_is_fine() {
        let token = CancellationToken::new();

        let (out, stats) = run_parallel((0..10usize).collect(), 0, &token, mix);
        assert_eq!(stats.workers, 1);
        assert_eq!(values(&out).len(), 10);

        let (out, stats) = run_parallel((0..3usize).collect(), 16, &token, mix);
        assert_eq!(stats.workers, 3);
        assert_eq!(values(&out), (0..3).map(mix).collect::<Vec<_>>());

        let (out, stats) = run_parallel(Vec::<usize>::new(), 8, &token, mix);
        assert!(out.is_empty());
        assert_eq!(stats.workers, 1);
        assert_eq!(stats.executed, 0);
    }

    #[test]
    fn available_workers_is_at_least_one() {
        assert!(available_workers() >= 1);
    }
}

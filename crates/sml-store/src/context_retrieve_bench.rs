//! CONTEXT-3 G1 measurement helpers: per-query wall-clock latency
//! percentiles and Linux process peak-RSS sampling.
//!
//! These helpers only *measure*; they never change retrieval behaviour.
//! Latency is wall-clock (`std::time::Instant`) per call, summarised with the
//! nearest-rank percentile. Peak RSS is the kernel's `VmHWM` from
//! `/proc/self/status`; it can be reset between cells by writing `5` to
//! `/proc/self/clear_refs` (Linux >= 4.0). Where procfs is unavailable the
//! values are `None` and must be reported as `NOT_MEASURED`, never estimated.
//!
//! Page-fault counters are the cumulative `minflt` / `majflt` fields of
//! `/proc/self/stat` (process-wide, all threads that have been reaped plus the
//! live ones as reported by the kernel). `VmPeak` is the peak *virtual* size
//! and, unlike `VmHWM`, cannot be reset; it is a process-level figure only.

use std::{fs, time::Instant};

/// Nearest-rank percentile of an ascending-sorted sample (`0 < p <= 100`).
/// Returns `None` for an empty sample or an out-of-range `p`.
#[must_use]
pub fn percentile_nearest_rank(sorted: &[u64], p: u32) -> Option<u64> {
    if sorted.is_empty() || p == 0 || p > 100 {
        return None;
    }
    let n = sorted.len();
    let rank = (u64::from(p) * n as u64).div_ceil(100) as usize;
    Some(sorted[rank.max(1) - 1])
}

/// Latency summary in nanoseconds over `samples` calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencySummary {
    pub samples: usize,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub max_ns: u64,
}

impl LatencySummary {
    /// Summarises raw per-call durations (sorted internally).
    #[must_use]
    pub fn from_samples(mut samples_ns: Vec<u64>) -> Option<Self> {
        samples_ns.sort_unstable();
        Some(Self {
            samples: samples_ns.len(),
            p50_ns: percentile_nearest_rank(&samples_ns, 50)?,
            p95_ns: percentile_nearest_rank(&samples_ns, 95)?,
            max_ns: *samples_ns.last()?,
        })
    }
}

/// Times `call` once per input, after `warmup` untimed passes over all
/// inputs. Errors abort the measurement (fail closed).
pub fn measure_latency<Q, E>(
    inputs: &[Q],
    warmup: usize,
    mut call: impl FnMut(&Q) -> Result<(), E>,
) -> Result<Option<LatencySummary>, E> {
    for _ in 0..warmup {
        for input in inputs {
            call(input)?;
        }
    }
    let mut samples = Vec::with_capacity(inputs.len());
    for input in inputs {
        let t0 = Instant::now();
        call(input)?;
        samples.push(u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    Ok(LatencySummary::from_samples(samples))
}

/// Parses a `Key:   <n> kB` line of `/proc/<pid>/status` into KiB.
#[must_use]
pub fn parse_proc_status_kib(status: &str, key: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let rest = line.strip_prefix(key)?.strip_prefix(':')?;
        let mut parts = rest.split_whitespace();
        let value = parts.next()?.parse().ok()?;
        (parts.next()? == "kB").then_some(value)
    })
}

/// Current process peak resident set (`VmHWM`, KiB); `None` if unavailable.
#[must_use]
pub fn peak_rss_kib() -> Option<u64> {
    parse_proc_status_kib(&fs::read_to_string("/proc/self/status").ok()?, "VmHWM")
}

/// Current process resident set (`VmRSS`, KiB); `None` if unavailable.
#[must_use]
pub fn current_rss_kib() -> Option<u64> {
    parse_proc_status_kib(&fs::read_to_string("/proc/self/status").ok()?, "VmRSS")
}

/// Resets `VmHWM` to the current RSS. Returns `false` if unsupported, in
/// which case per-cell peaks are cumulative and must be labelled as such.
#[must_use]
pub fn reset_peak_rss() -> bool {
    fs::write("/proc/self/clear_refs", "5").is_ok()
}

/// Cumulative process page-fault counters from `/proc/self/stat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FaultCounters {
    pub minor: u64,
    pub major: u64,
}

impl FaultCounters {
    /// `self - earlier`; `None` if a counter went backwards (not comparable).
    #[must_use]
    pub fn delta_since(self, earlier: Self) -> Option<Self> {
        Some(Self {
            minor: self.minor.checked_sub(earlier.minor)?,
            major: self.major.checked_sub(earlier.major)?,
        })
    }
}

/// Parses `minflt` (field 10) and `majflt` (field 12) of a `/proc/<pid>/stat`
/// line. The `comm` field may contain spaces and parentheses, so fields are
/// counted after the *last* `)`. Returns `None` on any malformed input.
#[must_use]
pub fn parse_proc_stat_faults(stat: &str) -> Option<FaultCounters> {
    let rest = &stat[stat.rfind(')')? + 1..];
    // After `)`: field 3 (state) is index 0, so field k is index k - 3.
    let f: Vec<&str> = rest.split_whitespace().collect();
    Some(FaultCounters {
        minor: f.get(10 - 3)?.parse().ok()?,
        major: f.get(12 - 3)?.parse().ok()?,
    })
}

/// Current cumulative process fault counters; `None` if unavailable.
#[must_use]
pub fn fault_counters() -> Option<FaultCounters> {
    parse_proc_stat_faults(&fs::read_to_string("/proc/self/stat").ok()?)
}

/// Process peak virtual size (`VmPeak`, KiB); `None` if unavailable.
#[must_use]
pub fn vm_peak_kib() -> Option<u64> {
    parse_proc_status_kib(&fs::read_to_string("/proc/self/status").ok()?, "VmPeak")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles() {
        let s: Vec<u64> = (1..=20).collect();
        assert_eq!(percentile_nearest_rank(&s, 50), Some(10));
        assert_eq!(percentile_nearest_rank(&s, 95), Some(19));
        assert_eq!(percentile_nearest_rank(&s, 100), Some(20));
        assert_eq!(percentile_nearest_rank(&s, 1), Some(1));
        assert_eq!(percentile_nearest_rank(&[7], 95), Some(7));
        assert_eq!(percentile_nearest_rank(&[], 50), None);
        assert_eq!(percentile_nearest_rank(&s, 0), None);
        assert_eq!(percentile_nearest_rank(&s, 101), None);
    }

    #[test]
    fn summary_sorts_and_rejects_empty() {
        let summary = LatencySummary::from_samples(vec![5, 1, 4, 2, 3]).unwrap();
        assert_eq!(
            summary,
            LatencySummary {
                samples: 5,
                p50_ns: 3,
                p95_ns: 5,
                max_ns: 5
            }
        );
        assert_eq!(LatencySummary::from_samples(Vec::new()), None);
    }

    #[test]
    fn measure_latency_counts_warmup_and_fails_closed() {
        let mut calls = 0;
        let summary = measure_latency(&[1, 2, 3], 2, |_| {
            calls += 1;
            Ok::<(), ()>(())
        })
        .unwrap()
        .unwrap();
        assert_eq!(calls, 9);
        assert_eq!(summary.samples, 3);
        assert!(measure_latency(&[1, 2], 0, |&x| if x == 2 { Err(()) } else { Ok(()) }).is_err());
    }

    #[test]
    fn parses_proc_status_lines() {
        let status = "Name:\tx\nVmHWM:\t  12345 kB\nVmRSS:\t 678 kB\nBad:\t9 MB\n";
        assert_eq!(parse_proc_status_kib(status, "VmHWM"), Some(12345));
        assert_eq!(parse_proc_status_kib(status, "VmRSS"), Some(678));
        assert_eq!(parse_proc_status_kib(status, "Bad"), None);
        assert_eq!(parse_proc_status_kib(status, "VmH"), None);
        assert_eq!(parse_proc_status_kib(status, "Missing"), None);
    }

    #[test]
    fn parses_proc_stat_faults_with_hostile_comm() {
        // pid (comm) state ppid pgrp session tty tpgid flags minflt cminflt majflt
        let stat = "42 (a b) c)) S 1 42 42 0 -1 4194560 1234 7 5 0 99";
        assert_eq!(
            parse_proc_stat_faults(stat),
            Some(FaultCounters {
                minor: 1234,
                major: 5
            })
        );
        assert_eq!(parse_proc_stat_faults("42 (x) S 1 42"), None);
        assert_eq!(parse_proc_stat_faults("no paren 1 2 3"), None);
        assert_eq!(parse_proc_stat_faults("1 (x) S 1 1 1 0 -1 0 abc 0 0"), None);
    }

    #[test]
    fn fault_delta_refuses_backwards_counters() {
        let a = FaultCounters {
            minor: 10,
            major: 2,
        };
        let b = FaultCounters {
            minor: 15,
            major: 2,
        };
        assert_eq!(b.delta_since(a), Some(FaultCounters { minor: 5, major: 0 }));
        assert_eq!(a.delta_since(b), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_faults_and_vmpeak_are_readable() {
        let f = fault_counters().unwrap();
        assert!(f.minor > 0);
        assert!(vm_peak_kib().unwrap() > 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_rss_is_readable() {
        // Concurrent test threads can move RSS between reads; read once.
        let status = fs::read_to_string("/proc/self/status").unwrap();
        let peak = parse_proc_status_kib(&status, "VmHWM").unwrap();
        let current = parse_proc_status_kib(&status, "VmRSS").unwrap();
        assert!(peak >= current && current > 0);
        assert!(peak_rss_kib().is_some() && current_rss_kib().is_some());
    }
}

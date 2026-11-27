//! Host resource sampling for the sml-tutord supervisor's backpressure gate.
//!
//! The supervisor starts new learning work only when the host has headroom:
//! free memory, load relative to the CPU count, and free disk on the volume
//! where work is written. This module takes those readings and checks them
//! against a [`ResourceBudget`].
//!
//! Linux `/proc` only: memory comes from `/proc/meminfo`, load from
//! `/proc/loadavg`, and free disk from `df -Pk` (run directly, never through a
//! shell). Every read is fallible, and a failed read is reported as `None` for
//! that field, meaning *unknown*. [`ResourceBudget::admit`] treats unknown as
//! not admitted, so a host we cannot measure is never admitted: the gate fails
//! closed.

use std::fs;
use std::path::Path;
use std::process::Command;

/// One point-in-time reading of host resources. `None` means unknown.
#[derive(Clone, Debug, PartialEq)]
pub struct HostSample {
    /// `MemTotal` from `/proc/meminfo`, in KiB.
    pub mem_total_kib: Option<u64>,
    /// `MemAvailable` from `/proc/meminfo`, in KiB.
    pub mem_available_kib: Option<u64>,
    /// The 1-minute load average from `/proc/loadavg`.
    pub load1: Option<f64>,
    /// Logical CPUs from `available_parallelism`, never less than 1.
    pub cpus: usize,
    /// Available space on the probed volume, in KiB, from `df -Pk`.
    pub disk_free_kib: Option<u64>,
}

/// Extracts `(MemTotal, MemAvailable)` in KiB from `/proc/meminfo` text.
///
/// Lines look like `MemAvailable:   16777216 kB`. A missing or malformed line
/// yields `None` for that field.
pub fn parse_meminfo(text: &str) -> (Option<u64>, Option<u64>) {
    let mut total = None;
    let mut available = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = parse_kib(rest);
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available = parse_kib(rest);
        }
    }
    (total, available)
}

/// Parses `<number> kB` (the value part of a meminfo line).
fn parse_kib(rest: &str) -> Option<u64> {
    let mut fields = rest.split_whitespace();
    let value: u64 = fields.next()?.parse().ok()?;
    let unit = fields.next();
    if fields.next().is_some() || unit.is_some_and(|u| u != "kB") {
        return None;
    }
    Some(value)
}

/// Returns the first field of `/proc/loadavg` text if it is a finite,
/// non-negative number.
pub fn parse_loadavg(text: &str) -> Option<f64> {
    let load: f64 = text.split_whitespace().next()?.parse().ok()?;
    (load.is_finite() && load >= 0.0).then_some(load)
}

/// Returns the Available column (4th) of the last non-empty line of
/// `df -Pk PATH` output. The output must contain a header line followed by at
/// least one data line.
pub fn parse_df_pk(text: &str) -> Option<u64> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    lines.next()?; // header
    let data = lines.next_back()?;
    data.split_whitespace().nth(3)?.parse().ok()
}

/// Takes one reading of the host. Any failed read leaves its field `None`.
///
/// `disk_path` is the volume whose free space is measured; it is passed to
/// `df` as an argument, with no shell involved.
pub fn sample(disk_path: &Path) -> HostSample {
    let (mem_total_kib, mem_available_kib) = fs::read_to_string("/proc/meminfo")
        .map(|text| parse_meminfo(&text))
        .unwrap_or((None, None));
    let load1 = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| parse_loadavg(&text));
    HostSample {
        mem_total_kib,
        mem_available_kib,
        load1,
        cpus: std::thread::available_parallelism().map_or(1, |n| n.get()),
        disk_free_kib: read_disk_free(disk_path),
    }
}

fn read_disk_free(path: &Path) -> Option<u64> {
    let output = Command::new("df")
        .arg("-Pk")
        .arg("--")
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_df_pk(std::str::from_utf8(&output.stdout).ok()?)
}

/// Minimum headroom a host must have before new learning work is admitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResourceBudget {
    /// Minimum `MemAvailable`, in KiB.
    pub min_mem_available_kib: u64,
    /// Maximum `load1 / cpus`.
    pub max_load_per_cpu: f64,
    /// Minimum free disk on the probed volume, in KiB.
    pub min_disk_free_kib: u64,
}

impl Default for ResourceBudget {
    /// 4 GiB available memory, load at most 0.85 per CPU, 2 GiB free disk.
    fn default() -> Self {
        Self {
            min_mem_available_kib: 4 * 1024 * 1024,
            max_load_per_cpu: 0.85,
            min_disk_free_kib: 2 * 1024 * 1024,
        }
    }
}

impl ResourceBudget {
    /// `Ok(())` when every limit is met. Otherwise `Err` lists every violated
    /// or unknown field, so a single rejected sample explains all its causes.
    pub fn admit(&self, s: &HostSample) -> Result<(), Vec<String>> {
        let mut reasons = Vec::new();

        let min_mem = self.min_mem_available_kib;
        match s.mem_available_kib {
            None => reasons.push("mem_available unknown".to_owned()),
            Some(avail) if avail < min_mem => {
                reasons.push(format!("mem_available {avail} KiB < {min_mem} KiB"));
            }
            Some(_) => {}
        }

        let max_load = self.max_load_per_cpu;
        match s.load1 {
            None => reasons.push("load1 unknown".to_owned()),
            Some(load1) => {
                let per_cpu = load1 / s.cpus.max(1) as f64;
                if per_cpu.is_nan() || per_cpu > max_load {
                    reasons.push(format!("load1/cpu {per_cpu:.2} > {max_load:.2}"));
                }
            }
        }

        let min_disk = self.min_disk_free_kib;
        match s.disk_free_kib {
            None => reasons.push("disk_free unknown".to_owned()),
            Some(free) if free < min_disk => {
                reasons.push(format!("disk_free {free} KiB < {min_disk} KiB"));
            }
            Some(_) => {}
        }

        if reasons.is_empty() {
            Ok(())
        } else {
            Err(reasons)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMINFO: &str = "\
MemTotal:       32768000 kB
MemFree:         1234567 kB
MemAvailable:   16777216 kB
Buffers:          123456 kB
";

    const DF: &str = "\
Filesystem     1024-blocks      Used Available Capacity Mounted on
/dev/sda1       103081248  45012344  52913456      46% /
";

    fn healthy() -> HostSample {
        HostSample {
            mem_total_kib: Some(32 * 1024 * 1024),
            mem_available_kib: Some(8 * 1024 * 1024),
            load1: Some(1.0),
            cpus: 4,
            disk_free_kib: Some(4 * 1024 * 1024),
        }
    }

    #[test]
    fn parse_meminfo_realistic() {
        assert_eq!(parse_meminfo(MEMINFO), (Some(32768000), Some(16777216)));
    }

    #[test]
    fn parse_meminfo_missing_available() {
        let text = "MemTotal:       32768000 kB\nMemFree:         1234567 kB\n";
        assert_eq!(parse_meminfo(text), (Some(32768000), None));
    }

    #[test]
    fn parse_meminfo_rejects_garbage_and_wrong_unit() {
        assert_eq!(parse_meminfo("hello\nworld"), (None, None));
        assert_eq!(parse_meminfo("MemAvailable: abc kB"), (None, None));
        assert_eq!(parse_meminfo("MemAvailable: 100 MB"), (None, None));
    }

    #[test]
    fn parse_loadavg_valid() {
        assert_eq!(parse_loadavg("0.52 0.58 0.59 1/345 12345\n"), Some(0.52));
        assert_eq!(parse_loadavg("0\n"), Some(0.0));
    }

    #[test]
    fn parse_loadavg_rejects_garbage_negative_and_nonfinite() {
        assert_eq!(parse_loadavg(""), None);
        assert_eq!(parse_loadavg("garbage 0.1"), None);
        assert_eq!(parse_loadavg("-1.00 0 0"), None);
        assert_eq!(parse_loadavg("NaN 0 0"), None);
        assert_eq!(parse_loadavg("inf 0 0"), None);
    }

    #[test]
    fn parse_df_pk_realistic() {
        assert_eq!(parse_df_pk(DF), Some(52913456));
        let trailing = format!("{DF}\n\n");
        assert_eq!(parse_df_pk(&trailing), Some(52913456));
    }

    #[test]
    fn parse_df_pk_rejects_garbage() {
        assert_eq!(parse_df_pk(""), None);
        assert_eq!(parse_df_pk("not a df output at all"), None);
        assert_eq!(parse_df_pk(DF.lines().next().unwrap()), None);
        assert_eq!(
            parse_df_pk("Filesystem 1K-blocks Used Available\n/dev/x 1 2 n/a"),
            None
        );
    }

    #[test]
    fn sample_missing_path_reports_unknown_disk() {
        let s = sample(Path::new("/definitely/not/a/real/path/for/host_probe"));
        assert_eq!(s.disk_free_kib, None);
        assert!(s.cpus >= 1);
    }

    #[test]
    fn admit_ok() {
        assert_eq!(ResourceBudget::default().admit(&healthy()), Ok(()));
    }

    #[test]
    fn admit_low_memory() {
        let mut s = healthy();
        s.mem_available_kib = Some(1000);
        let err = ResourceBudget::default().admit(&s).unwrap_err();
        assert_eq!(err, vec!["mem_available 1000 KiB < 4194304 KiB".to_owned()]);
    }

    #[test]
    fn admit_high_load() {
        let mut s = healthy();
        s.load1 = Some(4.8);
        let err = ResourceBudget::default().admit(&s).unwrap_err();
        assert_eq!(err, vec!["load1/cpu 1.20 > 0.85".to_owned()]);
    }

    #[test]
    fn admit_low_disk() {
        let mut s = healthy();
        s.disk_free_kib = Some(10);
        let err = ResourceBudget::default().admit(&s).unwrap_err();
        assert_eq!(err, vec!["disk_free 10 KiB < 2097152 KiB".to_owned()]);
    }

    #[test]
    fn admit_unknown_fields_fail_closed() {
        let s = HostSample {
            mem_total_kib: None,
            mem_available_kib: None,
            load1: None,
            cpus: 2,
            disk_free_kib: None,
        };
        let err = ResourceBudget::default().admit(&s).unwrap_err();
        assert_eq!(
            err,
            vec![
                "mem_available unknown".to_owned(),
                "load1 unknown".to_owned(),
                "disk_free unknown".to_owned(),
            ]
        );
    }

    #[test]
    fn admit_lists_every_violation() {
        let s = HostSample {
            mem_available_kib: Some(0),
            disk_free_kib: Some(0),
            ..healthy()
        };
        let err = ResourceBudget::default().admit(&s).unwrap_err();
        assert_eq!(err.len(), 2);
    }

    #[test]
    fn admit_normalises_load_by_cpus() {
        let budget = ResourceBudget::default();
        let mut s = healthy();
        s.load1 = Some(4.0);
        s.cpus = 8;
        assert_eq!(budget.admit(&s), Ok(()));
        s.cpus = 4;
        assert!(budget.admit(&s).is_err());
        s.cpus = 0;
        assert!(budget.admit(&s).is_err(), "cpus 0 is treated as 1");
    }
}

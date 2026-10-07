//! Offline audit of frozen PVP6 CPU diagnostic CSV; no runtime dependencies.
#![forbid(unsafe_code)]
use std::{collections::BTreeMap, fs, path::Path};
const SOURCE: &str = "ffad90f795ccd806998efe07f347ba303def5cfa";
const ARMS: [&str; 5] = [
    "PASCAL_ANF_MATERIALIZE",
    "SPARSE_ANF_SCAN",
    "SUBMASK_COEFFICIENT_QUERY",
    "GENERIC_TRUTH_MATERIALIZE",
    "RESIDENT_TRUTH_TABLE",
];
const CELLS: [(usize, &str, usize, usize); 20] = [
    (4, "AFFINE_UNIFORM", 512, 1),
    (32, "AFFINE_UNIFORM", 512, 1),
    (256, "AFFINE_UNIFORM", 512, 1),
    (1024, "AFFINE_UNIFORM", 512, 1),
    (256, "AFFINE_UNIFORM", 128, 1),
    (256, "AFFINE_UNIFORM", 128, 4),
    (256, "AFFINE_UNIFORM", 128, 16),
    (256, "AFFINE_UNIFORM", 512, 4),
    (256, "AFFINE_UNIFORM", 512, 16),
    (256, "AFFINE_UNIFORM", 2048, 1),
    (256, "AFFINE_UNIFORM", 2048, 4),
    (256, "AFFINE_UNIFORM", 2048, 16),
    (256, "LOW_POPCOUNT", 128, 1),
    (256, "LOW_POPCOUNT", 512, 1),
    (256, "HIGH_POPCOUNT", 128, 1),
    (256, "HIGH_POPCOUNT", 512, 1),
    (4, "AFFINE_UNIFORM", 32, 1),
    (4, "LOW_POPCOUNT", 32, 1),
    (4, "HIGH_POPCOUNT", 32, 1),
    (1024, "AFFINE_UNIFORM", 2048, 16),
];
type Row<'a> = BTreeMap<&'a str, &'a str>;
fn row(line: &str) -> Result<(&str, Row<'_>), String> {
    let mut parts = line.split(',');
    let kind = parts.next().ok_or("empty row")?;
    let mut values = BTreeMap::new();
    for field in parts {
        let (key, value) = field.split_once('=').ok_or("invalid field")?;
        if values.insert(key, value).is_some() {
            return Err("duplicate field".into());
        }
    }
    Ok((kind, values))
}
fn value<'a>(r: &Row<'a>, key: &str) -> Result<&'a str, String> {
    r.get(key).copied().ok_or_else(|| format!("missing {key}"))
}
fn number(r: &Row<'_>, key: &str) -> Result<u128, String> {
    value(r, key)?.parse().map_err(|_| format!("invalid {key}"))
}
fn require(ok: bool, label: &str) -> Result<(), String> {
    if ok {
        Ok(())
    } else {
        Err(label.into())
    }
}
fn median(times: &[u128]) -> u128 {
    let mut v = times.to_vec();
    v.sort_unstable();
    (v[9] + v[10]) / 2
}
fn p95(times: &[u128]) -> u128 {
    let mut v = times.to_vec();
    v.sort_unstable();
    v[18]
}
fn audit(text: &str, process: usize) -> Result<Vec<(usize, usize, u128, u128)>, String> {
    require(text.ends_with('\n'), "missing final LF")?;
    let mut samples = 0_usize;
    let mut summaries = 0_usize;
    let mut words = 0_u128;
    let mut headers = 0;
    let mut ends = 0;
    let mut caps = 0;
    let mut exits = 0;
    let mut promotion = 0;
    let mut times: Vec<[Vec<u128>; 5]> = (0..20)
        .map(|_| std::array::from_fn(|_| Vec::new()))
        .collect();
    let mut result = Vec::new();
    for line in text.lines() {
        if line == "PROMOTION,BLOCKED,synthetic_shared_CPU_diagnostic_only" {
            promotion += 1;
            continue;
        }
        let (kind, r) = row(line)?;
        match kind {
            "CONTROL_CGROUP" => {
                require(
                    r.len() == 3 && samples == 0 && headers == 0,
                    "control header shape/order",
                )?;
                require(
                    number(&r, "memory_max")? == 1073741824
                        && value(&r, "cpu_max")? == "100000 100000"
                        && number(&r, "uid")? > 0,
                    "control limits",
                )?;
                caps += 1;
            }
            "PVP6_TASK_BEGIN" => {
                require(
                    r.len() == 9 && samples == 0 && caps == 1,
                    "begin shape/order",
                )?;
                require(
                    number(&r, "protocol")? == 1
                        && number(&r, "process")? == process as u128
                        && value(&r, "source")? == SOURCE
                        && value(&r, "arch")? == "aarch64",
                    "begin identity",
                )?;
                require(
                    number(&r, "cells")? == 20
                        && number(&r, "arms")? == 5
                        && number(&r, "warmups")? == 5
                        && number(&r, "repetitions")? == 20
                        && value(&r, "admission")? == "shared_CPU_diagnostic",
                    "begin contract",
                )?;
                headers += 1;
            }
            "PVP6_SAMPLE" => {
                require(
                    r.len() == 14 && headers == 1 && ends == 0 && samples < 2500,
                    "sample shape/coverage",
                )?;
                let cell = samples / 125;
                let rep = (samples % 125) / 5;
                let position = samples % 5;
                let warm = rep < 5;
                let relative = if warm { rep } else { rep - 5 };
                let offset = (process - 1 + cell + relative % 5) % 5;
                let step = if (relative / 5) % 2 == 0 {
                    position
                } else {
                    (5 - position) % 5
                };
                let arm = (offset + step) % 5;
                let (density, schedule, q, reuse) = CELLS[cell];
                require(
                    number(&r, "process")? == process as u128
                        && number(&r, "cell")? == cell as u128
                        && number(&r, "repetition")? == relative as u128
                        && number(&r, "position")? == position as u128,
                    "sample key/order",
                )?;
                require(
                    number(&r, "density")? == density as u128
                        && value(&r, "schedule")? == schedule
                        && number(&r, "q")? == q as u128
                        && number(&r, "reuse")? == reuse as u128,
                    "sample fixture",
                )?;
                require(
                    number(&r, "warmup")? == u128::from(warm) && value(&r, "arm")? == ARMS[arm],
                    "sample arm/order",
                )?;
                let elapsed = number(&r, "ns")?;
                require(
                    elapsed > 0
                        && elapsed <= 120_000_000_000
                        && number(&r, "words")? == (q * reuse) as u128
                        && number(&r, "mismatches")? == 0
                        && number(&r, "exact")? == 1,
                    "sample exactness/time",
                )?;
                words += (q * reuse) as u128;
                if !warm {
                    times[cell][arm].push(elapsed);
                }
                samples += 1;
            }
            "PVP6_SUMMARY" => {
                let cell = summaries / 5;
                let arm = summaries % 5;
                require(
                    r.len() == 7 && cell < 20 && samples == (cell + 1) * 125,
                    "summary shape/order",
                )?;
                require(
                    number(&r, "process")? == process as u128
                        && number(&r, "cell")? == cell as u128
                        && value(&r, "arm")? == ARMS[arm]
                        && number(&r, "samples")? == 20,
                    "summary key/count",
                )?;
                require(times[cell][arm].len() == 20, "missing measured samples")?;
                let m = median(&times[cell][arm]);
                let p = p95(&times[cell][arm]);
                require(
                    number(&r, "median_ns")? == m && number(&r, "p95_ns")? == p,
                    "summary arithmetic",
                )?;
                require(
                    value(&r, "lifecycle")?
                        == if arm == 4 {
                            "resident_lookup"
                        } else {
                            "activation_plus_query"
                        },
                    "lifecycle",
                )?;
                result.push((cell, arm, m, p));
                summaries += 1;
            }
            "PVP6_TASK_END" => {
                require(
                    r.len() == 5 && samples == 2500 && summaries == 100,
                    "end coverage",
                )?;
                require(
                    number(&r, "process")? == process as u128
                        && number(&r, "measured")? == 2000
                        && number(&r, "warmups")? == 500
                        && number(&r, "verified_words")? == words
                        && number(&r, "mismatches")? == 0,
                    "end accounting",
                )?;
                ends += 1;
            }
            "CONTROL_END" => {
                require(r.len() == 3 && ends == 1, "control end order")?;
                require(
                    number(&r, "process")? == process as u128
                        && number(&r, "exit")? == 0
                        && number(&r, "memory_peak")? <= 1073741824,
                    "control exit/peak",
                )?;
                exits += 1;
            }
            _ => return Err(format!("unknown row {kind}")),
        }
    }
    require(
        (headers, ends, caps, exits, promotion) == (1, 1, 1, 1, 1)
            && samples == 2500
            && summaries == 100,
        "incomplete/duplicate corpus",
    )?;
    Ok(result)
}
fn run(root: &Path) -> Result<(), String> {
    require(
        fs::read_to_string(root.join("source-revision.txt"))
            .map_err(|e| e.to_string())?
            .trim()
            == SOURCE,
        "source revision",
    )?;
    require(
        fs::read_to_string(root.join("controller-exit.txt"))
            .map_err(|e| e.to_string())?
            .trim()
            == "0",
        "controller exit",
    )?;
    require(
        fs::read_to_string(root.join("campaign-status.txt")).map_err(|e| e.to_string())?
            == "COMPLETE,processes=10\n",
        "campaign incomplete",
    )?;
    require(
        fs::read_to_string(root.join("control-revision.txt"))
            .map_err(|e| e.to_string())?
            .trim()
            == "f3010810b542fd0290580e07d1afe537d8aaa790",
        "control revision",
    )?;
    require(
        fs::read_to_string(root.join("executable-sha256.txt"))
            .map_err(|e| e.to_string())?
            .split_whitespace()
            .next()
            == Some("48568b82730f7539cab10a5a6b1cca895e625ec99f088ef102dd9c27c859ef5e"),
        "executable identity",
    )?;
    let compiler = fs::read_to_string(root.join("compiler.txt")).map_err(|e| e.to_string())?;
    require(
        compiler.starts_with("rustc 1.89.0 ")
            && compiler.contains("host: aarch64-unknown-linux-gnu"),
        "compiler identity",
    )?;
    let mut results = Vec::new();
    for process in 1..=10 {
        require(
            fs::read_to_string(root.join(format!("process-{process}-exit.txt")))
                .map_err(|e| e.to_string())?
                .trim()
                == "0",
            "process exit",
        )?;
        let text = fs::read_to_string(root.join(format!("process-{process}.csv")))
            .map_err(|e| e.to_string())?;
        for (cell, arm, m, p) in audit(&text, process)? {
            results.push((process, cell, arm, m, p));
        }
    }
    println!("PVP6_AUDIT,processes=10,measured=20000,warmups=5000,summaries=1000,mismatches=0");
    for (process, cell, arm, m, p) in results {
        println!("SUMMARY,{process},{cell},{},{m},{p}", ARMS[arm]);
    }
    Ok(())
}
fn main() {
    let arg = std::env::args()
        .nth(1)
        .expect("campaign directory required");
    if let Err(error) = run(Path::new(&arg)) {
        eprintln!("PVP6_AUDIT_REJECTED,{error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_and_malformed_fields() {
        assert!(row("X,a=1,a=2").is_err());
        assert!(row("X,no_separator").is_err());
    }
    #[test]
    fn rejects_partial_unknown_and_wrong_limits() {
        assert!(audit("", 1).is_err());
        assert!(audit("UNKNOWN,a=1\n", 1).is_err());
        assert!(audit(
            "CONTROL_CGROUP,memory_max=999,cpu_max=100000 100000,uid=2\n",
            1
        )
        .is_err());
    }
    #[test]
    fn exact_percentiles() {
        let t = (1..=20).collect::<Vec<_>>();
        assert_eq!(median(&t), 10);
        assert_eq!(p95(&t), 19);
    }
}

#[cfg(test)]
mod complete_corpus_tests {
    use super::*;
    fn fixture() -> String {
        let mut text = format!("CONTROL_CGROUP,memory_max=1073741824,cpu_max=100000 100000,uid=2\nPVP6_TASK_BEGIN,protocol=1,process=1,source={SOURCE},arch=aarch64,cells=20,arms=5,warmups=5,repetitions=20,admission=shared_CPU_diagnostic\n");
        let mut words = 0;
        for (cell, (density, schedule, q, reuse)) in CELLS.iter().copied().enumerate() {
            for repetition in 0..25 {
                let warm = repetition < 5;
                let relative = if warm { repetition } else { repetition - 5 };
                for position in 0..5 {
                    let step = if (relative / 5) % 2 == 0 {
                        position
                    } else {
                        (5 - position) % 5
                    };
                    let arm = (cell + relative % 5 + step) % 5;
                    words += q * reuse;
                    text.push_str(&format!("PVP6_SAMPLE,process=1,cell={cell},density={density},schedule={schedule},q={q},reuse={reuse},warmup={},repetition={relative},position={position},arm={},ns=100,words={},mismatches=0,exact=1\n",u8::from(warm),ARMS[arm],q*reuse));
                }
            }
            for (arm, name) in ARMS.iter().enumerate() {
                text.push_str(&format!("PVP6_SUMMARY,process=1,cell={cell},arm={name},samples=20,median_ns=100,p95_ns=100,lifecycle={}\n",if arm==4 {"resident_lookup"} else {"activation_plus_query"}));
            }
        }
        text.push_str(&format!("PVP6_TASK_END,process=1,measured=2000,warmups=500,verified_words={words},mismatches=0\nPROMOTION,BLOCKED,synthetic_shared_CPU_diagnostic_only\nCONTROL_END,process=1,exit=0,memory_peak=4096\n"));
        text
    }
    #[test]
    fn accepts_complete_declared_corpus() {
        assert_eq!(audit(&fixture(), 1).unwrap().len(), 100);
    }
    #[test]
    fn rejects_full_corpus_mutations() {
        let original = fixture();
        for (before, after) in [
            ("exact=1", "exact=0"),
            ("words=512", "words=511"),
            ("median_ns=100", "median_ns=99"),
            ("mismatches=0", "mismatches=1"),
            (
                "lifecycle=resident_lookup",
                "lifecycle=activation_plus_query",
            ),
            ("source=ffad", "source=aaaa"),
            ("memory_peak=4096", "memory_peak=1073741825"),
        ] {
            assert!(
                audit(&original.replacen(before, after, 1), 1).is_err(),
                "{before}"
            );
        }
        assert!(audit(&original, 2).is_err());
        let lines = original.lines().collect::<Vec<_>>();
        let missing = lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 2)
            .map(|(_, s)| *s)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert!(audit(&missing, 1).is_err());
        let duplicate = lines[..3]
            .iter()
            .chain(lines[2..].iter())
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert!(audit(&duplicate, 1).is_err());
    }
}

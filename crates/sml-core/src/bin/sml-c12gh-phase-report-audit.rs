//! Audit the six GitHub-hosted A6-C12g/h phase-transition summary exports.
//!
//! These are three hardware/timing replicas of the SAME fixed seed sets, NOT
//! three independent statistical populations. The canonical FROZEN and TEST
//! case traces remain archived by GitHub Actions (run IDs in report).
//! This offline verification does not itself reproduce the original learners.

use std::collections::{BTreeMap, HashSet};

const CSV: &str = include_str!("../../../../research/results/A6-C12GH-PHASE-SUMMARY.csv");
const EXPECTED_COLUMNS: usize = 22;

fn label_eta(rate: &str) -> Result<f64, String> {
    match rate {
        "0" => Ok(0.0),
        "1/32" => Ok(1.0 / 32.0),
        "1/16" => Ok(1.0 / 16.0),
        "1/8" => Ok(1.0 / 8.0),
        "1/5" => Ok(0.2),
        "3/10" => Ok(0.3),
        "2/5" => Ok(0.4),
        _ => Err(format!("unregistered noise rate {rate}")),
    }
}

fn scheduled_h(task: &str, rate: &str) -> Result<[usize; 3], String> {
    match (task, rate) {
        ("xor_xor_xor", "0") => Ok([1, 2, 4]),
        ("xor_xor_xor", "1/5") => Ok([2, 4, 8]),
        ("xor_xor_xor", "3/10") => Ok([8, 16, 32]),
        ("xor_xor_xor", "2/5") => Ok([128, 256, 512]),
        ("and_xor_xor", "0") => Ok([1, 2, 4]),
        ("and_xor_xor", "1/5") => Ok([8, 16, 32]),
        ("and_xor_xor", "3/10") => Ok([32, 64, 128]),
        ("and_xor_xor", "2/5") => Ok([128, 256, 512]),
        _ => Err(format!("unregistered H experiment: {task} {rate}")),
    }
}

fn wilson_95(success: usize, trials: usize) -> (f64, f64) {
    let n = trials as f64;
    let z: f64 = 1.959_963_984_540_054;
    let p = success as f64 / n;
    let z2 = z * z;
    let center = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let radius =
        z / (1.0 + z2 / n) * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    ((center - radius).max(0.0), (center + radius).min(1.0))
}

fn audit(csv: &str) -> Result<(usize, usize), String> {
    let mut lines = csv.lines();
    let header = lines.next().ok_or("missing CSV header")?;
    if header.split(',').count() != EXPECTED_COLUMNS || !header.starts_with("study,task,label_noise,method,proxy_words,proxy_bits,successes,trials,") {
        return Err("unexpected CSV schema".into());
    }
    let mut seen = HashSet::new();
    let mut groups = BTreeMap::<(String, String, String, String), Vec<(usize, usize, usize)>>::new();
    let mut count_g = 0usize;
    let mut count_h = 0usize;

    for (line_number, line) in lines.enumerate() {
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != EXPECTED_COLUMNS {
            return Err(format!("line {}: expected 22 CSV columns", line_number + 2));
        }
        let (study, task, rate, method) = (fields[0], fields[1], fields[2], fields[3]);
        let words: usize = fields[4].parse().map_err(|_| "bad words")?;
        let bits: usize = fields[5].parse().map_err(|_| "bad bits")?;
        let successes: usize = fields[6].parse().map_err(|_| "bad success count")?;
        let trials: usize = fields[7].parse().map_err(|_| "bad trial count")?;
        let train_successes: usize = fields[8].parse().map_err(|_| "bad train count")?;
        let lo: f64 = fields[9].parse().map_err(|_| "bad Wilson lower")?;
        let hi: f64 = fields[10].parse().map_err(|_| "bad Wilson upper")?;
        let rho: f64 = fields[11].parse().map_err(|_| "bad proxy rho")?;
        let estimate: f64 = fields[12].parse().map_err(|_| "bad theory estimate")?;
        if !["xor_xor_xor", "and_xor_xor"].contains(&task)
            || !["walsh3_stream", "hybrid_w2_mitm", "gauss_clean"].contains(&method)
        {
            return Err(format!("line {}: invalid task or method", line_number + 2));
        }
        let correct_budgets: &[usize] = match study {
            "G" => &[8, 16, 32, 64, 128, 256, 512],
            "H" => &[1, 2, 4, 8, 16, 32, 64, 128, 256, 512],
            _ => return Err(format!("line {}: invalid study", line_number + 2)),
        };
        if !correct_budgets.contains(&words) || bits != words * 64 {
            return Err(format!("line {}: invalid registered budget", line_number + 2));
        }
        let eta = label_eta(rate)?;
        if (study == "G" && !["0", "1/32", "1/16", "1/8", "1/5"].contains(&rate))
            || (study == "H" && !["0", "1/5", "3/10", "2/5"].contains(&rate))
            || (method == "gauss_clean" && rate != "0")
        {
            return Err(format!("line {}: unregistered design cell", line_number + 2));
        }
        let expected_trials = if study == "H" && scheduled_h(task, rate)?.contains(&words) {
            16
        } else {
            8
        };
        if trials != expected_trials || successes > trials || train_successes > trials {
            return Err(format!("line {}: invalid 8/16 seed census", line_number + 2));
        }

        let rho_expected = (1.0 - 2.0 * eta).powi(2);
        let amp = if task == "xor_xor_xor" { rho_expected } else { rho_expected / 2.0 };
        let expected_theory = 2.0 * (2.0 * 317_440.0f64).ln() / (amp * amp);
        if (rho - rho_expected).abs() > 0.000_001
            || (estimate - expected_theory).abs() > 0.000_002
        {
            return Err(format!("line {}: theory mismatch", line_number + 2));
        }
        let (expected_lo, expected_hi) = wilson_95(successes, trials);
        if (lo - expected_lo).abs() > 0.000_002 || (hi - expected_hi).abs() > 0.000_002 {
            return Err(format!("line {}: Wilson mismatch", line_number + 2));
        }
        for field in fields.iter().take(EXPECTED_COLUMNS).skip(13) {
            let runtime: f64 = field.parse().map_err(|_| "bad runtime")?;
            if !runtime.is_finite() || runtime <= 0.0 {
                return Err(format!("line {}: invalid timing", line_number + 2));
            }
        }
        let unique_key = (study, task, rate, method, words);
        if !seen.insert(unique_key) {
            return Err(format!("line {}: duplicate experimental cell", line_number + 2));
        }
        groups.entry((study.into(), task.into(), rate.into(), method.into()))
            .or_default().push((words, successes, trials));
        match study {
            "G" => count_g += 1,
            "H" => count_h += 1,
            _ => unreachable!(),
        }
    }

    if count_g != 154 || count_h != 180 || groups.len() != 40 {
        return Err(format!("bad design census: G={count_g}, H={count_h}, groups={}", groups.len()));
    }
    for ((study, _, _, method), values) in &groups {
        let registered = if study == "G" { 7 } else { 10 };
        if values.len() != registered {
            return Err("missing or duplicate budget per group".into());
        }
        if method == "gauss_clean" && values.iter().any(|(_, _, trials)| *trials != if study == "G" { 8 } else { 16 }) {
            // In H clean Gauss only the three theory-window budgets use 16 seeds.
            if study == "G" {
                return Err("G clean Gauss census mismatch".into());
            }
        }
    }

    // Mandatory negative controls and theory-guided boundary transitions.
    for (task, rate, method, expected_first_words) in [
        ("xor_xor_xor", "0", "walsh3_stream", Some(1usize)),
        ("xor_xor_xor", "1/5", "walsh3_stream", Some(4)),
        ("xor_xor_xor", "3/10", "walsh3_stream", Some(32)),
        ("xor_xor_xor", "2/5", "walsh3_stream", Some(256)),
        ("xor_xor_xor", "2/5", "hybrid_w2_mitm", None),
        ("xor_xor_xor", "3/10", "hybrid_w2_mitm", None),
        ("and_xor_xor", "1/5", "walsh3_stream", Some(16)),
        ("and_xor_xor", "3/10", "walsh3_stream", Some(128)),
        ("and_xor_xor", "3/10", "hybrid_w2_mitm", Some(64)),
        ("and_xor_xor", "2/5", "walsh3_stream", None),
        ("and_xor_xor", "2/5", "hybrid_w2_mitm", None),
        ("and_xor_xor", "0", "gauss_clean", None),
    ] {
        let key = ("H".into(), task.into(), rate.into(), method.into());
        let first = groups.get(&key).ok_or("missing mandatory group")?.iter()
            .filter(|(_, success, trials)| 8 * success >= 7 * trials)
            .map(|(words, _, _)| *words).min();
        if first != expected_first_words {
            return Err(format!("H threshold regression for {task}/{rate}/{method}: {first:?}"));
        }
    }
    Ok((count_g, count_h))
}

fn main() {
    let (g, h) = audit(CSV).expect("A6-C12g/h qualified CSV audit failed");
    println!("A6C12GH_AUDIT_OK,c12g_rows={g},c12h_rows={h},statistical_replications_not_tripled=true");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_fixture_recomputes_wilson_theory_and_phase_boundaries() {
        assert_eq!(audit(CSV), Ok((154, 180)));
    }

    #[test]
    fn duplicated_or_missing_experiment_is_rejected() {
        let mut doubled = CSV.to_string();
        doubled.push_str(CSV.lines().nth(1).unwrap());
        doubled.push('\n');
        assert!(audit(&doubled).is_err());
        let missing = CSV.lines().filter(|line| !line.starts_with("H,xor_xor_xor,2/5,hybrid_w2_mitm,512,")).collect::<Vec<_>>().join("\n");
        assert!(audit(&missing).is_err());
    }
}

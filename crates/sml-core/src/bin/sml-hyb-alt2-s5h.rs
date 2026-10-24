//! H-ALT-2: streaming GB-11 S5h proof harness for two competing bases.
//! S5h data stay local to the authorized runner; no TEST or MOR operations.
//! This binary is deliberately NOT a performance or BPC result by itself.

use sml_core::{
    elastic_ngram_paged_sha::{hex, Sha256},
    gb11_p1_stream::{read_trace, TraceRecord},
    hyb_alt_pairwise_residual::{Basis, Head, Master, HEAD_BYTES, SCALES, SEEDS, THRESHOLDS},
    hyb_h1_residual::CountTable,
};
use std::{
    collections::BTreeSet,
    error::Error,
    fs::{self, File},
    io::Write,
    path::Path,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const GO_MARGIN_BPC: f64 = 0.02;
const CONTROL_LAMBDAS: [f64; 4] = [0.0, 0.25, 0.5, 1.0];

#[derive(Debug)]
struct Profile {
    docs: BTreeSet<u32>,
    rows: u64,
    bpc: f64,
    hash: String,
}

fn reported(json: &str, field: &str, attribute: &str) -> Result<String> {
    let row = json
        .lines()
        .find(|line| line.trim().starts_with(&format!("\"{field}\"")))
        .ok_or_else(|| format!("trace.json is missing {field}"))?;
    let (_, value) = row
        .split_once(&format!("\"{attribute}\": "))
        .ok_or_else(|| format!("trace.json is missing {attribute} for {field}"))?;
    if let Some(quoted) = value.strip_prefix('"') {
        return Ok(quoted
            .split_once('"')
            .ok_or("malformed quoted attribute")?
            .0
            .to_owned());
    }
    Ok(value
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .next()
        .ok_or("missing numeric attribute")?
        .to_owned())
}

fn audit(path: &Path, split: u8, json: &str, field: &str) -> Result<Profile> {
    let mut sha = Sha256::default();
    let mut docs = BTreeSet::new();
    let mut last = None;
    let mut fault = None;
    let (rows, bits) = read_trace(&mut File::open(path)?, |record| {
        if fault.is_some() {
            return;
        }
        let index = (record.doc_index, record.position);
        if record.split != split || last.is_some_and(|previous| index <= previous) {
            fault = Some(format!(
                "unsorted, repeated or wrong split record at {index:?}"
            ));
            return;
        }
        last = Some(index);
        docs.insert(record.doc_index);
        sha.update(&record.encode());
    })?;
    if let Some(message) = fault {
        return Err(message.into());
    }
    if rows == 0 {
        return Err("empty trace".into());
    }
    let hash = hex(&sha.finish());
    if hash != reported(json, field, "sha256")? {
        return Err(format!("{field} SHA256 does not match trace.json").into());
    }
    let bpc = bits / rows as f64;
    let expected: f64 = reported(json, field, "bpc")?.parse()?;
    if (bpc - expected).abs() > 1e-6 {
        return Err(format!("{field} BPC {bpc} differs from expected {expected}").into());
    }
    Ok(Profile {
        docs,
        rows,
        bpc,
        hash,
    })
}

struct Selection {
    basis: Basis,
    tau: f32,
    scale: i32,
    head: Head,
    hold_bpc: f64,
}

fn select(master: &Master, basis: Basis, hold: &[TraceRecord]) -> Result<Selection> {
    let mut best = None;
    for tau in THRESHOLDS {
        for scale in SCALES {
            let head = master.export(tau, scale)?;
            let bpc = hold.iter().map(|r| head.cost(r)).sum::<f64>() / hold.len() as f64;
            let candidate = Selection {
                basis,
                tau,
                scale,
                head,
                hold_bpc: bpc,
            };
            if best
                .as_ref()
                .is_none_or(|previous: &Selection| candidate.hold_bpc < previous.hold_bpc)
            {
                best = Some(candidate);
            }
        }
    }
    Ok(best.ok_or("invalid H-ALT-2 hyperparameter grid")?)
}

fn run(trace: &Path, output: &Path) -> Result<String> {
    let json = fs::read_to_string(trace.join("trace.json"))?;
    if !json.contains("SML_GB11_P1_TRACE_V1")
        || !json.contains("\"record_bytes\": 40")
        || !json.contains("\"test\": \"UNOPENED\"")
    {
        return Err("unqualified P1 trace metadata or TEST already opened".into());
    }
    let train = audit(&trace.join("trace-train.bin"), 0, &json, "train_trace")?;
    let dev = audit(&trace.join("trace-dev.bin"), 1, &json, "dev_trace")?;
    if !train.docs.is_disjoint(&dev.docs) {
        return Err("TRAIN/DEV document overlap".into());
    }
    let tail = train.docs.len().div_ceil(16);
    if train.docs.len() < 16 || tail == 0 {
        return Err("not enough disjoint TRAIN documents for TRAIN-HOLD".into());
    }
    let hold_from = *train
        .docs
        .iter()
        .nth(train.docs.len() - tail)
        .ok_or("missing TRAIN-HOLD boundary")?;

    let mut masters: Vec<(u64, Basis, Master)> = SEEDS
        .iter()
        .flat_map(|&seed| {
            Basis::ALL
                .into_iter()
                .map(move |basis| (seed, basis, Master::new(seed, basis)))
        })
        .collect();
    let mut count = CountTable::with_bytes(HEAD_BYTES);
    let mut hold = Vec::new();
    let mut error = None;
    read_trace(&mut File::open(trace.join("trace-train.bin"))?, |r| {
        if r.doc_index >= hold_from {
            hold.push(*r);
        } else {
            for (_, _, master) in &mut masters {
                master.train(r);
            }
            count.train(r);
        }
        if r.split != 0 {
            error = Some("fit stream contains non-TRAIN label");
        }
    })?;
    if let Some(message) = error {
        return Err(message.into());
    }
    if hold.is_empty() {
        return Err("empty TRAIN-HOLD".into());
    }
    let (lambda, count_hold) = CONTROL_LAMBDAS
        .iter()
        .map(|&l| {
            (
                l,
                hold.iter().map(|r| count.cost(r, l)).sum::<f64>() / hold.len() as f64,
            )
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .ok_or("no Boolean control weight")?;

    let mut selected = Vec::new();
    for seed in SEEDS {
        let best = masters
            .iter()
            .filter(|(s, _, _)| *s == seed)
            .map(|(_, basis, master)| select(master, *basis, &hold))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .min_by(|a, b| a.hold_bpc.total_cmp(&b.hold_bpc))
            .ok_or("empty seed candidates")?;
        selected.push((seed, best));
    }
    drop(hold);
    drop(masters);

    let mut a0_sum = 0.0;
    let mut a0c_sum = 0.0;
    let mut head_bits = vec![0.0; selected.len()];
    let mut nanos = [0_u128; 3];
    let mut mismatches = 0_u64;
    let mut observed = 0_u64;
    read_trace(&mut File::open(trace.join("trace-dev.bin"))?, |r| {
        let t = Instant::now();
        a0_sum += r.cost_bits();
        nanos[0] += t.elapsed().as_nanos();
        let t = Instant::now();
        a0c_sum += count.cost(r, lambda);
        nanos[1] += t.elapsed().as_nanos();
        for (i, (_, winner)) in selected.iter().enumerate() {
            let t = Instant::now();
            head_bits[i] += winner.head.cost(r);
            if i == 0 {
                nanos[2] += t.elapsed().as_nanos();
            }
            // Fixed systematic sample; exhaustive oracle on 4096 patterns
            // is already in the library tests on both bases.
            if observed % 4096 == 0 {
                for depth in 0..8 {
                    if winner.head.score(r, depth) != winner.head.oracle_score(r, depth) {
                        mismatches += 1;
                    }
                }
            }
        }
        observed += 1;
    })?;
    if observed != dev.rows || mismatches > 0 {
        return Err(
            format!("DEV count or integer oracle failure: {observed}, {mismatches}").into(),
        );
    }
    let a0_bpc = a0_sum / observed as f64;
    if (a0_bpc - dev.bpc).abs() > 1e-6 {
        return Err("DEV baseline changed while evaluating H-ALT-2".into());
    }
    let a0c_bpc = a0c_sum / observed as f64;
    let a1_bpc: Vec<f64> = head_bits
        .iter()
        .map(|&bits| bits / observed as f64)
        .collect();
    let spread = (a1_bpc[0] - a1_bpc[1]).abs();
    let reference = a0_bpc.min(a0c_bpc);
    let go = a1_bpc
        .iter()
        .all(|&b| b < reference - GO_MARGIN_BPC && reference - b > 2.0 * spread);
    let micros = |ns: u128| ns as f64 / 1000.0 / observed as f64;

    let mut result = format!(
        "schema\tH_ALT2_S5H_V1\nverdict\t{}\nsource_trace_sha256\t{}\ntrain_trace_sha256\t{}\ndev_trace_sha256\t{}\ntrain_records\t{}\ndev_records\t{}\nhold_documents\t{}\ncontrol_bytes\t{}\nhead_weight_bytes\t{}\na0_bpc\t{:.9}\na0c_bpc\t{:.9}\na0c_lambda\t{}\na0c_hold_bpc\t{:.9}\nseed_spread_bpc\t{:.9}\nexact_integer_oracle_mismatches\t{}\nparity_sample_stride\t4096\nus_per_byte_a0\t{:.5}\nus_per_byte_a0c\t{:.5}\nus_per_byte_a1_seed11\t{:.5}\ntest\tUNOPENED\n",
        if go { "GO" } else { "NO_GO" },
        hex(&sml_core::elastic_ngram_paged_sha::sha256(json.as_bytes())),
        train.hash,
        dev.hash,
        train.rows,
        dev.rows,
        tail,
        count.bytes(),
        HEAD_BYTES,
        a0_bpc,
        a0c_bpc,
        lambda,
        count_hold,
        spread,
        mismatches,
        micros(nanos[0]),
        micros(nanos[1]),
        micros(nanos[2]),
    );
    for (i, (seed, winner)) in selected.iter().enumerate() {
        result += &format!(
            "seed{seed}_basis\t{}\nseed{seed}_tau\t{}\nseed{seed}_scale\t{}\nseed{seed}_hold_bpc\t{:.9}\nseed{seed}_dev_bpc\t{:.9}\nseed{seed}_active_weights\t{}\n",
            winner.basis.label(),
            winner.tau,
            winner.scale,
            winner.hold_bpc,
            a1_bpc[i],
            winner.head.active_weights(),
        );
    }
    // Write only aggregated evidence; raw traces and weight vectors stay private.
    fs::create_dir(output)?;
    fs::write(output.join("h-alt2.tsv"), &result)?;
    Ok(result)
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || args[1] == "--help" {
        eprintln!("usage: sml-hyb-alt2-s5h TRACE_DIR NEW_OUTPUT_DIR");
        std::process::exit(2);
    }
    match run(Path::new(&args[1]), Path::new(&args[2])) {
        Ok(report) => {
            print!("{report}");
            std::io::stdout().flush().expect("stdout flush");
        }
        Err(error) => {
            eprintln!("H_ALT2_NOT_QUALIFIED: {error}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_auditor_rejects_split_and_digest_corruption() {
        let root = std::env::temp_dir().join(format!("sml-halt2-unit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let sample = TraceRecord {
            doc_index: 100,
            position: 2,
            split: 0,
            target: 42,
            context: 0,
            word: 0,
            logits: [0; 8],
            confidence: 0,
        };
        let path = root.join("train");
        fs::write(&path, sample.encode()).unwrap();
        let digest = hex(&sml_core::elastic_ngram_paged_sha::sha256(&sample.encode()));
        let json = format!("  \"train_trace\": {{\"sha256\": \"{digest}\", \"bpc\": 8.000000000}}");
        assert_eq!(audit(&path, 0, &json, "train_trace").unwrap().rows, 1);
        assert!(audit(&path, 1, &json, "train_trace").is_err());
        assert!(audit(
            &path,
            0,
            &json.replace(&digest, &"0".repeat(64)),
            "train_trace"
        )
        .is_err());
        let mut torn = sample.encode().to_vec();
        torn.pop();
        fs::write(&path, &torn).unwrap();
        assert!(audit(&path, 0, &json, "train_trace").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_trace_parser_rejects_missing_attributes() {
        assert!(reported("{}", "dev_trace", "sha256").is_err());
        assert!(reported("  \"dev_trace\": {}", "dev_trace", "bpc").is_err());
    }
}

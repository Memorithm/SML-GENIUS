//! BAF-H4 CAL gate: score a **frozen** neural compressor with a native ANF
//! conditional head. No new real-text eval or TEST is opened by this program.
//!
//! Usage:
//!   cargo run --locked --release -p sml-core --bin sml-hyb-h4-baf-s5h -- \
//!     gate P1_H1D_OUTPUT H1D_HEAD_OUTPUT NEW_OUTPUT_DIRECTORY
//!   cargo run --locked -p sml-core --bin sml-hyb-h4-baf-s5h -- smoke
//!
//! The P1 output is from `sml-gb11-p1-s5h run-h1d`, and the H-1d heads
//! are the ones published in H-1d. Gate sees CAL-FIT (doc % 32 == 7) only;
//! CAL-SEL (doc % 32 == 23) is used as a *single final veto*.
//! EVAL2/3/4/5, the existing DEV and TEST are never opened.
//! Research only; GO of this CAL gate is NOT model-level superiority.

use sml_core::{
    elastic_ngram_paged_sha::{hex, Sha256},
    gb11_p1_stream::{TraceRecord, TRACE_RECORD_BYTES},
    hyb_baf_neural_gate::{fit_head_gate, HeadGateEvidence, HeadGateObservation, HeadGatePolicy},
    hyb_h1d_bank_head::{BankPlane, BankRecord, BANK_PLANE_BYTES},
    hyb_h3_engine::parse_h1d_head,
};
use std::{
    error::Error,
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
const HEAD_SEEDS: [u64; 2] = [11, 23];
const HEAD_HASH: [&str; 2] = [
    "b77e3f1301c414ff21246b3c5ad600b07e2adf44633a671ecd90fd1160e5e46e",
    "3c6192e8a248fb47680d941866e6cf22d6483636f539fa2aa4564430127c990f",
];
const P1_HASH: &str = "7f4c233d6c4d67e1d300068799f4653cfcaedda4158587478c99ec1d31c3c418";
const NULL_BPC_MARGIN: f64 = 0.005;
const NEURAL_BPC_MARGIN: f64 = 0.010;

fn digest_file(path: &Path) -> Result<String> {
    let mut file = BufReader::with_capacity(1 << 20, File::open(path)?);
    let mut sha = Sha256::default();
    let mut buf = [0u8; 65536];
    loop {
        let got = file.read(&mut buf)?;
        if got == 0 {
            break;
        }
        sha.update(&buf[..got]);
    }
    Ok(hex(&sha.finish()))
}

fn bpc(microbits: i128, rows: u64) -> f64 {
    microbits as f64 / 1e6 / rows as f64
}

fn apply_gate(p1_dir: &Path, h1d_dir: &Path, out: &Path) -> Result<()> {
    if out.exists() {
        return Err(format!(
            "output already exists (refuse overwrite): {}",
            out.display()
        )
        .into());
    }
    let p1 = p1_dir.join("bytes_q4");
    let weights = p1.join("weights.bin");
    let p1_hash = digest_file(&weights)?;
    if p1_hash != P1_HASH {
        return Err(format!("frozen P1 weights hash mismatch: {p1_hash}").into());
    }

    let mut loaded = Vec::with_capacity(2);
    let mut head_digests = Vec::with_capacity(2);
    for (index, &seed) in HEAD_SEEDS.iter().enumerate() {
        let path = h1d_dir.join(format!("head-a1d-seed{seed}.tsv"));
        let digest = digest_file(&path)?;
        if digest != HEAD_HASH[index] {
            return Err(format!("frozen H-1d head seed {seed} hash mismatch: {digest}").into());
        }
        let head = parse_h1d_head(&fs::read_to_string(path)?)
            .map_err(|e| format!("invalid frozen head seed {seed}: {e}"))?;
        loaded.push(head);
        head_digests.push(digest);
    }
    let traces_path = p1.join("trace-cal.bin");
    let banks_path = p1.join("trace-cal.banks.bin");
    let tr_size = fs::metadata(&traces_path)?.len();
    let side_size = fs::metadata(&banks_path)?.len();
    if tr_size == 0
        || tr_size % TRACE_RECORD_BYTES as u64 != 0
        || side_size != tr_size / TRACE_RECORD_BYTES as u64 * BANK_PLANE_BYTES as u64
    {
        return Err("CAL trace/sidecar missing, empty or length-misaligned".into());
    }
    let mut traces = BufReader::with_capacity(1 << 20, File::open(&traces_path)?);
    let mut banks = BufReader::with_capacity(1 << 20, File::open(&banks_path)?);
    let mut raw_trace = [0u8; TRACE_RECORD_BYTES];
    let mut raw_bank = [0u8; BANK_PLANE_BYTES];
    let mut train = [HeadGateEvidence::default(), HeadGateEvidence::default()];
    let mut validation = [HeadGateEvidence::default(), HeadGateEvidence::default()];
    let mut previous = None;

    for _ in 0..(tr_size / TRACE_RECORD_BYTES as u64) {
        traces.read_exact(&mut raw_trace)?;
        banks.read_exact(&mut raw_bank)?;
        let record = BankRecord::pair(
            TraceRecord::decode(&raw_trace),
            BankPlane::decode(&raw_bank),
        )
        .ok_or("CAL trace and bank sidecar disagree on byte identity")?;
        let key = (record.record.doc_index, record.record.position);
        if previous.is_some_and(|old| key <= old) {
            return Err("CAL trace has duplicate or unordered positions".into());
        }
        previous = Some(key);
        if record.record.split != 2 {
            return Err("CAL trace contained a non-CAL split; fail closed".into());
        }
        let is_train = match record.record.doc_index % 32 {
            7 => true,
            23 => false,
            _ => return Err("CAL contains document outside pre-registered split".into()),
        };
        for (i, (head, eighths)) in loaded.iter().enumerate() {
            let evidence = HeadGateObservation::from_record(&record, head, *eighths)?;
            if is_train {
                train[i].observe(evidence)?;
            } else {
                validation[i].observe(evidence)?;
            }
        }
    }

    let mut report = format!(
        "seed,train_bytes,cal_sel_bytes,train_p1_bpc,train_neural_bpc,train_baf_bpc,cal_p1_bpc,cal_neural_bpc,cal_baf_bpc,cal_hash_bpc,cal_neural_minus_baf_bpc,cal_hash_minus_baf_bpc,train_baf_selected,cal_baf_selected,hash_threshold,candidate_evals,admitted\n"
    );
    let mut gates = String::new();
    let mut all_pass = true;
    for (i, &seed) in HEAD_SEEDS.iter().enumerate() {
        let fit = fit_head_gate(&train[i], &validation[i], HeadGatePolicy::default())?;
        let hash_threshold = train[i].match_hash_activity(fit.train_selected);
        let (cal_hash, _) = validation[i].hashed_microbits_and_selected(hash_threshold);
        let train_baf = train[i].routed_microbits(&fit.gate);
        let cal_baf = validation[i].routed_microbits(&fit.gate);
        let neural_gain = bpc(
            validation[i].neural_microbits - cal_baf,
            validation[i].samples,
        );
        let hash_gain = bpc(cal_hash - cal_baf, validation[i].samples);
        let pass = fit.admitted && neural_gain >= NEURAL_BPC_MARGIN && hash_gain >= NULL_BPC_MARGIN;
        all_pass &= pass;
        use std::fmt::Write as _;
        writeln!(
            report,
            "{seed},{},{},{:.7},{:.7},{:.7},{:.7},{:.7},{:.7},{:.7},{:.7},{:.7},{},{},{},{},{}",
            train[i].samples,
            validation[i].samples,
            bpc(train[i].p1_microbits, train[i].samples),
            bpc(train[i].neural_microbits, train[i].samples),
            bpc(train_baf, train[i].samples),
            bpc(validation[i].p1_microbits, validation[i].samples),
            bpc(validation[i].neural_microbits, validation[i].samples),
            bpc(cal_baf, validation[i].samples),
            bpc(cal_hash, validation[i].samples),
            neural_gain,
            hash_gain,
            fit.train_selected,
            fit.validation_selected,
            hash_threshold,
            fit.evaluations,
            pass
        )?;
        writeln!(
            gates,
            "seed\t{seed}\nhead_sha256\t{}\nbias\t{}\nterms\t{}\nhash_threshold\t{}\nadmitted\t{}\n",
            head_digests[i],
            u8::from(fit.gate.bias),
            fit.gate.terms
                .iter()
                .map(|t| format!("{:x}:{:x}", t.event_mask, t.state_mask))
                .collect::<Vec<_>>()
                .join(","),
            hash_threshold,
            pass
        )?;
    }
    fs::create_dir(out)?;
    File::create(out.join("h4-cal.csv"))?.write_all(report.as_bytes())?;
    File::create(out.join("h4-gates.tsv"))?.write_all(gates.as_bytes())?;
    File::create(out.join("h4-verdict.txt"))?.write_all(
        format!(
            "source_p1_sha256\t{p1_hash}\nverdict\t{}\nsealed_eval_opened\tfalse\n",
            if all_pass {
                "CAL_GATE_PASS"
            } else {
                "NO_GO_CAL_GATE"
            }
        )
        .as_bytes(),
    )?;
    print!("{report}");
    println!(
        "verdict,{}",
        if all_pass {
            "CAL_GATE_PASS"
        } else {
            "NO_GO_CAL_GATE"
        }
    );
    Ok(())
}

fn smoke() -> Result<()> {
    use sml_core::hyb_baf_neural_gate::{fit_head_gate, HeadGatePolicy};
    use sml_core::hyb_h1d_bank_head::replay_bank_records;
    let mut train = HeadGateEvidence::default();
    let mut val = HeadGateEvidence::default();
    let records = replay_bank_records(128);
    let head = sml_core::hyb_h1d_bank_head::BankHead::new(vec![0; 1536], [0; 8], [0; 8]);
    for (index, mut record) in records.into_iter().enumerate() {
        record.record.doc_index = if index < 64 { 7 } else { 23 };
        record.record.position = (index % 64) as u32;
        let row = HeadGateObservation::from_record(&record, &head, 8)?;
        if index < 64 {
            train.observe(row)?;
        } else {
            val.observe(row)?;
        }
    }
    let fit = fit_head_gate(&train, &val, HeadGatePolicy::default())?;
    if fit.admitted {
        return Err("zero neural head is a null control and must not be admitted".into());
    }
    println!("BAF-H4,SMOKE,null_head_gate_not_admitted,pass");
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.as_slice() {
        [_, op] if op == "smoke" => smoke(),
        [_, op, p1, h1d, out] if op == "gate" => {
            apply_gate(Path::new(p1), Path::new(h1d), Path::new(out))
        }
        _ => Err("usage: sml-hyb-h4-baf-s5h smoke | gate P1_H1D_OUT H1D_OUT NEW_OUTPUT_DIR".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synthetic_null_head_cannot_improve_code_length() {
        smoke().unwrap();
    }
}

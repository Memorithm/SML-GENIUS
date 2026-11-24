//! First bounded SML-TUTOR cycle: native train-only constructive ANF fitting.
//! A separately verified tutor supplies four labelled TRAIN rows. DEV and TEST
//! use disjoint full input vectors; TEST is evaluated only after the DEV gate.
use sml_core::{
    anf_constructive::{fit_residual_gate, residual_output_errors, validate_and_commit},
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
};
use std::{env, error::Error, fs, io::Write, path::Path};

const WIDTH: usize = 6;
const BASELINE: usize = 4;
type Row = (usize, bool);

fn parse_train(text: &str) -> Result<Vec<Row>, &'static str> {
    let mut rows = [None; 4];
    for line in text.lines() {
        let fields: Vec<_> = line.split('|').collect();
        let [a, b, label] = fields.as_slice() else {
            return Err("expected a|b|label");
        };
        let bit = |s: &str| match s {
            "0" => Ok(0_usize),
            "1" => Ok(1_usize),
            _ => Err("non-binary row"),
        };
        let address = bit(a)? | (bit(b)? << 1);
        let target = bit(label)? != 0;
        if rows[address].replace(target).is_some() {
            return Err("duplicate TRAIN address");
        }
    }
    rows.into_iter()
        .enumerate()
        .map(|(i, y)| y.map(|label| (i, label)).ok_or("missing TRAIN address"))
        .collect()
}

fn data(rows: &[Row]) -> LaneData {
    assert!(!rows.is_empty() && rows.len() < 64);
    let mut inputs = vec![0_u64; 5];
    let mut targets = 0_u64;
    for (lane, &(address, label)) in rows.iter().enumerate() {
        for (bit, word) in inputs.iter_mut().take(4).enumerate() {
            if address & (1 << bit) != 0 {
                *word |= 1 << lane;
            }
        }
        if label {
            targets |= 1 << lane;
        }
    }
    LaneData {
        input_bits: 5,
        target_bits: 1,
        blocks: vec![LaneBlock {
            steps: 1,
            inputs,
            targets: vec![targets],
            valid: (1_u64 << rows.len()) - 1,
        }],
    }
}

fn independent_rows(start: usize, end: usize) -> Vec<Row> {
    (start..end)
        .map(|a| (a, (a & 1 != 0) != (a & 2 != 0)))
        .collect()
}

fn initial() -> LocalAnfNetwork {
    LocalAnfNetwork::with_input_bits(
        1,
        5,
        vec![LocalAnfGate::zero(vec![1], WIDTH).unwrap()],
        LocalAnfGate::from_truth_table(vec![0], &[false, true], WIDTH).unwrap(),
    )
    .unwrap()
}

fn encode(gate: &LocalAnfGate) -> String {
    let inputs = gate
        .inputs()
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let coeff = (0..gate.monomial_count()).fold(0_u64, |word, m| {
        word | (u64::from(gate.coefficient(m)) << m)
    });
    format!("SML_TUTOR_GATE_V1\n{inputs}\n{coeff}\n")
}

fn decode(text: &str) -> Result<LocalAnfGate, Box<dyn Error>> {
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 3 || lines[0] != "SML_TUTOR_GATE_V1" {
        return Err("checkpoint schema".into());
    }
    let inputs = lines[1]
        .split(',')
        .map(str::parse::<usize>)
        .collect::<Result<Vec<_>, _>>()?;
    if inputs.is_empty() || inputs.len() > 4 || inputs.iter().any(|i| !(1..=4).contains(i)) {
        return Err("checkpoint input domain".into());
    }
    Ok(LocalAnfGate::new(inputs, vec![lines[2].parse()?], WIDTH)?)
}

fn cycle(train: &[Row], checkpoint: &Path) -> Result<(), Box<dyn Error>> {
    let mut net = initial();
    let training = data(train);
    let dev = data(&independent_rows(4, 8));
    let old = net.clone();
    let fit = fit_residual_gate(&net, &training, 0, 0, BASELINE, &[1, 2, 3, 4], 4)?;
    let decision = validate_and_commit(&mut net, &dev, 0, 0, BASELINE, &fit)?;
    println!(
        "SML_TUTOR_DEV baseline_errors={} candidate_errors={} samples={} committed={}",
        decision.baseline_validation_errors,
        decision.candidate_validation_errors,
        decision.validation_samples,
        decision.committed
    );
    if !decision.committed || fit.train_errors != 0 || decision.candidate_validation_errors != 0 {
        return Err("DEV rejected candidate".into());
    }

    // The native learner has now frozen its candidate. TEST cannot change it.
    let test = data(&independent_rows(8, 16));
    let (before, samples) = residual_output_errors(&old, &test, 0, 0, BASELINE)?;
    let (after, tested) = residual_output_errors(&net, &test, 0, 0, BASELINE)?;
    println!("SML_TUTOR_TEST before_errors={before} after_errors={after} samples={tested}");
    if samples != tested || after != 0 || before <= after || net == old {
        return Err("TEST veto or unchanged learner".into());
    }

    // Isolated checkpoint, never modifies any production default. The output
    // path is supplied by the trusted experiment controller, not teacher text.
    let encoded = encode(&net.gates()[0]);
    let restored = decode(&encoded)?;
    if restored != net.gates()[0] {
        return Err("checkpoint roundtrip mismatch".into());
    }
    let mut reloaded = initial();
    reloaded.gates_mut()[0] = restored;
    if residual_output_errors(&reloaded, &test, 0, 0, BASELINE)? != (after, tested) {
        return Err("reloaded checkpoint differs".into());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(checkpoint)?;
    file.write_all(encoded.as_bytes())?;
    file.sync_all()?;
    println!(
        "SML_TUTOR_NATIVE_LEARNER=PASS train_samples={} train_errors={} learned_monomials={}",
        fit.train_samples,
        fit.train_errors,
        net.gates()[0].nonzero_monomials()
    );
    println!("SML_TUTOR_CHECKPOINT=ISOLATED");
    Ok(())
}

// Version 2 stores both learned gates. Do not fit XOR again on resume.
fn restore_xor(text: &str) -> Result<LocalAnfNetwork, Box<dyn Error>> {
    if text.len() > 128 {
        return Err("XOR checkpoint too large".into());
    }
    let mut net = initial();
    net.gates_mut()[0] = decode(text)?;
    Ok(net)
}

fn and_rows(start: usize, end: usize) -> Vec<Row> {
    (start..end)
        .map(|a| (a, a & 1 != 0 && a & 2 != 0))
        .collect()
}

fn encode_bank(xor: &LocalAnfGate, and: &LocalAnfGate) -> String {
    format!("SML_TUTOR_BANK_V2\n{}{}", encode(xor), encode(and))
}

fn decode_bank(text: &str) -> Result<(LocalAnfNetwork, LocalAnfNetwork), Box<dyn Error>> {
    let lines: Vec<_> = text.lines().collect();
    if text.len() > 256 || lines.len() != 7 || lines[0] != "SML_TUTOR_BANK_V2" {
        return Err("bank checkpoint schema".into());
    }
    let xor = restore_xor(&lines[1..4].join("\n"))?;
    let and = restore_xor(&lines[4..7].join("\n"))?;
    Ok((xor, and))
}

fn verify_bank(text: &str) -> Result<(), Box<dyn Error>> {
    let (xor, and) = decode_bank(text)?;
    let x = residual_output_errors(&xor, &data(&independent_rows(8, 16)), 0, 0, BASELINE)?;
    let a = residual_output_errors(&and, &data(&and_rows(8, 16)), 0, 0, BASELINE)?;
    if x != (0, 8) || a != (0, 8) {
        return Err("bank retention gate failed".into());
    }
    println!("SML_TUTOR_BANK_RELOAD=PASS xor_errors=0 and_errors=0 samples_per_skill=8");
    Ok(())
}

fn extend_and(xor_text: &str, train: &[Row]) -> Result<String, Box<dyn Error>> {
    let xor = restore_xor(xor_text)?;
    let xor_test = data(&independent_rows(8, 16));
    if residual_output_errors(&xor, &xor_test, 0, 0, BASELINE)? != (0, 8) {
        return Err("parent XOR checkpoint failed retention".into());
    }
    let mut and = initial();
    let before = and.clone();
    let fit = fit_residual_gate(&and, &data(train), 0, 0, BASELINE, &[1, 2, 3, 4], 4)?;
    let decision = validate_and_commit(&mut and, &data(&and_rows(4, 8)), 0, 0, BASELINE, &fit)?;
    if !decision.committed || fit.train_errors != 0 || decision.candidate_validation_errors != 0 {
        return Err("AND DEV rejected candidate".into());
    }
    let test = data(&and_rows(8, 16));
    let baseline = residual_output_errors(&before, &test, 0, 0, BASELINE)?;
    let after = residual_output_errors(&and, &test, 0, 0, BASELINE)?;
    if baseline != (2, 8) || after != (0, 8) || and == before {
        return Err("AND TEST veto".into());
    }
    let bank = encode_bank(&xor.gates()[0], &and.gates()[0]);
    let (reloaded_xor, reloaded_and) = decode_bank(&bank)?;
    if reloaded_xor != xor || reloaded_and != and {
        return Err("bank coefficient roundtrip changed".into());
    }
    verify_bank(&bank)?;
    println!("SML_TUTOR_AND_LEARNED=PASS before_errors=2 after_errors=0 samples=8");
    println!("SML_TUTOR_XOR_RETAINED=PASS");
    Ok(bank)
}

// Publish a complete immutable checkpoint only after fsync. Same-directory
// hard_link provides atomic no-clobber publication on the target Linux FS.
fn publish_bank(path: &Path, text: &str) -> Result<(), Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temp = parent.join(format!(".sml-tutor-bank-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::hard_link(&temp, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    drop(file);
    let cleanup = fs::remove_file(&temp);
    result?;
    cleanup?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

// Read-only TRAIN feedback from an actual reloaded native gate. Labels never
// enter the source vector; the oracle check protects the curriculum boundary.
fn train_feedback(
    net: &LocalAnfNetwork,
    train: &[Row],
    skill: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut feedback = Vec::new();
    for &(address, label) in train {
        let a = address & 1 != 0;
        let b = address & 2 != 0;
        let expected = match skill {
            "xor" => a ^ b,
            "and" => a & b,
            _ => return Err("unsupported feedback skill".into()),
        };
        if address >= 4 || label != expected {
            return Err("TRAIN label contradicts independent oracle".into());
        }
        let mut source = [0_u64; WIDTH];
        source[1] = u64::from(a);
        source[2] = u64::from(b);
        let mut next = [0_u64; 1];
        let mut scratch = [0_u64; 16];
        net.step_lanes(&mut source, &mut next, &mut scratch);
        let prediction = net.readout().evaluate_lanes(&source, &mut scratch) & 1;
        feedback.push(format!(
            "TRAIN|{skill}|{}|{}|0|{}|{prediction}",
            u8::from(a),
            u8::from(b),
            u8::from(label)
        ));
    }
    Ok(feedback)
}

fn score_train_bank(
    bank: &str,
    xor_train: &[Row],
    and_train: &[Row],
) -> Result<Vec<String>, Box<dyn Error>> {
    let (xor, and) = decode_bank(bank)?;
    let mut rows = train_feedback(&xor, xor_train, "xor")?;
    rows.extend(train_feedback(&and, and_train, "and")?);
    Ok(rows)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 4 && args[0] == "--score-train-bank" {
        let bank = fs::read_to_string(&args[1])?;
        let xor = fs::read_to_string(&args[2])?;
        let and = fs::read_to_string(&args[3])?;
        if xor.len() > 128 || and.len() > 128 {
            return Err("TRAIN file too large".into());
        }
        let rows = score_train_bank(&bank, &parse_train(&xor)?, &parse_train(&and)?)?;
        for row in rows {
            println!("{row}");
        }
        return Ok(());
    }
    if args.len() == 2 && args[0] == "--verify-bank" {
        return verify_bank(&fs::read_to_string(&args[1])?);
    }
    if args.len() == 4 && args[0] == "--resume-and" {
        let parent = fs::read_to_string(&args[1])?;
        let raw = fs::read_to_string(&args[2])?;
        if raw.len() > 128 {
            return Err("AND TRAIN file too large".into());
        }
        let bank = extend_and(&parent, &parse_train(&raw)?)?;
        publish_bank(Path::new(&args[3]), &bank)?;
        println!("SML_TUTOR_BANK_CHECKPOINT=ISOLATED");
        return Ok(());
    }
    if args.len() != 2 {
        return Err(
            "usage: TRAIN NEW_GATE | --resume-and XOR_GATE TRAIN NEW_BANK | --verify-bank BANK"
                .into(),
        );
    }
    let raw = fs::read_to_string(&args[0])?;
    if raw.len() > 128 {
        return Err("TRAIN file too large".into());
    }
    let train = parse_train(&raw)?;
    cycle(&train, Path::new(&args[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn train_only_fit_and_nuisance_holdout() {
        let train = parse_train("0|0|0\n1|0|1\n0|1|1\n1|1|0\n").unwrap();
        let mut net = initial();
        let fit = fit_residual_gate(&net, &data(&train), 0, 0, BASELINE, &[1, 2, 3, 4], 4).unwrap();
        assert_eq!(fit.train_errors, 0);
        let decision = validate_and_commit(
            &mut net,
            &data(&independent_rows(4, 8)),
            0,
            0,
            BASELINE,
            &fit,
        )
        .unwrap();
        assert!(decision.committed);
        assert_eq!(
            residual_output_errors(&net, &data(&independent_rows(8, 16)), 0, 0, BASELINE).unwrap(),
            (0, 8)
        );
        assert_eq!(decode(&encode(&net.gates()[0])).unwrap(), net.gates()[0]);
    }

    #[test]
    fn resume_adds_and_without_changing_xor() {
        let xor_train = parse_train("0|0|0\n1|0|1\n0|1|1\n1|1|0\n").unwrap();
        let net = initial();
        let fitted =
            fit_residual_gate(&net, &data(&xor_train), 0, 0, BASELINE, &[1, 2, 3, 4], 4).unwrap();
        let parent = encode(&fitted.gate);
        let and_train = parse_train("0|0|0\n1|0|0\n0|1|0\n1|1|1\n").unwrap();
        let bank = extend_and(&parent, &and_train).unwrap();
        let (xor, and) = decode_bank(&bank).unwrap();
        assert_eq!(xor.gates()[0], fitted.gate);
        assert_eq!(and.gates()[0].nonzero_monomials(), 1);
        assert!(verify_bank(&bank).is_ok());
        assert!(extend_and("broken", &and_train).is_err());
        assert!(extend_and(&parent, &xor_train).is_err());
    }

    #[test]
    fn feedback_reads_model_predictions_without_training_or_label_inputs() {
        let x = parse_train("0|0|0\n1|0|1\n0|1|1\n1|1|0\n").unwrap();
        let a = parse_train("0|0|0\n1|0|0\n0|1|0\n1|1|1\n").unwrap();
        let zero = initial();
        let feedback = train_feedback(&zero, &x, "xor").unwrap();
        assert_eq!(feedback[1], "TRAIN|xor|1|0|0|1|0");
        let fitted =
            fit_residual_gate(&zero, &data(&x), 0, 0, BASELINE, &[1, 2, 3, 4], 4).unwrap();
        let bank = extend_and(&encode(&fitted.gate), &a).unwrap();
        let rows = score_train_bank(&bank, &x, &a).unwrap();
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[1], "TRAIN|xor|1|0|0|1|1");
        assert_eq!(rows[7], "TRAIN|and|1|1|0|1|1");
        assert!(score_train_bank(&bank, &a, &a).is_err());
    }

    #[test]
    fn malformed_and_duplicate_rows_fail_closed() {
        assert!(parse_train("0|0|0\n0|0|1\n").is_err());
        assert!(parse_train("0|2|0\n").is_err());
        assert!(parse_train("0|0|0\n").is_err());
        assert!(decode("SML_TUTOR_GATE_V1\n0\n1\n").is_err());
    }
}

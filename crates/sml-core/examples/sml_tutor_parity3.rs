//! Bounded three-input parity learner. Actual teacher corpus is admitted by
//! RemoteOps; the native learner constructs a gate from TRAIN only.
//! DEV and TEST vary nuisance inputs not present in TRAIN.
use sml_core::{
    anf_constructive::{fit_residual_gate, residual_output_errors, validate_and_commit},
    anf_local::{LocalAnfGate, LocalAnfNetwork},
    anf_local_incremental::{LaneBlock, LaneData},
};
use std::{env, error::Error, fs, io::Write, path::Path};

const WIDTH: usize = 7;
const BASELINE: usize = 5;
type Row = (usize, bool);

fn parse_train(text: &str) -> Result<Vec<Row>, &'static str> {
    let mut rows = [None; 8];
    for line in text.lines() {
        let fields: Vec<_> = line.split('|').collect();
        let [a, b, c, label] = fields.as_slice() else {
            return Err("expected a|b|c|label");
        };
        let bit = |s: &str| match s {
            "0" => Ok(0_usize),
            "1" => Ok(1_usize),
            _ => Err("non-binary row"),
        };
        let address = bit(a)? | (bit(b)? << 1) | (bit(c)? << 2);
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
    let mut inputs = vec![0_u64; 6];
    let mut targets = 0_u64;
    for (lane, &(address, label)) in rows.iter().enumerate() {
        for (bit, word) in inputs.iter_mut().take(5).enumerate() {
            if address & (1 << bit) != 0 {
                *word |= 1 << lane;
            }
        }
        if label {
            targets |= 1 << lane;
        }
    }
    LaneData {
        input_bits: 6,
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
        .map(|a| (a, (a & 1 != 0) ^ (a & 2 != 0) ^ (a & 4 != 0)))
        .collect()
}

fn initial() -> LocalAnfNetwork {
    LocalAnfNetwork::with_input_bits(
        1,
        6,
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
    format!("SML_TUTOR_PARITY3_V1\n{inputs}\n{coeff}\n")
}

fn decode(text: &str) -> Result<LocalAnfGate, Box<dyn Error>> {
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 3 || lines[0] != "SML_TUTOR_PARITY3_V1" {
        return Err("checkpoint schema".into());
    }
    let inputs = lines[1]
        .split(',')
        .map(str::parse::<usize>)
        .collect::<Result<Vec<_>, _>>()?;
    if inputs.is_empty() || inputs.len() > 4 || inputs.iter().any(|i| !(1..=5).contains(i)) {
        return Err("checkpoint input domain".into());
    }
    Ok(LocalAnfGate::new(inputs, vec![lines[2].parse()?], WIDTH)?)
}

fn qualify(train: &[Row]) -> Result<String, Box<dyn Error>> {
    let mut net = initial();
    let old = net.clone();
    let fit = fit_residual_gate(&net, &data(train), 0, 0, BASELINE, &[1, 2, 3, 4, 5], 4)?;
    let decision = validate_and_commit(
        &mut net,
        &data(&independent_rows(8, 16)),
        0,
        0,
        BASELINE,
        &fit,
    )?;
    if !decision.committed || fit.train_errors != 0 || decision.candidate_validation_errors != 0 {
        return Err("parity3 DEV veto".into());
    }
    let test = data(&independent_rows(16, 32));
    let before = residual_output_errors(&old, &test, 0, 0, BASELINE)?;
    let after = residual_output_errors(&net, &test, 0, 0, BASELINE)?;
    if before != (8, 16) || after != (0, 16) || old == net {
        return Err("parity3 TEST veto".into());
    }
    let gate = &net.gates()[0];
    let checkpoint = encode(gate);
    if decode(&checkpoint)? != *gate {
        return Err("parity3 checkpoint mismatch".into());
    }
    println!("SML_TUTOR_PARITY3_TEST=PASS before_errors=8 after_errors=0 samples=16");
    println!(
        "SML_TUTOR_PARITY3_LEARNED=PASS train_rows=8 monomials={}",
        gate.nonzero_monomials()
    );
    Ok(checkpoint)
}

fn restore(raw: &str) -> Result<LocalAnfNetwork, Box<dyn Error>> {
    if raw.len() > 128 {
        return Err("oversized parity3 checkpoint".into());
    }
    let mut net = initial();
    net.gates_mut()[0] = decode(raw)?;
    Ok(net)
}

fn verify(net: &LocalAnfNetwork) -> Result<(), Box<dyn Error>> {
    let test = data(&independent_rows(16, 32));
    if residual_output_errors(net, &test, 0, 0, BASELINE)? != (0, 16) {
        return Err("parity3 reload veto".into());
    }
    println!("SML_TUTOR_PARITY3_RELOAD=PASS errors=0 samples=16");
    Ok(())
}

fn score(net: &LocalAnfNetwork, train: &[Row]) -> Result<Vec<String>, Box<dyn Error>> {
    let mut rows = Vec::new();
    for &(address, label) in train {
        let a = address & 1 != 0;
        let b = address & 2 != 0;
        let c = address & 4 != 0;
        if label != (a ^ b ^ c) {
            return Err("incorrect TRAIN label".into());
        }
        let mut source = [0_u64; WIDTH];
        source[1] = u64::from(a);
        source[2] = u64::from(b);
        source[3] = u64::from(c);
        let mut next = [0_u64; 1];
        let mut scratch = [0_u64; 16];
        net.step_lanes(&mut source, &mut next, &mut scratch);
        let prediction = net.readout().evaluate_lanes(&source, &mut scratch) & 1;
        rows.push(format!(
            "TRAIN|parity3|{}|{}|{}|{}|{prediction}",
            u8::from(a),
            u8::from(b),
            u8::from(c),
            u8::from(label)
        ));
    }
    Ok(rows)
}

fn load_train(path: &Path) -> Result<Vec<Row>, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?;
    if raw.len() > 128 {
        return Err("oversized TRAIN".into());
    }
    Ok(parse_train(&raw)?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 2 && args[0] == "--verify" {
        return verify(&restore(&fs::read_to_string(&args[1])?)?);
    }
    if args.len() == 3 && args[0] == "--score-train" {
        let net = restore(&fs::read_to_string(&args[1])?)?;
        for row in score(&net, &load_train(Path::new(&args[2]))?)? {
            println!("{row}");
        }
        return Ok(());
    }
    if args.len() != 2 {
        return Err("usage: TRAIN NEW_GATE | --verify GATE | --score-train GATE TRAIN".into());
    }
    let checkpoint = qualify(&load_train(Path::new(&args[0]))?)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    file.write_all(checkpoint.as_bytes())?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRAIN: &str = "0|0|0|0\n1|0|0|1\n0|1|0|1\n1|1|0|0\n0|0|1|1\n1|0|1|0\n0|1|1|0\n1|1|1|1\n";

    #[test]
    fn native_parity3_fit_reload_and_feedback() {
        let train = parse_train(TRAIN).unwrap();
        let zero = score(&initial(), &train).unwrap();
        assert_eq!(zero[7], "TRAIN|parity3|1|1|1|1|0");
        let net = restore(&qualify(&train).unwrap()).unwrap();
        assert_eq!(net.gates()[0].nonzero_monomials(), 3);
        assert!(verify(&net).is_ok());
        let rows = score(&net, &train).unwrap();
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[7], "TRAIN|parity3|1|1|1|1|1");
    }

    #[test]
    fn invalid_training_cannot_commit() {
        let wrong = TRAIN.replace("1|1|1|1", "1|1|1|0");
        assert!(qualify(&parse_train(&wrong).unwrap()).is_err());
        assert!(parse_train(&format!("{TRAIN}0|0|0|0\n")).is_err());
        assert!(restore("SML_TUTOR_PARITY3_V1\n0\n1\n").is_err());
    }
}

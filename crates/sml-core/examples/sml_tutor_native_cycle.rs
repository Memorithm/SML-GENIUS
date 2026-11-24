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

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: sml_tutor_native_cycle TRAIN_FILE NEW_CHECKPOINT".into());
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
    fn malformed_and_duplicate_rows_fail_closed() {
        assert!(parse_train("0|0|0\n0|0|1\n").is_err());
        assert!(parse_train("0|2|0\n").is_err());
        assert!(parse_train("0|0|0\n").is_err());
        assert!(decode("SML_TUTOR_GATE_V1\n0\n1\n").is_err());
    }
}

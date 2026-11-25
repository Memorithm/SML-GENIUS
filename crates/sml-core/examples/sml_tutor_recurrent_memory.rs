//! Learn a recurrent conditional-memory transition from verified TRAIN labels.
//! Fit a local ANF by the native Mobius constructor, then evaluate autonomous
//! multi-step rollouts. No teacher-forced state enters DEV or TEST.
use sml_core::anf_local::{LocalAnfGate, LocalAnfNetwork};
use std::{env, error::Error, fs, io::Write};

const WIDTH: usize = 3;

fn parse_train(raw: &str) -> Result<Vec<bool>, Box<dyn Error>> {
    if raw.len() > 128 {
        return Err("oversized TRAIN".into());
    }
    let mut rows = [None; 8];
    for line in raw.lines() {
        let fields: Vec<_> = line.split('|').collect();
        let [a, b, c, y] = fields.as_slice() else {
            return Err("expected old|input|enable|next".into());
        };
        let bit = |s: &str| match s {
            "0" => Ok(0_usize),
            "1" => Ok(1_usize),
            _ => Err("non-binary TRAIN"),
        };
        let address = bit(a)? | (bit(b)? << 1) | (bit(c)? << 2);
        if rows[address].replace(bit(y)? != 0).is_some() {
            return Err("duplicate TRAIN address".into());
        }
    }
    rows.into_iter()
        .map(|row| row.ok_or_else(|| "missing TRAIN address".into()))
        .collect()
}

fn network(gate: LocalAnfGate) -> Result<LocalAnfNetwork, Box<dyn Error>> {
    let readout = LocalAnfGate::from_truth_table(vec![0], &[false, true], WIDTH)?;
    Ok(LocalAnfNetwork::with_input_bits(1, 2, vec![gate], readout)?)
}

fn fit(table: &[bool]) -> Result<LocalAnfNetwork, Box<dyn Error>> {
    network(LocalAnfGate::from_truth_table(vec![0, 1, 2], table, WIDTH)?)
}

// All input/enable sequences of the given length, both initial states.
// Each lane feeds its own predicted state back into the next native step.
// The independent oracle only supplies the comparison target.
fn rollouts(net: &LocalAnfNetwork, steps: usize) -> (u64, u64) {
    assert!((3..=6).contains(&steps));
    let sequences = 1_usize << (2 * steps);
    let mut errors = 0;
    let mut samples = 0;
    for initial in [false, true] {
        for start in (0..sequences).step_by(64) {
            let mut source = [u64::from(initial) * u64::MAX, 0, 0];
            let mut expected = [initial; 64];
            let mut next = [0_u64; 1];
            let mut scratch = [0_u64; 8];
            for t in 0..steps {
                source[1] = 0;
                source[2] = 0;
                for (lane, state) in expected.iter_mut().enumerate() {
                    let command = (start + lane) >> (2 * t);
                    let input = command & 1 != 0;
                    let enable = command & 2 != 0;
                    source[1] |= u64::from(input) << lane;
                    source[2] |= u64::from(enable) << lane;
                    if enable {
                        *state = input;
                    }
                }
                net.step_lanes(&mut source, &mut next, &mut scratch);
                let prediction = net.readout().evaluate_lanes(&source, &mut scratch);
                let target = expected
                    .iter()
                    .enumerate()
                    .fold(0_u64, |word, (lane, &state)| {
                        word | (u64::from(state) << lane)
                    });
                errors += u64::from((prediction ^ target).count_ones());
                samples += 64;
            }
        }
    }
    (errors, samples)
}

fn encode(net: &LocalAnfNetwork) -> String {
    let gate = &net.gates()[0];
    let word = (0..gate.monomial_count()).fold(0_u64, |word, m| {
        word | (u64::from(gate.coefficient(m)) << m)
    });
    format!("SML_TUTOR_RECURRENT_V1\n0,1,2\n{word}\n")
}

fn restore(raw: &str) -> Result<LocalAnfNetwork, Box<dyn Error>> {
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() > 128 || lines.len() != 3 {
        return Err("recurrent checkpoint schema".into());
    }
    if lines[0] != "SML_TUTOR_RECURRENT_V1" || lines[1] != "0,1,2" {
        return Err("recurrent checkpoint domain".into());
    }
    let gate = LocalAnfGate::new(vec![0, 1, 2], vec![lines[2].parse()?], WIDTH)?;
    network(gate)
}

fn qualify(table: &[bool]) -> Result<String, Box<dyn Error>> {
    let baseline = network(LocalAnfGate::zero(vec![0, 1, 2], WIDTH)?)?;
    let candidate = fit(table)?;
    // DEV checks length 3 before any longer TEST rollout.
    let dev_before = rollouts(&baseline, 3);
    let dev_after = rollouts(&candidate, 3);
    if dev_after != (0, 384) || dev_before.0 == 0 {
        return Err("DEV veto".into());
    }
    for steps in [5, 6] {
        let before = rollouts(&baseline, steps);
        let after = rollouts(&candidate, steps);
        println!(
            "SML_TUTOR_RECURRENT_TEST steps={steps} before={} after={} samples={}",
            before.0, after.0, after.1
        );
        if after.0 != 0 || before.0 == 0 || before.1 != after.1 {
            return Err("TEST veto".into());
        }
    }
    let checkpoint = encode(&candidate);
    if restore(&checkpoint)? != candidate {
        return Err("checkpoint mismatch".into());
    }
    println!(
        "SML_TUTOR_RECURRENT_LEARNED=PASS train_rows=8 monomials={}",
        candidate.gates()[0].nonzero_monomials()
    );
    Ok(checkpoint)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 2 && args[0] == "--verify" {
        let net = restore(&fs::read_to_string(&args[1])?)?;
        if rollouts(&net, 6) != (0, 49152) {
            return Err("recurrent reload veto".into());
        }
        println!("SML_TUTOR_RECURRENT_RELOAD=PASS steps=6 samples=49152 errors=0");
        return Ok(());
    }
    if args.len() != 2 {
        return Err("usage: TRAIN NEW_CHECKPOINT | --verify CHECKPOINT".into());
    }
    let table = parse_train(&fs::read_to_string(&args[0])?)?;
    let checkpoint = qualify(&table)?;
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

    const TRAIN: &str = "0|0|0|0\n1|0|0|1\n0|1|0|0\n1|1|0|1\n0|0|1|0\n1|0|1|0\n0|1|1|1\n1|1|1|1\n";

    #[test]
    fn learns_and_reloads_true_recurrence() {
        let checkpoint = qualify(&parse_train(TRAIN).unwrap()).unwrap();
        let net = restore(&checkpoint).unwrap();
        assert_eq!(rollouts(&net, 5), (0, 10240));
        assert_eq!(rollouts(&net, 6), (0, 49152));
        assert_eq!(net.gates()[0].nonzero_monomials(), 3);
    }

    #[test]
    fn wrong_transition_and_duplicate_are_rejected() {
        let wrong = TRAIN.replace("1|0|0|1", "1|0|0|0");
        assert!(qualify(&parse_train(&wrong).unwrap()).is_err());
        assert!(parse_train(&format!("{TRAIN}0|0|0|0\n")).is_err());
        assert!(restore("SML_TUTOR_RECURRENT_V1\n0,1,2\n256\n").is_err());
    }
}

//! Learn conditional SET or CLEAR from external TRAIN labels only.
//! Reuse native recurrent ANF evaluation with independent trajectory oracles.
use sml_core::anf_local::{LocalAnfGate, LocalAnfNetwork};
use std::{env, error::Error, fs, io::Write};

const WIDTH: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    Set,
    Clear,
}

impl Task {
    fn parse(raw: &str) -> Result<Self, Box<dyn Error>> {
        match raw {
            "conditional-set" => Ok(Self::Set),
            "conditional-clear" => Ok(Self::Clear),
            _ => Err("unsupported recurrent task".into()),
        }
    }

    fn marker(self) -> &'static str {
        match self {
            Self::Set => "SML_TUTOR_SET_V1",
            Self::Clear => "SML_TUTOR_CLEAR_V1",
        }
    }

    fn target(self, state: bool, input: bool, enable: bool) -> bool {
        match self {
            Self::Set => state || (input && enable),
            Self::Clear => state && !(input && enable),
        }
    }
}


fn parse_train(raw: &str) -> Result<Vec<bool>, Box<dyn Error>> {
    if raw.len() > 128 {
        return Err("oversized TRAIN".into());
    }
    let mut rows = [None; 8];
    for line in raw.lines() {
        let fields: Vec<_> = line.split('|').collect();
        let [a, b, c, y] = fields.as_slice() else {
            return Err("expected old|request|enable|next".into());
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
fn rollouts(net: &LocalAnfNetwork, task: Task, steps: usize) -> (u64, u64) {
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
                    *state = task.target(*state, input, enable);
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

fn encode(net: &LocalAnfNetwork, task: Task) -> String {
    let gate = &net.gates()[0];
    let word = (0..gate.monomial_count()).fold(0_u64, |word, m| {
        word | (u64::from(gate.coefficient(m)) << m)
    });
    format!("{}\n0,1,2\n{word}\n", task.marker())
}

fn restore(raw: &str, task: Task) -> Result<LocalAnfNetwork, Box<dyn Error>> {
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() > 128 || lines.len() != 3 {
        return Err("recurrent checkpoint schema".into());
    }
    if lines[0] != task.marker() || lines[1] != "0,1,2" {
        return Err("recurrent checkpoint domain".into());
    }
    let gate = LocalAnfGate::new(vec![0, 1, 2], vec![lines[2].parse()?], WIDTH)?;
    network(gate)
}

fn qualify(table: &[bool], task: Task) -> Result<String, Box<dyn Error>> {
    let baseline = network(LocalAnfGate::zero(vec![0, 1, 2], WIDTH)?)?;
    let candidate = fit(table)?;
    // DEV checks length 3 before any longer TEST rollout.
    let dev_before = rollouts(&baseline, task, 3);
    let dev_after = rollouts(&candidate, task, 3);
    if dev_after != (0, 384) || dev_before.0 == 0 {
        return Err("DEV veto".into());
    }
    for steps in [5, 6] {
        let before = rollouts(&baseline, task, steps);
        let after = rollouts(&candidate, task, steps);
        println!(
            "SML_TUTOR_SET_CLEAR_TEST steps={steps} before={} after={} samples={}",
            before.0, after.0, after.1
        );
        if after.0 != 0 || before.0 == 0 || before.1 != after.1 {
            return Err("TEST veto".into());
        }
    }
    let checkpoint = encode(&candidate, task);
    if restore(&checkpoint, task)? != candidate {
        return Err("checkpoint mismatch".into());
    }
    println!(
        "SML_TUTOR_SET_CLEAR_LEARNED=PASS train_rows=8 monomials={}",
        candidate.gates()[0].nonzero_monomials()
    );
    Ok(checkpoint)
}

fn read_bounded(path: &str) -> Result<String, Box<dyn Error>> {
    use std::io::Read;

    let mut raw = String::new();
    fs::File::open(path)?.take(129).read_to_string(&mut raw)?;
    if raw.len() > 128 {
        return Err("oversized input".into());
    }
    Ok(raw)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    match args.as_slice() {
        [mode, skill, path] if mode == "--verify" => {
            let task = Task::parse(skill)?;
            let net = restore(&read_bounded(path)?, task)?;
            if rollouts(&net, task, 6) != (0, 49152) {
                return Err("recurrent reload veto".into());
            }
            println!("SML_TUTOR_SET_CLEAR_RELOAD=PASS skill={skill} samples=49152 errors=0");
        }
        [skill, train, path] => {
            let task = Task::parse(skill)?;
            let table = parse_train(&read_bounded(train)?)?;
            let checkpoint = qualify(&table, task)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(checkpoint.as_bytes())?;
            file.sync_all()?;
        }
        _ => return Err("usage: SKILL TRAIN NEW_CHECKPOINT | --verify SKILL CHECKPOINT".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SET: &str = "0|0|0|0\n1|0|0|1\n0|1|0|0\n1|1|0|1\n0|0|1|0\n1|0|1|1\n0|1|1|1\n1|1|1|1\n";
    const CLEAR: &str = "0|0|0|0\n1|0|0|1\n0|1|0|0\n1|1|0|1\n0|0|1|0\n1|0|1|1\n0|1|1|0\n1|1|1|0\n";

    #[test]
    fn both_transitions_learn_and_reload() {
        for (task, train) in [(Task::Set, SET), (Task::Clear, CLEAR)] {
            let checkpoint = qualify(&parse_train(train).unwrap(), task).unwrap();
            let net = restore(&checkpoint, task).unwrap();
            assert_eq!(rollouts(&net, task, 3), (0, 384));
            assert_eq!(rollouts(&net, task, 5), (0, 10240));
            assert_eq!(rollouts(&net, task, 6), (0, 49152));
            assert!(restore(
                &checkpoint,
                if task == Task::Set {
                    Task::Clear
                } else {
                    Task::Set
                },
            )
            .is_err());
        }
    }

    #[test]
    fn wrong_labels_and_incomplete_corpus_fail_closed() {
        assert!(qualify(&parse_train(SET).unwrap(), Task::Clear).is_err());
        assert!(qualify(&parse_train(CLEAR).unwrap(), Task::Set).is_err());
        assert!(parse_train("0|0|0|0\n").is_err());
        assert!(parse_train(&format!("{SET}0|0|0|0\n")).is_err());
    }
}

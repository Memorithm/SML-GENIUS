//! Select the next recurrent lesson using measured direct reuse deficits.
//! Diagnostic catalog is supplied; this is not open-ended skill invention.
//! No fitting, no teacher-generated labels, no DEV/TEST feedback or promotion.
use sml_core::anf_local::LocalAnfGate;
use std::{env, error::Error, fs::File, io::Read};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Task {
    Toggle,
    Set,
    Clear,
}

impl Task {
    const ALL: [Self; 3] = [Self::Toggle, Self::Set, Self::Clear];

    fn name(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::Set => "conditional-set",
            Self::Clear => "conditional-clear",
        }
    }

    fn target(self, a: bool, b: bool, c: bool) -> bool {
        match self {
            Self::Toggle => a ^ (b & c),
            Self::Set => a | (b & c),
            Self::Clear => a & !(b & c),
        }
    }
}

fn restore(raw: &str, marker: &str) -> Result<LocalAnfGate, Box<dyn Error>> {
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() > 128 || lines.len() != 3 || lines[0] != marker || lines[1] != "0,1,2" {
        return Err("checkpoint schema".into());
    }
    let word: u64 = lines[2].parse()?;
    Ok(LocalAnfGate::new(vec![0, 1, 2], vec![word], 3)?)
}

fn error_count(gate: &LocalAnfGate, target: impl Fn(bool, bool, bool) -> bool) -> usize {
    (0..8)
        .filter(|&i| {
            let a = i & 1 != 0;
            let b = i & 2 != 0;
            let c = i & 4 != 0;
            gate.evaluate(&[a, b, c]) != target(a, b, c)
        })
        .count()
}

// Rank by maximum residual error after best single-transition direct reuse.
// Stable catalog order resolves ties. Already-covered tasks are not lessons.
// No claim is made about what an unrestricted composition could compute.
fn plan(gates: &[LocalAnfGate]) -> (Vec<(Task, usize)>, Option<Task>) {
    let mut deficits = Vec::new();
    let mut selected = None;
    let mut worst = 0;
    for task in Task::ALL {
        let errors = gates
            .iter()
            .map(|gate| error_count(gate, |a, b, c| task.target(a, b, c)))
            .min()
            .unwrap_or(8);
        deficits.push((task, errors));
        if errors > worst {
            worst = errors;
            selected = Some(task);
        }
    }
    (deficits, selected)
}

fn read_bounded(path: &str) -> Result<String, Box<dyn Error>> {
    let mut raw = String::new();
    File::open(path)?.take(129).read_to_string(&mut raw)?;
    if raw.len() > 128 {
        return Err("oversized checkpoint".into());
    }
    Ok(raw)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if !(1..=2).contains(&args.len()) {
        return Err("usage: RECURRENT_GATE [TOGGLE_GATE]".into());
    }
    let overwrite = restore(&read_bounded(&args[0])?, "SML_TUTOR_RECURRENT_V1")?;
    if error_count(&overwrite, |a, b, c| if c { b } else { a }) != 0 {
        return Err("invalid retained OVERWRITE behavior".into());
    }
    let mut gates = vec![overwrite];
    if let Some(path) = args.get(1) {
        let toggle = restore(&read_bounded(path)?, "SML_TUTOR_TOGGLE_V1")?;
        if error_count(&toggle, |a, b, c| Task::Toggle.target(a, b, c)) != 0 {
            return Err("invalid retained TOGGLE behavior".into());
        }
        gates.push(toggle);
    }
    let (deficits, next) = plan(&gates);
    for (task, errors) in deficits {
        println!(
            "SML_TUTOR_GAP task={} errors={errors} addresses=8 direct_reuse=true",
            task.name()
        );
    }
    let next = next.map_or("none", Task::name);
    println!("SML_TUTOR_NEXT_LESSON={next} selection=measured_gap catalog=bounded");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_selection_when_new_transition_is_retained() {
        let overwrite = restore("SML_TUTOR_RECURRENT_V1\n0,1,2\n98\n", "SML_TUTOR_RECURRENT_V1")
            .unwrap();
        let toggle =
            restore("SML_TUTOR_TOGGLE_V1\n0,1,2\n66\n", "SML_TUTOR_TOGGLE_V1").unwrap();
        let (before, next) = plan(&[overwrite.clone()]);
        assert_eq!(before, vec![(Task::Toggle, 2), (Task::Set, 1), (Task::Clear, 3)]);
        assert_eq!(next, Some(Task::Clear));
        let (after, next) = plan(&[overwrite, toggle]);
        assert_eq!(after, vec![(Task::Toggle, 0), (Task::Set, 1), (Task::Clear, 1)]);
        assert_eq!(next, Some(Task::Set));
    }

    #[test]
    fn full_direct_coverage_halts() {
        let gates: Vec<_> = Task::ALL
            .iter()
            .map(|task| {
                let table: Vec<_> = (0..8)
                    .map(|i| task.target(i & 1 != 0, i & 2 != 0, i & 4 != 0))
                    .collect();
                LocalAnfGate::from_truth_table(vec![0, 1, 2], &table, 3).unwrap()
            })
            .collect();
        assert_eq!(plan(&gates).1, None);
    }

    #[test]
    fn malformed_checkpoint_rejected() {
        for raw in [
            "SML_TUTOR_RECURRENT_V1\n0,1,2\n256\n",
            "SML_TUTOR_RECURRENT_V1\n1,0,2\n98\n",
            "SML_TUTOR_RECURRENT_V1\n0,1,2\n98\nextra\n",
        ] {
            assert!(restore(raw, "SML_TUTOR_RECURRENT_V1").is_err());
        }
    }
}

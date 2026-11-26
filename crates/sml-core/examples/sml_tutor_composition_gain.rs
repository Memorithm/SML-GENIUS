//! Measure marginal coverage from an independently acquired four-input gate.
//! Same expression-tree budget before and after; no fitting or publication.
//! Gate count is a structural cost, not an execution-time measurement.
use sml_core::anf_local::LocalAnfGate;
use std::{env, error::Error, fs::File, io::Read};

const INPUTS: [u16; 4] = [0xaaaa, 0xcccc, 0xf0f0, 0xff00];
const UNIVERSE: usize = 65536;
const MAX_COST: usize = 3;
const MARKERS: [&str; 4] = [
    "SML_TUTOR_RECURRENT_V1",
    "SML_TUTOR_TOGGLE_V1",
    "SML_TUTOR_SET_V1",
    "SML_TUTOR_CLEAR_V1",
];

fn gate_bits(address: usize) -> [bool; 3] {
    [address & 1 != 0, address & 2 != 0, address & 4 != 0]
}

fn bits(address: usize) -> [bool; 4] {
    [
        address & 1 != 0,
        address & 2 != 0,
        address & 4 != 0,
        address & 8 != 0,
    ]
}

fn oracle(kind: usize, [a, b, c]: [bool; 3]) -> bool {
    match kind {
        0 => {
            if c {
                b
            } else {
                a
            }
        }
        1 => a ^ (b & c),
        2 => a | (b & c),
        3 => a & !(b & c),
        _ => unreachable!(),
    }
}

fn restore(raw: &str, kind: usize) -> Result<LocalAnfGate, Box<dyn Error>> {
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() > 128 || lines.len() != 3 || lines[0] != MARKERS[kind] || lines[1] != "0,1,2" {
        return Err("checkpoint schema".into());
    }
    let gate = LocalAnfGate::new(vec![0, 1, 2], vec![lines[2].parse()?], 3)?;
    if (0..8).any(|i| gate.evaluate(&gate_bits(i)) != oracle(kind, gate_bits(i))) {
        return Err("retained gate behavior".into());
    }
    Ok(gate)
}

fn read_gate(path: &str, kind: usize) -> Result<LocalAnfGate, Box<dyn Error>> {
    let mut raw = String::new();
    File::open(path)?.take(129).read_to_string(&mut raw)?;
    restore(&raw, kind)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Recipe {
    Input(usize),
    Gate { kind: usize, children: [u16; 3] },
    Acquired { children: [u16; 4] },
}

struct Closure {
    levels: Vec<Vec<u16>>,
    recipes: Vec<Option<Recipe>>,
}

// Every tree has a root gate and three or four subtrees. Replacing a subtree
// with its cheapest equivalent truth signature cannot increase tree cost.
// Thus the first discovered signature has minimum tree cost within this grammar.
fn discover(gates: &[LocalAnfGate], acquired: Option<&LocalAnfGate>, budget: usize) -> Closure {
    let mut recipes = vec![None; UNIVERSE];
    for (i, signature) in INPUTS.into_iter().enumerate() {
        recipes[usize::from(signature)] = Some(Recipe::Input(i));
    }
    let mut levels = vec![INPUTS.to_vec()];
    for cost in 1..=budget {
        let mut next = Vec::new();
        for left in 0..cost {
            for middle in 0..(cost - left) {
                let right = cost - 1 - left - middle;
                for &a in &levels[left] {
                    for &b in &levels[middle] {
                        for &c in &levels[right] {
                            let children = [a, b, c];
                            for (kind, gate) in gates.iter().enumerate() {
                                let mut scratch = [0; 8];
                                let signature = gate.evaluate_lanes(
                                    &[u64::from(a), u64::from(b), u64::from(c)],
                                    &mut scratch,
                                ) as u16;
                                let slot = &mut recipes[usize::from(signature)];
                                if slot.is_none() {
                                    *slot = Some(Recipe::Gate { kind, children });
                                    next.push(signature);
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(gate) = acquired {
            for first in 0..cost {
                for second in 0..(cost - first) {
                    for third in 0..(cost - first - second) {
                        let fourth = cost - 1 - first - second - third;
                        for &a in &levels[first] {
                            for &b in &levels[second] {
                                for &c in &levels[third] {
                                    for &d in &levels[fourth] {
                                        let children = [a, b, c, d];
                                        let values = children.map(u64::from);
                                        let mut scratch = [0; 16];
                                        let sig = gate.evaluate_lanes(&values, &mut scratch);
                                        let signature = sig as u16;
                                        let slot = &mut recipes[usize::from(signature)];
                                        if slot.is_none() {
                                            *slot = Some(Recipe::Acquired { children });
                                            next.push(signature);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        levels.push(next);
    }
    Closure { levels, recipes }
}

// Scalar replay uses native evaluate, independently of the bit-sliced search.
fn replay(
    closure: &Closure,
    gates: &[LocalAnfGate],
    acquired: Option<&LocalAnfGate>,
    signature: u16,
    input: &[bool; 4],
) -> (bool, usize) {
    match closure.recipes[usize::from(signature)].expect("discovered recipe") {
        Recipe::Input(i) => (input[i], 0),
        Recipe::Gate { kind, children } => {
            let [a, b, c] = children.map(|child| replay(closure, gates, acquired, child, input));
            (gates[kind].evaluate(&[a.0, b.0, c.0]), 1 + a.1 + b.1 + c.1)
        }
        Recipe::Acquired { children } => {
            let [a, b, c, d] = children.map(|child| replay(closure, gates, acquired, child, input));
            (
                acquired
                    .expect("acquired gate")
                    .evaluate(&[a.0, b.0, c.0, d.0]),
                1 + a.1 + b.1 + c.1 + d.1,
            )
        }
    }
}

fn verify(
    closure: &Closure,
    gates: &[LocalAnfGate],
    acquired: Option<&LocalAnfGate>,
) -> Result<(), Box<dyn Error>> {
    for (cost, level) in closure.levels.iter().enumerate() {
        for &signature in level {
            for address in 0..16 {
                let input = bits(address);
                let (value, actual_cost) = replay(closure, gates, acquired, signature, &input);
                if value != (signature & (1 << address) != 0) || actual_cost != cost {
                    return Err("composition witness replay mismatch".into());
                }
            }
        }
    }
    Ok(())
}

fn restore_acquired(raw: &str, expected: &str) -> Result<(LocalAnfGate, u16), Box<dyn Error>> {
    let signature = u16::from_str_radix(expected, 16)?;
    let lines: Vec<_> = raw.lines().collect();
    if raw.len() > 128
        || format!("{signature:04x}") != expected
        || signature & 1 != 0
        || lines.len() != 4
        || lines[0] != "SML_TUTOR_COMPOSITION4_GATE_V1"
        || lines[1] != expected
        || lines[2] != "0,1,2,3"
    {
        return Err("acquired checkpoint schema".into());
    }
    let word: u64 = lines[3].parse()?;
    let gate = LocalAnfGate::new(vec![0, 1, 2, 3], vec![word], 4)?;
    if (0..16).any(|i| gate.evaluate(&bits(i)) != (signature & (1 << i) != 0)) {
        return Err("acquired checkpoint behavior".into());
    }
    let canonical = format!("SML_TUTOR_COMPOSITION4_GATE_V1\n{signature:04x}\n0,1,2,3\n{word}\n");
    if raw != canonical {
        return Err("noncanonical acquired checkpoint".into());
    }
    Ok((gate, signature))
}

fn cost_map(closure: &Closure) -> Vec<usize> {
    let mut costs = vec![usize::MAX; UNIVERSE];
    for (cost, level) in closure.levels.iter().enumerate() {
        for &signature in level {
            costs[usize::from(signature)] = cost;
        }
    }
    costs
}

type Coverage = (Vec<usize>, Vec<usize>);

fn measure(
    gates: &[LocalAnfGate],
    acquired: &LocalAnfGate,
    signature: u16,
) -> Result<Coverage, Box<dyn Error>> {
    let before = discover(gates, None, MAX_COST);
    verify(&before, gates, None)?;
    let after = discover(gates, Some(acquired), MAX_COST);
    verify(&after, gates, Some(acquired))?;
    let old = cost_map(&before);
    let new = cost_map(&after);
    if old[usize::from(signature)] != usize::MAX || new[usize::from(signature)] != 1 {
        return Err("candidate is not a new one-gate capability".into());
    }
    if old.iter().zip(&new).any(|(a, b)| b > a) {
        return Err("retained capability regressed".into());
    }
    if after.levels.iter().flatten().any(|s| s & 1 != 0) {
        return Err("zero-preserving invariant violated".into());
    }
    Ok((
        before.levels.iter().map(Vec::len).collect(),
        after.levels.iter().map(Vec::len).collect(),
    ))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if args.len() != 6 {
        return Err("usage: OVERWRITE TOGGLE SET CLEAR SIGNATURE ACQUIRED_GATE".into());
    }
    let gates = args[..4]
        .iter()
        .enumerate()
        .map(|(kind, path)| read_gate(path, kind))
        .collect::<Result<Vec<_>, _>>()?;
    let mut raw = String::new();
    File::open(&args[5])?.take(129).read_to_string(&mut raw)?;
    let (acquired, signature) = restore_acquired(&raw, &args[4])?;
    let (before, after) = measure(&gates, &acquired, signature)?;
    let (mut old_total, mut new_total) = (0, 0);
    for (cost, (old, new)) in before.into_iter().zip(after).enumerate() {
        old_total += old;
        new_total += new;
        println!("SML_TUTOR_GAIN cost={cost} before={old_total} after={new_total}");
    }
    println!(
        "SML_TUTOR_GAIN=PASS signature={signature:04x} before={old_total} after={new_total} gain={} retained=true",
        new_total - old_total
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<LocalAnfGate> {
        MARKERS
            .iter()
            .zip([98, 66, 194, 130])
            .enumerate()
            .map(|(kind, (marker, word))| {
                restore(&format!("{marker}\n0,1,2\n{word}\n"), kind).unwrap()
            })
            .collect()
    }

    #[test]
    fn reference_marginal_coverage_and_retention() {
        // Synthetic fixture only; coefficients are never computed in the CLI.
        let signature = 0x099e;
        let table: Vec<_> = (0..16).map(|i| signature & (1 << i) != 0).collect();
        let gate = LocalAnfGate::from_truth_table(vec![0, 1, 2, 3], &table, 4).unwrap();
        let (before, after) = measure(&fixture(), &gate, signature).unwrap();
        assert_eq!(before, vec![4, 91, 1840, 11681]);
        assert_eq!(after, vec![4, 145, 5984, 24910]);
    }

    #[test]
    fn rejects_corrupt_or_mismatched_acquired_checkpoint() {
        let raw = "SML_TUTOR_COMPOSITION4_GATE_V1\n099e\n0,1,2,3\n0\n";
        assert!(restore_acquired(raw, "099e").is_err());
        assert!(restore_acquired(raw, "8000").is_err());
    }
}

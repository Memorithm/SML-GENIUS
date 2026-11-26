//! Re-select the next composition gap from a bounded bank of learned gates.
//! Exact tree closure and independent scalar replay; no fitting or publication.
//! A gate count is a structural budget, not measured execution cost.
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
    Acquired { kind: usize, children: [u16; 4] },
}

struct Closure {
    levels: Vec<Vec<u16>>,
    recipes: Vec<Option<Recipe>>,
}

// Every tree has a root gate and three or four subtrees. Replacing a subtree
// with its cheapest equivalent truth signature cannot increase tree cost.
// Thus the first discovered signature has minimum tree cost within this grammar.
fn discover(gates: &[LocalAnfGate], acquired: &[LocalAnfGate], budget: usize) -> Closure {
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
        for (kind, gate) in acquired.iter().enumerate() {
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
                                            *slot = Some(Recipe::Acquired { kind, children });
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
    acquired: &[LocalAnfGate],
    signature: u16,
    input: &[bool; 4],
) -> (bool, usize) {
    match closure.recipes[usize::from(signature)].expect("discovered recipe") {
        Recipe::Input(i) => (input[i], 0),
        Recipe::Gate { kind, children } => {
            let [a, b, c] = children.map(|child| replay(closure, gates, acquired, child, input));
            (gates[kind].evaluate(&[a.0, b.0, c.0]), 1 + a.1 + b.1 + c.1)
        }
        Recipe::Acquired { kind, children } => {
            let [a, b, c, d] = children.map(|child| replay(closure, gates, acquired, child, input));
            (
                acquired[kind].evaluate(&[a.0, b.0, c.0, d.0]),
                1 + a.1 + b.1 + c.1 + d.1,
            )
        }
    }
}

fn verify(
    closure: &Closure,
    gates: &[LocalAnfGate],
    acquired: &[LocalAnfGate],
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

fn full_support(signature: u16) -> bool {
    (0..4).all(|bit| {
        let mask = 1 << bit;
        (0..16).any(|i| (signature >> i) & 1 != (signature >> (i ^ mask)) & 1)
    })
}

// Exact multi-source shortest-path distance on the 15-dimensional Boolean
// hypercube of zero-preserving truth tables. Every edge flips one output bit.
fn distances(closure: &Closure) -> Vec<u8> {
    let mut distance = vec![u8::MAX; UNIVERSE];
    let mut queue = Vec::with_capacity(UNIVERSE / 2);
    for &signature in closure.levels.iter().flatten() {
        distance[usize::from(signature)] = 0;
        queue.push(signature);
    }
    let mut cursor = 0;
    while cursor < queue.len() {
        let signature = queue[cursor];
        cursor += 1;
        let next_distance = distance[usize::from(signature)] + 1;
        for bit in 1..16 {
            let neighbor = signature ^ (1 << bit);
            let entry = &mut distance[usize::from(neighbor)];
            if *entry == u8::MAX {
                *entry = next_distance;
                queue.push(neighbor);
            }
        }
    }
    distance
}

// Rank uncovered full-support functions by exact distance to the bounded
// closure. A gap is cost-limited, not a proof of logical unlearnability.
fn select_gap(closure: &Closure) -> Option<(u16, u8)> {
    let distance = distances(closure);
    let mut best = None;
    for signature in (0u16..=u16::MAX).step_by(2) {
        if !full_support(signature) || closure.recipes[usize::from(signature)].is_some() {
            continue;
        }
        let residual = distance[usize::from(signature)];
        if best.is_none_or(|(_, old_distance)| residual > old_distance) {
            best = Some((signature, residual));
        }
    }
    best
}

fn read_acquired(path: &str) -> Result<(LocalAnfGate, u16), Box<dyn Error>> {
    let mut raw = String::new();
    File::open(path)?.take(129).read_to_string(&mut raw)?;
    let expected = raw.lines().nth(1).ok_or("missing signature")?;
    let (gate, signature) = restore_acquired(&raw, expected)?;
    if !full_support(signature) {
        return Err("learned gate lacks full support".into());
    }
    Ok((gate, signature))
}

fn checked_bank(
    gates: &[LocalAnfGate],
    learned: &[(LocalAnfGate, u16)],
) -> Result<Closure, Box<dyn Error>> {
    let mut active = Vec::new();
    let mut previous = discover(gates, &active, MAX_COST);
    verify(&previous, gates, &active)?;
    for (gate, signature) in learned {
        if select_gap(&previous).map(|(s, _)| s) != Some(*signature) {
            return Err("learned gate is not the selected next gap".into());
        }
        active.push(gate.clone());
        let next = discover(gates, &active, MAX_COST);
        verify(&next, gates, &active)?;
        let mut costs = vec![usize::MAX; UNIVERSE];
        for (cost, level) in next.levels.iter().enumerate() {
            for &s in level {
                costs[usize::from(s)] = cost;
            }
        }
        for (cost, level) in previous.levels.iter().enumerate() {
            if level.iter().any(|&s| costs[usize::from(s)] > cost) {
                return Err("learned gate regressed previous capability".into());
            }
        }
        if costs[usize::from(*signature)] != 1
            || next.levels.iter().flatten().any(|s| s & 1 != 0)
        {
            return Err("learned gate admission invariant".into());
        }
        previous = next;
    }
    Ok(previous)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    if !(5..=6).contains(&args.len()) {
        return Err("usage: OVERWRITE TOGGLE SET CLEAR ACQUIRED [NEXT_ACQUIRED]".into());
    }
    let gates = args[..4]
        .iter()
        .enumerate()
        .map(|(kind, path)| read_gate(path, kind))
        .collect::<Result<Vec<_>, _>>()?;
    let learned = args[4..]
        .iter()
        .map(|path| read_acquired(path))
        .collect::<Result<Vec<_>, _>>()?;
    let closure = checked_bank(&gates, &learned)?;
    for (cost, level) in closure.levels.iter().enumerate() {
        println!(
            "SML_TUTOR_RESELECT cost={cost} new_signatures={}",
            level.len()
        );
    }
    let reachable: usize = closure.levels.iter().map(Vec::len).sum();
    println!(
        "SML_TUTOR_RESELECT_CLOSURE reachable={reachable} remaining={} learned={} budget={MAX_COST}",
        UNIVERSE / 2 - reachable,
        learned.len()
    );
    match select_gap(&closure) {
        Some((signature, residual)) => println!(
            "SML_TUTOR_RESELECT_NEXT signature={signature:04x} residual={residual} addresses=16"
        ),
        None => println!("SML_TUTOR_RESELECT_NEXT=none"),
    }
    println!("SML_TUTOR_RESELECT_REPLAY=PASS");
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

    fn learned(signature: u16) -> (LocalAnfGate, u16) {
        let table: Vec<_> = (0..16).map(|i| signature & (1 << i) != 0).collect();
        (
            LocalAnfGate::from_truth_table(vec![0, 1, 2, 3], &table, 4).unwrap(),
            signature,
        )
    }

    #[test]
    fn reselect_and_measure_two_successive_acquisitions() {
        let gates = fixture();
        let first = checked_bank(&gates, &[learned(0x099e)]).unwrap();
        assert_eq!(
            first.levels.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4, 145, 5984, 24910]
        );
        assert_eq!(select_gap(&first), Some((0x1668, 2)));
        let second = checked_bank(&gates, &[learned(0x099e), learned(0x1668)]).unwrap();
        assert_eq!(
            second.levels.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![4, 158, 7382, 24808]
        );
        assert_eq!(select_gap(&second), Some((0x166e, 1)));
    }

    #[test]
    fn rejects_wrong_order_and_duplicate() {
        let gates = fixture();
        assert!(checked_bank(&gates, &[learned(0x1668)]).is_err());
        assert!(checked_bank(&gates, &[learned(0x099e), learned(0x099e)]).is_err());
        let corrupt = "SML_TUTOR_COMPOSITION4_GATE_V1\n099e\n0,1,2,3\n0\n";
        assert!(restore_acquired(corrupt, "099e").is_err());
    }
}
